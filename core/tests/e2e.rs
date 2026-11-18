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
    // The owner has every permission (spec § 9).
    assert_eq!(community.owner, Some(alice.account().pubkey));
    assert_eq!(community.perms.len(), chaton_core::perms::PERMISSIONS.len());
    let names = |c: &chaton_core::community::Community| -> Vec<String> {
        c.channels.iter().map(|ch| ch.name.clone()).collect()
    };
    assert_eq!(names(&community), ["general"]);
    let random = alice
        .create_channel(&community.id, "random", false, None)
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
    // A plain member: @everyone's permissions, nothing to moderate with.
    let can = |c: &chaton_core::community::Community, p: &str| c.perms.iter().any(|x| x == p);
    assert!(can(&joined, "send_messages") && !can(&joined, "kick_members"));
    assert!(joined.channels[0].perms.iter().any(|p| p == "view_channel"));
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
    let Update::ChannelMessage { message, .. } = got else {
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
    let promoted = bob.communities().await.unwrap();
    assert!(can(&promoted[0], "kick_members") && !can(&promoted[0], "manage_roles"));
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

    // Alice adds a Gardien role (kick_members, listed apart) and describes the community.
    let mut roles = alice.communities().await.unwrap()[0].all_roles.clone();
    roles.insert(
        2,
        chaton_core::perms::Role {
            id: "gardien".into(),
            name: "Gardien".into(),
            color: "#e5a50a".into(),
            attrs: vec!["hoist".into(), "kick_members".into()],
        },
    );
    let edit = chaton_core::community::CommunityEdit {
        about: Some("Pour les chats".into()),
        roles: Some(roles),
        ..Default::default()
    };
    alice.update_community(&community.id, edit).await.unwrap();
    let edited = &alice.communities().await.unwrap()[0];
    assert_eq!(edited.about.as_deref(), Some("Pour les chats"));
    assert_eq!(edited.all_roles[2].id, "gardien");
    alice
        .set_roles(&community.id, bob.account().pubkey, &["gardien"])
        .await
        .unwrap();
    // The relay's 39001 comes after the OK: wait for it.
    let bob_pk = bob.account().pubkey;
    let bob_as_member = eventually("bob is gardien", || {
        let (alice, id) = (&alice, community.id.clone());
        async move {
            let m = alice
                .members(&id)
                .await
                .ok()?
                .into_iter()
                .find(|m| m.pubkey == bob_pk)?;
            (m.hoist.as_deref() == Some("gardien")).then_some(m)
        }
    })
    .await;
    assert_eq!(bob_as_member.hoist.as_deref(), Some("gardien"));
    assert_eq!(bob_as_member.color.as_deref(), Some("#e5a50a"));

    // Bob, Gardien, can't remove the owner nor give himself more, but removes Carol:
    // her posts are refused, the community leaves her list.
    let res = bob
        .remove_member(&community.id, alice.account().pubkey, "")
        .await;
    assert!(matches!(res, Err(Error::NotDelivered(_))), "{res:?}");
    let grab = chaton_core::community::CommunityEdit {
        owner: Some(bob.account().pubkey),
        ..Default::default()
    };
    let res = bob.update_community(&community.id, grab).await;
    assert!(matches!(res, Err(Error::NotDelivered(_))), "{res:?}");
    bob.remove_member(&community.id, carol.account().pubkey, "")
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
        .set_community_picture(&community.id, png.clone(), "image/png", false)
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
        .create_channel(&community.id, "Salon", true, None)
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

/// Step 12d (spec § 9, Salons et catégories), against two instances (see two_instances above).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs two running servers, see the module docs"]
async fn channels_categories_and_a_foreign_admin() {
    use chaton_core::community::{Category, ChannelEdit, CommunityEdit, Placement};
    let (server_a, _) = setup();
    let server_b = std::env::var("CHATON_TEST_SERVER_B").expect("CHATON_TEST_SERVER_B");
    let alice = Arc::new(
        Session::register(&temp_dir("alice"), &server_a, &unique("alice"))
            .await
            .unwrap(),
    );
    // Alice listens, like the app: her store follows what the others change.
    let listener = alice.clone();
    tokio::spawn(async move { listener.listen(|_| {}).await });
    let carol = Session::register(&temp_dir("carol"), &server_a, &unique("carol"))
        .await
        .unwrap();
    let bob = Session::register(&temp_dir("bob"), &server_b, &unique("bob"))
        .await
        .unwrap();

    // Bob, from instance B, joins Alice's community on A and becomes an admin.
    let community = alice.create_community("Les chats").await.unwrap();
    let invite = alice.invite(&community.id).await.unwrap();
    bob.join(&invite).await.unwrap();
    carol.join(&invite).await.unwrap();
    alice
        .set_roles(&community.id, bob.account().pubkey, &["admin"])
        .await
        .unwrap();

    // Bob and Alice each add a channel at the same time: the relay lists both.
    let (from_b, from_a) = tokio::join!(
        bob.create_channel(&community.id, "de-b", false, None),
        alice.create_channel(&community.id, "de-a", true, None),
    );
    let (from_b, from_a) = (from_b.unwrap(), from_a.unwrap());
    let channels = eventually("both channels listed", || async {
        let c = alice.communities().await.ok()?.pop()?;
        let ids: Vec<String> = c.channels.iter().map(|ch| ch.id.clone()).collect();
        (ids.contains(&from_b.id) && ids.contains(&from_a.id)).then_some(c.channels)
    })
    .await;
    assert_eq!(channels.len(), 3, "{channels:?}");
    let general = channels[0].clone();

    // Alice puts the voice channel in its category, first.
    let edit = CommunityEdit {
        categories: Some(vec![Category {
            id: "vocal".into(),
            name: "Salons vocaux".into(),
        }]),
        channels: Some(vec![
            Placement {
                id: from_a.id.clone(),
                category: Some("vocal".into()),
            },
            Placement {
                id: general.id.clone(),
                category: None,
            },
        ]),
        ..Default::default()
    };
    alice.update_community(&community.id, edit).await.unwrap();
    let arranged = alice.communities().await.unwrap().pop().unwrap();
    assert_eq!(arranged.categories[0].name, "Salons vocaux");
    let order: Vec<(&str, Option<&str>)> = arranged
        .channels
        .iter()
        .map(|ch| (ch.id.as_str(), ch.category.as_deref()))
        .collect();
    // de-b, left out of the edit, is kept at the end.
    assert_eq!(
        order,
        [
            (from_a.id.as_str(), Some("vocal")),
            (general.id.as_str(), None),
            (from_b.id.as_str(), None)
        ]
    );

    // Topic and slow mode on #general: Carol waits, Bob (admin) doesn't.
    let edit = ChannelEdit {
        topic: Some("On parle de chats".into()),
        slowmode: Some(60),
        ..Default::default()
    };
    alice
        .edit_channel(&community.id, &general.id, edit)
        .await
        .unwrap();
    let general = alice.communities().await.unwrap().pop().unwrap().channels[1].clone();
    assert_eq!(
        (general.topic.as_deref(), general.slowmode),
        (Some("On parle de chats"), 60)
    );
    carol.send(&community.id, &general.id, "un").await.unwrap();
    let res = carol.send(&community.id, &general.id, "deux").await;
    assert!(
        matches!(&res, Err(Error::NotDelivered(r)) if r.contains("slow-mode")),
        "{res:?}"
    );
    bob.send(&community.id, &general.id, "un").await.unwrap();
    bob.send(&community.id, &general.id, "deux").await.unwrap();
    // Nobody writes in a voice channel.
    let res = bob.send(&community.id, &from_a.id, "allô").await;
    assert!(matches!(res, Err(Error::NotDelivered(_))), "{res:?}");

    // Bob deletes his channel: it's gone from the community.
    bob.delete_channel(&community.id, &from_b.id).await.unwrap();
    eventually("channel deleted", || async {
        let c = alice.communities().await.ok()?.pop()?;
        c.channels.iter().all(|ch| ch.id != from_b.id).then_some(())
    })
    .await;
}

/// Step 12e (spec § 9, Salons privés): a channel only some see, history included.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running server, see the module docs"]
async fn a_private_channel() {
    use chaton_core::community::{ChannelEdit, ChannelOverride};
    let (server, _) = setup();
    let alice = Session::register(&temp_dir("alice"), &server, &unique("alice"))
        .await
        .unwrap();
    let bob = Arc::new(
        Session::register(&temp_dir("bob"), &server, &unique("bob"))
            .await
            .unwrap(),
    );
    let listener = bob.clone();
    tokio::spawn(async move { listener.listen(|_| {}).await });

    let community = alice.create_community("Les chats").await.unwrap();
    let invite = alice.invite(&community.id).await.unwrap();
    bob.join(&invite).await.unwrap();
    let staff = alice
        .create_channel(&community.id, "staff", false, None)
        .await
        .unwrap();
    let private = ChannelEdit {
        overrides: Some(vec![
            ChannelOverride {
                target: "everyone".into(),
                allow: vec![],
                deny: vec!["view_channel".into()],
            },
            ChannelOverride {
                target: "moderator".into(),
                allow: vec!["view_channel".into()],
                deny: vec![],
            },
        ]),
        ..Default::default()
    };
    alice
        .edit_channel(&community.id, &staff.id, private)
        .await
        .unwrap();
    alice
        .send(&community.id, &staff.id, "entre nous")
        .await
        .unwrap();
    let mine = alice.communities().await.unwrap().pop().unwrap();
    let ch = mine.channels.iter().find(|c| c.id == staff.id).unwrap();
    assert!(ch.private && ch.overrides.len() == 2, "{ch:?}");

    // Bob, a plain member, doesn't see it.
    let visible = |s: Arc<Session>, id: String| async move {
        let c = s.communities().await.ok()?.pop()?;
        Some(c.channels.iter().any(|ch| ch.id == id))
    };
    eventually("staff hidden from bob", || async {
        (visible(bob.clone(), staff.id.clone()).await == Some(false)).then_some(())
    })
    .await;
    assert!(bob.channel_messages(&staff.id).await.unwrap().is_empty());

    // Moderator: the channel and its whole history.
    alice
        .set_roles(&community.id, bob.account().pubkey, &["moderator"])
        .await
        .unwrap();
    let history = eventually("bob reads staff", || async {
        let m = bob.channel_messages(&staff.id).await.ok()?;
        (visible(bob.clone(), staff.id.clone()).await == Some(true) && !m.is_empty()).then_some(m)
    })
    .await;
    assert_eq!(history[0].text, "entre nous");

    // Back to member: gone again.
    alice
        .set_roles(&community.id, bob.account().pubkey, &[])
        .await
        .unwrap();
    eventually("staff hidden again", || async {
        (visible(bob.clone(), staff.id.clone()).await == Some(false)).then_some(())
    })
    .await;
}

/// Step 12f (spec § 9, Modération): timeouts, bans, the audit log.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running server, see the module docs"]
async fn bans_timeouts_and_the_audit_log() {
    use chaton_core::nostr::prelude::Timestamp;
    let (server, _) = setup();
    let alice = Session::register(&temp_dir("alice"), &server, &unique("alice"))
        .await
        .unwrap();
    let bob = Session::register(&temp_dir("bob"), &server, &unique("bob"))
        .await
        .unwrap();
    let carol = Arc::new(
        Session::register(&temp_dir("carol"), &server, &unique("carol"))
            .await
            .unwrap(),
    );
    let listener = carol.clone();
    tokio::spawn(async move { listener.listen(|_| {}).await });
    let community = alice.create_community("Les chats").await.unwrap();
    let general = alice.communities().await.unwrap()[0].channels[0].clone();
    let invite = alice.invite(&community.id).await.unwrap();
    bob.join(&invite).await.unwrap();
    carol.join(&invite).await.unwrap();
    let carol_pk = carol.account().pubkey;
    alice
        .set_roles(&community.id, bob.account().pubkey, &["moderator"])
        .await
        .unwrap();

    // Bob times Carol out: she reads, she doesn't write.
    let hour = Timestamp::now().as_secs() + 3600;
    bob.timeout(&community.id, carol_pk, hour, "On se calme")
        .await
        .unwrap();
    let mine = eventually("carol sees her timeout", || async {
        let c = carol.communities().await.ok()?.pop()?;
        (c.timeout == hour).then_some(c)
    })
    .await;
    assert!(
        !mine.perms.contains(&"send_messages".to_string()),
        "{:?}",
        mine.perms
    );
    let member = alice
        .members(&community.id)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.pubkey == carol_pk)
        .unwrap();
    assert_eq!(member.timeout, hour);
    let res = carol.send(&community.id, &general.id, "chut").await;
    assert!(matches!(res, Err(Error::NotDelivered(_))), "{res:?}");
    bob.timeout(&community.id, carol_pk, 0, "").await.unwrap();
    eventually("timeout lifted", || async {
        (carol.communities().await.ok()?.pop()?.timeout == 0).then_some(())
    })
    .await;
    let message = carol
        .send(&community.id, &general.id, "pardon")
        .await
        .unwrap();
    alice
        .delete_message(&community.id, &general.id, message.id)
        .await
        .unwrap();

    // Banned, Carol can't come back; unbanned, she can.
    bob.ban(&community.id, carol_pk, "Spam").await.unwrap();
    assert!(carol.join(&invite).await.is_err());
    let bans = alice.bans(&community.id).await.unwrap();
    assert_eq!(
        bans.iter()
            .map(|b| (b.pubkey, b.reason.as_deref()))
            .collect::<Vec<_>>(),
        [(carol_pk, Some("Spam"))]
    );
    bob.unban(&community.id, carol_pk).await.unwrap();
    assert!(alice.bans(&community.id).await.unwrap().is_empty());
    // A second identical 9021 within the same second is the same event (same id): wait.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    carol.join(&invite).await.unwrap();

    // The log (same-second actions come in any order).
    let log = alice.audit_log(&community.id).await.unwrap();
    let entry = |action: &str, until: u64| {
        log.iter()
            .find(|e| e.action == action && e.until == until)
            .unwrap_or_else(|| panic!("no {action} in {log:?}"))
    };
    assert_eq!(entry("ban", 0).reason.as_deref(), Some("Spam"));
    assert_eq!(entry("unban", 0).target, Some(carol_pk));
    assert_eq!(entry("delete_message", 0).target, Some(carol_pk));
    assert_eq!(
        entry("timeout", hour).reason.as_deref(),
        Some("On se calme")
    );
    assert_eq!(entry("timeout", 0).target, Some(carol_pk));
    assert_eq!(entry("roles", 0).roles, ["moderator"]);
    assert!(log.windows(2).all(|w| w[0].at >= w[1].at), "newest first");
    // Message deletions stay readable in their channel (to hide the messages); the rest isn't.
    let hers = carol.audit_log(&community.id).await.unwrap();
    assert!(
        hers.iter().all(|e| e.action == "delete_message"),
        "{hers:?}"
    );
}

/// Step 12g (spec § 9, Invitations): limited invites, listed, revoked.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running server, see the module docs"]
async fn invites_listed_used_up_and_revoked() {
    use chaton_core::nostr::prelude::Timestamp;
    let (server, _) = setup();
    let alice = Session::register(&temp_dir("alice"), &server, &unique("alice"))
        .await
        .unwrap();
    let bob = Session::register(&temp_dir("bob"), &server, &unique("bob"))
        .await
        .unwrap();
    let carol = Session::register(&temp_dir("carol"), &server, &unique("carol"))
        .await
        .unwrap();
    let community = alice.create_community("Les chats").await.unwrap();
    let week = Timestamp::now().as_secs() + 7 * 86400;
    let once = alice.invite_with(&community.id, week, 1).await.unwrap();
    let open = alice.invite(&community.id).await.unwrap();

    let listed = alice.invites(&community.id).await.unwrap();
    assert_eq!(listed.len(), 2, "{listed:?}");
    let limited = listed.iter().find(|i| i.text == once).unwrap();
    assert_eq!(
        (limited.expires, limited.max_uses, limited.uses),
        (week, 1, 0)
    );

    // One use: Bob gets in, Carol doesn't, and it leaves the list.
    bob.join(&once).await.unwrap();
    assert!(matches!(carol.join(&once).await, Err(Error::InvalidInvite)));
    let listed = alice.invites(&community.id).await.unwrap();
    assert_eq!(listed.iter().map(|i| &i.text).collect::<Vec<_>>(), [&open]);

    // Revoked: gone, and refused.
    let code = listed[0].code.clone();
    alice.revoke_invite(&community.id, &code).await.unwrap();
    assert!(alice.invites(&community.id).await.unwrap().is_empty());
    assert!(matches!(carol.join(&open).await, Err(Error::InvalidInvite)));
}

/// Step 12h (spec § 11, Modération vocale): mute, move, disconnect.
#[cfg(feature = "voice")]
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running server with LiveKit, see the module docs"]
async fn voice_moderation_mute_move_disconnect() {
    use base64::Engine;
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
    let (tx, mut bob_rx) = tokio::sync::mpsc::unbounded_channel();
    let listener = bob.clone();
    tokio::spawn(async move { listener.listen(move |u| drop(tx.send(u))).await });
    let community = alice.create_community("Vocal").await.unwrap();
    let hall = alice
        .create_channel(&community.id, "Salon", true, None)
        .await
        .unwrap();
    let corner = alice
        .create_channel(&community.id, "Coin", true, None)
        .await
        .unwrap();
    bob.join(&alice.invite(&community.id).await.unwrap())
        .await
        .unwrap();
    let bob_pk = bob.account().pubkey;
    bob.join_voice_channel(&community.id, &hall.id, false)
        .await
        .unwrap();
    // The channel as Alice sees it: (participants, muted).
    let seen = |id: String| {
        let (alice, community) = (alice.clone(), community.id.clone());
        async move {
            let all = alice.communities().await.ok()?;
            let ch = all
                .into_iter()
                .find(|c| c.id == community)?
                .channels
                .into_iter()
                .find(|ch| ch.id == id)?;
            Some((ch.participants, ch.muted))
        }
    };
    eventually("bob in the hall", || {
        let seen = seen(hall.id.clone());
        async move { seen.await.filter(|(p, _)| p.contains(&bob_pk)) }
    })
    .await;

    // Members can't.
    assert!(matches!(
        bob.moderate_voice(
            &community.id,
            &hall.id,
            &alice.account().pubkey,
            "mute",
            None
        )
        .await,
        Err(Error::NotAllowed)
    ));

    // Mute: everyone sees it, and Bob's next token has no microphone.
    alice
        .moderate_voice(&community.id, &hall.id, &bob_pk, "mute", None)
        .await
        .unwrap();
    eventually("bob shown muted", || {
        let seen = seen(hall.id.clone());
        async move { seen.await.filter(|(_, m)| m == &[bob_pk]) }
    })
    .await;
    let video = |token: String| {
        let payload = token.split('.').nth(1).unwrap().to_string();
        let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .unwrap();
        serde_json::from_slice::<serde_json::Value>(&raw).unwrap()["video"].clone()
    };
    let token = bob.channel_token(&community.id, &hall.id).await.unwrap();
    assert_eq!(
        video(token.participant_token)["canPublishSources"],
        serde_json::json!(["screen_share"])
    );
    alice
        .moderate_voice(&community.id, &hall.id, &bob_pk, "unmute", None)
        .await
        .unwrap();
    eventually("bob unmuted", || {
        let seen = seen(hall.id.clone());
        async move { seen.await.filter(|(_, m)| m.is_empty()) }
    })
    .await;

    // Move: Bob is told, and joins the corner (what the app does).
    alice
        .moderate_voice(&community.id, &hall.id, &bob_pk, "move", Some(&corner.id))
        .await
        .unwrap();
    let corner_id = corner.id.clone();
    let Update::VoiceMove {
        community: c,
        channel,
    } = next_matching(&mut bob_rx, "bob told to move", |u| {
        matches!(u, Update::VoiceMove { .. })
    })
    .await
    else {
        unreachable!()
    };
    assert_eq!(
        (c.as_str(), channel.as_str()),
        (community.id.as_str(), corner_id.as_str())
    );
    let st = bob.join_voice_channel(&c, &channel, false).await.unwrap();
    assert!(matches!(&st.target, VoiceTarget::Channel { channel, .. } if *channel == corner_id));
    eventually("bob in the corner", || {
        let seen = seen(corner.id.clone());
        async move { seen.await.filter(|(p, _)| p == &[bob_pk]) }
    })
    .await;

    // Disconnect: Bob is out.
    alice
        .moderate_voice(&community.id, &corner.id, &bob_pk, "disconnect", None)
        .await
        .unwrap();
    next_matching(&mut bob_rx, "bob disconnected", |u| {
        matches!(u, Update::Voice { state: None })
    })
    .await;
    assert!(bob.voice_state().await.is_none());
}

/// Step 12i (spec § 9, Messages): a reply points to its parent, a reaction is
/// counted then taken back, a member can't pin, a moderator can.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running server, see the module docs"]
async fn replies_reactions_and_pins() {
    let (server, _) = setup();
    let register = |name: &'static str| {
        let server = server.clone();
        async move {
            let s = Arc::new(
                Session::register(&temp_dir(name), &server, &unique(name))
                    .await
                    .unwrap(),
            );
            let listener = s.clone();
            tokio::spawn(async move { listener.listen(|_| {}).await });
            s
        }
    };
    let (alice, bob) = (register("alice").await, register("bob").await);
    let community = alice.create_community("Les chats").await.unwrap();
    let general = alice.communities().await.unwrap()[0].channels[0].id.clone();
    bob.join(&alice.invite(&community.id).await.unwrap())
        .await
        .unwrap();
    let hi = alice
        .send(&community.id, &general, "Qui a faim ?")
        .await
        .unwrap();
    // What `who` sees of message `id`, once it's there.
    let seen = |who: Arc<Session>, id| {
        let general = general.clone();
        async move {
            who.channel_messages(&general)
                .await
                .ok()?
                .into_iter()
                .find(|m| m.id == id)
        }
    };
    eventually("bob has alice's message", || seen(bob.clone(), hi.id)).await;

    // A reply: it points to its parent, for everyone.
    let reply = bob
        .reply(&community.id, &general, "Moi !", hi.id)
        .await
        .unwrap();
    let got = eventually("alice has the reply", || seen(alice.clone(), reply.id)).await;
    let parent = got.reply_to.unwrap();
    assert_eq!((parent.id, parent.author), (hi.id, alice.account().pubkey));

    // A reaction: counted, mine for Bob only, then taken back.
    bob.react(&community.id, &general, hi.id, "🐱")
        .await
        .unwrap();
    let got = eventually("alice sees the reaction", || {
        let seen = seen(alice.clone(), hi.id);
        async move { seen.await.filter(|m| !m.reactions.is_empty()) }
    })
    .await;
    assert_eq!(got.reactions.len(), 1);
    let r = &got.reactions[0];
    assert_eq!((r.emoji.as_str(), r.count, r.mine), ("🐱", 1, false));
    assert!(seen(bob.clone(), hi.id).await.unwrap().reactions[0].mine);
    assert!(
        bob.react(&community.id, &general, hi.id, "🐱")
            .await
            .is_err()
    );
    bob.unreact(&community.id, &general, hi.id, "🐱")
        .await
        .unwrap();
    eventually("the reaction is gone", || {
        let seen = seen(alice.clone(), hi.id);
        async move { seen.await.filter(|m| m.reactions.is_empty()) }
    })
    .await;

    // Pins: not for members; Alice's shows for Bob.
    assert!(
        bob.set_pinned(&community.id, &general, hi.id, true)
            .await
            .is_err()
    );
    alice
        .set_pinned(&community.id, &general, hi.id, true)
        .await
        .unwrap();
    eventually("bob sees the pin", || {
        let seen = seen(bob.clone(), hi.id);
        async move { seen.await.filter(|m| m.pinned) }
    })
    .await;
    alice
        .set_pinned(&community.id, &general, hi.id, false)
        .await
        .unwrap();
    eventually("bob sees it unpinned", || {
        let seen = seen(bob.clone(), hi.id);
        async move { seen.await.filter(|m| !m.pinned) }
    })
    .await;
}

/// Step 12j (spec § 9, Messages, « Non-lus et notifications »): a mention
/// counts, marking read clears, my level decides what notifies.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a running server, see the module docs"]
async fn mentions_unreads_and_notify() {
    use chaton_core::reads::NotifyLevel;
    let (server, _) = setup();
    let new = |name: &'static str| {
        let server = server.clone();
        async move {
            Arc::new(
                Session::register(&temp_dir(name), &server, &unique(name))
                    .await
                    .unwrap(),
            )
        }
    };
    let (alice, bob) = (new("alice").await, new("bob").await);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let listener = alice.clone();
    tokio::spawn(async move { listener.listen(|u| drop(tx.send(u))).await });
    let community = alice.create_community("Les chats").await.unwrap();
    let general = alice.communities().await.unwrap()[0].channels[0].id.clone();
    bob.join(&alice.invite(&community.id).await.unwrap())
        .await
        .unwrap();
    // The history so far counts as read; read marks are to the second.
    let state = || async {
        let c = alice.communities().await.unwrap().remove(0);
        let ch = c.channels.iter().find(|ch| ch.id == general).unwrap();
        (ch.unread, ch.mentions, c.notify)
    };
    assert_eq!(state().await, (false, 0, NotifyLevel::Mentions));
    tokio::time::sleep(Duration::from_millis(1100)).await;
    // What Alice's listener says of message `id`: (mention, notify).
    let mut heard = async |id| loop {
        match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await {
            Ok(Some(Update::ChannelMessage { message, notify })) if message.id == id => {
                return (message.mention, notify);
            }
            Ok(Some(_)) => {}
            other => panic!("no update for {id}: {other:?}"),
        }
    };

    let plain = bob.send(&community.id, &general, "Salut").await.unwrap();
    assert_eq!(heard(plain.id).await, (false, None));
    let me = vec![alice.account().pubkey.to_hex()];
    let ping = bob
        .post(&community.id, &general, "@alice regarde", None, &me)
        .await
        .unwrap();
    assert_eq!(ping.mentions, me);
    assert_eq!(
        heard(ping.id).await,
        (true, Some("#general · Les chats".to_string()))
    );
    assert_eq!(state().await, (true, 1, NotifyLevel::Mentions));
    let shown = alice.channel_messages(&general).await.unwrap();
    let flag = |id| shown.iter().find(|m| m.id == id).unwrap().mention;
    assert!(flag(ping.id) && !flag(plain.id));
    let everyone = ["everyone".to_string()];
    assert!(
        bob.post(&community.id, &general, "@everyone", None, &everyone)
            .await
            .is_err()
    );

    alice.mark_read(&general).await.unwrap();
    assert_eq!(state().await, (false, 0, NotifyLevel::Mentions));

    // Nothing notifies; a mention still counts.
    alice
        .set_notify_level(&community.id, NotifyLevel::None)
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let again = bob
        .post(&community.id, &general, "@alice ?", None, &me)
        .await
        .unwrap();
    assert_eq!(heard(again.id).await, (true, None));
    assert_eq!(state().await, (true, 1, NotifyLevel::None));
}

/// Step 12k (spec § 12): those who wrote to me, and my communities' members,
/// show online at their status, live (well within a tick); invisible and a
/// block hide them.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn presence_in_dms_and_communities() {
    use chaton_core::presence::Status;
    use std::collections::HashMap;
    let (server, _) = setup();
    let new = |name: &'static str| {
        let server = server.clone();
        async move {
            Arc::new(
                Session::register(&temp_dir(name), &server, &unique(name))
                    .await
                    .unwrap(),
            )
        }
    };
    let (alice, bob, carol) = (new("alice").await, new("bob").await, new("carol").await);
    let a = alice.account().pubkey;
    let watch = |s: &Arc<Session>| {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let listener = s.clone();
        tokio::spawn(async move {
            listener
                .listen(|u| {
                    if let Update::Presence { online } = u {
                        drop(tx.send(online));
                    }
                })
                .await
        });
        rx
    };
    type Online = tokio::sync::mpsc::UnboundedReceiver<
        HashMap<chaton_core::nostr::prelude::PublicKey, Status>,
    >;
    // The tick is 30 s: within 5 s, it came in live.
    let sees = async |rx: &mut Online, status: Option<Status>| loop {
        match tokio::time::timeout(Duration::from_secs(5), rx.recv()).await {
            Ok(Some(online)) if online.get(&a).copied() == status => return,
            Ok(Some(_)) => {}
            other => panic!("never saw Alice {status:?}: {other:?}"),
        }
    };
    let (mut at_bob, mut at_carol) = (watch(&bob), watch(&carol));
    alice.set_status(Status::Online).await.unwrap();
    // Bob doesn't know Alice's presence key until she writes to him.
    assert_eq!(bob.presence_tick().await.unwrap().get(&a), None);
    alice
        .send_dm(&[bob.account().pubkey], "coucou")
        .await
        .unwrap();
    sees(&mut at_bob, Some(Status::Online)).await;
    alice.set_status(Status::Dnd).await.unwrap();
    sees(&mut at_bob, Some(Status::Dnd)).await;
    alice.set_away(true).await.unwrap(); // dnd stays dnd
    alice.set_status(Status::Online).await.unwrap();
    sees(&mut at_bob, Some(Status::Idle)).await;
    alice.set_away(false).await.unwrap();
    sees(&mut at_bob, Some(Status::Online)).await;

    // Carol never wrote to Alice: she sees her through their community.
    let community = alice.create_community("Les chats").await.unwrap();
    carol
        .join(&alice.invite(&community.id).await.unwrap())
        .await
        .unwrap();
    alice.set_status(Status::Online).await.unwrap();
    sees(&mut at_carol, Some(Status::Online)).await;

    // A block changes Alice's key: Bob loses her, Carol doesn't.
    alice.block_key(bob.account().pubkey).await.unwrap();
    sees(&mut at_bob, None).await;
    // Invisible: gone for everyone, right away.
    alice.set_status(Status::Invisible).await.unwrap();
    sees(&mut at_carol, None).await;
    assert_eq!(alice.my_status(), Status::Invisible);
}
