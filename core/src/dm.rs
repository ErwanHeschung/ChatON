//! Private messages: NIP-17 (rumor 14 → seal 13 → gift wrap 1059). See spec §5.

use nostr::nips::nip59::{GiftWrapBuilder, UnwrappedGift};
use nostr::prelude::{
    Event, FinalizeEvent, FinalizeUnsignedEvent, Keys, Kind, PublicKey, Tag, Timestamp,
};

use crate::{Error, Result, event};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub sender: PublicKey,
    pub text: String,
    pub created_at: Timestamp,
}

/// Returns the two gift wraps to publish: `[to receiver, to self]`.
/// The copy to self lets the sender's other devices see the conversation.
pub fn wrap(sender: &Keys, receiver: PublicKey, text: &str) -> Result<[Event; 2]> {
    let rumor = event::builder(
        Kind::PrivateDirectMessage,
        [Tag::public_key(receiver)],
        text,
    )?
    .finalize_unsigned(sender.public_key());
    Ok([
        GiftWrapBuilder::new(receiver, rumor.clone()).finalize(sender)?,
        GiftWrapBuilder::new(sender.public_key(), rumor).finalize(sender)?,
    ])
}

/// Decrypts a gift wrap addressed to `keys`. `nostr` verifies the gift wrap and
/// seal signatures and that the rumor author is the seal signer.
pub fn unwrap(keys: &Keys, gift_wrap: &Event) -> Result<Message> {
    let UnwrappedGift { sender, rumor } = UnwrappedGift::from_gift_wrap(keys, gift_wrap)?;
    if rumor.kind != Kind::PrivateDirectMessage {
        return Err(Error::Rejected(event::Reject::UnexpectedKind));
    }
    Ok(Message {
        sender,
        text: rumor.content,
        created_at: rumor.created_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::nips::nip44::{self, Version};
    use nostr::prelude::EventBuilder;

    #[test]
    fn round_trip_receiver_and_self_copy() {
        let (alice, bob, carol) = (Keys::generate(), Keys::generate(), Keys::generate());
        let [to_bob, to_self] = wrap(&alice, bob.public_key(), "Salut Bob 👋").unwrap();

        let msg = unwrap(&bob, &to_bob).unwrap();
        assert_eq!(msg.sender, alice.public_key());
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
