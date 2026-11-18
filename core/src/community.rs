//! Communities (NIP-29, spec protocol § 9): a community is a group hosted by an
//! instance's relay, its channels are child groups. The relay enforces the
//! permissions and signs each group's state (39000-39003) with its own key
//! (NIP-11 `self`): state signed by anyone else is ignored.
//!
//! Everything received is in the local store (nostr-sdk saves it): reads come
//! from there, `sync_community` fills it and keeps a live subscription.

use std::collections::{HashMap, HashSet};

use nostr_sdk::prelude::{
    Event, EventId, Filter, FinalizeEvent, Kind, PublicKey, ReqTarget, SingleLetterTag,
    SubscriptionId, Tag, Timestamp,
};
use serde::{Deserialize, Serialize};

use crate::media::{self, Attachment};
use crate::perms::{self, Override, Policy, Role};
use crate::session::{Session, TIMEOUT, backfill, http, is_local_host};
use crate::{Error, Result, event, link};

pub const ROLE_ADMIN: &str = "admin";
pub const ROLE_MODERATOR: &str = "moderator";

const STATE_KINDS: [Kind; 5] = [
    Kind::GroupMetadata,
    Kind::GroupAdmins,
    Kind::GroupMembers,
    Kind::GroupRoles,
    Kind::GroupLivekitParticipants,
];
const ID_LEN: usize = 12;
/// Lines of the audit log fetched at once.
// ponytail: the last 500 only; page with `until` if a community needs more history.
const AUDIT_LIMIT: usize = 500;
const INVITE_LEN: usize = 8;
/// Lowercase Crockford base32: a subset of the `[a-z0-9]` the spec allows for ids.
const ID_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Community {
    pub id: String,
    pub relay: String,
    pub name: String,
    pub picture: Option<String>,
    pub about: Option<String>,
    pub banner: Option<String>,
    pub channels: Vec<Channel>,
    /// In display order; a channel names its own (spec § 9, Salons et catégories).
    pub categories: Vec<Category>,
    /// My role ids here, empty for a plain member.
    pub roles: Vec<String>,
    /// The owner (spec § 9), if the relay names one.
    pub owner: Option<PublicKey>,
    /// My permissions in the community (spec § 9), in spec order.
    pub perms: Vec<String>,
    /// Every role, highest first, `everyone` last.
    pub all_roles: Vec<Role>,
    /// The roles strictly below mine: those I may edit, move, delete and give.
    pub manageable_roles: Vec<String>,
    /// End of my timeout (unix seconds), 0: none (spec § 9, Modération).
    pub timeout: u64,
}

/// What `update_community` changes; `None` keeps it (spec § 9, Réglages).
#[derive(Debug, Default, Deserialize)]
pub struct CommunityEdit {
    pub name: Option<String>,
    pub about: Option<String>,
    /// The whole list, highest first, `everyone` last.
    pub roles: Option<Vec<Role>>,
    /// Gives the community away (owner only).
    pub owner: Option<PublicKey>,
    /// Every category, in order (`manage_channels`).
    pub categories: Option<Vec<Category>>,
    /// Every channel, in order, each in its category (`manage_channels`).
    pub channels: Option<Vec<Placement>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub name: String,
}

/// Where a channel sits: its place in the list, and its category.
#[derive(Debug, Clone, Deserialize)]
pub struct Placement {
    pub id: String,
    pub category: Option<String>,
}

/// What `edit_channel` changes; `None` keeps it. 0 turns slow mode or the limit off.
#[derive(Debug, Default, Deserialize)]
pub struct ChannelEdit {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub slowmode: Option<u32>,
    pub user_limit: Option<u32>,
    /// Every override of the channel (`manage_roles` for what changes).
    pub overrides: Option<Vec<ChannelOverride>>,
}

/// What a channel allows and denies one target: a role id, `everyone` or a key (spec § 9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelOverride {
    pub target: String,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

/// One entry per target, from a channel's allow and deny tags.
fn override_views(overrides: &[Override]) -> Vec<ChannelOverride> {
    let mut out: Vec<ChannelOverride> = Vec::new();
    for o in overrides {
        let i = match out.iter().position(|v| v.target == o.target) {
            Some(i) => i,
            None => {
                out.push(ChannelOverride {
                    target: o.target.clone(),
                    allow: Vec::new(),
                    deny: Vec::new(),
                });
                out.len() - 1
            }
        };
        let side = if o.allow {
            &mut out[i].allow
        } else {
            &mut out[i].deny
        };
        side.extend(o.perms.names());
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    /// A voice channel (NIP-29 `livekit`, spec § 11).
    pub voice: bool,
    /// Who is in it, by the relay's 39004 (voice channels only).
    pub participants: Vec<PublicKey>,
    /// My permissions in this channel (spec § 9).
    pub perms: Vec<String>,
    /// Its category's id; none: above the categories.
    pub category: Option<String>,
    pub topic: Option<String>,
    /// Seconds between two messages of a member, 0: off.
    pub slowmode: u32,
    /// Most people in a voice channel, 0: no limit.
    pub user_limit: u32,
    /// A member without roles can't see it (spec § 9, Salons privés).
    pub private: bool,
    pub overrides: Vec<ChannelOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Member {
    pub pubkey: PublicKey,
    pub roles: Vec<String>,
    /// The community's owner (spec § 9).
    pub owner: bool,
    /// Strictly below me in the hierarchy: I may act on them, given the permission.
    pub manageable: bool,
    /// The color of their highest colored role.
    pub color: Option<String>,
    /// Their highest `hoist` role: the section they're listed under.
    pub hoist: Option<String>,
    /// End of their timeout (unix seconds), 0: none (spec § 9, Modération).
    pub timeout: u64,
}

/// A banned key, from its `9001` (spec § 9, Modération).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Ban {
    pub pubkey: PublicKey,
    pub by: PublicKey,
    pub reason: Option<String>,
    pub at: u64,
}

/// One line of the audit log (spec § 9, Modération), newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditEntry {
    pub id: EventId,
    pub at: u64,
    pub actor: PublicKey,
    /// `roles`, `timeout`, `kick`, `ban`, `unban`, `delete_message`, `edit`,
    /// `create_channel`, `delete_channel` or `leave`.
    pub action: &'static str,
    pub target: Option<PublicKey>,
    /// Where it happened, when it's a channel.
    pub channel: Option<String>,
    pub reason: Option<String>,
    /// `roles`: the roles given (none: plain member).
    pub roles: Vec<String>,
    /// `timeout`: its end, 0: lifted.
    pub until: u64,
}

/// What an accepted moderation event means for the log; None: not part of it.
fn audit_entry(ev: &Event, community: &str) -> Option<AuditEntry> {
    let tag = |name: &str| tag_values(ev, name).next().map(str::to_string);
    let target = tag("p").and_then(|p| PublicKey::from_hex(&p).ok());
    let group = tag("h")?;
    let mut entry = AuditEntry {
        id: ev.id,
        at: ev.created_at.as_secs(),
        actor: ev.pubkey,
        action: "edit",
        target,
        channel: (group != community).then_some(group),
        reason: (!ev.content.is_empty()).then(|| ev.content.clone()),
        roles: Vec::new(),
        until: 0,
    };
    entry.action = match ev.kind {
        Kind::GroupPutUser => match tag("timeout") {
            Some(until) => {
                entry.until = until.parse().unwrap_or_default();
                "timeout"
            }
            None => {
                entry.roles = admins(ev).into_values().next().unwrap_or_default();
                "roles"
            }
        },
        Kind::GroupRemoveUser if ev.tags.iter().any(|t| t.as_slice() == ["ban"]) => "ban",
        Kind::GroupRemoveUser => "kick",
        Kind::GroupDeleteEvent => match tag("k").as_deref() {
            Some("9001") => "unban",
            Some("9") if target.is_some_and(|t| t != ev.pubkey) => "delete_message",
            _ => return None,
        },
        Kind::GroupEditMetadata => "edit",
        Kind::GroupCreateGroup if entry.channel.is_some() => "create_channel",
        Kind::GroupDeleteGroup if entry.channel.is_some() => "delete_channel",
        Kind::GroupLeaveRequest => "leave",
        _ => return None,
    };
    Some(entry)
}

/// Running timeouts of a `39002`: `["timeout", key, unix]`.
fn timeouts(ev: &Event, now: u64) -> HashMap<PublicKey, u64> {
    ev.tags
        .iter()
        .filter_map(|tag| match tag.as_slice() {
            [k, pk, until, ..] if k == "timeout" => Some((
                PublicKey::from_hex(pk).ok()?,
                until.parse::<u64>().ok().filter(|u| *u > now)?,
            )),
            _ => None,
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChannelMessage {
    pub id: EventId,
    pub channel: String,
    pub author: PublicKey,
    /// The caption when there's an attachment.
    pub text: String,
    pub created_at: Timestamp,
    pub attachment: Option<Attachment>,
}

/// `<host>'<community>/<code>` (spec § 9, invitations).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    pub host: String,
    pub community: String,
    pub code: String,
}

impl Invite {
    pub fn parse(text: &str) -> Result<Self> {
        let bad = || Error::Invalid("not a community invite".into());
        let (host, rest) = text.trim().split_once('\'').ok_or_else(bad)?;
        let (community, code) = rest.split_once('/').ok_or_else(bad)?;
        let code = code.to_uppercase();
        let host_ok = !host.is_empty() && !host.contains(['/', '@', ' ']);
        if !host_ok
            || !is_id(community)
            || code.len() != INVITE_LEN
            || !code.bytes().all(|b| link::ALPHABET.contains(&b))
        {
            return Err(bad());
        }
        Ok(Self {
            host: host.to_lowercase(),
            community: community.into(),
            code,
        })
    }

    /// The community's relay: `wss://`, `ws://` for a local dev instance.
    pub fn relay(&self) -> String {
        let scheme = if is_local_host(&self.host) {
            "ws"
        } else {
            "wss"
        };
        format!("{scheme}://{}", self.host)
    }
}

impl std::fmt::Display for Invite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}'{}/{}", self.host, self.community, self.code)
    }
}

/// Something to refresh, from `Session::listen`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Update {
    Dm {
        message: crate::dm::Message,
        /// From a room where I never wrote: don't notify.
        request: bool,
    },
    /// My blocks changed (another device): read DMs and channels again.
    Blocks,
    ChannelMessage {
        message: ChannelMessage,
    },
    Deleted {
        channel: String,
        ids: Vec<EventId>,
    },
    /// My communities, their channels or members changed: read them again.
    Communities,
    /// Someone calls me (spec § 11): ring until `until`, a `call-end` for
    /// this call, or I join or decline.
    Ring {
        message: crate::dm::Message,
        until: Timestamp,
    },
    /// The voice channel or call I'm in changed; `None`: I'm in none.
    Voice {
        state: Option<crate::voice::VoiceState>,
    },
}

impl Session {
    /// Creates a community on my instance, with a first `general` channel.
    pub async fn create_community(&self, name: &str) -> Result<Community> {
        let relay = self.account().relay.clone();
        let id = random_id()?;
        self.group_event(&relay, Kind::GroupCreateGroup, &id, [], "")
            .await?;
        self.add_to_list(&id, &relay).await?;
        self.edit_community(&id, |meta, _| meta.name = name.into())
            .await?;
        self.create_channel(&id, "general", false, None).await?;
        self.community(&id)
            .await?
            .ok_or_else(|| Error::Server("community not published".into()))
    }

    /// `manage_channels`: one 9007, the relay adds it at the end of the
    /// community (spec § 9, Salons et catégories). A voice channel stays one.
    pub async fn create_channel(
        &self,
        community: &str,
        name: &str,
        voice: bool,
        category: Option<&str>,
    ) -> Result<Channel> {
        let relay = self.relay_of(community).await?;
        let id = random_id()?;
        let mut tags = vec![
            Tag::parse(["parent", community])?,
            Tag::parse(["name", name])?,
        ];
        if voice {
            tags.push(Tag::parse(["livekit"])?);
        }
        if let Some(category) = category {
            tags.push(Tag::parse(["category", category])?);
        }
        self.group_event(&relay, Kind::GroupCreateGroup, &id, tags, "")
            .await?;
        self.sync_community(community, &relay).await?;
        self.communities()
            .await?
            .into_iter()
            .find(|c| c.id == community)
            .and_then(|c| c.channels.into_iter().find(|ch| ch.id == id))
            .ok_or_else(|| Error::Server("channel not published".into()))
    }

    /// `manage_channels` in it: name, topic, slow mode, user limit. Sends all
    /// of its settings, read just before (spec § 9).
    pub async fn edit_channel(
        &self,
        community: &str,
        channel: &str,
        edit: ChannelEdit,
    ) -> Result<()> {
        let relay = self.relay_of(community).await?;
        self.fetch_state(&relay, &[channel.to_string()]).await?;
        let key = self.relay_key(&relay).await?;
        let mut meta = self
            .metadata(key, channel)
            .await?
            .ok_or_else(|| Error::Server("channel state not found".into()))?;
        if let Some(name) = edit.name {
            meta.name = name;
        }
        if let Some(topic) = edit.topic {
            meta.about = Some(topic).filter(|t| !t.is_empty());
        }
        let slowmode = edit.slowmode.unwrap_or(meta.slowmode);
        let limit = edit.user_limit.unwrap_or(meta.user_limit);
        let mut tags = vec![
            Tag::parse(["name", &meta.name])?,
            Tag::parse(["parent", community])?,
        ];
        if let Some(about) = &meta.about {
            tags.push(Tag::parse(["about", about])?);
        }
        if meta.voice {
            tags.push(Tag::parse(["livekit"])?);
        }
        if slowmode > 0 {
            tags.push(Tag::parse(["slowmode", &slowmode.to_string()])?);
        }
        if limit > 0 {
            tags.push(Tag::parse(["user_limit", &limit.to_string()])?);
        }
        // Unchanged unless edited: the relay keeps what's sent.
        let overrides = edit
            .overrides
            .unwrap_or_else(|| override_views(&meta.overrides));
        for o in &overrides {
            for (kind, perms) in [("allow", &o.allow), ("deny", &o.deny)] {
                if !perms.is_empty() {
                    let mut tag = vec![kind, o.target.as_str()];
                    tag.extend(perms.iter().map(String::as_str));
                    tags.push(Tag::parse(tag)?);
                }
            }
        }
        self.group_event(&relay, Kind::GroupEditMetadata, channel, tags, "")
            .await?;
        self.sync_community(community, &relay).await
    }

    /// `manage_channels` in it: the relay takes it off the community.
    pub async fn delete_channel(&self, community: &str, channel: &str) -> Result<()> {
        let relay = self.relay_of(community).await?;
        self.group_event(&relay, Kind::GroupDeleteGroup, channel, [], "")
            .await?;
        self.sync_community(community, &relay).await
    }

    /// A reusable invite (admins and moderators), to share as text.
    pub async fn invite(&self, community: &str) -> Result<String> {
        let relay = self.relay_of(community).await?;
        let code = link::random(link::ALPHABET, INVITE_LEN)?;
        self.group_event(
            &relay,
            Kind::GroupCreateInvite,
            community,
            [Tag::parse(["code", &code])?],
            "",
        )
        .await?;
        Ok(Invite {
            host: host_of(&relay).into(),
            community: community.into(),
            code,
        }
        .to_string())
    }

    /// Joins with an invite, then lists the community for my other devices (10009).
    pub async fn join(&self, invite: &str) -> Result<Community> {
        let invite = Invite::parse(invite)?;
        let relay = invite.relay();
        self.ensure_relay(&relay).await?;
        let joined = self
            .group_event(
                &relay,
                Kind::GroupJoinRequest,
                &invite.community,
                [
                    Tag::parse(["code", &invite.code])?,
                    // Where I come from: the host checks it (spec federation § 6).
                    Tag::parse(["nip05", &self.account().handle])?,
                ],
                "",
            )
            .await;
        match joined {
            // Already in, e.g. from another device: just list it.
            Err(Error::NotDelivered(reason)) if reason.contains("duplicate:") => {}
            Err(Error::NotDelivered(reason)) if reason.contains("invite code") => {
                return Err(Error::InvalidInvite);
            }
            other => drop(other?),
        }
        self.add_to_list(&invite.community, &relay).await?;
        self.sync_community(&invite.community, &relay).await?;
        self.community(&invite.community)
            .await?
            .ok_or_else(|| Error::Server("community state not found".into()))
    }

    pub async fn leave(&self, community: &str) -> Result<()> {
        let relay = self.relay_of(community).await?;
        self.group_event(&relay, Kind::GroupLeaveRequest, community, [], "")
            .await?;
        let mut list = self.group_list().await?;
        list.retain(|(id, _)| id != community);
        self.publish_list(&list).await?;
        self.client.unsubscribe(&subscription(community)).await?;
        Ok(())
    }

    /// My communities, in the order of my list. Hidden until their state is
    /// synced, and once I'm no longer a member.
    pub async fn communities(&self) -> Result<Vec<Community>> {
        let mut all = Vec::new();
        for (id, _) in self.group_list().await? {
            if let Some(c) = self.community(&id).await? {
                all.push(c);
            }
        }
        Ok(all)
    }

    pub async fn members(&self, community: &str) -> Result<Vec<Member>> {
        let key = self.relay_key(&self.relay_of(community).await?).await?;
        let Some(members) = self.state(key, Kind::GroupMembers, community).await? else {
            return Ok(Vec::new());
        };
        let roles = self
            .state(key, Kind::GroupAdmins, community)
            .await?
            .map(|ev| admins(&ev))
            .unwrap_or_default();
        let policy = self.policy(key, community).await?;
        let me = self.account().pubkey;
        let timeouts = timeouts(&members, Timestamp::now().as_secs());
        fn as_member<'a>(
            roles: &'a HashMap<PublicKey, Vec<String>>,
            pk: &PublicKey,
            hex: &'a str,
        ) -> perms::Member<'a> {
            perms::Member {
                pubkey: hex,
                roles: roles.get(pk).map(Vec::as_slice).unwrap_or_default(),
                timeout: 0,
            }
        }
        let me_hex = me.to_hex();
        let mut all: Vec<Member> = tag_values(&members, "p")
            .filter_map(|pk| PublicKey::from_hex(pk).ok())
            .map(|pubkey| {
                let hex = pubkey.to_hex();
                let mine = roles.get(&pubkey).map(Vec::as_slice).unwrap_or_default();
                let highest = |pick: &dyn Fn(&perms::Role) -> bool| {
                    policy
                        .roles
                        .iter()
                        .find(|r| mine.contains(&r.id) && pick(r))
                };
                Member {
                    color: highest(&|r| !r.color.is_empty()).map(|r| r.color.clone()),
                    hoist: highest(&|r| r.attrs.iter().any(|a| a == "hoist")).map(|r| r.id.clone()),
                    roles: mine.to_vec(),
                    owner: hex == policy.owner,
                    timeout: timeouts.get(&pubkey).copied().unwrap_or_default(),
                    manageable: policy.outranks(
                        &as_member(&roles, &me, &me_hex),
                        &as_member(&roles, &pubkey, &hex),
                    ),
                    pubkey,
                }
            })
            .collect();
        all.sort_by_key(|m| m.pubkey);
        Ok(all)
    }

    /// Gives `pubkey` exactly `roles` (none = plain member). Admins only.
    pub async fn set_roles(
        &self,
        community: &str,
        pubkey: PublicKey,
        roles: &[&str],
    ) -> Result<()> {
        let relay = self.relay_of(community).await?;
        let pk = pubkey.to_hex();
        let tag = Tag::parse([&["p", pk.as_str()], roles].concat())?;
        self.group_event(&relay, Kind::GroupPutUser, community, [tag], "")
            .await
            .map(drop)
    }

    /// Admins and moderators; a moderator can't remove an admin.
    pub async fn remove_member(
        &self,
        community: &str,
        pubkey: PublicKey,
        reason: &str,
    ) -> Result<()> {
        let relay = self.relay_of(community).await?;
        let tag = Tag::parse(["p", &pubkey.to_hex()])?;
        self.group_event(&relay, Kind::GroupRemoveUser, community, [tag], reason)
            .await
            .map(drop)
    }

    /// Removes `pubkey` for good, member or not (spec § 9, Modération). `ban_members`.
    pub async fn ban(&self, community: &str, pubkey: PublicKey, reason: &str) -> Result<()> {
        let relay = self.relay_of(community).await?;
        let tags = [Tag::parse(["p", &pubkey.to_hex()])?, Tag::parse(["ban"])?];
        self.group_event(&relay, Kind::GroupRemoveUser, community, tags, reason)
            .await
            .map(drop)
    }

    /// Lets `pubkey` back in: deletes its ban. `ban_members`.
    pub async fn unban(&self, community: &str, pubkey: PublicKey) -> Result<()> {
        let relay = self.relay_of(community).await?;
        let bans = self.ban_events(community, &relay).await?;
        let Some(ban) = bans
            .iter()
            .find(|ev| tag_values(ev, "p").any(|p| p == pubkey.to_hex()))
        else {
            return Ok(()); // not banned (any more)
        };
        let tags = [
            Tag::event(ban.id),
            Tag::parse(["k", "9001"])?,
            Tag::parse(["p", &pubkey.to_hex()])?,
        ];
        self.group_event(&relay, Kind::GroupDeleteEvent, community, tags, "")
            .await
            .map(drop)
    }

    /// Keeps `pubkey` from writing and talking until `until` (unix seconds,
    /// 28 days at most), 0 lifts it. `moderate_members`.
    pub async fn timeout(
        &self,
        community: &str,
        pubkey: PublicKey,
        until: u64,
        reason: &str,
    ) -> Result<()> {
        let relay = self.relay_of(community).await?;
        let tags = [
            Tag::parse(["p", &pubkey.to_hex()])?,
            Tag::parse(["timeout", &until.to_string()])?,
        ];
        self.group_event(&relay, Kind::GroupPutUser, community, tags, reason)
            .await
            .map(drop)
    }

    /// Who is banned, newest first. `view_audit_log` (the relay serves the bans only then).
    pub async fn bans(&self, community: &str) -> Result<Vec<Ban>> {
        let relay = self.relay_of(community).await?;
        let mut bans: Vec<Ban> = self
            .ban_events(community, &relay)
            .await?
            .into_iter()
            .filter_map(|ev| {
                let pubkey = PublicKey::from_hex(tag_values(&ev, "p").next()?).ok()?;
                Some(Ban {
                    pubkey,
                    by: ev.pubkey,
                    reason: (!ev.content.is_empty()).then_some(ev.content),
                    at: ev.created_at.as_secs(),
                })
            })
            .collect();
        bans.sort_by_key(|b| std::cmp::Reverse(b.at));
        Ok(bans)
    }

    /// The running bans: `9001` `["ban"]` that no unban (`9005` `["k", "9001"]`) names.
    async fn ban_events(&self, community: &str, relay: &str) -> Result<Vec<Event>> {
        let bans = Filter::new()
            .kind(Kind::GroupRemoveUser)
            .custom_tag(SingleLetterTag::LOWERCASE_H, community);
        let unbans = Filter::new()
            .kind(Kind::GroupDeleteEvent)
            .custom_tag(SingleLetterTag::LOWERCASE_H, community)
            .custom_tag(SingleLetterTag::LOWERCASE_K, "9001");
        let events = self
            .client
            .fetch_events(ReqTarget::single(relay, [bans, unbans]))
            .timeout(TIMEOUT)
            .await?;
        let lifted: Vec<&str> = events
            .iter()
            .filter(|ev| ev.kind == Kind::GroupDeleteEvent)
            .flat_map(|ev| tag_values(ev, "e"))
            .collect();
        Ok(events
            .iter()
            .filter(|ev| ev.kind == Kind::GroupRemoveUser)
            .filter(|ev| ev.tags.iter().any(|t| t.as_slice() == ["ban"]))
            .filter(|ev| !lifted.contains(&ev.id.to_hex().as_str()))
            .cloned()
            .collect())
    }

    /// The last moderation actions in the community and its channels, newest first. `view_audit_log`.
    pub async fn audit_log(&self, community: &str) -> Result<Vec<AuditEntry>> {
        let relay = self.relay_of(community).await?;
        let key = self.relay_key(&relay).await?;
        let mut groups = self
            .metadata(key, community)
            .await?
            .map(|m| m.children)
            .unwrap_or_default();
        groups.push(community.to_string());
        let filter = Filter::new()
            .kinds([
                Kind::GroupPutUser,
                Kind::GroupRemoveUser,
                Kind::GroupEditMetadata,
                Kind::GroupDeleteEvent,
                Kind::GroupCreateGroup,
                Kind::GroupDeleteGroup,
                Kind::GroupLeaveRequest,
            ])
            .custom_tags(SingleLetterTag::LOWERCASE_H, groups)
            .limit(AUDIT_LIMIT);
        let events = self
            .client
            .fetch_events(ReqTarget::single(&relay, [filter]))
            .timeout(TIMEOUT)
            .await?;
        // The relay's own (joins, departures it records) aren't anyone's action.
        let mut log: Vec<AuditEntry> = events
            .iter()
            .filter(|ev| ev.pubkey != key)
            .filter_map(|ev| audit_entry(ev, community))
            .collect();
        log.sort_by_key(|e| std::cmp::Reverse(e.at));
        Ok(log)
    }

    /// Messages of `channel`, oldest first, without the deleted ones.
    pub async fn channel_messages(&self, channel: &str) -> Result<Vec<ChannelMessage>> {
        let in_channel = Filter::new().custom_tag(SingleLetterTag::LOWERCASE_H, channel);
        let db = self.client.database();
        let messages = db.query(in_channel.clone().kind(Kind::ChatMessage)).await?;
        let deletions = db.query(in_channel.kind(Kind::GroupDeleteEvent)).await?;
        let blocked = self.blocks().await?.keys;
        let shown = messages.iter().filter(|m| !blocked.contains(&m.pubkey));
        Ok(visible(shown, deletions.iter()))
    }

    pub async fn send(&self, community: &str, channel: &str, text: &str) -> Result<ChannelMessage> {
        let relay = self.relay_of(community).await?;
        let ev = self
            .group_event(&relay, Kind::ChatMessage, channel, [], text)
            .await?;
        message(&ev).ok_or_else(|| Error::Invalid("empty message".into()))
    }

    /// Encrypts `bytes`, uploads them to my instance and posts them in `channel`.
    pub async fn send_file(
        &self,
        community: &str,
        channel: &str,
        bytes: &[u8],
        mime: &str,
        name: &str,
        caption: &str,
    ) -> Result<ChannelMessage> {
        let relay = self.relay_of(community).await?;
        let file = self.upload_attachment(bytes, mime, name).await?;
        let ev = self
            .group_event(
                &relay,
                Kind::ChatMessage,
                channel,
                [file.to_imeta()?],
                caption,
            )
            .await?;
        message(&ev).ok_or_else(|| Error::Invalid("empty message".into()))
    }

    /// `manage_community`: a public picture or banner, uploaded to my instance (spec § 10).
    pub async fn set_community_picture(
        &self,
        community: &str,
        bytes: Vec<u8>,
        mime: &str,
        banner: bool,
    ) -> Result<()> {
        let url = media::upload(&self.keys, &self.account().relay, bytes, mime).await?;
        self.edit_community(community, |meta, _| match banner {
            true => meta.banner = Some(url),
            false => meta.picture = Some(url),
        })
        .await
    }

    /// Name, description, roles, owner: each needs its permission (spec § 9, Réglages).
    pub async fn update_community(&self, community: &str, edit: CommunityEdit) -> Result<()> {
        self.edit_community(community, |meta, policy| {
            if let Some(name) = edit.name {
                meta.name = name;
            }
            if let Some(about) = edit.about {
                meta.about = Some(about).filter(|a| !a.is_empty());
            }
            if let Some(roles) = edit.roles {
                policy.roles = roles;
            }
            if let Some(owner) = edit.owner {
                policy.owner = owner.to_hex();
            }
            if let Some(categories) = edit.categories {
                meta.categories = categories;
            }
            if let Some(channels) = edit.channels {
                meta.children = channels.iter().map(|c| c.id.clone()).collect();
                meta.placement = channels
                    .into_iter()
                    .filter_map(|c| Some((c.id, c.category?)))
                    .collect();
            }
        })
        .await
    }

    /// Owner only: deletes the community and its channels for everyone.
    pub async fn delete_community(&self, community: &str) -> Result<()> {
        let relay = self.relay_of(community).await?;
        self.group_event(&relay, Kind::GroupDeleteGroup, community, [], "")
            .await?;
        let mut list = self.group_list().await?;
        list.retain(|(id, _)| id != community);
        self.publish_list(&list).await?;
        self.client.unsubscribe(&subscription(community)).await?;
        Ok(())
    }

    /// My own messages; any message for admins and moderators.
    pub async fn delete_message(&self, community: &str, channel: &str, id: EventId) -> Result<()> {
        let relay = self.relay_of(community).await?;
        // k and p say what went, and whose, for the audit log (spec § 9, Modération).
        let mut tags = vec![Tag::event(id), Tag::parse(["k", "9"])?];
        if let Some(author) = self
            .client
            .database()
            .event_by_id(&id)
            .await?
            .map(|e| e.pubkey)
        {
            tags.push(Tag::parse(["p", &author.to_hex()])?);
        }
        self.group_event(&relay, Kind::GroupDeleteEvent, channel, tags, "")
            .await
            .map(drop)
    }

    /// My list (10009) from my relay, then every community in it. One
    /// unreachable instance doesn't stop the others.
    pub(crate) async fn sync_communities(&self) -> Result<()> {
        let relay = self.account().relay.clone();
        // My lists: communities (10009) and blocks (10000), kept in sync between devices.
        let me = Filter::new().author(self.account().pubkey);
        let mine = [Kind::SimpleGroups, Kind::MuteList].map(|k| me.clone().kind(k));
        self.client
            .fetch_events(ReqTarget::single(&relay, mine.clone().map(|f| f.limit(1))))
            .timeout(TIMEOUT)
            .await?;
        self.resubscribe(
            SubscriptionId::new("communities"),
            &relay,
            mine.map(|f| f.since(Timestamp::now())).to_vec(),
        )
        .await?;
        for (id, relay) in self.group_list().await? {
            // ponytail: errors dropped; surface "instance unreachable" in the UI later.
            let _ = self.sync_community(&id, &relay).await;
        }
        Ok(())
    }

    /// Fetches the state and new messages of a community, then subscribes to it.
    /// Called again when its channels change, to follow the new ones.
    pub(crate) async fn sync_community(&self, id: &str, relay: &str) -> Result<()> {
        let channels = self.catch_up(id, relay).await?;
        let key = self.relay_key(relay).await?;
        let mut groups = channels.clone();
        groups.push(id.to_string());
        let state = Filter::new()
            .author(key)
            .kinds(STATE_KINDS)
            .identifiers(groups);
        let mut filters = vec![state];
        if !channels.is_empty() {
            let live = Filter::new()
                .kinds([Kind::ChatMessage, Kind::GroupDeleteEvent])
                .custom_tags(SingleLetterTag::LOWERCASE_H, channels);
            filters.push(live.since(Timestamp::now()));
        }
        self.resubscribe(subscription(id), relay, filters).await
    }

    /// Fetches community `id`'s state and the history of each channel I may
    /// see since its newest stored message; returns its channel ids.
    async fn catch_up(&self, id: &str, relay: &str) -> Result<Vec<String>> {
        self.ensure_relay(relay).await?;
        let key = self.relay_key(relay).await?;
        let answered = !self.fetch_state(relay, &[id.to_string()]).await?.is_empty();
        let channels = match self.metadata(key, id).await? {
            Some(meta) => meta.children,
            None => Vec::new(),
        };
        let seen = self.fetch_state(relay, &channels).await?;
        if answered {
            // The relay only serves the state of channels I may see (spec § 9, Salons
            // privés): one it withheld was closed to me, forget what I had of it.
            for gone in channels.iter().filter(|c| !seen.contains(*c)) {
                let stale = Filter::new()
                    .author(key)
                    .kind(Kind::GroupMetadata)
                    .identifier(gone);
                self.client.database().delete(stale).await?;
            }
        }

        // Each channel I may see, since its newest stored message: one just
        // opened to me (spec § 9, Salons privés) comes whole.
        // ponytail: one REQ per channel; batch the ones with history if communities grow big.
        for channel in channels.iter().filter(|c| seen.contains(*c)) {
            let chat = Filter::new()
                .kinds([Kind::ChatMessage, Kind::GroupDeleteEvent])
                .custom_tag(SingleLetterTag::LOWERCASE_H, channel);
            let stored = self.client.database().query(chat.clone().limit(1)).await?;
            let history = match stored.iter().map(|e| e.created_at).max() {
                Some(last) => chat.since(last),
                None => chat,
            };
            backfill(history, |f| async move {
                let events = self
                    .client
                    .fetch_events(ReqTarget::single(relay, [f]))
                    .timeout(TIMEOUT)
                    .await?;
                Ok(events.into_iter().collect())
            })
            .await?;
        }
        Ok(channels)
    }

    /// Replaces the subscription `id`: nostr-sdk refuses an id already in use
    /// (and only says so per relay), so the old one is closed first.
    async fn resubscribe(
        &self,
        id: SubscriptionId,
        relay: &str,
        filters: Vec<Filter>,
    ) -> Result<()> {
        let _guard = self.resubscribing.lock().await;
        let _ = self.client.unsubscribe(&id).await;
        let out = self
            .client
            .subscribe(ReqTarget::single(relay, filters))
            .with_id(id)
            .await?;
        if out.success.is_empty() {
            return Err(Error::NotDelivered(format!("{:?}", out.failed)));
        }
        Ok(())
    }

    /// What a received event changes, if anything. `None` for events of other kinds.
    pub(crate) async fn community_update(&self, ev: &Event) -> Option<Update> {
        if ev.kind == Kind::ChatMessage {
            if self.blocks().await.ok()?.keys.contains(&ev.pubkey) {
                return None;
            }
            return message(ev).map(|message| Update::ChannelMessage { message });
        }
        if ev.kind == Kind::GroupDeleteEvent {
            return Some(Update::Deleted {
                channel: tag_values(ev, "h").next()?.to_string(),
                ids: tag_values(ev, "e")
                    .filter_map(|id| EventId::from_hex(id).ok())
                    .collect(),
            });
        }
        if ev.kind == Kind::MuteList && ev.pubkey == self.account().pubkey {
            return Some(Update::Blocks);
        }
        if ev.kind == Kind::SimpleGroups && ev.pubkey == self.account().pubkey {
            // Another device joined or left something.
            let _ = self.sync_communities().await;
            return Some(Update::Communities);
        }
        if !STATE_KINDS.contains(&ev.kind) || !self.is_relay_key(ev.pubkey) {
            return None;
        }
        if matches!(
            ev.kind,
            Kind::GroupMetadata | Kind::GroupAdmins | Kind::GroupRoles
        ) {
            // Channels, roles or a channel's permissions changed: what I may see too.
            // Fetch the history of channels just opened to me; follow new channels.
            let id = ev.tags.identifier()?;
            if let Some((community, relay)) = self.community_of(ev.pubkey, &id).await {
                let _ = if ev.kind == Kind::GroupMetadata && community == id {
                    self.sync_community(&community, &relay).await
                } else {
                    self.catch_up(&community, &relay).await.map(drop)
                };
            }
        }
        Some(Update::Communities)
    }

    /// The community of group `id` (itself, or the one listing it), with its relay.
    async fn community_of(&self, key: PublicKey, id: &str) -> Option<(String, String)> {
        for (community, relay) in self.group_list().await.ok()? {
            let listed = self
                .metadata(key, &community)
                .await
                .ok()
                .flatten()
                .is_some_and(|m| m.children.iter().any(|c| c == id));
            if community == id || listed {
                return Some((community, relay));
            }
        }
        None
    }

    async fn community(&self, id: &str) -> Result<Option<Community>> {
        let relay = self.relay_of(id).await?;
        let Some(key) = self.cached_relay_key(&relay) else {
            return Ok(None); // never synced: unknown relay key
        };
        let Some(meta) = self.metadata(key, id).await? else {
            return Ok(None);
        };
        let me = self.account().pubkey;
        let member = self
            .state(key, Kind::GroupMembers, id)
            .await?
            .is_some_and(|ev| tag_values(&ev, "p").any(|p| p == me.to_hex()));
        if !member {
            return Ok(None);
        }
        let roles = self
            .state(key, Kind::GroupAdmins, id)
            .await?
            .and_then(|ev| admins(&ev).remove(&me))
            .unwrap_or_default();
        let policy = self.policy(key, id).await?;
        let me_hex = me.to_hex();
        let now = Timestamp::now().as_secs();
        let timeout = self
            .state(key, Kind::GroupMembers, id)
            .await?
            .and_then(|ev| timeouts(&ev, now).remove(&me))
            .unwrap_or_default();
        let mine = perms::Member {
            pubkey: &me_hex,
            roles: &roles,
            timeout,
        };
        let nobody = perms::Member::default();
        let mut channels = Vec::new();
        for child in meta.children {
            if let Some(m) = self.metadata(key, &child).await? {
                let perms = policy.effective(&mine, Some(&m.overrides), now);
                if !perms.has("view_channel") {
                    continue; // a private channel I can't see
                }
                let participants = match m.voice {
                    true => self
                        .state(key, Kind::GroupLivekitParticipants, &child)
                        .await?
                        .map(|ev| participants(&ev))
                        .unwrap_or_default(),
                    false => Vec::new(),
                };
                let category = meta
                    .placement
                    .get(&child)
                    .filter(|c| meta.categories.iter().any(|k| k.id == **c))
                    .cloned();
                channels.push(Channel {
                    id: child,
                    name: m.name,
                    voice: m.voice,
                    participants,
                    perms: perms.names(),
                    private: !policy
                        .effective(&nobody, Some(&m.overrides), now)
                        .has("view_channel"),
                    overrides: override_views(&m.overrides),
                    category,
                    topic: m.about,
                    slowmode: m.slowmode,
                    user_limit: m.user_limit,
                });
            }
        }
        // Shown only if hosted by the community's instance (spec § 10).
        let picture = meta
            .picture
            .filter(|url| media::allowed(url, &[host_of(&relay)]).is_some());
        let banner = meta
            .banner
            .filter(|url| media::allowed(url, &[host_of(&relay)]).is_some());
        Ok(Some(Community {
            id: id.into(),
            relay,
            name: meta.name,
            picture,
            about: meta.about,
            banner,
            channels,
            categories: meta.categories,
            manageable_roles: policy.roles_below(&mine),
            timeout,
            all_roles: policy.roles.clone(),
            perms: policy.effective(&mine, None, now).names(),
            owner: meta.owner.and_then(|pk| PublicKey::from_hex(&pk).ok()),
            roles,
        }))
    }

    /// Builds, signs and publishes a group event to `relay`, and keeps it locally.
    async fn group_event<I>(
        &self,
        relay: &str,
        kind: Kind,
        group: &str,
        tags: I,
        content: &str,
    ) -> Result<Event>
    where
        I: IntoIterator<Item = Tag>,
    {
        let mut all = vec![Tag::parse(["h", group])?];
        all.extend(tags);
        let ev = event::build(&self.keys, kind, all, content)?;
        self.publish_to(relay, &ev).await?;
        Ok(ev)
    }

    /// Sends a community's 9002 with every setting, from its state read just
    /// before and changed by `edit` (spec § 9, Réglages), then syncs it.
    async fn edit_community(
        &self,
        community: &str,
        edit: impl FnOnce(&mut Metadata, &mut Policy),
    ) -> Result<()> {
        let relay = self.relay_of(community).await?;
        self.fetch_state(&relay, &[community.to_string()]).await?;
        let key = self.relay_key(&relay).await?;
        let mut meta = self
            .metadata(key, community)
            .await?
            .ok_or_else(|| Error::Server("community state not found".into()))?;
        let mut policy = self.policy(key, community).await?;
        edit(&mut meta, &mut policy);
        let mut tags = vec![Tag::parse(["name", &meta.name])?];
        for (k, v) in [
            ("picture", &meta.picture),
            ("about", &meta.about),
            ("banner", &meta.banner),
        ] {
            if let Some(v) = v {
                tags.push(Tag::parse([k, v])?);
            }
        }
        for child in &meta.children {
            match meta.placement.get(child) {
                Some(category) => tags.push(Tag::parse(["child", child, category])?),
                None => tags.push(Tag::parse(["child", child])?),
            }
        }
        for c in &meta.categories {
            tags.push(Tag::parse(["category", &c.id, &c.name])?);
        }
        tags.push(Tag::parse(["owner", &policy.owner])?);
        for r in &policy.roles {
            let mut tag = vec!["role", &r.id, &r.name, &r.color];
            tag.extend(r.attrs.iter().map(String::as_str));
            tags.push(Tag::parse(tag)?);
        }
        self.group_event(&relay, Kind::GroupEditMetadata, community, tags, "")
            .await?;
        self.sync_community(community, &relay).await
    }

    async fn publish_to(&self, relay: &str, ev: &Event) -> Result<()> {
        let out = self.client.send_event(ev).to([relay]).await?;
        if out.success.is_empty() {
            let reasons: Vec<String> = out.failed.into_values().collect();
            return Err(Error::NotDelivered(reasons.join("; ")));
        }
        self.client.database().save_event(ev).await?;
        Ok(())
    }

    async fn ensure_relay(&self, relay: &str) -> Result<()> {
        self.client.add_relay(relay).await?;
        self.client.connect_relay(relay).await?;
        Ok(())
    }

    /// Fetches the relay-signed state of `groups` into the local store.
    /// Fetches the relay-signed state of `groups`; returns the ids it served.
    async fn fetch_state(&self, relay: &str, groups: &[String]) -> Result<Vec<String>> {
        if groups.is_empty() {
            return Ok(Vec::new());
        }
        let key = self.relay_key(relay).await?;
        let filter = Filter::new()
            .author(key)
            .kinds(STATE_KINDS)
            .identifiers(groups.iter().cloned());
        let events = self
            .client
            .fetch_events(ReqTarget::single(relay, [filter]))
            .timeout(TIMEOUT)
            .await?;
        Ok(events.iter().filter_map(|e| e.tags.identifier()).collect())
    }

    async fn state(&self, key: PublicKey, kind: Kind, group: &str) -> Result<Option<Event>> {
        let filter = Filter::new()
            .author(key)
            .kind(kind)
            .identifier(group)
            .limit(1);
        Ok(self.client.database().query(filter).await?.first().cloned())
    }

    /// Owner (39000) and roles (39003) of a community; the default roles if it has none.
    async fn policy(&self, key: PublicKey, community: &str) -> Result<Policy> {
        Ok(Policy {
            owner: self
                .metadata(key, community)
                .await?
                .and_then(|m| m.owner)
                .unwrap_or_default(),
            roles: self
                .state(key, Kind::GroupRoles, community)
                .await?
                .and_then(|ev| perms::parse_roles(ev.tags.iter().map(|t| t.as_slice())))
                .unwrap_or_else(perms::default_roles),
        })
    }

    async fn metadata(&self, key: PublicKey, group: &str) -> Result<Option<Metadata>> {
        Ok(self
            .state(key, Kind::GroupMetadata, group)
            .await?
            .map(|ev| metadata(&ev)))
    }

    /// The relay key from NIP-11 `self`, fetched once per relay.
    // ponytail: in memory only; persist it (pinning) if relays start rotating keys.
    async fn relay_key(&self, relay: &str) -> Result<PublicKey> {
        if let Some(key) = self.cached_relay_key(relay) {
            return Ok(key);
        }
        #[derive(Deserialize)]
        struct Info {
            #[serde(rename = "self")]
            key: Option<PublicKey>,
        }
        let url = relay.replacen("ws", "http", 1); // ws→http, wss→https
        let info: Info = http()
            .get(url)
            .header("Accept", "application/nostr+json")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let key = info
            .key
            .ok_or_else(|| Error::Server(format!("{relay} has no relay key (NIP-11 self)")))?;
        if let Ok(mut keys) = self.relay_keys.lock() {
            keys.insert(relay.to_string(), key);
        }
        Ok(key)
    }

    fn cached_relay_key(&self, relay: &str) -> Option<PublicKey> {
        self.relay_keys.lock().ok()?.get(relay).copied()
    }

    fn is_relay_key(&self, pubkey: PublicKey) -> bool {
        self.relay_keys
            .lock()
            .is_ok_and(|keys| keys.values().any(|k| *k == pubkey))
    }

    pub(crate) async fn relay_of(&self, community: &str) -> Result<String> {
        self.group_list()
            .await?
            .into_iter()
            .find(|(id, _)| id == community)
            .map(|(_, relay)| relay)
            .ok_or(Error::NotFound)
    }

    /// Relays of my communities (for the auto-download rule, spec § 10).
    pub(crate) async fn communities_relays(&self) -> Result<Vec<String>> {
        Ok(self
            .group_list()
            .await?
            .into_iter()
            .map(|(_, relay)| relay)
            .collect())
    }

    /// My communities, `(id, relay)`, from my latest 10009.
    async fn group_list(&self) -> Result<Vec<(String, String)>> {
        Ok(self
            .list_event()
            .await?
            .map(|ev| groups(&ev))
            .unwrap_or_default())
    }

    async fn list_event(&self) -> Result<Option<Event>> {
        let filter = Filter::new()
            .author(self.account().pubkey)
            .kind(Kind::SimpleGroups)
            .limit(1);
        Ok(self.client.database().query(filter).await?.first().cloned())
    }

    async fn add_to_list(&self, id: &str, relay: &str) -> Result<()> {
        let mut list = self.group_list().await?;
        if !list.iter().any(|(c, _)| c == id) {
            list.push((id.into(), relay.into()));
        }
        self.publish_list(&list).await
    }

    async fn publish_list(&self, list: &[(String, String)]) -> Result<()> {
        let tags = list
            .iter()
            .map(|(id, relay)| Tag::parse(["group", id, relay]))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        // Replaceable: same second = lowest id wins, so always move forward.
        let mut at = Timestamp::now();
        if let Some(last) = self.list_event().await? {
            at = at.max(last.created_at + 1);
        }
        let ev = event::builder(Kind::SimpleGroups, tags, "")?
            .custom_created_at(at)
            .finalize(&self.keys)?;
        self.publish_to(&self.account().relay, &ev).await
    }
}

#[derive(Debug, Default, PartialEq)]
struct Metadata {
    name: String,
    picture: Option<String>,
    about: Option<String>,
    banner: Option<String>,
    children: Vec<String>,
    /// Channel → category id (communities).
    placement: HashMap<String, String>,
    categories: Vec<Category>,
    voice: bool,
    owner: Option<String>,
    slowmode: u32,
    user_limit: u32,
    /// A channel's allow and deny tags.
    overrides: Vec<Override>,
}

fn metadata(ev: &Event) -> Metadata {
    let mut meta = Metadata {
        overrides: perms::parse_overrides(ev.tags.iter().map(|t| t.as_slice())),
        ..Default::default()
    };
    for tag in ev.tags.iter() {
        match tag.as_slice() {
            [k, v, ..] if k == "name" => meta.name = v.clone(),
            [k, v, ..] if k == "picture" => meta.picture = Some(v.clone()),
            [k, v, ..] if k == "about" => meta.about = Some(v.clone()),
            [k, v, ..] if k == "banner" => meta.banner = Some(v.clone()),
            [k, v, rest @ ..] if k == "child" => {
                meta.children.push(v.clone());
                if let Some(category) = rest.first().filter(|c| !c.is_empty()) {
                    meta.placement.insert(v.clone(), category.clone());
                }
            }
            [k, id, name, ..] if k == "category" => meta.categories.push(Category {
                id: id.clone(),
                name: name.clone(),
            }),
            [k, v, ..] if k == "slowmode" => meta.slowmode = v.parse().unwrap_or(0),
            [k, v, ..] if k == "user_limit" => meta.user_limit = v.parse().unwrap_or(0),
            [k, ..] if k == "livekit" => meta.voice = true,
            [k, v, ..] if k == "owner" => meta.owner = Some(v.clone()),
            _ => {}
        }
    }
    meta
}

/// Roles per key, from a 39001.
fn admins(ev: &Event) -> HashMap<PublicKey, Vec<String>> {
    ev.tags
        .iter()
        .filter_map(|tag| match tag.as_slice() {
            [k, pk, roles @ ..] if k == "p" => {
                Some((PublicKey::from_hex(pk).ok()?, roles.to_vec()))
            }
            _ => None,
        })
        .collect()
}

/// Keys in a voice channel, from a 39004 (each once).
fn participants(ev: &Event) -> Vec<PublicKey> {
    let mut all: Vec<PublicKey> = tag_values(ev, "participant")
        .filter_map(|pk| PublicKey::from_hex(pk).ok())
        .collect();
    all.sort();
    all.dedup();
    all
}

/// `(id, relay)` of each `group` tag of a 10009.
fn groups(ev: &Event) -> Vec<(String, String)> {
    ev.tags
        .iter()
        .filter_map(|tag| match tag.as_slice() {
            [k, id, relay, ..] if k == "group" && is_id(id) => Some((id.clone(), relay.clone())),
            _ => None,
        })
        .collect()
}

fn message(ev: &Event) -> Option<ChannelMessage> {
    Some(ChannelMessage {
        id: ev.id,
        channel: tag_values(ev, "h").next()?.to_string(),
        author: ev.pubkey,
        text: ev.content.clone(),
        created_at: ev.created_at,
        attachment: ev.tags.iter().find_map(Attachment::from_imeta),
    })
}

/// Messages not targeted by a deletion, oldest first (same second: id order).
fn visible<'a>(
    messages: impl Iterator<Item = &'a Event>,
    deletions: impl Iterator<Item = &'a Event>,
) -> Vec<ChannelMessage> {
    let deleted: HashSet<String> = deletions
        .flat_map(|d| tag_values(d, "e").map(str::to_string).collect::<Vec<_>>())
        .collect();
    let mut all: Vec<ChannelMessage> = messages
        .filter(|m| !deleted.contains(&m.id.to_hex()))
        .filter_map(message)
        .collect();
    all.sort_by_key(|m| (m.created_at, m.id));
    all
}

fn tag_values<'a>(ev: &'a Event, name: &'a str) -> impl Iterator<Item = &'a str> {
    ev.tags.iter().filter_map(move |tag| match tag.as_slice() {
        [k, v, ..] if k == name => Some(v.as_str()),
        _ => None,
    })
}

fn subscription(community: &str) -> SubscriptionId {
    SubscriptionId::new(format!("community:{community}"))
}

fn random_id() -> Result<String> {
    link::random(ID_ALPHABET, ID_LEN)
}

fn is_id(s: &str) -> bool {
    s.len() == ID_LEN
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

fn host_of(relay: &str) -> &str {
    let rest = relay.split_once("://").map_or(relay, |(_, rest)| rest);
    rest.trim_end_matches('/')
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::{EventBuilder, Keys};

    fn ev(keys: &Keys, kind: Kind, tags: &[&[&str]], at: u64) -> Event {
        EventBuilder::new(kind, "x")
            .tags(tags.iter().map(|t| Tag::parse(t.iter().copied()).unwrap()))
            .custom_created_at(Timestamp::from_secs(at))
            .finalize(keys)
            .unwrap()
    }

    #[test]
    fn invite_round_trip() {
        let invite = Invite::parse(" chaton.example'k3j9x2m8q0aa/ab12cd34 ").unwrap();
        assert_eq!(invite.relay(), "wss://chaton.example");
        assert_eq!(invite.to_string(), "chaton.example'k3j9x2m8q0aa/AB12CD34");
        let local = Invite::parse("localhost:8099'k3j9x2m8q0aa/AB12CD34").unwrap();
        assert_eq!(local.relay(), "ws://localhost:8099");
        assert_eq!(host_of("ws://localhost:8099/"), "localhost:8099");
        for bad in [
            "",
            "k3j9x2m8q0aa/AB12CD34",
            "chaton.example'k3j9x2/AB12CD34",
            "chaton.example'k3j9x2m8q0aa/AB12CD3",
            "chaton.example'k3j9x2m8q0aa/AB12CD3U",
            "evil.example/x'k3j9x2m8q0aa/AB12CD34",
            "chaton.example'K3J9X2M8Q0AA/AB12CD34",
        ] {
            assert!(Invite::parse(bad).is_err(), "{bad:?} accepted");
        }
        let id = random_id().unwrap();
        assert!(is_id(&id), "{id}");
    }

    #[test]
    fn parses_group_state() {
        let relay = Keys::generate();
        let (alice, bob) = (Keys::generate().public_key(), Keys::generate().public_key());
        let meta = ev(
            &relay,
            Kind::GroupMetadata,
            &[
                &["d", "k3j9x2m8q0aa"],
                &["name", "Les chats"],
                &["private"],
                &["child", "g7w2p5n1c4bb"],
                &["child", "a1b2c3d4e5f6"],
            ],
            1,
        );
        assert_eq!(
            metadata(&meta),
            Metadata {
                name: "Les chats".into(),
                picture: None,
                children: vec!["g7w2p5n1c4bb".into(), "a1b2c3d4e5f6".into()],
                voice: false,
                owner: None,
                ..Default::default()
            }
        );
        let voice = ev(
            &relay,
            Kind::GroupMetadata,
            &[&["d", "v8t3r6m2d5cc"], &["name", "Vocal"], &["livekit"]],
            1,
        );
        assert!(metadata(&voice).voice);
        let laid_out = ev(
            &relay,
            Kind::GroupMetadata,
            &[
                &["d", "k3j9x2m8q0aa"],
                &["child", "g7w2p5n1c4bb", "texte"],
                &["child", "a1b2c3d4e5f6"],
                &["category", "texte", "Salons textuels"],
                &["slowmode", "10"],
                &["user_limit", "5"],
            ],
            1,
        );
        let m = metadata(&laid_out);
        assert_eq!(m.children, ["g7w2p5n1c4bb", "a1b2c3d4e5f6"]);
        assert_eq!(
            m.placement.get("g7w2p5n1c4bb").map(String::as_str),
            Some("texte")
        );
        assert!(!m.placement.contains_key("a1b2c3d4e5f6"));
        assert_eq!(m.categories[0].name, "Salons textuels");
        assert_eq!((m.slowmode, m.user_limit), (10, 5));
        let present = ev(
            &relay,
            Kind::GroupLivekitParticipants,
            &[
                &["d", "v8t3r6m2d5cc"],
                &["participant", &bob.to_hex()],
                &["participant", &alice.to_hex()],
                &["participant", &bob.to_hex()],
                &["participant", "nope"],
            ],
            1,
        );
        let mut both = vec![alice, bob];
        both.sort();
        assert_eq!(participants(&present), both);
        let roles = ev(
            &relay,
            Kind::GroupAdmins,
            &[
                &["d", "k3j9x2m8q0aa"],
                &["p", &alice.to_hex(), "admin"],
                &["p", &bob.to_hex(), "moderator"],
            ],
            1,
        );
        let roles = admins(&roles);
        assert_eq!(roles[&alice], ["admin"]);
        assert_eq!(roles[&bob], ["moderator"]);
        let list = ev(
            &relay,
            Kind::SimpleGroups,
            &[
                &["group", "k3j9x2m8q0aa", "wss://a.example"],
                &["group", "../etc"],
                &["group", "BAD", "wss://x"],
            ],
            1,
        );
        assert_eq!(
            groups(&list),
            [("k3j9x2m8q0aa".into(), "wss://a.example".into())]
        );
    }

    #[test]
    fn deleted_messages_are_hidden() {
        let (alice, bob) = (Keys::generate(), Keys::generate());
        let h: &[&str] = &["h", "g7w2p5n1c4bb"];
        let first = ev(&alice, Kind::ChatMessage, &[h], 20);
        let second = ev(&bob, Kind::ChatMessage, &[h], 10);
        let gone = ev(&bob, Kind::ChatMessage, &[h], 15);
        let deletion = ev(
            &alice,
            Kind::GroupDeleteEvent,
            &[h, &["e", &gone.id.to_hex()]],
            30,
        );
        let shown = visible(
            [&first, &second, &gone].into_iter(),
            [&deletion].into_iter(),
        );
        let ids: Vec<EventId> = shown.iter().map(|m| m.id).collect();
        assert_eq!(ids, [second.id, first.id]);
        assert_eq!(shown[0].channel, "g7w2p5n1c4bb");
    }
}
