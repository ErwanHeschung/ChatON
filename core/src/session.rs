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
use crate::media::{self, Attachment};
use crate::{Error, Result, event};

const ACCOUNT_FILE: &str = "account.json";
const EVENTS_DB: &str = "events.db";
/// Downloaded files, as downloaded (attachments stay encrypted).
const MEDIA_DIR: &str = "media";
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
    /// Profile picture, only if hosted by the peer's own instance (spec § 10).
    pub picture: Option<String>,
    #[serde(skip)]
    pub relays: Vec<RelayUrl>,
}

/// A 1:1 or a group (spec § 5): its members, its latest name, its latest message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conversation {
    pub room: Vec<PublicKey>,
    pub subject: Option<String>,
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
    /// Updates not coming from a relay (voice), merged into `listen`.
    pub(crate) updates: tokio::sync::broadcast::Sender<Update>,
    /// The voice channel or call I'm in (one at a time).
    #[cfg(feature = "voice")]
    pub(crate) voice: crate::voice::Slot,
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
            updates: tokio::sync::broadcast::channel(64).0,
            #[cfg(feature = "voice")]
            voice: Default::default(),
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

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
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
        // Best effort: an instance that's down shouldn't keep me logged in.
        drop(self.push_unregister().await);
        self.close().await;
        Identity::delete(&self.account.handle)?;
        for file in [
            ACCOUNT_FILE,
            EVENTS_DB,
            "events.db-wal",
            "events.db-shm",
            crate::push::PUSH_FILE,
        ] {
            match std::fs::remove_file(self.dir.join(file)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
        }
        match std::fs::remove_dir_all(self.dir.join(MEDIA_DIR)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
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
            let mut peer = resolve(&profile.nip05?).await.ok()?;
            let domain = peer.handle.as_deref()?.split_once('@')?.1.to_string();
            peer.picture = profile
                .picture
                .filter(|url| media::allowed(url, &[domain]).is_some());
            (peer.pubkey == pubkey).then_some(peer)
        }
        .await;
        let peer = verified.unwrap_or_else(|| unverified(pubkey));
        self.remember(peer.clone());
        peer
    }

    /// Sends `text` to `to`, one person or a group's members (NIP-17), and
    /// returns the message as stored.
    pub async fn send_dm(&self, to: &[PublicKey], text: &str) -> Result<Message> {
        self.deliver(dm::wrap(&self.keys, to, text)?).await
    }

    /// Names a group, or renames it (spec § 5). Creating a group is naming it.
    pub async fn name_group(&self, to: &[PublicKey], subject: &str) -> Result<Message> {
        self.deliver(dm::wrap_subject(&self.keys, to, subject)?)
            .await
    }

    /// Encrypts and uploads a file, then sends it to `to` (NIP-17 kind 15).
    pub async fn send_dm_file(
        &self,
        to: &[PublicKey],
        bytes: &[u8],
        mime: &str,
        name: &str,
    ) -> Result<Message> {
        let file = self.upload_attachment(bytes, mime, name).await?;
        self.deliver(dm::wrap_file(&self.keys, to, &file)?).await
    }

    /// Sets my profile picture (public, uploaded to my instance).
    pub async fn set_profile_picture(&self, bytes: Vec<u8>, mime: &str) -> Result<()> {
        let url = media::upload(&self.keys, &self.account.relay, bytes, mime).await?;
        let name = self.account.handle.split('@').next().unwrap_or_default();
        let url = Url::parse(&url).map_err(|e| Error::Invalid(e.to_string()))?;
        let profile = Metadata::new()
            .name(name)
            .nip05(&self.account.handle)
            .picture(url);
        let ev = event::build(&self.keys, Kind::Metadata, [], &profile.as_json())?;
        let out = self
            .client
            .send_event(&ev)
            .to([self.account.relay.as_str()])
            .await?;
        if out.success.is_empty() {
            return Err(Error::NotDelivered(format!("{:?}", out.failed)));
        }
        if let Ok(mut peers) = self.peers.lock() {
            peers.remove(&self.account.pubkey); // next `peer` reads the new picture
        }
        Ok(())
    }

    /// The decrypted file. Without `explicit` (a click), only from my instance
    /// and my communities' instances (spec § 10), else `NotAllowed`.
    pub async fn open_attachment(&self, file: &Attachment, explicit: bool) -> Result<Vec<u8>> {
        if !explicit && media::allowed(&file.url, &self.trusted_hosts().await?).is_none() {
            return Err(Error::NotAllowed);
        }
        let cipher = self.cached_download(&file.url, &file.sha256).await?;
        media::decrypt(file, &cipher)
    }

    /// A community or profile picture, from a trusted host only.
    pub async fn picture(&self, url: &str) -> Result<Vec<u8>> {
        let hash = media::allowed(url, &self.trusted_hosts().await?).ok_or(Error::NotAllowed)?;
        self.cached_download(url, &hash).await
    }

    pub(crate) async fn upload_attachment(
        &self,
        bytes: &[u8],
        mime: &str,
        name: &str,
    ) -> Result<Attachment> {
        if bytes.len() > media::MAX_SIZE {
            return Err(Error::TooLarge);
        }
        let (cipher, mut file) = media::encrypt(bytes, mime, name)?;
        file.url = media::upload(
            &self.keys,
            &self.account.relay,
            cipher,
            "application/octet-stream",
        )
        .await?;
        Ok(file)
    }

    /// My instance and the instances of my communities.
    async fn trusted_hosts(&self) -> Result<Vec<String>> {
        let mut hosts: Vec<String> = self.communities_relays().await?;
        hosts.push(self.account.relay.clone());
        Ok(hosts.iter().filter_map(|r| media::host(r)).collect())
    }

    /// Blobs are cached as downloaded: ciphertext for attachments.
    async fn cached_download(&self, url: &str, sha256: &str) -> Result<Vec<u8>> {
        let path = self.dir.join(MEDIA_DIR).join(sha256);
        if let Ok(bytes) = std::fs::read(&path)
            && media::sha256_hex(&bytes) == sha256
        {
            return Ok(bytes);
        }
        let bytes = media::download(url, sha256).await?;
        std::fs::create_dir_all(self.dir.join(MEDIA_DIR))?;
        std::fs::write(&path, &bytes)?;
        Ok(bytes)
    }

    /// Each member's gift wrap to their relays, then my copy to mine. The copy
    /// goes out if anyone got the message; then an error names who didn't.
    // ponytail: one member after the other (≤ 9). Send in parallel if it gets slow.
    pub(crate) async fn deliver(&self, wraps: dm::Wraps) -> Result<Message> {
        let mut failed = Vec::new();
        for (to, gift) in &wraps.members {
            if let Err(e) = self.send_to_inbox(*to, gift).await {
                failed.push(e.to_string());
            }
        }
        if failed.len() == wraps.members.len() {
            return Err(Error::NotDelivered(failed.join("; ")));
        }
        self.client
            .send_event(&wraps.to_self)
            .to([self.account.relay.as_str()])
            .await?;
        if !failed.is_empty() {
            return Err(Error::NotDelivered(failed.join("; ")));
        }
        dm::unwrap(&self.keys, &wraps.to_self)
    }

    async fn send_to_inbox(&self, to: PublicKey, gift: &Event) -> Result<()> {
        let peer = self.peer(to).await;
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
            .send_event(gift)
            .to(relays.iter().map(String::as_str))
            .await?;
        if out.success.is_empty() {
            return Err(Error::NotDelivered(format!("{:?}", out.failed)));
        }
        Ok(())
    }

    /// Conversations, most recent first.
    pub async fn conversations(&self) -> Result<Vec<Conversation>> {
        Ok(group(self.messages_all().await?))
    }

    /// Messages of the conversation with `members` (me added), oldest first.
    pub async fn messages(&self, members: &[PublicKey]) -> Result<Vec<Message>> {
        let room = dm::room(self.keys.public_key(), members);
        let mut all = self.messages_all().await?;
        all.retain(|m| m.room == room);
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

    /// Calls `on_update` for each new DM, channel message, community change,
    /// incoming call or voice change, until the session closes.
    pub async fn listen(&self, on_update: impl Fn(Update)) {
        let mut notifications = self.client.notifications();
        let mut local = self.updates.subscribe();
        loop {
            let notification = tokio::select! {
                n = notifications.next() => match n {
                    Some(n) => n,
                    None => return,
                },
                Ok(update) = local.recv() => {
                    on_update(update);
                    continue;
                }
            };
            let ClientNotification::Event { event, .. } = notification else {
                continue;
            };
            if event.kind == Kind::GiftWrap {
                if let Ok(message) = dm::unwrap(&self.keys, &event) {
                    let ring = dm::rings(&message, self.account.pubkey, Timestamp::now())
                        && self.invited_by_caller_instance(&message).await;
                    #[cfg(feature = "voice")]
                    if let Some(call) = message.call.as_ref().filter(|c| c.ended)
                        && message.sender != self.account.pubkey
                    {
                        drop(self.call_ended(&call.id).await);
                    }
                    on_update(Update::Dm {
                        message: message.clone(),
                    });
                    if ring {
                        let until = message.created_at + dm::RING_SECS;
                        on_update(Update::Ring { message, until });
                    }
                }
            } else if let Some(update) = self.community_update(&event).await {
                on_update(update);
            }
        }
    }

    /// A call invite must point at its sender's instance (domain of their
    /// verified handle), so no one can make me contact a server of their choice.
    pub(crate) async fn invited_by_caller_instance(&self, message: &Message) -> bool {
        let Some(invite) = message.call.as_ref().and_then(|c| c.invite.as_ref()) else {
            return false;
        };
        let handle = self.peer(message.sender).await.handle;
        handle.as_deref().and_then(instance_url) == Some(invite.url.clone())
    }

    /// A NIP-98 signed request, built by hand: rust-nostr's HttpMethod has no DELETE.
    pub(crate) async fn nip98(
        &self,
        method: reqwest::Method,
        url: &str,
        body: String,
    ) -> Result<reqwest::Response> {
        let auth = event::build(
            &self.keys,
            Kind::HttpAuth,
            [
                Tag::parse(["u", url])?,
                Tag::parse(["method", method.as_str()])?,
                Tag::parse(["payload", &hex::encode(Sha256::digest(&body))])?,
            ],
            "",
        )?;
        let auth = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            serde_json::to_string(&auth)?,
        );
        Ok(http()
            .request(method, url)
            .header("Authorization", format!("Nostr {auth}"))
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?)
    }

    pub async fn close(&self) {
        #[cfg(feature = "voice")]
        drop(self.leave_voice().await);
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
        picture: None,
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
/// One conversation per room. `messages` oldest first: the latest name wins.
fn group(messages: Vec<Message>) -> Vec<Conversation> {
    let mut rooms: HashMap<Vec<PublicKey>, Conversation> = HashMap::new();
    for m in messages {
        let c = rooms.entry(m.room.clone()).or_insert_with(|| Conversation {
            room: m.room.clone(),
            subject: None,
            last: m.clone(),
        });
        if m.subject.is_some() {
            c.subject.clone_from(&m.subject);
        }
        c.last = m;
    }
    let mut conversations: Vec<Conversation> = rooms.into_values().collect();
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
        picture: None,
        relays: Vec::new(),
    }
}

fn npub(pubkey: &PublicKey) -> String {
    match pubkey.to_bech32() {
        Ok(npub) => npub,
        Err(never) => match never {},
    }
}

/// The base URL of the instance of `pseudo@instance`: `https://instance`,
/// plain HTTP for a local dev instance.
pub(crate) fn instance_url(handle: &str) -> Option<String> {
    let domain = handle.split_once('@')?.1;
    let scheme = if is_local_host(domain) {
        "http"
    } else {
        "https"
    };
    Some(format!("{scheme}://{domain}"))
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
    fn groups_conversations_by_room_latest_first() {
        let (me, bob, carol) = (Keys::generate(), Keys::generate(), Keys::generate());
        let (pk_me, pk_bob, pk_carol) = (me.public_key(), bob.public_key(), carol.public_key());
        // My copy of what I send, my wrap of what bob sends.
        let mine = |wraps: dm::Wraps| dm::unwrap(&me, &wraps.to_self).unwrap();
        let from_bob = |wraps: dm::Wraps| {
            let (_, gift) = wraps
                .members
                .into_iter()
                .find(|(pk, _)| *pk == pk_me)
                .unwrap();
            dm::unwrap(&me, &gift).unwrap()
        };
        let mut msgs = vec![
            from_bob(dm::wrap(&bob, &[pk_me], "1 from bob").unwrap()),
            mine(dm::wrap_subject(&me, &[pk_bob, pk_carol], "Projet").unwrap()),
            mine(dm::wrap(&me, &[pk_carol], "3 to carol").unwrap()),
            from_bob(dm::wrap(&bob, &[pk_me, pk_carol], "4 in the group").unwrap()),
            mine(dm::wrap(&me, &[pk_bob], "5 to bob").unwrap()),
        ];
        for (i, m) in msgs.iter_mut().enumerate() {
            m.created_at = Timestamp::from_secs(100 + i as u64);
        }

        let convs = group(msgs);
        let rooms: Vec<_> = convs
            .iter()
            .map(|c| (c.room.len(), c.last.text.as_str()))
            .collect();
        // The 1:1 with bob and the group with bob stay apart.
        assert_eq!(
            rooms,
            [(2, "5 to bob"), (3, "4 in the group"), (2, "3 to carol")]
        );
        assert_eq!(convs[1].subject.as_deref(), Some("Projet"));
        assert_eq!(convs[0].room, dm::room(pk_me, &[pk_bob]));
        assert_eq!(convs[0].subject, None);
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
