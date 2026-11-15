//! End to end against a running ChatON server (step 4 "done when"):
//!
//! ```sh
//! cd server && SERVER_PORT=:8099 SERVER_PUBLICURL=http://localhost:8099 go run ./cmd/server
//! cd core && CHATON_TEST_SERVER=http://localhost:8099 cargo test --test e2e -- --ignored
//! ```

use std::path::PathBuf;
use std::sync::{Arc, Once};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chaton_core::Error;
use chaton_core::link::{self, LinkOffer};
use chaton_core::nostr::prelude::ToBech32;
use chaton_core::session::Session;

fn unique(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    format!("{prefix}{nanos}")
}

/// Tests run in parallel: one mock keychain for all of them.
fn setup() -> (String, String) {
    static STORE: Once = Once::new();
    STORE.call_once(|| keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap()));
    let server = std::env::var("CHATON_TEST_SERVER").expect("CHATON_TEST_SERVER");
    let host = server
        .split("://")
        .nth(1)
        .unwrap()
        .trim_end_matches('/')
        .to_string();
    (server, host)
}

fn temp_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(unique(&format!("chaton-e2e-{name}-")))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running server, see the module docs"]
async fn two_users_find_each_other_and_exchange_dms() {
    let (server, host) = setup();

    let (alice_name, bob_name) = (unique("alice"), unique("bob"));
    let alice_dir = temp_dir("alice");
    let alice = Session::register(&alice_dir, &server, &alice_name)
        .await
        .unwrap();
    let bob = Arc::new(
        Session::register(&temp_dir("bob"), &server, &bob_name)
            .await
            .unwrap(),
    );
    assert_eq!(alice.account().handle, format!("{alice_name}@{host}"));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let listener = bob.clone();
    tokio::spawn(async move { listener.listen(move |m| drop(tx.send(m))).await });

    // Alice finds @bob and writes to him.
    let peer = alice.search(&format!("@{bob_name}@{host}")).await.unwrap();
    assert_eq!(peer.npub, bob.account().npub);
    let sent = alice.send_dm(peer.pubkey, "salut Bob 👋").await.unwrap();

    let got = tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("bob got nothing")
        .unwrap();
    assert_eq!((got.id, got.text.as_str()), (sent.id, "salut Bob 👋"));

    // Bob sees Alice's verified handle, not just a key, and replies.
    let alice_seen = bob.peer(got.sender).await;
    assert_eq!(
        alice_seen.handle.as_deref(),
        Some(alice.account().handle.as_str())
    );
    // Nostr timestamps are in seconds: same-second messages have no defined order.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    bob.send_dm(got.sender, "salut Alice").await.unwrap();

    let bob_convs = bob.conversations().await.unwrap();
    assert_eq!(bob_convs.len(), 1);
    assert_eq!(bob_convs[0].last.text, "salut Alice");

    // Alice's history (her own copy + Bob's reply) survives a restart.
    let mut history = Vec::new();
    for _ in 0..50 {
        history = alice.messages(peer.pubkey).await.unwrap();
        if history.len() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(history.len(), 2, "alice history: {history:?}");
    alice.close().await;
    let reopened = Session::open(&alice_dir).await.unwrap().expect("account");
    assert_eq!(reopened.account().npub, alice.account().npub);
    let texts: Vec<_> = reopened
        .messages(peer.pubkey)
        .await
        .unwrap()
        .into_iter()
        .map(|m| m.text)
        .collect();
    assert_eq!(texts, ["salut Bob 👋", "salut Alice"]);

    // The handle is taken, and a key that isn't Bob's can't log in as Bob.
    let taken = Session::register(&temp_dir("mallory"), &server, &alice_name).await;
    assert!(
        matches!(taken, Err(Error::HandleTaken)),
        "{:?}",
        taken.err()
    );
    let other = chaton_core::nostr::prelude::Keys::generate()
        .secret_key()
        .to_bech32()
        .unwrap();
    let wrong =
        Session::import_key(&temp_dir("mallory2"), &format!("{bob_name}@{host}"), &other).await;
    assert!(
        matches!(wrong, Err(Error::KeyMismatch)),
        "{:?}",
        wrong.err()
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running server, see the module docs"]
async fn a_second_device_links_with_a_code() {
    let (server, _) = setup();
    let (alice_name, bob_name) = (unique("alice"), unique("bob"));
    let alice = Arc::new(
        Session::register(&temp_dir("alice"), &server, &alice_name)
            .await
            .unwrap(),
    );
    let bob = Session::register(&temp_dir("bob"), &server, &bob_name)
        .await
        .unwrap();
    let handle = alice.account().handle.clone();
    bob.send_dm(alice.account().pubkey, "hello Alice")
        .await
        .unwrap();

    // Device A shows a code; the new device types it, A confirms.
    let offer = LinkOffer::new(&alice).await.unwrap();
    let code = offer.code();
    let a_side = tokio::spawn(offer.run(|name| async move { name == "laptop" }));
    // A human takes seconds to type the code: let A's subscription settle.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let laptop_dir = temp_dir("alice-laptop");
    let laptop = link::join(&laptop_dir, &handle, &code, "laptop")
        .await
        .unwrap();
    assert_eq!(a_side.await.unwrap().unwrap(), "laptop");
    assert_eq!(laptop.account().npub, alice.account().npub);
    let history = laptop.messages(bob.account().pubkey).await.unwrap();
    assert_eq!(history.len(), 1, "{history:?}");
    assert_eq!(history[0].text, "hello Alice");

    // What the laptop sends shows up live on device A (its self copy).
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let listener = alice.clone();
    tokio::spawn(async move { listener.listen(move |m| drop(tx.send(m))).await });
    tokio::time::sleep(Duration::from_millis(1100)).await;
    laptop
        .send_dm(bob.account().pubkey, "from my laptop")
        .await
        .unwrap();
    let got = tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("device A got nothing")
        .unwrap();
    assert_eq!(got.text, "from my laptop");

    // A wrong secret: both sides fail, A never asks to confirm, the code is burnt.
    let offer = LinkOffer::new(&alice).await.unwrap();
    let mut wrong = offer.code();
    let last = wrong.pop().unwrap();
    wrong.push(if last == '0' { '1' } else { '0' });
    let a_side = tokio::spawn(offer.run(|_| async { panic!("asked to confirm") }));
    tokio::time::sleep(Duration::from_millis(300)).await;
    let res = link::join(&temp_dir("mallory"), &handle, &wrong, "evil").await;
    assert!(matches!(res, Err(Error::WrongCode)), "{:?}", res.err());
    let res = a_side.await.unwrap();
    assert!(matches!(res, Err(Error::WrongCode)), "{res:?}");

    // Nobody showing that code: expired.
    let res = link::join(&temp_dir("mallory"), &handle, &code, "evil").await;
    assert!(matches!(res, Err(Error::CodeExpired)), "{:?}", res.err());

    // Logout forgets the account on the laptop; the exported key brings it back.
    let nsec = laptop.export_key();
    laptop.logout().await.unwrap();
    assert!(Session::open(&laptop_dir).await.unwrap().is_none());
    let back = Session::import_key(&laptop_dir, &handle, &nsec)
        .await
        .unwrap();
    assert_eq!(back.account().npub, alice.account().npub);
}
