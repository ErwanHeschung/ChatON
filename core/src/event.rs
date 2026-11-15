//! Building and validating events per `spec/protocol.md` (§1, §2, §6).

use nostr::prelude::{Event, EventBuilder, FinalizeEvent, Keys, Kind, Tag, Timestamp};

use crate::Result;

pub const PROTOCOL_VERSION: &str = "0";

/// Max `created_at` drift into the future (spec §6).
pub const MAX_FUTURE_SECS: u64 = 15 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    SignedRumor,
    BadId,
    BadSig,
    Future,
    /// A gift wrap whose rumor isn't a kind 14 message.
    UnexpectedKind,
}

/// Builder with the `["v","0"]` tag (except seal/gift wrap, NIP-59) and
/// control characters stripped from content and tags (spec §1).
pub fn builder<I>(kind: Kind, tags: I, content: &str) -> Result<EventBuilder>
where
    I: IntoIterator<Item = Tag>,
{
    let mut clean_tags = Vec::new();
    if kind != Kind::Seal && kind != Kind::GiftWrap {
        clean_tags.push(Tag::custom("v", [PROTOCOL_VERSION]));
    }
    for tag in tags {
        clean_tags.push(Tag::parse(tag.as_slice().iter().map(|s| clean(s)))?);
    }
    Ok(EventBuilder::new(kind, clean(content)).tags(clean_tags))
}

/// Builds and signs an event.
pub fn build<I>(keys: &Keys, kind: Kind, tags: I, content: &str) -> Result<Event>
where
    I: IntoIterator<Item = Tag>,
{
    Ok(builder(kind, tags, content)?.finalize(keys)?)
}

/// Checks a received event, in spec §6 order.
///
/// A kind 14 rumor is never a signed `Event`: if one arrives as such, it's rejected.
pub fn validate(event: &Event, now: Timestamp) -> std::result::Result<(), Reject> {
    if event.kind == Kind::PrivateDirectMessage || event.kind == crate::dm::FILE_MESSAGE {
        return Err(Reject::SignedRumor);
    }
    if !event.verify_id() {
        return Err(Reject::BadId);
    }
    if !event.verify_signature() {
        return Err(Reject::BadSig);
    }
    if event.created_at.as_secs() > now.as_secs() + MAX_FUTURE_SECS {
        return Err(Reject::Future);
    }
    Ok(())
}

/// Removes U+0000–U+001F except `\t \n \r`: NIP-01 leaves most of them
/// undefined, and some relays get the id of U+0008/U+000C wrong (spec § 1).
fn clean(s: &str) -> String {
    s.chars()
        .filter(|&c| c >= ' ' || matches!(c, '\t' | '\n' | '\r'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_adds_version_and_strips_control_chars() {
        let keys = Keys::generate();
        let ev = build(
            &keys,
            Kind::Metadata,
            [Tag::custom("t", ["a\u{1}\u{8}b"])],
            "hi\u{0}\tthere\u{c}\n",
        )
        .unwrap();
        assert_eq!(ev.content, "hi\tthere\n");
        let tags = ev.tags.clone().to_vec();
        assert_eq!(tags[0].as_slice(), ["v", "0"]);
        assert_eq!(tags[1].as_slice(), ["t", "ab"]);
        assert_eq!(validate(&ev, Timestamp::now()), Ok(()));
    }

    #[test]
    fn no_version_tag_on_seal_and_gift_wrap() {
        for kind in [Kind::Seal, Kind::GiftWrap] {
            let ev = build(&Keys::generate(), kind, [], "x").unwrap();
            assert!(ev.tags.is_empty(), "{kind:?}");
        }
    }

    #[test]
    fn rejects_far_future() {
        let ev = builder(Kind::Metadata, [], "")
            .unwrap()
            .custom_created_at(Timestamp::from(10_000))
            .finalize(&Keys::generate())
            .unwrap();
        assert_eq!(
            validate(&ev, Timestamp::from(10_000 - MAX_FUTURE_SECS)),
            Ok(())
        );
        assert_eq!(
            validate(&ev, Timestamp::from(10_000 - MAX_FUTURE_SECS - 1)),
            Err(Reject::Future)
        );
    }
}
