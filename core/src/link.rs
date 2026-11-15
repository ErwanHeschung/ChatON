//! Device linking (spec/protocol.md § 8): a signed-in device A hands its key to
//! a new device B over the relay, protected by a short code and SPAKE2.
//!
//! The relay only sees SPAKE2 messages and NIP-44 ciphertexts under the shared
//! key `K`: it can't test codes offline, and a code is burnt after one attempt.

use std::path::Path;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use nostr::nips::nip44::v2::{self, ConversationKey};
use std::pin::Pin;

use nostr_sdk::prelude::{
    Client, ClientNotification, Filter, Keys, Kind, PublicKey, SingleLetterTag, StreamExt, Tag,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spake2::{Ed25519Group, Identity, Password, Spake2};
use tokio::time::Instant;

use crate::session::{Session, resolve};
use crate::{Error, Result, event};

pub const KIND: u16 = 24135;
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ"; // Crockford base32
const CODE_LEN: usize = 10;
const CHANNEL_LEN: usize = 4;
const TTL: Duration = Duration::from_secs(5 * 60);
/// A answers B's first message without user action: silence means no device shows this code.
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);
const PREFIX: &str = "chaton-link-v0";

/// A link code: `channel` (4 chars) picks the relay channel, `secret` (6 chars) is the SPAKE2 password.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Code(String);

impl Code {
    fn generate() -> Result<Self> {
        let mut bytes = [0u8; CODE_LEN];
        getrandom::fill(&mut bytes).map_err(|e| Error::Invalid(e.to_string()))?;
        // 256 is a multiple of 32: `& 31` keeps every char uniform.
        Ok(Self(
            bytes
                .iter()
                .map(|b| ALPHABET[(b & 31) as usize] as char)
                .collect(),
        ))
    }

    /// Tolerant input: case, dashes and spaces ignored, I/L → 1, O → 0.
    fn parse(input: &str) -> Result<Self> {
        let code: String = input
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .map(|c| match c.to_ascii_uppercase() {
                'I' | 'L' => '1',
                'O' => '0',
                c => c,
            })
            .collect();
        if code.len() != CODE_LEN || !code.bytes().all(|b| ALPHABET.contains(&b)) {
            return Err(Error::WrongCode);
        }
        Ok(Self(code))
    }

    /// `XXXX-XXX-XXX`
    fn display(&self) -> String {
        let c = &self.0;
        format!("{}-{}-{}", &c[..4], &c[4..7], &c[7..])
    }

    fn secret(&self) -> &str {
        &self.0[CHANNEL_LEN..]
    }

    /// The `c` tag: hex(sha256("chaton-link-v0|" + handle + "|" + channel)).
    fn tag(&self, handle: &str) -> String {
        let digest = Sha256::digest(format!("{PREFIX}|{handle}|{}", &self.0[..CHANNEL_LEN]));
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Msg {
    Pake { msg: String },
    Hello { data: String },
    Key { data: String },
    Refused,
    Failed,
    Done { data: String },
}

/// Where link messages travel: the relay, or memory in tests.
trait Channel {
    async fn send(&mut self, msg: Msg) -> Result<()>;
    async fn recv(&mut self) -> Result<Msg>;
}

/// Shown on device A: the code to type on the new device, until `run` ends.
pub struct LinkOffer {
    code: Code,
    handle: String,
    nsec: String,
    channel: RelayChannel,
}

impl LinkOffer {
    /// Device A: listens on the channel right away, so the new device can talk.
    pub async fn new(session: &Session) -> Result<Self> {
        let code = Code::generate()?;
        let handle = session.account().handle.clone();
        let channel = RelayChannel::open(&session.account().relay, code.tag(&handle)).await?;
        Ok(Self {
            code,
            handle,
            nsec: session.export_key(),
            channel,
        })
    }

    /// `XXXX-XXX-XXX`, to type on the new device.
    pub fn code(&self) -> String {
        self.code.display()
    }

    /// Runs the exchange once (the code is then burnt). `confirm` gets the new
    /// device's name and says whether the user accepts. Returns that name.
    pub async fn run<Fut>(mut self, confirm: impl FnOnce(String) -> Fut) -> Result<String>
    where
        Fut: Future<Output = bool>,
    {
        let result = offer(
            &mut self.channel,
            self.code.secret(),
            &self.handle,
            &self.nsec,
            confirm,
        )
        .await;
        self.channel.client.shutdown().await;
        result
    }
}

/// Device B: gets the key of `handle` from a signed-in device showing `code`,
/// checks it against the directory and opens the account in `dir`.
pub async fn join(dir: &Path, handle: &str, code: &str, device_name: &str) -> Result<Session> {
    let code = Code::parse(code)?;
    let peer = resolve(handle).await?;
    let handle = peer.handle.ok_or(Error::NotFound)?;
    let relay = peer.relays.first().ok_or(Error::NotFound)?.to_string();
    let mut channel = RelayChannel::open(&relay, code.tag(&handle)).await?;
    let result = accept(
        &mut channel,
        code.secret(),
        &handle,
        device_name,
        peer.pubkey,
    )
    .await;
    channel.client.shutdown().await;
    Session::import_key(dir, &handle, &result?).await
}

/// Device A's side: B pake → A pake → B hello → A key | refused | failed → B done.
async fn offer<Fut: Future<Output = bool>>(
    ch: &mut impl Channel,
    secret: &str,
    handle: &str,
    nsec: &str,
    confirm: impl FnOnce(String) -> Fut,
) -> Result<String> {
    let Msg::Pake { msg } = ch.recv().await? else {
        return Err(unexpected());
    };
    let (pake, out) =
        Spake2::<Ed25519Group>::start_a(&password(secret), &id("A", handle), &id("B", handle));
    ch.send(Msg::Pake {
        msg: B64.encode(out),
    })
    .await?;
    let key = finish(pake, &msg)?;

    let Msg::Hello { data } = ch.recv().await? else {
        return Err(unexpected());
    };
    let Ok(name) = decrypt(&key, &data) else {
        ch.send(Msg::Failed).await?;
        return Err(Error::WrongCode);
    };
    if !confirm(name.clone()).await {
        ch.send(Msg::Refused).await?;
        return Err(Error::LinkRefused);
    }
    ch.send(Msg::Key {
        data: encrypt(&key, nsec)?,
    })
    .await?;

    match ch.recv().await? {
        Msg::Done { data } if decrypt(&key, &data).is_ok_and(|ok| ok == "ok") => Ok(name),
        _ => Err(unexpected()),
    }
}

/// Device B's side. Returns the received `nsec`, once it's checked to be `expected`'s.
async fn accept(
    ch: &mut impl Channel,
    secret: &str,
    handle: &str,
    device_name: &str,
    expected: PublicKey,
) -> Result<String> {
    let (pake, out) =
        Spake2::<Ed25519Group>::start_b(&password(secret), &id("A", handle), &id("B", handle));
    ch.send(Msg::Pake {
        msg: B64.encode(out),
    })
    .await?;
    let reply = tokio::time::timeout(REPLY_TIMEOUT, ch.recv()).await;
    let Msg::Pake { msg } = reply.map_err(|_| Error::CodeExpired)?? else {
        return Err(unexpected());
    };
    let key = finish(pake, &msg)?;
    ch.send(Msg::Hello {
        data: encrypt(&key, device_name)?,
    })
    .await?;

    let nsec = match ch.recv().await? {
        Msg::Key { data } => decrypt(&key, &data).map_err(|_| Error::WrongCode)?,
        Msg::Refused => return Err(Error::LinkRefused),
        Msg::Failed => return Err(Error::WrongCode),
        _ => return Err(unexpected()),
    };
    if Keys::parse(&nsec)?.public_key() != expected {
        return Err(Error::KeyMismatch);
    }
    ch.send(Msg::Done {
        data: encrypt(&key, "ok")?,
    })
    .await?;
    Ok(nsec)
}

fn password(secret: &str) -> Password {
    Password::new(secret.as_bytes())
}

fn id(side: &str, handle: &str) -> Identity {
    Identity::new(format!("{PREFIX}|{side}|{handle}").as_bytes())
}

fn finish(pake: Spake2<Ed25519Group>, msg: &str) -> Result<ConversationKey> {
    let msg = B64.decode(msg).map_err(|_| unexpected())?;
    let key: [u8; 32] = pake
        .finish(&msg)
        .map_err(|_| unexpected())?
        .try_into()
        .map_err(|_| unexpected())?;
    Ok(ConversationKey::new(key))
}

fn encrypt(key: &ConversationKey, plaintext: &str) -> Result<String> {
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce).map_err(|e| Error::Invalid(e.to_string()))?;
    let payload = v2::encrypt_to_bytes_with_nonce(key, plaintext.as_bytes(), nonce)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(B64.encode(payload))
}

fn decrypt(key: &ConversationKey, data: &str) -> Result<String> {
    let payload = B64.decode(data).map_err(|_| unexpected())?;
    let plain = v2::decrypt_to_bytes(key, &payload).map_err(|_| Error::WrongCode)?;
    String::from_utf8(plain).map_err(|_| unexpected())
}

fn unexpected() -> Error {
    Error::Invalid("unexpected device link message".into())
}

/// Ephemeral kind 24135 events on one `c` channel, each side with a throwaway key.
struct RelayChannel {
    client: Client,
    keys: Keys,
    relay: String,
    tag: String,
    notifications: Pin<Box<dyn futures_core::Stream<Item = ClientNotification> + Send>>,
    /// The first other key heard on the channel; anyone else is ignored.
    peer: Option<PublicKey>,
    deadline: Instant,
}

impl RelayChannel {
    async fn open(relay: &str, tag: String) -> Result<Self> {
        let client = Client::default();
        client.add_relay(relay).await?;
        client.connect().and_wait(Duration::from_secs(10)).await;
        let notifications = client.notifications();
        let c = SingleLetterTag::from_char('c').map_err(|e| Error::Invalid(e.to_string()))?;
        let filter = Filter::new()
            .kind(Kind::Custom(KIND))
            .custom_tag(c, tag.clone());
        client.subscribe(filter).await?;
        Ok(Self {
            client,
            keys: Keys::generate(),
            relay: relay.to_string(),
            tag,
            notifications,
            peer: None,
            deadline: Instant::now() + TTL,
        })
    }
}

impl Channel for RelayChannel {
    async fn send(&mut self, msg: Msg) -> Result<()> {
        let content = serde_json::to_string(&msg)?;
        let ev = event::build(
            &self.keys,
            Kind::Custom(KIND),
            [Tag::parse(["c", self.tag.as_str()])?],
            &content,
        )?;
        let out = self
            .client
            .send_event(&ev)
            .to([self.relay.as_str()])
            .await?;
        if out.success.is_empty() {
            return Err(Error::NotDelivered(format!("{:?}", out.failed)));
        }
        Ok(())
    }

    async fn recv(&mut self) -> Result<Msg> {
        loop {
            let notification = tokio::time::timeout_at(self.deadline, self.notifications.next())
                .await
                .map_err(|_| Error::CodeExpired)?
                .ok_or(Error::CodeExpired)?;
            let ClientNotification::Event { event, .. } = notification else {
                continue;
            };
            if event.kind != Kind::Custom(KIND) || event.pubkey == self.keys.public_key() {
                continue;
            }
            if *self.peer.get_or_insert(event.pubkey) != event.pubkey {
                continue;
            }
            return serde_json::from_str(&event.content).map_err(|_| unexpected());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::ToBech32;
    use tokio::sync::mpsc;

    struct Mem(mpsc::UnboundedSender<Msg>, mpsc::UnboundedReceiver<Msg>);

    impl Channel for Mem {
        async fn send(&mut self, msg: Msg) -> Result<()> {
            self.0.send(msg).map_err(|_| Error::CodeExpired)
        }
        async fn recv(&mut self) -> Result<Msg> {
            self.1.recv().await.ok_or(Error::CodeExpired)
        }
    }

    fn pair() -> (Mem, Mem) {
        let (a_tx, b_rx) = mpsc::unbounded_channel();
        let (b_tx, a_rx) = mpsc::unbounded_channel();
        (Mem(a_tx, a_rx), Mem(b_tx, b_rx))
    }

    /// Runs A (code `a_secret`, answering `yes`) against B (typing `b_secret`).
    async fn link(
        a_secret: &str,
        b_secret: &str,
        yes: bool,
    ) -> (Result<String>, Result<String>, Keys) {
        let keys = Keys::generate();
        let nsec = keys.secret_key().to_bech32().unwrap();
        let (mut a, mut b) = pair();
        let (a_res, b_res) = tokio::join!(
            offer(&mut a, a_secret, "alice@x", &nsec, |name| {
                assert_eq!(name, "laptop");
                std::future::ready(yes)
            }),
            accept(&mut b, b_secret, "alice@x", "laptop", keys.public_key()),
        );
        (a_res, b_res, keys)
    }

    #[tokio::test]
    async fn right_code_hands_over_the_key() {
        let (a, b, keys) = link("Q9XM2B", "Q9XM2B", true).await;
        assert_eq!(a.unwrap(), "laptop");
        assert_eq!(
            Keys::parse(&b.unwrap()).unwrap().public_key(),
            keys.public_key()
        );
    }

    #[tokio::test]
    async fn wrong_code_sends_no_key() {
        let (mut a, mut b) = pair();
        let nsec = Keys::generate().secret_key().to_bech32().unwrap();
        let (a_res, b_res) = tokio::join!(
            offer(&mut a, "Q9XM2B", "alice@x", &nsec, async |_| panic!(
                "asked to confirm"
            )),
            async {
                // B speaks pake + hello with the wrong secret, then checks what comes back.
                let (pake, out) = Spake2::<Ed25519Group>::start_b(
                    &password("Q9XM2C"),
                    &id("A", "alice@x"),
                    &id("B", "alice@x"),
                );
                b.send(Msg::Pake {
                    msg: B64.encode(out),
                })
                .await
                .unwrap();
                let Msg::Pake { msg } = b.recv().await.unwrap() else {
                    panic!()
                };
                let key = finish(pake, &msg).unwrap();
                b.send(Msg::Hello {
                    data: encrypt(&key, "laptop").unwrap(),
                })
                .await
                .unwrap();
                b.recv().await.unwrap()
            },
        );
        assert!(matches!(a_res, Err(Error::WrongCode)), "{a_res:?}");
        assert!(matches!(b_res, Msg::Failed), "{b_res:?}");

        // And the regular B side reports it.
        let (_, b, _) = link("Q9XM2B", "Q9XM2C", true).await;
        assert!(matches!(b, Err(Error::WrongCode)), "{b:?}");
    }

    #[tokio::test]
    async fn refused_on_a() {
        let (a, b, _) = link("Q9XM2B", "Q9XM2B", false).await;
        assert!(matches!(a, Err(Error::LinkRefused)), "{a:?}");
        assert!(matches!(b, Err(Error::LinkRefused)), "{b:?}");
    }

    #[test]
    fn code_round_trip_and_tolerant_input() {
        let code = Code::generate().unwrap();
        assert_eq!(Code::parse(&code.display()).unwrap(), code);
        assert_eq!(code.display().len(), 12);
        let typed = Code::parse(" k7f4-q9xm-2b ").unwrap();
        assert_eq!(typed.display(), "K7F4-Q9X-M2B");
        assert_eq!(Code::parse("k7f4 q9x m2b").unwrap(), typed);
        assert_eq!(
            Code::parse("k7f4-q9x-ilo").unwrap().display(),
            "K7F4-Q9X-110"
        );
        assert_eq!(typed.secret(), "Q9XM2B");
        assert!(Code::parse("K7F4-Q9X-M2").is_err());
        assert!(Code::parse("K7F4-Q9X-M2U").is_err()); // U isn't Crockford
        // Same handle and channel → same tag; another handle → another tag.
        assert_eq!(
            typed.tag("a@x"),
            Code::parse("K7F4000000").unwrap().tag("a@x")
        );
        assert_ne!(typed.tag("a@x"), typed.tag("b@x"));
        assert_eq!(typed.tag("a@x").len(), 64);
    }
}
