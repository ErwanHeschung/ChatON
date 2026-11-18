//! Who is online (spec protocol § 12): my status, synced between my devices
//! (`30078`), and the signals (`24136`) each device sends and reads.

use std::collections::HashMap;
use std::sync::MutexGuard;

use nostr::event::FinalizeEvent;
use nostr_sdk::prelude::{
    Event, Filter, Keys, Kind, PublicKey, SecretKey, SingleLetterTag, SubscriptionId, Tag,
    Timestamp,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::community::tag_values;
use crate::session::{Session, instance_url};
use crate::{Error, Result, event};

pub const PRESENCE: Kind = Kind::Custom(24136);
const SETTING: &str = "chaton/presence";
/// A signal older than this is gone (the relay forgets it too).
pub const TTL_SECS: u64 = 150;
/// Between two signals of a device; `listen` reads every `TICK_SECS`.
const BEAT_SECS: u64 = 60;
pub const TICK_SECS: u64 = 30;
/// A device leaving (invisible, a new key): replaces its last signal.
const OFFLINE: &str = "offline";

/// Ordered from least to most present: a person shows their most present device.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Idle,
    #[default]
    Online,
    Dnd,
    /// Publishes nothing: others see me offline.
    Invisible,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Idle => "idle",
            Status::Online => "online",
            Status::Dnd => "dnd",
            Status::Invisible => "invisible",
        }
    }

    /// A published status (never `invisible`).
    fn published(s: &str) -> Option<Self> {
        [Status::Idle, Status::Online, Status::Dnd]
            .into_iter()
            .find(|st| st.as_str() == s)
    }
}

/// What all my devices share, in my `30078`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct Setting {
    status: Status,
    epoch: u64,
}

#[derive(Default)]
pub(crate) struct State {
    setting: Setting,
    /// No activity for a while, or in the background: `online` shows `idle`.
    away: bool,
    /// This device, to tell my devices apart in my signals.
    device: String,
    last_beat: Option<Timestamp>,
    /// Per person, per device: their last status and when.
    seen: HashMap<PublicKey, HashMap<String, (Status, Timestamp)>>,
    /// Presence keys I follow: whose, and on which relay.
    contacts: HashMap<PublicKey, (PublicKey, String)>,
    /// Read from my DMs once; then again for each new key.
    contacts_known: bool,
    communities: Vec<String>,
    /// What each relay's subscription asks for now.
    targets: HashMap<String, Vec<Filter>>,
}

impl State {
    pub(crate) fn new() -> Result<Self> {
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes).map_err(|e| Error::Invalid(e.to_string()))?;
        Ok(Self {
            device: hex::encode(bytes),
            ..Default::default()
        })
    }

    /// What my devices publish now; `None`: nothing (invisible).
    fn shown(&self) -> Option<Status> {
        match self.setting.status {
            Status::Invisible => None,
            Status::Online if self.away => Some(Status::Idle),
            s => Some(s),
        }
    }

    fn record(&mut self, who: PublicKey, ev: &Event) {
        let tag = |name| tag_values(ev, name).next();
        let (Some(status), Some(device)) = (tag("status"), tag("device")) else {
            return;
        };
        let devices = self.seen.entry(who).or_default();
        if devices
            .get(device)
            .is_some_and(|(_, at)| *at > ev.created_at)
        {
            return; // older than what I have
        }
        match Status::published(status) {
            Some(status) => drop(devices.insert(device.to_string(), (status, ev.created_at))),
            None if status == OFFLINE => drop(devices.remove(device)),
            None => {}
        }
    }

    /// Everyone with a live signal, at their most present device's status.
    fn online(&mut self, now: Timestamp) -> HashMap<PublicKey, Status> {
        let fresh = |at: &Timestamp| now.as_secs().saturating_sub(at.as_secs()) <= TTL_SECS;
        self.seen.retain(|_, devices| {
            devices.retain(|_, (_, at)| fresh(at));
            !devices.is_empty()
        });
        self.seen
            .iter()
            .filter_map(|(pk, devices)| Some((*pk, devices.values().map(|(s, _)| *s).max()?)))
            .collect()
    }
}

/// My presence key for `epoch`: the same on all my devices, unlinkable to me.
pub(crate) fn presence_keys(me: &Keys, epoch: u64) -> Result<Keys> {
    let secret: [u8; 32] = Sha256::new()
        .chain_update(b"chaton-presence-v0")
        .chain_update(me.secret_key().as_secret_bytes())
        .chain_update(epoch.to_be_bytes())
        .finalize()
        .into();
    Ok(Keys::new(SecretKey::from_slice(&secret)?))
}

impl Session {
    fn presence_state(&self) -> MutexGuard<'_, State> {
        self.presence.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The status I chose (`invisible` included), as my devices share it.
    pub fn my_status(&self) -> Status {
        self.presence_state().setting.status
    }

    /// Chooses my status for all my devices, and tells others right away.
    pub async fn set_status(&self, status: Status) -> Result<()> {
        let setting = Setting {
            status,
            ..self.presence_state().setting
        };
        self.save_setting(setting).await?;
        self.beat(true).await
    }

    /// This device is idle (no activity, or in the background) or back.
    pub async fn set_away(&self, away: bool) -> Result<()> {
        let changed = {
            let mut state = self.presence_state();
            let before = state.shown();
            state.away = away;
            before != state.shown()
        };
        if changed {
            self.beat(true).await
        } else {
            Ok(())
        }
    }

    /// A new presence key (I blocked someone): those I write to learn it.
    pub(crate) async fn change_presence_key(&self) -> Result<()> {
        let mut setting = self.presence_state().setting;
        setting.epoch += 1;
        self.save_setting(setting).await?;
        // The old key goes offline now rather than in TTL_SECS.
        drop(self.signal(OFFLINE, setting.epoch - 1).await);
        self.beat(true).await
    }

    /// `nip05` and `presence` tags for the DM rumors I send (spec § 5, § 12).
    pub(crate) fn author_tags(&self) -> Result<Vec<Tag>> {
        let epoch = self.presence_state().setting.epoch;
        let key = presence_keys(&self.keys, epoch)?.public_key().to_hex();
        let account = self.account();
        Ok(vec![
            Tag::parse(["nip05", account.handle.as_str()])?,
            Tag::parse(["presence", key.as_str(), account.relay.as_str()])?,
        ])
    }

    /// Reads my setting from the local store (after a sync, or another device's change).
    pub(crate) async fn load_presence(&self) -> Result<()> {
        let mine = Filter::new()
            .author(self.account().pubkey)
            .kind(Kind::ApplicationSpecificData)
            .identifier(SETTING)
            .limit(1);
        if let Some(ev) = self.client.database().query(mine).await?.first() {
            let plain = self.decrypt_for_me(&ev.content)?;
            self.presence_state().setting = serde_json::from_str(&plain)?;
        }
        Ok(())
    }

    async fn save_setting(&self, setting: Setting) -> Result<()> {
        let plain = serde_json::to_string(&setting)?;
        self.publish_for_me(
            Kind::ApplicationSpecificData,
            vec![Tag::identifier(SETTING)],
            &plain,
        )
        .await?;
        self.presence_state().setting = setting;
        Ok(())
    }

    /// Sends my signals (spec § 12) if due, or now when `force`. Invisible
    /// sends `offline` once (when forced), then nothing.
    pub(crate) async fn beat(&self, force: bool) -> Result<()> {
        let now = Timestamp::now();
        let (shown, epoch) = {
            let mut state = self.presence_state();
            let due = state
                .last_beat
                .is_none_or(|t| now.as_secs() >= t.as_secs() + BEAT_SECS - 5);
            if !force && !due {
                return Ok(());
            }
            state.last_beat = Some(now);
            (state.shown(), state.setting.epoch)
        };
        match shown {
            Some(status) => self.signal(status.as_str(), epoch).await,
            None if force => self.signal(OFFLINE, epoch).await,
            None => Ok(()),
        }
    }

    /// One signal with my presence key for `epoch` on my relay, one as me in
    /// each of my communities.
    async fn signal(&self, status: &str, epoch: u64) -> Result<()> {
        let device = self.presence_state().device.clone();
        let tags = |h: Option<&str>| -> Result<Vec<Tag>> {
            let mut tags = vec![
                Tag::parse(["status", status])?,
                Tag::parse(["device", device.as_str()])?,
            ];
            if let Some(h) = h {
                tags.push(Tag::parse(["h", h])?);
            }
            Ok(tags)
        };
        let dm = event::builder(PRESENCE, tags(None)?, "")?
            .finalize(&presence_keys(&self.keys, epoch)?)?;
        self.client
            .send_event(&dm)
            .to([self.account().relay.as_str()])
            .await?;
        for (id, relay) in self.group_list().await? {
            let ev = event::builder(PRESENCE, tags(Some(&id))?, "")?.finalize(&self.keys)?;
            // ponytail: an unreachable community just shows me offline there.
            drop(self.client.send_event(&ev).to([relay.as_str()]).await);
        }
        Ok(())
    }

    /// Sends my signals when due, keeps following my communities, and drops
    /// what expired. `listen` calls it every `TICK_SECS` and passes the result
    /// on as `Update::Presence`; signals themselves come in live, as events.
    pub async fn presence_tick(&self) -> Result<HashMap<PublicKey, Status>> {
        drop(self.beat(false).await);
        let contacts = !self.presence_state().contacts_known;
        self.follow_presence(contacts).await?;
        Ok(self.online_now())
    }

    /// A key I don't follow yet (a new contact, or theirs changed).
    pub(crate) fn new_presence_key(&self, key: &PublicKey) -> bool {
        !self.presence_state().contacts.contains_key(key)
    }

    /// Subscribes, one subscription per relay, to the signals of those who
    /// wrote to me (`contacts`: read them again from my DMs) and of my
    /// communities. Only what changed is sent again.
    pub(crate) async fn follow_presence(&self, contacts: bool) -> Result<()> {
        if contacts {
            // Each contact's latest presence key, read on their instance's relay
            // (that of their verified handle: no one picks a server for me).
            let me = self.account().pubkey;
            let mut latest: HashMap<PublicKey, PublicKey> = HashMap::new();
            for m in self.messages_all().await? {
                if m.sender != me
                    && let Some((key, _)) = m.presence
                {
                    latest.insert(m.sender, key);
                }
            }
            let mut found = HashMap::new();
            for (person, key) in latest {
                let handle = self.peer(person).await.handle;
                if let Some(url) = handle.as_deref().and_then(instance_url) {
                    found.insert(key, (person, url.replacen("http", "ws", 1)));
                }
            }
            let mut state = self.presence_state();
            state.contacts = found;
            state.contacts_known = true;
        }
        let communities = self.group_list().await?;
        let (targets, before) = {
            let mut state = self.presence_state();
            state.communities = communities.iter().map(|(id, _)| id.clone()).collect();
            let mut targets: HashMap<String, Vec<Filter>> = HashMap::new();
            let mut authors: HashMap<&String, Vec<PublicKey>> = HashMap::new();
            for (key, (_, relay)) in &state.contacts {
                authors.entry(relay).or_default().push(*key);
            }
            for (relay, mut keys) in authors {
                keys.sort();
                targets
                    .entry(relay.clone())
                    .or_default()
                    .push(Filter::new().kind(PRESENCE).authors(keys));
            }
            for (id, relay) in &communities {
                targets.entry(relay.clone()).or_default().push(
                    Filter::new()
                        .kind(PRESENCE)
                        .custom_tag(SingleLetterTag::LOWERCASE_H, id),
                );
            }
            let before = std::mem::replace(&mut state.targets, targets.clone());
            (targets, before)
        };
        for (relay, filters) in &targets {
            if before.get(relay) != Some(filters) {
                drop(self.ensure_relay(relay).await);
                // ponytail: a relay that fails stays unfollowed until the targets change.
                drop(
                    self.resubscribe(subscription(relay), relay, filters.clone())
                        .await,
                );
            }
        }
        for relay in before.keys().filter(|r| !targets.contains_key(*r)) {
            drop(self.client.unsubscribe(&subscription(relay)).await);
        }
        Ok(())
    }

    /// A signal from a subscription: everyone online, if it changed anything.
    pub(crate) fn presence_event(&self, ev: &Event) -> Option<HashMap<PublicKey, Status>> {
        let now = Timestamp::now();
        let me = self.account().pubkey;
        let mut state = self.presence_state();
        let who = match tag_values(ev, "h").next() {
            // As themselves, checked by the relay: only members publish.
            Some(h) => state
                .communities
                .iter()
                .any(|c| c == h)
                .then_some(ev.pubkey)?,
            None => state.contacts.get(&ev.pubkey)?.0,
        };
        if who == me {
            return None;
        }
        let before = state.online(now);
        state.record(who, ev);
        let after = state.online(now);
        (before != after).then_some(after)
    }

    fn online_now(&self) -> HashMap<PublicKey, Status> {
        let mut online = self.presence_state().online(Timestamp::now());
        online.remove(&self.account().pubkey);
        online
    }
}

fn subscription(relay: &str) -> SubscriptionId {
    SubscriptionId::new(format!("presence:{relay}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_key_per_epoch_and_most_present_device() {
        let me = Keys::generate();
        let (a, b) = (
            presence_keys(&me, 0).unwrap(),
            presence_keys(&me, 1).unwrap(),
        );
        assert_eq!(a.public_key(), presence_keys(&me, 0).unwrap().public_key());
        assert_ne!(a.public_key(), b.public_key());
        assert_ne!(a.public_key(), me.public_key());

        let mut state = State::new().unwrap();
        let bob = Keys::generate().public_key();
        let now = Timestamp::now();
        let signal = |status: &str, device: &str, at: Timestamp| {
            event::builder(
                PRESENCE,
                [
                    Tag::parse(["status", status]).unwrap(),
                    Tag::parse(["device", device]).unwrap(),
                ],
                "",
            )
            .unwrap()
            .custom_created_at(at)
            .finalize(&a)
            .unwrap()
        };
        state.record(bob, &signal("idle", "phone", now));
        state.record(bob, &signal("dnd", "laptop", now));
        assert_eq!(state.online(now).get(&bob), Some(&Status::Dnd));
        // An older signal doesn't replace a newer one; a stale device drops out.
        state.record(bob, &signal("online", "laptop", now - 200));
        assert_eq!(state.online(now).get(&bob), Some(&Status::Dnd));
        // Unknown statuses are ignored; `offline` drops the device.
        state.record(bob, &signal("invisible", "phone", now));
        state.record(bob, &signal("offline", "laptop", now));
        assert_eq!(state.online(now).get(&bob), Some(&Status::Idle));
        assert!(state.online(now + 151).is_empty());

        state.setting.status = Status::Online;
        state.away = true;
        assert_eq!(state.shown(), Some(Status::Idle));
        state.setting.status = Status::Invisible;
        assert_eq!(state.shown(), None);
    }
}
