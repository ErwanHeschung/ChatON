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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Message {
    /// Rumor id: the same for the receiver's copy and the sender's own copy.
    pub id: EventId,
    pub sender: PublicKey,
    pub receiver: PublicKey,
    pub text: String,
    pub created_at: Timestamp,
    /// A file (NIP-17 kind 15); `text` is then empty.
    pub attachment: Option<Attachment>,
}

impl Message {
    /// The other side of the conversation, seen from `me`.
    pub fn peer(&self, me: PublicKey) -> PublicKey {
        if self.sender == me {
            self.receiver
        } else {
            self.sender
        }
    }
}

/// Returns the two gift wraps to publish: `[to receiver, to self]`.
/// The copy to self lets the sender's other devices see the conversation.
pub fn wrap(sender: &Keys, receiver: PublicKey, text: &str) -> Result<[Event; 2]> {
    wrap_rumor(sender, receiver, Kind::PrivateDirectMessage, vec![], text)
}

/// Same, for an encrypted file already uploaded (kind 15, spec § 10).
pub fn wrap_file(sender: &Keys, receiver: PublicKey, file: &Attachment) -> Result<[Event; 2]> {
    wrap_rumor(
        sender,
        receiver,
        FILE_MESSAGE,
        file.to_dm_tags()?,
        &file.url,
    )
}

fn wrap_rumor(
    sender: &Keys,
    receiver: PublicKey,
    kind: Kind,
    tags: Vec<Tag>,
    content: &str,
) -> Result<[Event; 2]> {
    let all = std::iter::once(Tag::public_key(receiver)).chain(tags);
    let rumor = event::builder(kind, all, content)?.finalize_unsigned(sender.public_key());
    Ok([
        GiftWrapBuilder::new(receiver, rumor.clone()).finalize(sender)?,
        GiftWrapBuilder::new(sender.public_key(), rumor).finalize(sender)?,
    ])
}

/// Decrypts a gift wrap addressed to `keys`. `nostr` verifies the gift wrap and
/// seal signatures and that the rumor author is the seal signer.
pub fn unwrap(keys: &Keys, gift_wrap: &Event) -> Result<Message> {
    let UnwrappedGift { sender, mut rumor } = UnwrappedGift::from_gift_wrap(keys, gift_wrap)?;
    let id = rumor.id(); // before touching the content
    let attachment = if rumor.kind == FILE_MESSAGE {
        let file = Attachment::from_dm_tags(&rumor.content, rumor.tags.iter())
            .ok_or(Error::Rejected(event::Reject::UnexpectedKind))?;
        rumor.content = String::new();
        Some(file)
    } else if rumor.kind == Kind::PrivateDirectMessage {
        None
    } else {
        return Err(Error::Rejected(event::Reject::UnexpectedKind));
    };
    let receiver = rumor
        .tags
        .public_keys()
        .next()
        .ok_or(Error::Rejected(event::Reject::UnexpectedKind))?;
    Ok(Message {
        id,
        sender,
        receiver,
        text: rumor.content,
        created_at: rumor.created_at,
        attachment,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::nips::nip44::{self, Version};
    use nostr::prelude::EventBuilder;

    #[test]
    fn file_round_trip() {
        let (alice, bob) = (Keys::generate(), Keys::generate());
        let (_, mut file) = crate::media::encrypt(b"%PDF", "application/pdf", "cv.pdf").unwrap();
        file.url = format!("https://chaton.example/{}", file.sha256);
        let [to_bob, to_self] = wrap_file(&alice, bob.public_key(), &file).unwrap();
        let got = unwrap(&bob, &to_bob).unwrap();
        assert_eq!(got.attachment.as_ref(), Some(&file));
        assert_eq!(got.text, "");
        // Same rumor id on both copies, and it's the id of the kind 15 as sent.
        assert_eq!(unwrap(&alice, &to_self).unwrap().id, got.id);
    }

    #[test]
    fn round_trip_receiver_and_self_copy() {
        let (alice, bob, carol) = (Keys::generate(), Keys::generate(), Keys::generate());
        let [to_bob, to_self] = wrap(&alice, bob.public_key(), "Salut Bob 👋").unwrap();

        let msg = unwrap(&bob, &to_bob).unwrap();
        assert_eq!(msg.sender, alice.public_key());
        assert_eq!(msg.receiver, bob.public_key());
        assert_eq!(msg.peer(bob.public_key()), alice.public_key());
        assert_eq!(msg.peer(alice.public_key()), bob.public_key());
        assert_eq!(msg.text, "Salut Bob 👋");
        assert_eq!(unwrap(&alice, &to_self).unwrap(), msg);

        assert!(unwrap(&carol, &to_bob).is_err());
        // the gift wrap hides the sender and carries no version tag
        assert_ne!(to_bob.pubkey, alice.public_key());
        assert_eq!(to_bob.tags.len(), 1);
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
