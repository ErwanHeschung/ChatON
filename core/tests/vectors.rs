//! Go↔Rust contract: reproduce `spec/test-vectors/events.json` byte for byte.

use chaton_core::event::{Reject, validate};
use chaton_core::nostr::prelude::{
    Event, EventId, Kind, PublicKey, Signature, Tag, Tags, Timestamp,
};
use secp256k1::{Keypair, Secp256k1};
use serde_json::Value;

const VECTORS: &str = include_str!("../../spec/test-vectors/events.json");

fn vectors() -> Value {
    serde_json::from_str(VECTORS).unwrap()
}

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or_else(|| panic!("missing {key}"))
}

#[test]
fn valid_vectors_reproduce_serialization_id_and_sig() {
    let vs = vectors();
    let aux: [u8; 32] = hex(str_of(&vs, "aux_rand")).try_into().unwrap();
    let secp = Secp256k1::new();
    let valid = vs["valid"].as_array().unwrap();
    assert_eq!(valid.len(), 45);

    for v in valid {
        let name = str_of(v, "name");
        let e = &v["event"];
        let pubkey = PublicKey::from_hex(str_of(e, "pubkey")).unwrap();
        let created_at = Timestamp::from(e["created_at"].as_u64().unwrap());
        let kind = Kind::from(e["kind"].as_u64().unwrap() as u16);
        let tags = Tags::from_list(
            e["tags"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| {
                    Tag::parse(t.as_array().unwrap().iter().map(|s| s.as_str().unwrap())).unwrap()
                })
                .collect(),
        );
        let content = str_of(e, "content");

        // Same tuple nostr hashes in EventId::compute.
        let serialized =
            serde_json::to_string(&(0u8, &pubkey, &created_at, &kind, &tags, content)).unwrap();
        assert_eq!(serialized, str_of(v, "serialized"), "{name}: serialization");

        let id = EventId::compute(&pubkey, &created_at, &kind, &tags, content);
        assert_eq!(id.to_hex(), str_of(v, "id"), "{name}: id");

        let Some(want_sig) = v["sig"].as_str() else {
            assert!(
                kind == Kind::PrivateDirectMessage || kind == Kind::from(15),
                "{name}: only rumors (14, 15) have no sig"
            );
            continue;
        };
        let keypair = Keypair::from_seckey_str(&secp, str_of(v, "privkey")).unwrap();
        assert_eq!(
            PublicKey::from(keypair.x_only_public_key().0),
            pubkey,
            "{name}: pubkey from privkey"
        );
        let sig = secp.sign_schnorr_with_aux_rand(id.as_bytes(), &keypair, &aux);
        assert_eq!(sig.to_string(), want_sig, "{name}: sig");
        let sig = Signature::from_byte_array(sig.to_byte_array());

        let event = Event::new(id, pubkey, created_at, kind, tags, content, sig);
        assert_eq!(validate(&event, created_at), Ok(()), "{name}: validate");
    }
}

#[test]
fn invalid_vectors_are_rejected_for_the_expected_reason() {
    let vs = vectors();
    for v in vs["invalid"].as_array().unwrap() {
        let name = str_of(v, "name");
        let event = Event::from_json(v["event"].to_string()).unwrap();
        let want = match str_of(v, "reason") {
            "bad_id" => Reject::BadId,
            "bad_sig" => Reject::BadSig,
            "signed_rumor" => Reject::SignedRumor,
            other => panic!("{name}: unknown reason {other}"),
        };
        assert_eq!(validate(&event, event.created_at), Err(want), "{name}");
    }
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// spec/test-vectors/permissions.json: the same cases Go runs (perms_test.go).
#[test]
fn permission_vectors() {
    use chaton_core::perms::{Member, PERMISSIONS, Policy, parse_overrides, parse_roles};
    let v: Value =
        serde_json::from_str(include_str!("../../spec/test-vectors/permissions.json")).unwrap();
    let tags = |v: &Value| -> Vec<Vec<String>> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|t| {
                t.as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s.as_str().unwrap().to_string())
                    .collect()
            })
            .collect()
    };
    let strings = |v: &Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(strings(&v["permissions"]), PERMISSIONS);
    let policy = Policy {
        owner: str_of(&v, "owner").into(),
        roles: parse_roles(tags(&v["roles"])).expect("roles"),
    };
    for c in v["effective"].as_array().unwrap() {
        let roles = strings(&c["roles"]);
        let m = Member {
            pubkey: str_of(c, "member"),
            roles: &roles,
            timeout: c["timeout"].as_u64().unwrap(),
        };
        let overrides = (!c["channel"].is_null()).then(|| parse_overrides(tags(&c["channel"])));
        let got = policy
            .effective(&m, overrides.as_deref(), c["now"].as_u64().unwrap())
            .names();
        assert_eq!(got, strings(&c["expect"]), "{}", str_of(c, "name"));
    }
    for c in v["outranks"].as_array().unwrap() {
        let (ar, br) = (strings(&c["a"]["roles"]), strings(&c["b"]["roles"]));
        let a = Member {
            pubkey: str_of(&c["a"], "pubkey"),
            roles: &ar,
            timeout: 0,
        };
        let b = Member {
            pubkey: str_of(&c["b"], "pubkey"),
            roles: &br,
            timeout: 0,
        };
        assert_eq!(
            policy.outranks(&a, &b),
            c["expect"].as_bool().unwrap(),
            "{}",
            str_of(c, "name")
        );
    }
}
