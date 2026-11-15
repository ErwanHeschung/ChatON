//! A signed-in account: relay connection, local event store, directory and DMs.
//!
//! Step 4 scope: one relay per user, the one of their instance (ROADMAP).
//! Only encrypted gift wraps are stored on disk; messages are decrypted on read.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use nostr::nips::nip05::{Nip05Address, Nip05Profile};
use nostr::nips::nip98::{HttpData, HttpMethod, Sha256Hash};
use nostr_sdk::prelude::{
    Client, ClientNotification, Event, Filter, Keys, Kind, Metadata, PublicKey, RelayUrl,
    SignerAuthenticator, StreamExt, Tag, Timestamp, ToBech32, Url,
};
use nostr_sqlite::prelude::NostrSqlite;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::community::Update;
use crate::dm::{self, Message};
use crate::identity::Identity;
use crate::{Error, Result, event};

const ACCOUNT_FILE: &str = "account.json";
const EVENTS_DB: &str = "events.db";
pub(crate) const TIMEOUT: Duration = Duration::from_secs(10);
/// NIP-59 backdates gift wraps up to 2 days: resync from that far before the last one.
const GIFT_WRAP_JITTER_SECS: u64 = 2 * 24 * 3600;

/// The local account. Not secret: the key itself is in the keychain under `handle`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// `pseudo@instance`
    pub handle: String,
    pub relay: String,
    pub pubkey: PublicKey,
    pub npub: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Peer {
    /// Verified `pseudo@instance` (NIP-05 maps it back to `pubkey`), if any.
    pub handle: Option<String>,
    pub pubkey: PublicKey,
    pub npub: String,
    #[serde(skip)]
    pub relays: Vec<RelayUrl>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conversation {
    pub peer: PublicKey,
    pub last: Message,
}

pub struct Session {
    dir: PathBuf,
    pub(crate) keys: Keys,
    account: Account,
    pub(crate) client: Client,
    peers: Mutex<HashMap<PublicKey, Peer>>,
    /// NIP-11 `self` key per community relay (see `community`).
    pub(crate) relay_keys: Mutex<HashMap<String, PublicKey>>,
    /// Serializes `community::resubscribe` (close, then open the same id).
    pub(crate) resubscribing: tokio::sync::Mutex<()>,
}

impl Session {
    /// Creates a key, claims `name` on `instance` (NIP-98), stores the key in the
    /// keychain and publishes the profile and relay lists.
    pub async fn register(dir: &Path, instance: &str, name: &str) -> Result<Self> {
        let instance = Url::parse(instance).map_err(|e| Error::Invalid(e.to_string()))?;
        let identity = Identity::generate();

        let claim_url = instance
            .join("/api/handle")
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let body = serde_json::json!({ "name": name }).to_string();
        let auth = HttpData::new(claim_url.clone(), HttpMethod::POST)
            .payload(Sha256Hash::from_byte_array(Sha256::digest(&body).into()))
            .to_authorization(identity.keys())
            .await?;
        let resp = http()
            .post(claim_url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;
        match resp.status() {
            StatusCode::CREATED => {}
            StatusCode::CONFLICT => return Err(Error::HandleTaken),
            StatusCode::BAD_REQUEST => return Err(Error::Invalid(resp.text().await?)),
            status => return Err(Error::Server(format!("{status}: {}", resp.text().await?))),
        }
        #[derive(Deserialize)]
        struct Claimed {
            name: String,
        }
        let claimed: Claimed = resp.json().await?;

        let host = host(&instance)?;
        let scheme = if instance.scheme() == "https" {
            "wss"
        } else {
            "ws"
        };
        let account = Account {
            handle: format!("{}@{host}", claimed.name),
            relay: format!("{scheme}://{host}"),
            pubkey: identity.public_key(),
            npub: identity.npub(),
        };
        let session = Self::create(dir, identity, account).await?;
        session.publish_profile(&claimed.name).await?;
        Ok(session)
    }

    /// Imports an existing key for `handle`. The directory must map the handle to it.
    pub async fn import_key(dir: &Path, handle: &str, nsec: &str) -> Result<Self> {
        let identity = Identity::from_nsec(nsec)?;
        let peer = resolve(handle).await?;
        if peer.pubkey != identity.public_key() {
            return Err(Error::KeyMismatch);
        }
        let relay = peer.relays.first().ok_or(Error::NotFound)?;
        let account = Account {
            handle: peer.handle.ok_or(Error::NotFound)?,
            relay: relay.to_string(),
            pubkey: identity.public_key(),
            npub: identity.npub(),
        };
        Self::create(dir, identity, account).await
    }

    /// Reopens the account saved in `dir`, if any.
    pub async fn open(dir: &Path) -> Result<Option<Self>> {
        let raw = match std::fs::read(dir.join(ACCOUNT_FILE)) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let account: Account = serde_json::from_slice(&raw)?;
        let identity = Identity::load(&account.handle)?;
        Ok(Some(
            Self::connect(dir, identity.keys().clone(), account).await?,
        ))
    }

    async fn create(dir: &Path, identity: Identity, account: Account) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        identity.save(&account.handle)?;
        std::fs::write(dir.join(ACCOUNT_FILE), serde_json::to_vec_pretty(&account)?)?;
        Self::connect(dir, identity.keys().clone(), account).await
    }

    async fn connect(dir: &Path, keys: Keys, account: Account) -> Result<Self> {
        let db = NostrSqlite::builder()
            .in_file(dir.join(EVENTS_DB))
            .build()
            .await?;
        let client = Client::builder()
            .authenticator(SignerAuthenticator::new(keys.clone()))
            .database(db)
            .build();
        client.add_relay(account.relay.as_str()).await?;
        client.connect().and_wait(TIMEOUT).await;

        // Gift wraps addressed to me, since the last one we have (minus NIP-59 jitter).
        // Fetched pages are saved by nostr-sdk; the subscription then keeps us live.
        let mut filter = Filter::new().kind(Kind::GiftWrap).pubkey(keys.public_key());
        let stored = client.database().query(filter.clone().limit(1)).await?;
        if let Some(last) = stored.iter().map(|e| e.created_at).max() {
            let since = last.as_secs().saturating_sub(GIFT_WRAP_JITTER_SECS);
            filter = filter.since(Timestamp::from_secs(since));
        }
        backfill(filter.clone(), |f| {
            let client = &client;
            async move {
                let events = client.fetch_events(f).timeout(TIMEOUT).await?;
                Ok(events.into_iter().collect())
            }
        })
        .await?;
        client.subscribe(filter).await?;

        let session = Self {
            dir: dir.to_path_buf(),
            keys,
            account,
            client,
            peers: Mutex::new(HashMap::new()),
            relay_keys: Mutex::new(HashMap::new()),
            resubscribing: tokio::sync::Mutex::new(()),
        };
        session.sync_communities().await?;
        Ok(session)
    }

    async fn publish_profile(&self, name: &str) -> Result<()> {
        let relay = self.account.relay.as_str();
        let profile = Metadata::new().name(name).nip05(&self.account.handle);
        let events = [
            event::build(&self.keys, Kind::Metadata, [], &profile.as_json())?,
            event::build(&self.keys, Kind::RelayList, [Tag::parse(["r", relay])?], "")?,
            event::build(
                &self.keys,
                Kind::InboxRelays,
                [Tag::parse(["relay", relay])?],
                "",
            )?,
        ];
        for ev in &events {
            let out = self.client.send_event(ev).to([relay]).await?;
            if out.success.is_empty() {
                return Err(Error::NotDelivered(format!("{:?}", out.failed)));
            }
        }
        Ok(())
    }

    pub fn account(&self) -> &Account {
        &self.account
    }

    /// The secret key (`nsec`), for a backup or a device link.
    pub fn export_key(&self) -> String {
        match self.keys.secret_key().to_bech32() {
            Ok(nsec) => nsec,
            Err(never) => match never {},
        }
    }

    /// Forgets the account on this device: keychain entry, account file and
    /// local history. Without another device or a backup, the account is lost.
    pub async fn logout(&self) -> Result<()> {
        self.close().await;
        Identity::delete(&self.account.handle)?;
        for file in [ACCOUNT_FILE, EVENTS_DB, "events.db-wal", "events.db-shm"] {
            match std::fs::remove_file(self.dir.join(file)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
        }
        Ok(())
    }

    /// Looks up `pseudo@instance` and remembers the result.
    pub async fn search(&self, handle: &str) -> Result<Peer> {
        let peer = resolve(handle).await?;
        self.remember(peer.clone());
        Ok(peer)
    }

    /// What to show for a key: its verified handle when its profile (kind 0)
    /// names a `nip05` that resolves back to it, else just the npub.
    pub async fn peer(&self, pubkey: PublicKey) -> Peer {
        if let Some(peer) = self.cached(&pubkey) {
            return peer;
        }
        let verified = async {
            let filter = Filter::new().author(pubkey).kind(Kind::Metadata).limit(1);
            let events = self
                .client
                .fetch_events(filter)
                .timeout(TIMEOUT)
                .await
                .ok()?;
            let profile = Metadata::from_json(&events.first()?.content).ok()?;
            let peer = resolve(&profile.nip05?).await.ok()?;
            (peer.pubkey == pubkey).then_some(peer)
        }
        .await;
        let peer = verified.unwrap_or_else(|| unverified(pubkey));
        self.remember(peer.clone());
        peer
    }

    /// Sends `text` to `to` (NIP-17) and returns the message as stored.
    pub async fn send_dm(&self, to: PublicKey, text: &str) -> Result<Message> {
        let peer = self.peer(to).await;
        let [to_peer, to_self] = dm::wrap(&self.keys, to, text)?;

        // ponytail: the peer's relays come from NIP-05 (their instance). Read their
        // 10050 list instead at step 10 (federation). Unverified peer: our own relay.
        let relays: Vec<String> = if peer.relays.is_empty() {
            vec![self.account.relay.clone()]
        } else {
            peer.relays.iter().map(|r| r.to_string()).collect()
        };
        for relay in &relays {
            self.client.add_relay(relay.as_str()).await?;
            self.client.connect_relay(relay.as_str()).await?;
        }
        let out = self
            .client
            .send_event(&to_peer)
            .to(relays.iter().map(String::as_str))
            .await?;
        if out.success.is_empty() {
            return Err(Error::NotDelivered(format!("{:?}", out.failed)));
        }
        self.client
            .send_event(&to_self)
            .to([self.account.relay.as_str()])
            .await?;
        dm::unwrap(&self.keys, &to_self)
    }

    /// Conversations, most recent first.
    pub async fn conversations(&self) -> Result<Vec<Conversation>> {
        Ok(group(self.keys.public_key(), self.messages_all().await?))
    }

    /// Messages with `peer`, oldest first.
    pub async fn messages(&self, peer: PublicKey) -> Result<Vec<Message>> {
        let me = self.keys.public_key();
        let mut all = self.messages_all().await?;
        all.retain(|m| m.peer(me) == peer);
        Ok(all)
    }

    // ponytail: decrypts the whole history on every call. Cache the plaintext
    // (encrypted at rest) when it gets slow. Order is per second (NIP-01
    // `created_at`): same-second messages fall back to id order.
    async fn messages_all(&self) -> Result<Vec<Message>> {
        let filter = Filter::new()
            .kind(Kind::GiftWrap)
            .pubkey(self.keys.public_key());
        let events = self.client.database().query(filter).await?;
        let by_id: BTreeMap<_, _> = events
            .iter()
            .filter_map(|ev| dm::unwrap(&self.keys, ev).ok())
            .map(|m| (m.id, m))
            .collect();
        let mut all: Vec<Message> = by_id.into_values().collect();
        all.sort_by_key(|m| m.created_at);
        Ok(all)
    }

    /// Calls `on_update` for each new DM, channel message or community change
    /// received, until the session closes.
    pub async fn listen(&self, on_update: impl Fn(Update)) {
        let mut notifications = self.client.notifications();
        while let Some(notification) = notifications.next().await {
            let ClientNotification::Event { event, .. } = notification else {
                continue;
            };
            if event.kind == Kind::GiftWrap {
                if let Ok(message) = dm::unwrap(&self.keys, &event) {
                    on_update(Update::Dm { message });
                }
            } else if let Some(update) = self.community_update(&event).await {
                on_update(update);
            }
        }
    }

    pub async fn close(&self) {
        self.client.shutdown().await;
    }

    fn cached(&self, pubkey: &PublicKey) -> Option<Peer> {
        self.peers.lock().ok()?.get(pubkey).cloned()
    }

    fn remember(&self, peer: Peer) {
        if let Ok(mut peers) = self.peers.lock() {
            peers.insert(peer.pubkey, peer);
        }
    }
}

/// Pages back through `filter` with `until`: the relay caps each answer (100
/// events for khatru's SQLite store). Stops on a page with nothing new.
// ponytail: more than a page of events in the same second would loop on that
// second; gift wrap timestamps are randomized (NIP-59), so it doesn't happen.
// Plain closure returning a future, not `AsyncFn`: keeps callers `Send` (Tauri).
pub(crate) async fn backfill<Fut>(filter: Filter, fetch: impl Fn(Filter) -> Fut) -> Result<usize>
where
    Fut: Future<Output = Result<Vec<Event>>>,
{
    let mut seen = std::collections::HashSet::new();
    let mut until = Timestamp::now();
    loop {
        let page = fetch(filter.clone().until(until)).await?;
        let Some(oldest) = page.iter().map(|e| e.created_at).min() else {
            return Ok(seen.len());
        };
        let before = seen.len();
        seen.extend(page.iter().map(|e| e.id));
        if seen.len() == before {
            return Ok(seen.len());
        }
        until = oldest;
    }
}

/// Resolves `pseudo@instance` through the instance directory (NIP-05).
pub async fn resolve(handle: &str) -> Result<Peer> {
    let handle = handle.trim().trim_start_matches('@').to_lowercase();
    let address = Nip05Address::parse(&handle)?;
    let json = http()
        .get(nip05_url(&address))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let profile = Nip05Profile::from_raw_json(&address, &json).map_err(|_| Error::NotFound)?;
    Ok(Peer {
        handle: Some(handle),
        npub: npub(&profile.public_key),
        pubkey: profile.public_key,
        relays: profile.relays,
    })
}

/// NIP-05 is HTTPS only; plain HTTP is allowed for a local dev instance.
fn nip05_url(address: &Nip05Address) -> String {
    let scheme = if is_local_host(address.domain()) {
        "http"
    } else {
        "https"
    };
    format!(
        "{scheme}://{}/.well-known/nostr.json?name={}",
        address.domain(),
        address.name()
    )
}

/// Last message per peer, most recent conversation first. `messages` is sorted oldest first.
fn group(me: PublicKey, messages: Vec<Message>) -> Vec<Conversation> {
    let mut last: HashMap<PublicKey, Message> = HashMap::new();
    for m in messages {
        last.insert(m.peer(me), m);
    }
    let mut conversations: Vec<Conversation> = last
        .into_iter()
        .map(|(peer, last)| Conversation { peer, last })
        .collect();
    conversations.sort_by_key(|c| std::cmp::Reverse(c.last.created_at));
    conversations
}

fn host(url: &Url) -> Result<String> {
    let host = url
        .host_str()
        .ok_or_else(|| Error::Invalid("instance URL has no host".into()))?;
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

fn unverified(pubkey: PublicKey) -> Peer {
    Peer {
        handle: None,
        npub: npub(&pubkey),
        pubkey,
        relays: Vec::new(),
    }
}

fn npub(pubkey: &PublicKey) -> String {
    match pubkey.to_bech32() {
        Ok(npub) => npub,
        Err(never) => match never {},
    }
}

/// `localhost` or `127.0.0.1`, with or without a port: plain HTTP/WS allowed (dev).
pub(crate) fn is_local_host(host: &str) -> bool {
    matches!(host.split(':').next(), Some("localhost" | "127.0.0.1"))
}

pub(crate) fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::event::FinalizeEvent;

    #[test]
    fn groups_conversations_by_peer_latest_first() {
        let (me, bob, carol) = (Keys::generate(), Keys::generate(), Keys::generate());
        let unwrap_mine = |sender: &Keys, receiver: &Keys, text: &str| {
            let [to_receiver, to_self] = dm::wrap(sender, receiver.public_key(), text).unwrap();
            let gift = if receiver.public_key() == me.public_key() {
                to_receiver
            } else {
                to_self
            };
            dm::unwrap(&me, &gift).unwrap()
        };
        let mut msgs = vec![
            unwrap_mine(&bob, &me, "1 from bob"),
            unwrap_mine(&me, &carol, "2 to carol"),
            unwrap_mine(&me, &bob, "3 to bob"),
        ];
        for (i, m) in msgs.iter_mut().enumerate() {
            m.created_at = Timestamp::from_secs(100 + i as u64);
        }

        let convs = group(me.public_key(), msgs);
        assert_eq!(convs.len(), 2);
        assert_eq!(convs[0].peer, bob.public_key());
        assert_eq!(convs[0].last.text, "3 to bob");
        assert_eq!(convs[1].peer, carol.public_key());
    }

    #[tokio::test]
    async fn backfill_pages_past_the_relay_cap() {
        let keys = Keys::generate();
        let all: Vec<Event> = (0..250)
            .map(|i| {
                nostr_sdk::prelude::EventBuilder::new(Kind::TextNote, i.to_string())
                    .custom_created_at(Timestamp::from_secs(1_000 + i / 2)) // 2 per second
                    .finalize(&keys)
                    .unwrap()
            })
            .collect();
        // A relay that answers newest first, at most 100 events.
        let fetch = |f: Filter| {
            let until = f.until.unwrap();
            let mut page: Vec<Event> = all
                .iter()
                .filter(|e| e.created_at <= until)
                .cloned()
                .collect();
            page.sort_by_key(|e| std::cmp::Reverse(e.created_at));
            page.truncate(100);
            std::future::ready(Ok(page))
        };
        assert_eq!(backfill(Filter::new(), fetch).await.unwrap(), 250);
    }

    #[test]
    fn nip05_url_plain_http_only_for_localhost() {
        let url = |h: &str| nip05_url(&Nip05Address::parse(h).unwrap());
        assert_eq!(
            url("bob@localhost:8099"),
            "http://localhost:8099/.well-known/nostr.json?name=bob"
        );
        assert_eq!(
            url("bob@chaton.example"),
            "https://chaton.example/.well-known/nostr.json?name=bob"
        );
        assert!(url("bob@localhost.evil.example").starts_with("https://"));
    }
}
