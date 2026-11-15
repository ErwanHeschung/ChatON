//! End to end against a running ChatON server (step 4 "done when"):
//!
//! ```sh
//! cd server && SERVER_PORT=:8099 SERVER_PUBLICURL=http://localhost:8099 go run ./cmd/server
//! cd core && CHATON_TEST_SERVER=http://localhost:8099 cargo test --test e2e -- --ignored
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chaton_core::Error;
use chaton_core::nostr::prelude::ToBech32;
use chaton_core::session::Session;

fn unique(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    format!("{prefix}{nanos}")
}

fn temp_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(unique(&format!("chaton-e2e-{name}-")))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running server, see the module docs"]
async fn two_users_find_each_other_and_exchange_dms() {
    let server = std::env::var("CHATON_TEST_SERVER").expect("CHATON_TEST_SERVER");
    let host = server.split("://").nth(1).unwrap().trim_end_matches('/');
    keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());

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
    let wrong = Session::login(&temp_dir("mallory2"), &format!("{bob_name}@{host}"), &other).await;
    assert!(
        matches!(wrong, Err(Error::KeyMismatch)),
        "{:?}",
        wrong.err()
    );
}
