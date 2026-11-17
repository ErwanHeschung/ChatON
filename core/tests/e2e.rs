//! End to end against a running ChatON server (step 4 "done when"):
//!
//! ```sh
//! cd server && SERVER_PORT=:8099 SERVER_PUBLICURL=http://localhost:8099 SERVER_PUSHALLOWPRIVATE=1 go run ./cmd/server
//! cd core && CHATON_TEST_SERVER=http://localhost:8099 cargo test --test e2e -- --ignored
//! ```
//!
//! Two instances (federation, step 10): a second server on :8098.
//!
//! ```sh
//! cd server && SERVER_PORT=:8099 SERVER_COUNTRY=FR … (the first one, as above, plus its country)
//! cd server && SERVER_PORT=:8098 SERVER_PUBLICURL=http://localhost:8098 SERVER_DBPATH=b.bolt SERVER_PUSHALLOWPRIVATE=1 go run ./cmd/server
//! cd core && CHATON_TEST_SERVER=http://localhost:8099 CHATON_TEST_SERVER_B=http://localhost:8098 cargo test --test e2e two_instances -- --ignored
//! ```
//!
//! Voice (feature `voice`): LiveKit's webhooks go to :8080 (`server/livekit.dev.yaml`).
//!
//! ```sh
//! npm run dev:livekit
//! cd server && SERVER_LIVEKITURL=ws://localhost:7880 SERVER_LIVEKITKEY=devkey \
//!   SERVER_LIVEKITSECRET=chaton-dev-secret-not-for-production go run ./cmd/server
//! cd core && CHATON_TEST_SERVER=http://localhost:8080 cargo test --features voice --test e2e voice -- --ignored
//! ```

use std::path::PathBuf;
use std::sync::{Arc, Once};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chaton_core::Error;
use chaton_core::community::Update;
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

/// Forwards the DMs of a `listen` to `tx`.
fn dms(tx: &tokio::sync::mpsc::UnboundedSender<chaton_core::dm::Message>, update: Update) {
    if let Update::Dm { message, .. } = update {
        drop(tx.send(message));
    }
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
    tokio::spawn(async move { listener.listen(move |u| dms(&tx, u)).await });

    // Alice finds @bob and writes to him.
    let peer = alice.search(&format!("@{bob_name}@{host}")).await.unwrap();
    assert_eq!(peer.npub, bob.account().npub);
    let sent = alice.send_dm(&[peer.pubkey], "salut Bob 👋").await.unwrap();

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
    bob.send_dm(&[got.sender], "salut Alice").await.unwrap();

    let bob_convs = bob.conversations().await.unwrap();
    assert_eq!(bob_convs.len(), 1);
    assert_eq!(bob_convs[0].last.text, "salut Alice");

    // Alice's history (her own copy + Bob's reply) survives a restart.
    let mut history = Vec::new();
    for _ in 0..50 {
        history = alice.messages(&[peer.pubkey]).await.unwrap();
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
        .messages(&[peer.pubkey])
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
    bob.send_dm(&[alice.account().pubkey], "hello Alice")
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
    let history = laptop.messages(&[bob.account().pubkey]).await.unwrap();
    assert_eq!(history.len(), 1, "{history:?}");
    assert_eq!(history[0].text, "hello Alice");

    // What the laptop sends shows up live on device A (its self copy).
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let listener = alice.clone();
    tokio::spawn(async move { listener.listen(move |u| dms(&tx, u)).await });
    tokio::time::sleep(Duration::from_millis(1100)).await;
    laptop
        .send_dm(&[bob.account().pubkey], "from my laptop")
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

/// Waits for an update matching `pred`, skipping the others.
async fn next_matching(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<Update>,
    what: &str,
    pred: impl Fn(&Update) -> bool,
) -> Update {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let update = rx.recv().await.expect("listener stopped");
            if pred(&update) {
                return update;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no update: {what}"))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running server, see the module docs"]
async fn a_community_with_channels_invites_and_moderation() {
    let (server, host) = setup();
    let register = |name: &'static str| {
        let server = server.clone();
        async move {
            Arc::new(
                Session::register(&temp_dir(name), &server, &unique(name))
                    .await
                    .unwrap(),
            )
        }
    };
    let (alice, bob, carol) = (
        register("alice").await,
        register("bob").await,
        register("carol").await,
    );

    // Alice listens from the start, like the app.
    let (tx, mut alice_rx) = tokio::sync::mpsc::unbounded_channel();
    let listener = alice.clone();
    tokio::spawn(async move { listener.listen(move |u| drop(tx.send(u))).await });

    // Alice creates a community: she's admin, it comes with #general; she adds #random.
    let community = alice.create_community("Les chats").await.unwrap();
    assert_eq!(community.name, "Les chats");
    assert_eq!(community.roles, ["admin"]);
    let names = |c: &chaton_core::community::Community| -> Vec<String> {
        c.channels.iter().map(|ch| ch.name.clone()).collect()
    };
    assert_eq!(names(&community), ["general"]);
    let random = alice
        .create_channel(&community.id, "random", false)
        .await
        .unwrap();
    let general = community.channels[0].clone();

    // Bob can't join without the right code; with the invite he sees both channels.
    let invite = alice.invite(&community.id).await.unwrap();
    assert!(
        invite.starts_with(&format!("{host}'{}/", community.id)),
        "{invite}"
    );
    let wrong = format!("{host}'{}/00000000", community.id);
    let res = bob.join(&wrong).await;
    assert!(matches!(res, Err(Error::InvalidInvite)), "{:?}", res.err());
    let joined = bob.join(&invite).await.unwrap();
    assert_eq!(names(&joined), ["general", "random"]);
    assert!(joined.roles.is_empty());
    carol.join(&invite).await.unwrap();

    // Live chat: Bob writes in #random, Alice (listening since the start) gets it.
    let hello = bob
        .send(&community.id, &random.id, "salut tout le monde")
        .await
        .unwrap();
    let got = next_matching(&mut alice_rx, "bob's message", |u| {
        matches!(u, Update::ChannelMessage { .. })
    })
    .await;
    let Update::ChannelMessage { message } = got else {
        unreachable!()
    };
    assert_eq!(
        (message.id, message.text.as_str()),
        (hello.id, "salut tout le monde")
    );
    assert_eq!(message.author, bob.account().pubkey);

    // Bob can't delete Alice's message; once moderator he can.
    let alices = alice
        .send(&community.id, &general.id, "règle n°1")
        .await
        .unwrap();
    let res = bob
        .delete_message(&community.id, &general.id, alices.id)
        .await;
    assert!(matches!(res, Err(Error::NotDelivered(_))), "{res:?}");
    alice
        .set_roles(&community.id, bob.account().pubkey, &["moderator"])
        .await
        .unwrap();
    let members = alice.members(&community.id).await.unwrap();
    let role = |pk| {
        members
            .iter()
            .find(|m| m.pubkey == pk)
            .map(|m| m.roles.clone())
    };
    assert_eq!(
        role(bob.account().pubkey),
        Some(vec!["moderator".to_string()])
    );
    assert_eq!(role(carol.account().pubkey), Some(vec![]));
    bob.delete_message(&community.id, &general.id, alices.id)
        .await
        .unwrap();
    next_matching(
        &mut alice_rx,
        "deletion",
        |u| matches!(u, Update::Deleted { ids, .. } if ids.contains(&alices.id)),
    )
    .await;
    let general_msgs = alice.channel_messages(&general.id).await.unwrap();
    assert!(general_msgs.is_empty(), "{general_msgs:?}");
    let random_msgs = alice.channel_messages(&random.id).await.unwrap();
    assert_eq!(random_msgs.len(), 1);

    // Alice removes Carol: her posts are refused, the community leaves her list.
    alice
        .remove_member(&community.id, carol.account().pubkey)
        .await
        .unwrap();
    let res = carol
        .send(&community.id, &general.id, "toujours là ?")
        .await;
    assert!(matches!(res, Err(Error::NotDelivered(_))), "{res:?}");
    let mut left = false;
    for _ in 0..50 {
        left = carol.communities().await.unwrap().is_empty();
        if left {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(left, "carol still lists the community");

    // Alice's linked laptop finds the community through her list (10009).
    let offer = LinkOffer::new(&alice).await.unwrap();
    let code = offer.code();
    let a_side = tokio::spawn(offer.run(|_| async { true }));
    tokio::time::sleep(Duration::from_millis(300)).await;
    let laptop = link::join(
        &temp_dir("alice-laptop"),
        &alice.account().handle,
        &code,
        "laptop",
    )
    .await
    .unwrap();
    a_side.await.unwrap().unwrap();
    let on_laptop = laptop.communities().await.unwrap();
    assert_eq!(on_laptop.len(), 1, "{on_laptop:?}");
    assert_eq!(names(&on_laptop[0]), ["general", "random"]);
    let history = laptop.channel_messages(&random.id).await.unwrap();
    assert_eq!(history.len(), 1, "{history:?}");
}

/// Polls `f` until it returns `Some`, for at most 5 s.
async fn eventually<T, F, Fut>(what: &str, f: F) -> T
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Option<T>>,
{
    for _ in 0..50 {
        if let Some(v) = f().await {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("never happened: {what}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running server, see the module docs"]
async fn files_in_channels_dms_and_pictures() {
    use chaton_core::media::{Attachment, MAX_SIZE};
    let (server, _) = setup();
    let alice = Session::register(&temp_dir("alice"), &server, &unique("alice"))
        .await
        .unwrap();
    let bob = Session::register(&temp_dir("bob"), &server, &unique("bob"))
        .await
        .unwrap();
    let community = alice.create_community("Photos").await.unwrap();
    let general = community.channels[0].id.clone();
    bob.join(&alice.invite(&community.id).await.unwrap())
        .await
        .unwrap();
    let file: Vec<u8> = (0..300_000u32).map(|i| (i * 7 % 251) as u8).collect();

    // Channel: Alice posts an encrypted file, Bob gets the same bytes.
    let sent = alice
        .send_file(
            &community.id,
            &general,
            &file,
            "image/png",
            "chat.png",
            "regardez",
        )
        .await
        .unwrap();
    let att = sent.attachment.clone().expect("attachment");
    assert_eq!(
        (att.name.as_str(), att.size),
        ("chat.png", file.len() as u64)
    );
    let got: Attachment = eventually("bob sees the file", || async {
        let msgs = bob.channel_messages(&general).await.ok()?;
        msgs.into_iter().find(|m| m.id == sent.id)?.attachment
    })
    .await;
    assert_eq!(got, att);
    assert_eq!(bob.open_attachment(&got, false).await.unwrap(), file);

    // The server only has ciphertext; a wrong key or hash doesn't decrypt.
    let stored = reqwest::get(&att.url).await.unwrap().bytes().await.unwrap();
    assert_ne!(&stored[..], &file[..]);
    let wrong_key = Attachment {
        key: "00".repeat(32),
        ..att.clone()
    };
    assert!(bob.open_attachment(&wrong_key, false).await.is_err());
    let wrong_hash = Attachment {
        sha256: "00".repeat(32),
        ..att.clone()
    };
    assert!(bob.open_attachment(&wrong_hash, false).await.is_err());

    // Not from my instance nor a community's: only on an explicit click.
    let elsewhere = Attachment {
        url: format!("https://elsewhere.example/{}", att.sha256),
        ..att.clone()
    };
    let res = bob.open_attachment(&elsewhere, false).await;
    assert!(matches!(res, Err(Error::NotAllowed)), "{res:?}");

    // Too large: refused before any upload.
    let res = alice
        .send_file(
            &community.id,
            &general,
            &vec![0; MAX_SIZE + 1],
            "a/b",
            "big",
            "",
        )
        .await;
    assert!(matches!(res, Err(Error::TooLarge)), "{res:?}");

    // DM: kind 15, same round trip.
    let pdf = b"%PDF-1.7 pretend".to_vec();
    let dm = alice
        .send_dm_file(&[bob.account().pubkey], &pdf, "application/pdf", "cv.pdf")
        .await
        .unwrap();
    let got = eventually("bob gets the DM file", || async {
        let msgs = bob.messages(&[alice.account().pubkey]).await.ok()?;
        msgs.into_iter().find(|m| m.id == dm.id)?.attachment
    })
    .await;
    assert_eq!(got.name, "cv.pdf");
    assert_eq!(bob.open_attachment(&got, false).await.unwrap(), pdf);

    // Pictures: public, shown when hosted by their own instance.
    let png = b"\x89PNG\r\n\x1a\n pretend".to_vec();
    alice
        .set_community_picture(&community.id, png.clone(), "image/png")
        .await
        .unwrap();
    let url = eventually("bob sees the community picture", || async {
        bob.communities().await.ok()?.into_iter().next()?.picture
    })
    .await;
    assert_eq!(bob.picture(&url).await.unwrap(), png);
    alice
        .set_profile_picture(png.clone(), "image/png")
        .await
        .unwrap();
    let alice_seen = bob.peer(alice.account().pubkey).await;
    let url = alice_seen.picture.expect("alice's picture");
    assert_eq!(bob.picture(&url).await.unwrap(), png);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running server, see the module docs"]
async fn a_named_group_of_three() {
    let (server, _) = setup();
    let register = |name: &'static str| {
        let server = server.clone();
        async move {
            Session::register(&temp_dir(name), &server, &unique(name))
                .await
                .unwrap()
        }
    };
    let (alice, bob, carol) = (
        register("alice").await,
        register("bob").await,
        register("carol").await,
    );
    let (pk_alice, pk_bob, pk_carol) = (
        alice.account().pubkey,
        bob.account().pubkey,
        carol.account().pubkey,
    );

    // Creating the group is naming it: everyone sees it right away.
    let named = alice
        .name_group(&[pk_bob, pk_carol], "Projet")
        .await
        .unwrap();
    assert_eq!(named.room.len(), 3);
    for (who, session) in [("bob", &bob), ("carol", &carol)] {
        let conv = eventually(&format!("{who} sees the group"), || async {
            let convs = session.conversations().await.ok()?;
            convs.into_iter().find(|c| c.room == named.room)
        })
        .await;
        assert_eq!(conv.subject.as_deref(), Some("Projet"));
    }

    // Bob replies to the room; carol gets both, in order.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let reply = bob.send_dm(&named.room, "je suis là").await.unwrap();
    assert_eq!(reply.room, named.room);
    let file = alice
        .send_dm_file(&[pk_bob, pk_carol], b"plan", "text/plain", "plan.txt")
        .await
        .unwrap();
    let got = eventually("carol gets the file", || async {
        let msgs = carol.messages(&[pk_alice, pk_bob]).await.ok()?;
        (msgs.len() == 3).then_some(msgs)
    })
    .await;
    // Same-second messages have no defined order: match by id.
    let by_id = |id| got.iter().find(|m| m.id == id).expect("message");
    assert_eq!(by_id(reply.id).text, "je suis là");
    let att = by_id(file.id).attachment.clone().expect("attachment");
    assert_eq!(carol.open_attachment(&att, false).await.unwrap(), b"plan");

    // Renaming: the latest name wins, for everyone.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    alice.name_group(&named.room, "Projet 2").await.unwrap();
    eventually("bob sees the new name", || async {
        let convs = bob.conversations().await.ok()?;
        let conv = convs.into_iter().find(|c| c.room == named.room)?;
        (conv.subject.as_deref() == Some("Projet 2")).then_some(())
    })
    .await;

    // A 1:1 with bob is its own conversation.
    alice.send_dm(&[pk_bob], "rien qu'à toi").await.unwrap();
    let convs = eventually("bob has two conversations", || async {
        let convs = bob.conversations().await.ok()?;
        (convs.len() == 2).then_some(convs)
    })
    .await;
    // Same second as the rename: no defined order, find it by its room.
    let one_to_one = convs.iter().find(|c| c.room.len() == 2).expect("1:1");
    assert_eq!(one_to_one.last.text, "rien qu'à toi");

    // 11 members: refused before sending.
    let crowd: Vec<_> = (0..10)
        .map(|_| chaton_core::nostr::prelude::Keys::generate().public_key())
        .collect();
    let res = alice.send_dm(&crowd, "trop").await;
    assert!(matches!(res, Err(Error::Invalid(_))), "{res:?}");
}

/// A stand-in push service on 127.0.0.1: answers 201 and reports each push.
async fn push_service() -> (String, tokio::sync::mpsc::UnboundedReceiver<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/up/bob", listener.local_addr().unwrap());
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Ok((mut conn, _)) = listener.accept().await {
            let mut buf = vec![0; 16 * 1024];
            drop(conn.read(&mut buf).await);
            drop(
                conn.write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 0\r\n\r\n")
                    .await,
            );
            tx.send(()).ok();
        }
    });
    (url, rx)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running server, see the module docs"]
async fn a_dm_wakes_the_other_device_by_push() {
    use chaton_core::push::Endpoint;
    let (server, _) = setup();
    let alice = Session::register(&temp_dir("alice"), &server, &unique("alice"))
        .await
        .unwrap();
    let bob_dir = temp_dir("bob");
    let bob = Session::register(&bob_dir, &server, &unique("bob"))
        .await
        .unwrap();
    let (pk_alice, pk_bob) = (alice.account().pubkey, bob.account().pubkey);

    let vapid = bob.vapid_key().await.unwrap();
    assert_eq!(vapid.len(), 87, "{vapid}");
    let (url, mut pushes) = push_service().await;
    // P-256 generator point: a valid key, the content is never read here.
    let endpoint = Endpoint {
        endpoint: url,
        p256dh: "BGsX0fLhLEJH-Lzm5WOkQPJ3A32BLeszoPShOUXYmMKWT-NC4v4af5uO5-tKfA-eFivOM1drMV7Oy7ZAaDe_UfU".into(),
        auth: "AAECAwQFBgcICQoLDA0ODw".into(),
    };
    bob.push_register(&endpoint)
        .await
        .expect("SERVER_PUSHALLOWPRIVATE=1 on the server?");

    // Bob writes: his self copy comes from his own device, no push.
    bob.send_dm(&[pk_alice], "salut").await.unwrap();
    let early = tokio::time::timeout(Duration::from_millis(1500), pushes.recv()).await;
    assert!(early.is_err(), "bob's own message woke him");

    // Alice writes: bob's phone is woken.
    alice.send_dm(&[pk_bob], "réveille-toi").await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), pushes.recv())
        .await
        .expect("no push")
        .unwrap();

    // Logout unregisters this device.
    assert!(bob_dir.join("push.json").exists());
    bob.logout().await.unwrap();
    assert!(!bob_dir.join("push.json").exists());
}

#[cfg(feature = "voice")]
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running server with LiveKit, see the module docs"]
async fn voice_channel_presence_and_an_encrypted_call() {
    use chaton_core::voice::VoiceTarget;

    let (server, _) = setup();
    let register = |name: &'static str| {
        let server = server.clone();
        async move {
            Arc::new(
                Session::register(&temp_dir(name), &server, &unique(name))
                    .await
                    .unwrap(),
            )
        }
    };
    let (alice, bob) = (register("alice").await, register("bob").await);
    let listen = |s: &Arc<Session>| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let listener = s.clone();
        tokio::spawn(async move { listener.listen(move |u| drop(tx.send(u))).await });
        rx
    };
    let (mut alice_rx, mut bob_rx) = (listen(&alice), listen(&bob));

    // A voice channel; Bob sees who's in it without joining.
    let community = alice.create_community("Vocal").await.unwrap();
    assert!(alice.voice_available(&community.id).await.unwrap());
    let channel = alice
        .create_channel(&community.id, "Salon", true)
        .await
        .unwrap();
    bob.join(&alice.invite(&community.id).await.unwrap())
        .await
        .unwrap();
    let state = alice
        .join_voice_channel(&community.id, &channel.id, false)
        .await
        .unwrap();
    assert!(!state.encrypted && !state.microphone);
    assert_eq!(state.participants, [alice.account().pubkey]);
    let in_channel = |s: Arc<Session>| {
        let (community, channel) = (community.id.clone(), channel.id.clone());
        async move {
            let all = s.communities().await.ok()?;
            let c = all.into_iter().find(|c| c.id == community)?;
            let ch = c.channels.into_iter().find(|ch| ch.id == channel)?;
            assert!(ch.voice);
            Some(ch.participants)
        }
    };
    let present = eventually("alice in the channel", || {
        let bob = bob.clone();
        async move { in_channel(bob).await.filter(|p| !p.is_empty()) }
    })
    .await;
    assert_eq!(present, [alice.account().pubkey]);

    // Bob joins: Alice hears of him, mute and deafen are hers alone.
    bob.join_voice_channel(&community.id, &channel.id, false)
        .await
        .unwrap();
    next_matching(
        &mut alice_rx,
        "bob in alice's room",
        |u| matches!(u, Update::Voice { state: Some(s) } if s.participants.len() == 2),
    )
    .await;
    let st = alice.set_deafened(true).await.unwrap();
    assert!(st.deafened && !bob.voice_state().await.unwrap().deafened);
    alice.leave_voice().await.unwrap();
    bob.leave_voice().await.unwrap();
    assert!(alice.voice_state().await.is_none());

    // An encrypted call: it rings for Bob, he joins, the last one out ends it.
    // Bob wrote to Alice first: a call from a stranger (a request) doesn't ring.
    bob.send_dm(&[alice.account().pubkey], "on s'appelle ?")
        .await
        .unwrap();
    let state = alice
        .start_call(&[bob.account().pubkey], false)
        .await
        .unwrap();
    assert!(state.encrypted);
    let VoiceTarget::Call { room, id } = state.target.clone() else {
        panic!("{state:?}")
    };
    let Update::Ring { message, .. } = next_matching(&mut bob_rx, "the call rings", |u| {
        matches!(u, Update::Ring { .. })
    })
    .await
    else {
        unreachable!()
    };
    assert_eq!(message.call.as_ref().unwrap().id, id);
    let joined = bob.join_call(&room, &id, false).await.unwrap();
    assert!(joined.encrypted);
    next_matching(
        &mut alice_rx,
        "bob in the call",
        |u| matches!(u, Update::Voice { state: Some(s) } if s.participants.len() == 2),
    )
    .await;

    // Alice shares her screen (a test pattern): Bob watches it, encrypted too.
    let me = alice.account().pubkey;
    alice.share_test_pattern().await.unwrap();
    next_matching(
        &mut bob_rx,
        "alice shares",
        |u| matches!(u, Update::Voice { state: Some(s) } if s.sharing == [me]),
    )
    .await;
    let (tx, mut images) = tokio::sync::mpsc::unbounded_channel();
    let sink: chaton_core::voice::Sink = Arc::new(move |jpeg| drop(tx.send(jpeg)));
    let st = bob.watch_screen(Some((me, sink))).await.unwrap();
    assert_eq!(st.watching, Some(me));
    let jpeg = tokio::time::timeout(Duration::from_secs(20), images.recv())
        .await
        .expect("an image of alice's screen")
        .unwrap();
    assert_eq!(jpeg[..2], [0xFF, 0xD8]);
    // Alice sees her own share.
    let (tx, mut mine) = tokio::sync::mpsc::unbounded_channel();
    let sink: chaton_core::voice::Sink = Arc::new(move |jpeg| drop(tx.send(jpeg)));
    alice.watch_screen(Some((me, sink))).await.unwrap();
    let jpeg = tokio::time::timeout(Duration::from_secs(20), mine.recv())
        .await
        .expect("an image of her own screen")
        .unwrap();
    assert_eq!(jpeg[..2], [0xFF, 0xD8]);
    alice.stop_screen_share().await.unwrap();
    next_matching(&mut bob_rx, "alice stopped sharing", |u| {
        matches!(u, Update::Voice { state: Some(s) } if s.sharing.is_empty() && s.watching.is_none())
    })
    .await;
    // A 1:1 call ends for both when one hangs up: Alice gets the end and leaves.
    bob.leave_voice().await.unwrap();
    next_matching(&mut alice_rx, "the call ended", |u| {
        matches!(u, Update::Dm { message, .. } if message.call.as_ref().is_some_and(|c| c.ended && c.id == id))
    })
    .await;
    next_matching(&mut alice_rx, "alice out of the call", |u| {
        matches!(u, Update::Voice { state: None })
    })
    .await;
    assert!(alice.voice_state().await.is_none());
    next_matching(&mut bob_rx, "bob's copy of the end", |u| {
        matches!(u, Update::Dm { message, .. } if message.call.as_ref().is_some_and(|c| c.ended && c.id == id))
    })
    .await;
}

type Dms = tokio::sync::mpsc::UnboundedReceiver<(chaton_core::dm::Message, bool)>;

async fn next(rx: &mut Dms) -> (chaton_core::dm::Message, bool) {
    tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("nothing came")
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs two running servers, see the module docs"]
async fn two_instances_requests_blocks_and_a_move() {
    let (server_a, host_a) = setup();
    let server_b = std::env::var("CHATON_TEST_SERVER_B").expect("CHATON_TEST_SERVER_B");
    let host_b = server_b.split("://").nth(1).unwrap().trim_end_matches('/');

    let (alice_dir, bob_dir) = (temp_dir("alice"), temp_dir("bob"));
    let alice = Session::register(&alice_dir, &server_a, &unique("alice"))
        .await
        .unwrap();
    let bob_name = unique("bob");
    let bob = Arc::new(
        Session::register(&bob_dir, &server_b, &bob_name)
            .await
            .unwrap(),
    );
    let listen = |s: Arc<Session>| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            s.listen(move |u| {
                if let Update::Dm { message, request } = u {
                    drop(tx.send((message, request)));
                }
            })
            .await
        });
        rx
    };

    alice
        .set_display_name("  Alice\n la chatte ")
        .await
        .unwrap();

    // Alice (A) writes to Bob (B), a stranger: a request, from a verified alice@A.
    let mut bob_rx = listen(bob.clone());
    let to_bob = alice.search(&format!("{bob_name}@{host_b}")).await.unwrap();
    alice
        .send_dm(&[to_bob.pubkey], "bonjour d'ailleurs")
        .await
        .unwrap();
    let (got, request) = next(&mut bob_rx).await;
    assert_eq!((got.text.as_str(), request), ("bonjour d'ailleurs", true));
    assert_eq!(
        bob.peer(got.sender).await.handle.as_deref(),
        Some(alice.account().handle.as_str())
    );
    assert!(bob.conversations().await.unwrap()[0].request);
    // Her display name, read on her instance's relay.
    let seen = bob.peer(got.sender).await;
    assert_eq!(seen.display_name.as_deref(), Some("Alice la chatte"));
    let alice_pk = got.sender;

    // Bob restarts (nothing in memory) and replies: found through her 10050.
    bob.close().await;
    let bob = Arc::new(Session::open(&bob_dir).await.unwrap().unwrap());
    let mut bob_rx = listen(bob.clone());
    let alice = Arc::new(alice);
    let mut alice_rx = listen(alice.clone());
    tokio::time::sleep(Duration::from_millis(1100)).await;
    bob.send_dm(&[alice_pk], "bien reçu").await.unwrap();
    let (got, request) = next(&mut alice_rx).await;
    assert_eq!((got.text.as_str(), request), ("bien reçu", false));
    assert!(!bob.conversations().await.unwrap()[0].request);

    // Bob's network: his instance, and A where Alice is (A says FR, if started with SERVER_COUNTRY=FR).
    let network = bob.instances().await.unwrap();
    assert!(network[0].mine && network[0].domain == host_b && network[0].reachable);
    let a = network
        .iter()
        .find(|i| i.domain == host_a.to_lowercase())
        .expect("A on the map");
    assert!(
        a.reachable && !a.mine && !a.blocked && a.people == [alice_pk],
        "{a:?}"
    );

    // Bob blocks instance A: Alice's next message is dropped, unblocking shows it.
    let blocks = bob.block_instance(&host_a).await.unwrap();
    assert_eq!(blocks.instances, [host_a.to_lowercase()]);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    alice
        .send_dm(&[to_bob.pubkey], "tu m'entends ?")
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(3), bob_rx.recv())
            .await
            .is_err(),
        "a blocked instance got through"
    );
    let last =
        |s: Arc<Session>| async move { s.conversations().await.unwrap()[0].last.text.clone() };
    assert_eq!(last(bob.clone()).await, "bien reçu");
    bob.unblock_instance(&host_a).await.unwrap();
    assert_eq!(last(bob.clone()).await, "tu m'entends ?");
    bob.block_key(alice_pk).await.unwrap();
    assert_eq!(last(bob.clone()).await, "bien reçu");
    bob.unblock_key(alice_pk).await.unwrap();

    // Alice moves to B; Bob, after a restart, writes to her new home.
    let new_name = unique("alice");
    let moved = alice.move_to(&server_b, &new_name).await.unwrap();
    assert_eq!(moved.handle, format!("{new_name}@{host_b}"));
    alice.close().await;
    let alice = Arc::new(Session::open(&alice_dir).await.unwrap().unwrap());
    assert_eq!(alice.account().handle, moved.handle);
    assert_eq!(
        alice.messages(&[to_bob.pubkey]).await.unwrap().len(),
        3,
        "history kept"
    );
    let mut alice_rx = listen(alice.clone());
    bob.close().await;
    let bob = Session::open(&bob_dir).await.unwrap().unwrap();
    let seen = bob.peer(alice_pk).await;
    assert_eq!(seen.handle.as_deref(), Some(moved.handle.as_str()));
    assert_eq!(
        seen.display_name.as_deref(),
        Some("Alice la chatte"),
        "kept by the move"
    );
    tokio::time::sleep(Duration::from_millis(1100)).await;
    bob.send_dm(&[alice_pk], "bienvenue sur B").await.unwrap();
    let (got, _) = next(&mut alice_rx).await;
    assert_eq!(got.text, "bienvenue sur B");
}
