//! Private messages: NIP-17 (rumor 14 → seal 13 → gift wrap 1059). See spec §5.

use nostr::nips::nip59::{GiftWrapBuilder, UnwrappedGift};
use nostr::prelude::{
    Event, EventId, FinalizeEvent, FinalizeUnsignedEvent, Keys, Kind, PublicKey, Tag, Timestamp,
};
use serde::Serialize;

use crate::media::Attachment;
use crate::{Error, Result, event};

/// NIP-17 file message (rumor, like 14).
pub const FILE_MESSAGE: Kind = Kind::Custom(15);

/// Members of a conversation at most, author included (spec § 5, groups).
pub const MAX_ROOM: usize = 10;
/// Group name length (NIP-17 `subject`), in characters.
pub const MAX_SUBJECT: usize = 100;
/// What other NIP-17 clients show for a call invite and its end (spec § 11).
pub const CALL_TEXT: &str = "📞 Appel vocal";
pub const CALL_END_TEXT: &str = "Appel terminé";
/// An invite rings this long after it was sent.
pub const RING_SECS: u64 = 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Message {
    /// Rumor id: the same in every member's copy.
    pub id: EventId,
    pub sender: PublicKey,
    /// Every member, author included, sorted: identifies the conversation.
    /// A 1:1 is `[me, peer]`.
    pub room: Vec<PublicKey>,
    pub text: String,
    pub created_at: Timestamp,
    /// The group name this message sets (NIP-17 `subject`), if any.
    pub subject: Option<String>,
    /// A file (NIP-17 kind 15); `text` is then empty.
    pub attachment: Option<Attachment>,
    /// A call invite or its end (spec § 11); `text` is then empty.
    pub call: Option<Call>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Call {
    /// 16 bytes, hex.
    pub id: String,
    /// `call-end`: the call is over.
    pub ended: bool,
    /// Where to join and the audio key: never sent to the UI.
    #[serde(skip)]
    pub(crate) invite: Option<CallInvite>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CallInvite {
    /// The caller's instance, which hands out the tokens.
    pub url: String,
    /// LiveKit E2EE shared key.
    pub key: [u8; 32],
}

impl std::fmt::Debug for CallInvite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallInvite")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

/// An invite from someone else, sent less than `RING_SECS` ago.
pub fn rings(message: &Message, me: PublicKey, now: Timestamp) -> bool {
    message.sender != me
        && message.call.as_ref().is_some_and(|c| c.invite.is_some())
        && now.as_secs().saturating_sub(message.created_at.as_secs()) < RING_SECS
}

/// The room of `me` with `members`: sorted, deduplicated, me included.
pub fn room(me: PublicKey, members: &[PublicKey]) -> Vec<PublicKey> {
    let mut room: Vec<PublicKey> = members.iter().copied().chain([me]).collect();
    room.sort();
    room.dedup();
    room
}

/// What `wrap*` returns: one gift wrap per member, and the copy to self that
/// lets the sender's other devices see the conversation.
pub struct Wraps {
    pub members: Vec<(PublicKey, Event)>,
    pub to_self: Event,
}

/// A text message to `to` (one person, or the other members of a group).
pub fn wrap(sender: &Keys, to: &[PublicKey], text: &str) -> Result<Wraps> {
    if text.is_empty() {
        return Err(Error::Invalid("empty message".into()));
    }
    wrap_rumor(sender, to, Kind::PrivateDirectMessage, vec![], text)
}

/// Same, for an encrypted file already uploaded (kind 15, spec § 10).
pub fn wrap_file(sender: &Keys, to: &[PublicKey], file: &Attachment) -> Result<Wraps> {
    wrap_rumor(sender, to, FILE_MESSAGE, file.to_dm_tags()?, &file.url)
}

/// Names or renames a group: an empty message with a `subject` (spec § 5).
pub fn wrap_subject(sender: &Keys, to: &[PublicKey], subject: &str) -> Result<Wraps> {
    let subject = subject.trim();
    if subject.is_empty() || subject.chars().count() > MAX_SUBJECT {
        return Err(Error::Invalid(format!(
            "a group name has 1 to {MAX_SUBJECT} characters"
        )));
    }
    let tag = Tag::parse(["subject", subject])?;
    wrap_rumor(sender, to, Kind::PrivateDirectMessage, vec![tag], "")
}

/// Invites `to` to call `id`, joined through `url` (my instance), audio encrypted with `key`.
pub fn wrap_call(
    sender: &Keys,
    to: &[PublicKey],
    id: &str,
    url: &str,
    key: &[u8; 32],
) -> Result<Wraps> {
    let tag = Tag::parse(["call", id, url, &hex::encode(key)])?;
    wrap_rumor(sender, to, Kind::PrivateDirectMessage, vec![tag], CALL_TEXT)
}

pub fn wrap_call_end(sender: &Keys, to: &[PublicKey], id: &str) -> Result<Wraps> {
    let tag = Tag::parse(["call-end", id])?;
    wrap_rumor(
        sender,
        to,
        Kind::PrivateDirectMessage,
        vec![tag],
        CALL_END_TEXT,
    )
}

/// The `call` or `call-end` tag; `Err` when malformed.
fn call(tags: &nostr::prelude::Tags) -> std::result::Result<Option<Call>, ()> {
    let hex32 = |s: &str, len: usize| {
        s.len() == len
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    for tag in tags.iter() {
        match tag.as_slice() {
            [k, id, url, key, ..] if k == "call" => {
                let key: [u8; 32] = hex::decode(key)
                    .ok()
                    .and_then(|k| k.try_into().ok())
                    .ok_or(())?;
                if !hex32(id, 32) || !url.starts_with("http") {
                    return Err(());
                }
                let invite = CallInvite {
                    url: url.trim_end_matches('/').to_string(),
                    key,
                };
                return Ok(Some(Call {
                    id: id.clone(),
                    ended: false,
                    invite: Some(invite),
                }));
            }
            [k, id, ..] if k == "call-end" => {
                return if hex32(id, 32) {
                    Ok(Some(Call {
                        id: id.clone(),
                        ended: true,
                        invite: None,
                    }))
                } else {
                    Err(())
                };
            }
            [k, ..] if k == "call" => return Err(()),
            _ => {}
        }
    }
    Ok(None)
}

fn wrap_rumor(
    sender: &Keys,
    to: &[PublicKey],
    kind: Kind,
    tags: Vec<Tag>,
    content: &str,
) -> Result<Wraps> {
    let me = sender.public_key();
    let room = room(me, to);
    if room.len() < 2 || room.len() > MAX_ROOM {
        return Err(Error::Invalid(format!(
            "a conversation has 2 to {MAX_ROOM} members"
        )));
    }
    let others: Vec<PublicKey> = room.into_iter().filter(|pk| *pk != me).collect();
    let all = others.iter().map(|pk| Tag::public_key(*pk)).chain(tags);
    let rumor = event::builder(kind, all, content)?.finalize_unsigned(me);
    let members = others
        .into_iter()
        .map(|pk| {
            Ok((
                pk,
                GiftWrapBuilder::new(pk, rumor.clone()).finalize(sender)?,
            ))
        })
        .collect::<Result<_>>()?;
    Ok(Wraps {
        members,
        to_self: GiftWrapBuilder::new(me, rumor).finalize(sender)?,
    })
}

/// Opens a gift wrap addressed to `keys`. The seal signature, the rumor author
/// (must equal the seal signer) and the room (I'm in it, at most `MAX_ROOM`).
pub fn unwrap(keys: &Keys, gift_wrap: &Event) -> Result<Message> {
    let reject = || Error::Rejected(event::Reject::UnexpectedKind);
    let UnwrappedGift { sender, mut rumor } = UnwrappedGift::from_gift_wrap(keys, gift_wrap)?;
    let id = rumor.id(); // before touching the content
    let subject = rumor
        .tags
        .iter()
        .find(|t| t.as_slice().first().map(String::as_str) == Some("subject"))
        .and_then(|t| t.content())
        .map(|s| s.chars().take(MAX_SUBJECT).collect::<String>());
    let call = if rumor.kind == Kind::PrivateDirectMessage {
        call(&rumor.tags).map_err(|()| reject())?
    } else {
        None
    };
    let attachment = if rumor.kind == FILE_MESSAGE {
        let file =
            Attachment::from_dm_tags(&rumor.content, rumor.tags.iter()).ok_or_else(reject)?;
        rumor.content = String::new();
        Some(file)
    } else if call.is_some() {
        rumor.content = String::new(); // shown by other clients, not by ChatON
        None
    } else if rumor.kind == Kind::PrivateDirectMessage {
        if rumor.content.is_empty() && subject.is_none() {
            return Err(reject());
        }
        None
    } else {
        return Err(reject());
    };
    let members: Vec<PublicKey> = rumor.tags.public_keys().collect();
    let room = room(sender, &members);
    if room.len() < 2 || room.len() > MAX_ROOM || !room.contains(&keys.public_key()) {
        return Err(reject());
    }
    Ok(Message {
        id,
        sender,
        room,
        text: rumor.content,
        created_at: rumor.created_at,
        subject,
        attachment,
        call,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::nips::nip44::{self, Version};
    use nostr::prelude::EventBuilder;

    #[test]
    fn call_round_trip_keeps_the_key_from_the_ui() {
        let (alice, bob) = (Keys::generate(), Keys::generate());
        let (id, key) = ("0f".repeat(16), [0xef; 32]);
        let wraps =
            wrap_call(&alice, &[bob.public_key()], &id, "https://a.example/", &key).unwrap();
        let got = unwrap(&bob, &wraps.members[0].1).unwrap();
        let call = got.call.clone().unwrap();
        assert_eq!(
            (call.id.as_str(), call.ended, got.text.as_str()),
            (id.as_str(), false, "")
        );
        let invite = call.invite.unwrap();
        assert_eq!(
            (invite.url.as_str(), invite.key),
            ("https://a.example", key)
        );
        let json = serde_json::to_string(&got).unwrap();
        assert!(
            !json.contains(&"ef".repeat(32)) && !json.contains("a.example"),
            "{json}"
        );
        assert!(!format!("{got:?}").contains("239, 239"));

        // Rings for Bob for a minute, never for Alice.
        let now = got.created_at;
        assert!(rings(&got, bob.public_key(), now));
        assert!(!rings(&got, alice.public_key(), now));
        assert!(!rings(&got, bob.public_key(), now + RING_SECS));

        let end = wrap_call_end(&bob, &[alice.public_key()], &id).unwrap();
        let end = unwrap(&alice, &end.members[0].1).unwrap();
        assert_eq!(end.call.as_ref().map(|c| c.ended), Some(true));
        assert!(!rings(&end, alice.public_key(), end.created_at));
    }

    #[test]
    fn malformed_calls_are_ignored() {
        let (alice, bob) = (Keys::generate(), Keys::generate());
        let id = "0f".repeat(16);
        for tag in [
            vec!["call", "short", "https://a.example", &"ef".repeat(32)],
            vec!["call", &id, "https://a.example", "nothex"],
            vec!["call", &id, "ftp://a.example", &"ef".repeat(32)],
            vec!["call", &id],
            vec!["call-end", "NOTHEX"],
        ] {
            let rumor = event::builder(
                Kind::PrivateDirectMessage,
                [
                    Tag::public_key(bob.public_key()),
                    Tag::parse(tag.clone()).unwrap(),
                ],
                CALL_TEXT,
            )
            .unwrap()
            .finalize_unsigned(alice.public_key());
            let gift = GiftWrapBuilder::new(bob.public_key(), rumor)
                .finalize(&alice)
                .unwrap();
            assert!(unwrap(&bob, &gift).is_err(), "{tag:?} accepted");
        }
    }

    #[test]
    fn file_round_trip() {
        let (alice, bob) = (Keys::generate(), Keys::generate());
        let (_, mut file) = crate::media::encrypt(b"%PDF", "application/pdf", "cv.pdf").unwrap();
        file.url = format!("https://chaton.example/{}", file.sha256);
        let wraps = wrap_file(&alice, &[bob.public_key()], &file).unwrap();
        let got = unwrap(&bob, &wraps.members[0].1).unwrap();
        assert_eq!(got.attachment.as_ref(), Some(&file));
        assert_eq!(got.text, "");
        // Same rumor id on both copies, and it's the id of the kind 15 as sent.
        assert_eq!(unwrap(&alice, &wraps.to_self).unwrap().id, got.id);
    }

    #[test]
    fn round_trip_receiver_and_self_copy() {
        let (alice, bob, carol) = (Keys::generate(), Keys::generate(), Keys::generate());
        let wraps = wrap(&alice, &[bob.public_key()], "Salut Bob 👋").unwrap();
        let (to, to_bob) = &wraps.members[0];
        assert_eq!((*to, wraps.members.len()), (bob.public_key(), 1));

        let msg = unwrap(&bob, to_bob).unwrap();
        assert_eq!(msg.sender, alice.public_key());
        assert_eq!(msg.room, room(alice.public_key(), &[bob.public_key()]));
        assert_eq!(msg.text, "Salut Bob 👋");
        assert_eq!(msg.subject, None);
        assert_eq!(unwrap(&alice, &wraps.to_self).unwrap(), msg);

        assert!(unwrap(&carol, to_bob).is_err());
        // the gift wrap hides the sender and carries no version tag
        assert_ne!(to_bob.pubkey, alice.public_key());
        assert_eq!(to_bob.tags.len(), 1);
    }

    #[test]
    fn a_named_group_of_three() {
        let (alice, bob, carol, dave) = (
            Keys::generate(),
            Keys::generate(),
            Keys::generate(),
            Keys::generate(),
        );
        // Me and duplicates in the list don't count.
        let to = [
            bob.public_key(),
            carol.public_key(),
            alice.public_key(),
            bob.public_key(),
        ];
        let wraps = wrap_subject(&alice, &to, "  Projet ").unwrap();
        assert_eq!(wraps.members.len(), 2);

        let expected = room(alice.public_key(), &[bob.public_key(), carol.public_key()]);
        let mine = unwrap(&alice, &wraps.to_self).unwrap();
        assert_eq!(mine.room, expected);
        assert_eq!(mine.subject.as_deref(), Some("Projet"));
        assert_eq!(mine.text, "");
        for (pk, gift) in &wraps.members {
            let keys = if *pk == bob.public_key() {
                &bob
            } else {
                &carol
            };
            assert_eq!(unwrap(keys, gift).unwrap(), mine);
            assert!(unwrap(&dave, gift).is_err());
        }

        // Bob's reply lands in the same room.
        let reply = wrap(&bob, &mine.room, "ok").unwrap();
        assert_eq!(unwrap(&bob, &reply.to_self).unwrap().room, expected);
        assert!(wrap_subject(&alice, &to, " ").is_err());
        assert!(wrap_subject(&alice, &to, &"x".repeat(MAX_SUBJECT + 1)).is_err());
        assert!(wrap(&alice, &[alice.public_key()], "to myself").is_err());
    }

    #[test]
    fn rooms_are_capped_both_ways() {
        let alice = Keys::generate();
        let others: Vec<PublicKey> = (0..MAX_ROOM)
            .map(|_| Keys::generate().public_key())
            .collect();
        assert!(wrap(&alice, &others[..MAX_ROOM - 1], "ten of us").is_ok());
        assert!(wrap(&alice, &others, "eleven").is_err());

        // Built by hand: a rumor for 11 members is ignored on reception.
        let bob = Keys::generate();
        let tags = others.iter().map(|pk| Tag::public_key(*pk));
        let rumor = event::builder(
            Kind::PrivateDirectMessage,
            tags.chain([Tag::public_key(bob.public_key())]),
            "x",
        )
        .unwrap()
        .finalize_unsigned(alice.public_key());
        let gift = GiftWrapBuilder::new(bob.public_key(), rumor)
            .finalize(&alice)
            .unwrap();
        assert!(unwrap(&bob, &gift).is_err());
    }

    #[test]
    fn empty_text_needs_a_subject() {
        let (alice, bob) = (Keys::generate(), Keys::generate());
        assert!(wrap(&alice, &[bob.public_key()], "").is_err());
        let rumor = event::builder(
            Kind::PrivateDirectMessage,
            [Tag::public_key(bob.public_key())],
            "",
        )
        .unwrap()
        .finalize_unsigned(alice.public_key());
        let gift = GiftWrapBuilder::new(bob.public_key(), rumor)
            .finalize(&alice)
            .unwrap();
        assert!(unwrap(&bob, &gift).is_err());
    }

    // A seal signed by Alice around a rumor claiming to be from Carol.
    #[test]
    fn rejects_rumor_author_different_from_seal_signer() {
        let (alice, bob, carol, ephemeral) = (
            Keys::generate(),
            Keys::generate(),
            Keys::generate(),
            Keys::generate(),
        );
        let rumor = event::builder(Kind::PrivateDirectMessage, [], "forged")
            .unwrap()
            .finalize_unsigned(carol.public_key());
        let sealed = nip44::encrypt(
            alice.secret_key(),
            &bob.public_key(),
            rumor.as_json(),
            Version::V2,
        )
        .unwrap();
        let seal = EventBuilder::new(Kind::Seal, sealed)
            .finalize(&alice)
            .unwrap();
        let wrapped = nip44::encrypt(
            ephemeral.secret_key(),
            &bob.public_key(),
            seal.as_json(),
            Version::V2,
        )
        .unwrap();
        let gift = EventBuilder::new(Kind::GiftWrap, wrapped)
            .tag(Tag::public_key(bob.public_key()))
            .finalize(&ephemeral)
            .unwrap();

        assert!(unwrap(&bob, &gift).is_err());
    }
}
