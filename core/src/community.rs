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
use crate::session::{Session, TIMEOUT, backfill, http, is_local_host};
use crate::{Error, Result, event, link};

pub const ROLE_ADMIN: &str = "admin";
pub const ROLE_MODERATOR: &str = "moderator";

const STATE_KINDS: [Kind; 4] = [
    Kind::GroupMetadata,
    Kind::GroupAdmins,
    Kind::GroupMembers,
    Kind::GroupRoles,
];
const ID_LEN: usize = 12;
const INVITE_LEN: usize = 8;
/// Lowercase Crockford base32: a subset of the `[a-z0-9]` the spec allows for ids.
const ID_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Community {
    pub id: String,
    pub relay: String,
    pub name: String,
    pub picture: Option<String>,
    pub channels: Vec<Channel>,
    /// My roles here (`admin`, `moderator`), empty for a plain member.
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Member {
    pub pubkey: PublicKey,
    pub roles: Vec<String>,
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
    },
    ChannelMessage {
        message: ChannelMessage,
    },
    Deleted {
        channel: String,
        ids: Vec<EventId>,
    },
    /// My communities, their channels or members changed: read them again.
    Communities,
}

impl Session {
    /// Creates a community on my instance, with a first `general` channel.
    pub async fn create_community(&self, name: &str) -> Result<Community> {
        let relay = self.account().relay.clone();
        let id = random_id()?;
        self.group_event(&relay, Kind::GroupCreateGroup, &id, [], "")
            .await?;
        self.edit_metadata(&relay, &id, name, None, None, &[])
            .await?;
        self.add_to_list(&id, &relay).await?;
        self.create_channel(&id, "general").await?;
        self.community(&id)
            .await?
            .ok_or_else(|| Error::Server("community not published".into()))
    }

    /// Adds a channel at the end of the community (admins only).
    pub async fn create_channel(&self, community: &str, name: &str) -> Result<Channel> {
        let relay = self.relay_of(community).await?;
        // Fresh state: 9002 replaces every tag, so it must list every child.
        self.fetch_state(&relay, &[community.to_string()]).await?;
        let key = self.relay_key(&relay).await?;
        let meta = self
            .metadata(key, community)
            .await?
            .ok_or_else(|| Error::Server("community state not found".into()))?;

        let id = random_id()?;
        self.group_event(&relay, Kind::GroupCreateGroup, &id, [], "")
            .await?;
        self.edit_metadata(&relay, &id, name, None, Some(community), &[])
            .await?;
        let mut children = meta.children;
        children.push(id.clone());
        self.edit_metadata(
            &relay,
            community,
            &meta.name,
            meta.picture.as_deref(),
            None,
            &children,
        )
        .await?;
        self.sync_community(community, &relay).await?;
        Ok(Channel {
            id,
            name: name.into(),
        })
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
                [Tag::parse(["code", &invite.code])?],
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
        let mut all: Vec<Member> = tag_values(&members, "p")
            .filter_map(|pk| PublicKey::from_hex(pk).ok())
            .map(|pubkey| Member {
                roles: roles.get(&pubkey).cloned().unwrap_or_default(),
                pubkey,
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
    pub async fn remove_member(&self, community: &str, pubkey: PublicKey) -> Result<()> {
        let relay = self.relay_of(community).await?;
        let tag = Tag::parse(["p", &pubkey.to_hex()])?;
        self.group_event(&relay, Kind::GroupRemoveUser, community, [tag], "")
            .await
            .map(drop)
    }

    /// Messages of `channel`, oldest first, without the deleted ones.
    pub async fn channel_messages(&self, channel: &str) -> Result<Vec<ChannelMessage>> {
        let in_channel = Filter::new().custom_tag(SingleLetterTag::LOWERCASE_H, channel);
        let db = self.client.database();
        let messages = db.query(in_channel.clone().kind(Kind::ChatMessage)).await?;
        let deletions = db.query(in_channel.kind(Kind::GroupDeleteEvent)).await?;
        Ok(visible(messages.iter(), deletions.iter()))
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

    /// Admins: a public picture, uploaded to my instance (spec § 10).
    pub async fn set_community_picture(
        &self,
        community: &str,
        bytes: Vec<u8>,
        mime: &str,
    ) -> Result<()> {
        let relay = self.relay_of(community).await?;
        let url = media::upload(&self.keys, &self.account().relay, bytes, mime).await?;
        self.fetch_state(&relay, &[community.to_string()]).await?;
        let key = self.relay_key(&relay).await?;
        let meta = self
            .metadata(key, community)
            .await?
            .ok_or_else(|| Error::Server("community state not found".into()))?;
        self.edit_metadata(
            &relay,
            community,
            &meta.name,
            Some(&url),
            None,
            &meta.children,
        )
        .await?;
        self.sync_community(community, &relay).await
    }

    /// My own messages; any message for admins and moderators.
    pub async fn delete_message(&self, community: &str, channel: &str, id: EventId) -> Result<()> {
        let relay = self.relay_of(community).await?;
        self.group_event(
            &relay,
            Kind::GroupDeleteEvent,
            channel,
            [Tag::event(id)],
            "",
        )
        .await
        .map(drop)
    }

    /// My list (10009) from my relay, then every community in it. One
    /// unreachable instance doesn't stop the others.
    pub(crate) async fn sync_communities(&self) -> Result<()> {
        let relay = self.account().relay.clone();
        let mine = Filter::new()
            .author(self.account().pubkey)
            .kind(Kind::SimpleGroups);
        self.client
            .fetch_events(ReqTarget::single(&relay, [mine.clone().limit(1)]))
            .timeout(TIMEOUT)
            .await?;
        self.resubscribe(
            SubscriptionId::new("communities"),
            &relay,
            vec![mine.since(Timestamp::now())],
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
        self.ensure_relay(relay).await?;
        let key = self.relay_key(relay).await?;
        self.fetch_state(relay, &[id.to_string()]).await?;
        let channels = match self.metadata(key, id).await? {
            Some(meta) => meta.children,
            None => Vec::new(),
        };
        let mut groups = channels.clone();
        groups.push(id.to_string());
        self.fetch_state(relay, &channels).await?;

        let state = Filter::new()
            .author(key)
            .kinds(STATE_KINDS)
            .identifiers(groups);
        let mut filters = vec![state];
        if !channels.is_empty() {
            let chat = Filter::new()
                .kinds([Kind::ChatMessage, Kind::GroupDeleteEvent])
                .custom_tags(SingleLetterTag::LOWERCASE_H, channels);
            // Since the newest one stored: we were following every channel until then.
            let mut history = chat.clone();
            let stored = self.client.database().query(chat.clone().limit(1)).await?;
            if let Some(last) = stored.iter().map(|e| e.created_at).max() {
                history = history.since(last);
            }
            backfill(history, |f| async move {
                let events = self
                    .client
                    .fetch_events(ReqTarget::single(relay, [f]))
                    .timeout(TIMEOUT)
                    .await?;
                Ok(events.into_iter().collect())
            })
            .await?;
            filters.push(chat.since(Timestamp::now()));
        }
        self.resubscribe(subscription(id), relay, filters).await
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
        if ev.kind == Kind::SimpleGroups && ev.pubkey == self.account().pubkey {
            // Another device joined or left something.
            let _ = self.sync_communities().await;
            return Some(Update::Communities);
        }
        if !STATE_KINDS.contains(&ev.kind) || !self.is_relay_key(ev.pubkey) {
            return None;
        }
        if ev.kind == Kind::GroupMetadata {
            let id = ev.tags.identifier()?;
            // A community's channels changed: follow the new ones.
            if let Some((_, relay)) = self
                .group_list()
                .await
                .ok()?
                .into_iter()
                .find(|(c, _)| *c == id)
            {
                let _ = self.sync_community(&id, &relay).await;
            }
        }
        Some(Update::Communities)
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
        let mut channels = Vec::new();
        for child in meta.children {
            if let Some(m) = self.metadata(key, &child).await? {
                channels.push(Channel {
                    id: child,
                    name: m.name,
                });
            }
        }
        // Shown only if hosted by the community's instance (spec § 10).
        let picture = meta
            .picture
            .filter(|url| media::allowed(url, &[host_of(&relay)]).is_some());
        Ok(Some(Community {
            id: id.into(),
            relay,
            name: meta.name,
            picture,
            channels,
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

    /// 9002 replaces every tag (NIP-29): always send them all.
    async fn edit_metadata(
        &self,
        relay: &str,
        group: &str,
        name: &str,
        picture: Option<&str>,
        parent: Option<&str>,
        children: &[String],
    ) -> Result<()> {
        let mut tags = vec![Tag::parse(["name", name])?];
        if let Some(picture) = picture {
            tags.push(Tag::parse(["picture", picture])?);
        }
        if let Some(parent) = parent {
            tags.push(Tag::parse(["parent", parent])?);
        }
        for child in children {
            tags.push(Tag::parse(["child", child])?);
        }
        self.group_event(relay, Kind::GroupEditMetadata, group, tags, "")
            .await
            .map(drop)
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
    async fn fetch_state(&self, relay: &str, groups: &[String]) -> Result<()> {
        if groups.is_empty() {
            return Ok(());
        }
        let key = self.relay_key(relay).await?;
        let filter = Filter::new()
            .author(key)
            .kinds(STATE_KINDS)
            .identifiers(groups.iter().cloned());
        self.client
            .fetch_events(ReqTarget::single(relay, [filter]))
            .timeout(TIMEOUT)
            .await?;
        Ok(())
    }

    async fn state(&self, key: PublicKey, kind: Kind, group: &str) -> Result<Option<Event>> {
        let filter = Filter::new()
            .author(key)
            .kind(kind)
            .identifier(group)
            .limit(1);
        Ok(self.client.database().query(filter).await?.first().cloned())
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

    async fn relay_of(&self, community: &str) -> Result<String> {
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
    children: Vec<String>,
}

fn metadata(ev: &Event) -> Metadata {
    let mut meta = Metadata::default();
    for tag in ev.tags.iter() {
        match tag.as_slice() {
            [k, v, ..] if k == "name" => meta.name = v.clone(),
            [k, v, ..] if k == "picture" => meta.picture = Some(v.clone()),
            [k, v, ..] if k == "child" => meta.children.push(v.clone()),
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
            }
        );
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
