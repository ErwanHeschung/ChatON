//! Tauri shell: thin commands over `chaton_core::session`. No logic, no crypto here.

mod push;
mod voice;

use std::path::PathBuf;
use std::sync::Arc;

use chaton_core::Error;
use chaton_core::community::{ChannelMessage, Community, Member, Update};
use chaton_core::dm::Message;
use chaton_core::link::{self, LinkOffer};
use chaton_core::media::Attachment;
use chaton_core::nostr::prelude::{EventId, PublicKey};
use chaton_core::session::{Account, Conversation, Peer, Session};
use chaton_core::voice::{ScreenSource, VoiceState};
use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{Mutex, oneshot};

#[derive(Default)]
struct App {
    session: Mutex<Option<Arc<Session>>>,
    /// The device link offer in progress on this device, if any.
    link: Mutex<Option<JoinHandle<()>>>,
    /// Answers the pending `link_request`.
    confirm: Mutex<Option<oneshot::Sender<bool>>>,
}

/// What the UI gets on failure: a stable `code` to branch on, and a message.
#[derive(Debug, Clone, Serialize)]
struct CmdError {
    code: &'static str,
    message: String,
}

impl From<Error> for CmdError {
    fn from(e: Error) -> Self {
        let code = match e {
            Error::HandleTaken => "handle_taken",
            Error::NotFound => "not_found",
            Error::KeyMismatch => "key_mismatch",
            Error::WrongCode => "wrong_code",
            Error::CodeExpired => "code_expired",
            Error::LinkRefused => "link_refused",
            Error::InvalidInvite => "invalid_invite",
            Error::TooLarge => "too_large",
            Error::NotAllowed => "not_allowed",
            Error::VoiceDisabled => "voice_disabled",
            Error::Voice(_) => "voice",
            Error::ScreenCapture => "screen_capture",
            Error::Invalid(_) | Error::Nostr(_) => "invalid",
            // The relay answered no (permissions, unknown account): not a network problem.
            Error::NotDelivered(ref reason)
                if ["restricted:", "invalid:", "duplicate:", "blocked:"]
                    .iter()
                    .any(|p| reason.contains(p)) =>
            {
                "refused"
            }
            Error::Keychain(_) => "keychain",
            Error::Http(_) | Error::Relay(_) | Error::NotDelivered(_) | Error::Server(_) => {
                "network"
            }
            _ => "internal",
        };
        Self {
            code,
            message: e.to_string(),
        }
    }
}

fn internal(e: impl ToString) -> CmdError {
    CmdError {
        code: "internal",
        message: e.to_string(),
    }
}

type Res<T> = Result<T, CmdError>;

/// App data dir, or a sub-directory per `CHATON_PROFILE` to run a second account side by side.
fn data_dir(app: &AppHandle) -> Res<PathBuf> {
    let dir = app.path().app_data_dir().map_err(internal)?;
    Ok(match std::env::var("CHATON_PROFILE") {
        Ok(profile) if !profile.is_empty() => dir.join(profile),
        _ => dir,
    })
}

/// Keeps the session and forwards what comes in to the UI: `dm` events for
/// private messages, `voice` for calls ringing and the room I'm in,
/// `community` events for the rest (see `Update`).
async fn start(app: &AppHandle, state: &App, session: Session) -> Account {
    let session = Arc::new(session);
    let account = session.account().clone();
    let (listener, handle) = (session.clone(), app.clone());
    tauri::async_runtime::spawn(async move {
        listener
            .listen(|update| {
                let _ = match update {
                    Update::Dm { message } => {
                        notify(&handle, &listener, &message);
                        handle.emit("dm", message)
                    }
                    Update::Ring { ref message, .. } => {
                        let body = chaton_core::dm::CALL_TEXT.to_string();
                        alert(&handle, &listener, message.sender, body);
                        handle.emit("voice", update)
                    }
                    voice @ Update::Voice { .. } => {
                        if matches!(voice, Update::Voice { state: None }) {
                            let handle = handle.clone();
                            tauri::async_runtime::spawn(
                                async move { voice::release(&handle).await },
                            );
                        }
                        handle.emit("voice", voice)
                    }
                    other => handle.emit("community", other),
                };
            })
            .await;
    });
    *state.session.lock().await = Some(session);
    account
}

/// Desktop: an OS notification for a message from someone else while the
/// window isn't focused. Android shows a generic one on push instead.
fn notify(app: &AppHandle, session: &Arc<Session>, message: &Message) {
    let body = match &message.attachment {
        Some(file) => format!("📎 {}", file.name),
        // A call notifies when it rings (`Update::Ring`), not as a message.
        None if message.call.is_some() => return,
        None if message.text.is_empty() => return, // a group rename
        None => message.text.chars().take(200).collect(),
    };
    alert(app, session, message.sender, body);
}

fn alert(app: &AppHandle, session: &Arc<Session>, sender: PublicKey, body: String) {
    let focused = app
        .get_webview_window("main")
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(true);
    if cfg!(mobile) || focused || sender == session.account().pubkey {
        return;
    }
    let (app, session) = (app.clone(), session.clone());
    tauri::async_runtime::spawn(async move {
        use tauri_plugin_notification::NotificationExt;
        let peer = session.peer(sender).await;
        let title = peer.handle.map_or(peer.npub, |h| format!("@{h}"));
        drop(app.notification().builder().title(title).body(body).show());
    });
}

async fn session(state: &App) -> Res<Arc<Session>> {
    state.session.lock().await.clone().ok_or(CmdError {
        code: "signed_out",
        message: "not signed in".into(),
    })
}

#[tauri::command]
async fn current_account(app: AppHandle, state: State<'_, App>) -> Res<Option<Account>> {
    if let Some(session) = state.session.lock().await.as_ref() {
        return Ok(Some(session.account().clone()));
    }
    match Session::open(&data_dir(&app)?).await? {
        Some(session) => Ok(Some(start(&app, &state, session).await)),
        None => Ok(None),
    }
}

#[tauri::command]
async fn register(
    app: AppHandle,
    state: State<'_, App>,
    instance: String,
    name: String,
) -> Res<Account> {
    let session = Session::register(&data_dir(&app)?, &instance, &name).await?;
    Ok(start(&app, &state, session).await)
}

#[tauri::command]
async fn import_key(
    app: AppHandle,
    state: State<'_, App>,
    handle: String,
    nsec: String,
) -> Res<Account> {
    let session = Session::import_key(&data_dir(&app)?, &handle, &nsec).await?;
    Ok(start(&app, &state, session).await)
}

/// New device: gets the key from a signed-in device showing `code`.
#[tauri::command]
async fn link_join(
    app: AppHandle,
    state: State<'_, App>,
    handle: String,
    code: String,
) -> Res<Account> {
    let session = link::join(&data_dir(&app)?, &handle, &code, &device_name()).await?;
    Ok(start(&app, &state, session).await)
}

#[derive(Clone, Serialize)]
struct LinkRequest {
    device_name: String,
}

#[derive(Clone, Serialize)]
struct LinkDone {
    device_name: Option<String>,
    error: Option<CmdError>,
}

/// Signed-in device: returns a code and waits for a new device in the background.
/// Emits `link_request` (answer with `link_confirm`), then `link_done`.
#[tauri::command]
async fn link_offer(app: AppHandle, state: State<'_, App>) -> Res<String> {
    let offer = LinkOffer::new(&*session(&state).await?).await?;
    let code = offer.code();
    let task = tauri::async_runtime::spawn(async move {
        let asker = app.clone();
        let confirm = move |device_name| async move {
            let (tx, rx) = oneshot::channel();
            *asker.state::<App>().confirm.lock().await = Some(tx);
            let _ = asker.emit("link_request", LinkRequest { device_name });
            rx.await.unwrap_or(false)
        };
        let done = match offer.run(confirm).await {
            Ok(name) => LinkDone {
                device_name: Some(name),
                error: None,
            },
            Err(e) => LinkDone {
                device_name: None,
                error: Some(e.into()),
            },
        };
        let _ = app.emit("link_done", done);
    });
    if let Some(previous) = state.link.lock().await.replace(task) {
        previous.abort();
    }
    Ok(code)
}

#[tauri::command]
async fn link_confirm(state: State<'_, App>, accept: bool) -> Res<()> {
    if let Some(tx) = state.confirm.lock().await.take() {
        let _ = tx.send(accept);
    }
    Ok(())
}

#[tauri::command]
async fn link_cancel(state: State<'_, App>) -> Res<()> {
    if let Some(task) = state.link.lock().await.take() {
        task.abort();
    }
    Ok(())
}

#[tauri::command]
async fn export_key(state: State<'_, App>) -> Res<String> {
    Ok(session(&state).await?.export_key())
}

/// Forgets the account on this device (key, account file, local history).
#[tauri::command]
async fn logout(state: State<'_, App>) -> Res<()> {
    let session = state.session.lock().await.take();
    if let Some(session) = session {
        session.logout().await?;
    }
    Ok(())
}

/// Shown on the other device when linking.
// ponytail: env/`/etc/hostname`, so macOS shows "macos"; use tauri-plugin-os if that matters.
fn device_name() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| std::env::consts::OS.to_string())
}

#[tauri::command]
async fn search_handle(state: State<'_, App>, handle: String) -> Res<Peer> {
    Ok(session(&state).await?.search(&handle).await?)
}

#[tauri::command]
async fn peer(state: State<'_, App>, pubkey: PublicKey) -> Res<Peer> {
    Ok(session(&state).await?.peer(pubkey).await)
}

#[tauri::command]
/// `room`: the conversation's members (me included or not): one person, or a group.
async fn send_dm(state: State<'_, App>, room: Vec<PublicKey>, text: String) -> Res<Message> {
    Ok(session(&state).await?.send_dm(&room, &text).await?)
}

/// Creates a group (its first event is its name) or renames it.
#[tauri::command]
async fn name_group(state: State<'_, App>, room: Vec<PublicKey>, subject: String) -> Res<Message> {
    Ok(session(&state).await?.name_group(&room, &subject).await?)
}

#[tauri::command]
async fn conversations(state: State<'_, App>) -> Res<Vec<Conversation>> {
    Ok(session(&state).await?.conversations().await?)
}

#[tauri::command]
async fn messages(state: State<'_, App>, room: Vec<PublicKey>) -> Res<Vec<Message>> {
    Ok(session(&state).await?.messages(&room).await?)
}

#[tauri::command]
async fn communities(state: State<'_, App>) -> Res<Vec<Community>> {
    Ok(session(&state).await?.communities().await?)
}

#[tauri::command]
async fn create_community(state: State<'_, App>, name: String) -> Res<Community> {
    Ok(session(&state).await?.create_community(&name).await?)
}

#[tauri::command]
async fn join_community(state: State<'_, App>, invite: String) -> Res<Community> {
    Ok(session(&state).await?.join(&invite).await?)
}

#[tauri::command]
async fn leave_community(state: State<'_, App>, community: String) -> Res<()> {
    Ok(session(&state).await?.leave(&community).await?)
}

#[tauri::command]
async fn create_channel(
    state: State<'_, App>,
    community: String,
    name: String,
    voice: bool,
) -> Res<()> {
    session(&state)
        .await?
        .create_channel(&community, &name, voice)
        .await?;
    Ok(())
}

#[tauri::command]
async fn voice_available(state: State<'_, App>, community: String) -> Res<bool> {
    Ok(session(&state).await?.voice_available(&community).await?)
}

#[tauri::command]
async fn join_voice_channel(
    app: AppHandle,
    state: State<'_, App>,
    community: String,
    channel: String,
) -> Res<VoiceState> {
    Ok(session(&state)
        .await?
        .join_voice_channel(&community, &channel, voice::prepare(&app).await)
        .await?)
}

/// Calls the conversation `room` (its members, me included or not).
#[tauri::command]
async fn start_call(
    app: AppHandle,
    state: State<'_, App>,
    room: Vec<PublicKey>,
) -> Res<VoiceState> {
    Ok(session(&state)
        .await?
        .start_call(&room, voice::prepare(&app).await)
        .await?)
}

#[tauri::command]
async fn join_call(
    app: AppHandle,
    state: State<'_, App>,
    room: Vec<PublicKey>,
    id: String,
) -> Res<VoiceState> {
    Ok(session(&state)
        .await?
        .join_call(&room, &id, voice::prepare(&app).await)
        .await?)
}

#[tauri::command]
async fn leave_voice(state: State<'_, App>) -> Res<()> {
    Ok(session(&state).await?.leave_voice().await?)
}

#[tauri::command]
async fn set_muted(state: State<'_, App>, muted: bool) -> Res<VoiceState> {
    Ok(session(&state).await?.set_muted(muted).await?)
}

#[tauri::command]
async fn set_deafened(state: State<'_, App>, deafened: bool) -> Res<VoiceState> {
    Ok(session(&state).await?.set_deafened(deafened).await?)
}

#[tauri::command]
async fn voice_state(state: State<'_, App>) -> Res<Option<VoiceState>> {
    Ok(session(&state).await?.voice_state().await)
}

#[tauri::command]
fn can_share_screen() -> bool {
    chaton_core::voice::can_share_screen()
}

/// Empty: the OS shows its own picker, start with no source.
#[tauri::command]
async fn screen_sources() -> Vec<ScreenSource> {
    chaton_core::voice::screen_sources().await
}

#[tauri::command]
async fn start_screen_share(state: State<'_, App>, source: Option<ScreenSource>) -> Res<()> {
    Ok(session(&state).await?.start_screen_share(source).await?)
}

#[tauri::command]
async fn stop_screen_share(state: State<'_, App>) -> Res<()> {
    Ok(session(&state).await?.stop_screen_share().await?)
}

/// Watches `key`'s screen: its images come as JPEG bytes on `images`.
#[tauri::command]
async fn watch_screen(
    state: State<'_, App>,
    key: PublicKey,
    images: tauri::ipc::Channel,
) -> Res<VoiceState> {
    let sink: chaton_core::voice::Sink =
        Arc::new(move |jpeg| drop(images.send(tauri::ipc::InvokeResponseBody::Raw(jpeg))));
    Ok(session(&state)
        .await?
        .watch_screen(Some((key, sink)))
        .await?)
}

#[tauri::command]
async fn stop_watching(state: State<'_, App>) -> Res<VoiceState> {
    Ok(session(&state).await?.watch_screen(None).await?)
}

#[tauri::command]
async fn create_invite(state: State<'_, App>, community: String) -> Res<String> {
    Ok(session(&state).await?.invite(&community).await?)
}

#[tauri::command]
async fn members(state: State<'_, App>, community: String) -> Res<Vec<Member>> {
    Ok(session(&state).await?.members(&community).await?)
}

#[tauri::command]
async fn set_roles(
    state: State<'_, App>,
    community: String,
    pubkey: PublicKey,
    roles: Vec<String>,
) -> Res<()> {
    let roles: Vec<&str> = roles.iter().map(String::as_str).collect();
    Ok(session(&state)
        .await?
        .set_roles(&community, pubkey, &roles)
        .await?)
}

#[tauri::command]
async fn remove_member(state: State<'_, App>, community: String, pubkey: PublicKey) -> Res<()> {
    Ok(session(&state)
        .await?
        .remove_member(&community, pubkey)
        .await?)
}

#[tauri::command]
async fn channel_messages(state: State<'_, App>, channel: String) -> Res<Vec<ChannelMessage>> {
    Ok(session(&state).await?.channel_messages(&channel).await?)
}

#[tauri::command]
async fn send_channel(
    state: State<'_, App>,
    community: String,
    channel: String,
    text: String,
) -> Res<ChannelMessage> {
    Ok(session(&state)
        .await?
        .send(&community, &channel, &text)
        .await?)
}

#[tauri::command]
async fn delete_message(
    state: State<'_, App>,
    community: String,
    channel: String,
    id: EventId,
) -> Res<()> {
    Ok(session(&state)
        .await?
        .delete_message(&community, &channel, id)
        .await?)
}

/// A file sent by the UI: raw bytes as the IPC body (no JSON), and its
/// metadata as a query string in the `x-meta` header (`name`, `mime`, ids).
struct Upload {
    bytes: Vec<u8>,
    meta: std::collections::HashMap<String, String>,
}

impl Upload {
    fn from(request: &tauri::ipc::Request<'_>) -> Res<Self> {
        let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
            return Err(internal("expected the file as raw bytes"));
        };
        let meta = request
            .headers()
            .get("x-meta")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let url = tauri::Url::parse(&format!("meta:?{meta}")).map_err(internal)?;
        Ok(Self {
            bytes: bytes.clone(),
            meta: url.query_pairs().into_owned().collect(),
        })
    }

    fn get(&self, key: &str) -> Res<&str> {
        self.meta
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| internal(format!("missing {key}")))
    }
}

#[tauri::command]
async fn send_dm_file(state: State<'_, App>, request: tauri::ipc::Request<'_>) -> Res<Message> {
    let file = Upload::from(&request)?;
    let room = file
        .get("room")?
        .split(',')
        .map(PublicKey::from_hex)
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    Ok(session(&state)
        .await?
        .send_dm_file(&room, &file.bytes, file.get("mime")?, file.get("name")?)
        .await?)
}

#[tauri::command]
async fn send_channel_file(
    state: State<'_, App>,
    request: tauri::ipc::Request<'_>,
) -> Res<ChannelMessage> {
    let file = Upload::from(&request)?;
    Ok(session(&state)
        .await?
        .send_file(
            file.get("community")?,
            file.get("channel")?,
            &file.bytes,
            file.get("mime")?,
            file.get("name")?,
            file.get("caption").unwrap_or_default(),
        )
        .await?)
}

#[tauri::command]
async fn set_profile_picture(state: State<'_, App>, request: tauri::ipc::Request<'_>) -> Res<()> {
    let file = Upload::from(&request)?;
    let mime = file.get("mime")?.to_string();
    Ok(session(&state)
        .await?
        .set_profile_picture(file.bytes, &mime)
        .await?)
}

#[tauri::command]
async fn set_community_picture(state: State<'_, App>, request: tauri::ipc::Request<'_>) -> Res<()> {
    let file = Upload::from(&request)?;
    let (community, mime) = (
        file.get("community")?.to_string(),
        file.get("mime")?.to_string(),
    );
    Ok(session(&state)
        .await?
        .set_community_picture(&community, file.bytes, &mime)
        .await?)
}

/// Decrypted bytes, as an `ArrayBuffer` for the UI. `explicit`: the user clicked.
#[tauri::command]
async fn open_attachment(
    state: State<'_, App>,
    attachment: Attachment,
    explicit: bool,
) -> Res<tauri::ipc::Response> {
    let bytes = session(&state)
        .await?
        .open_attachment(&attachment, explicit)
        .await?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
async fn picture(state: State<'_, App>, url: String) -> Res<tauri::ipc::Response> {
    let bytes = session(&state).await?.picture(&url).await?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Writes the decrypted file where the user chose (dialog `save()`).
#[tauri::command]
async fn save_attachment(state: State<'_, App>, attachment: Attachment, path: PathBuf) -> Res<()> {
    let bytes = session(&state)
        .await?
        .open_attachment(&attachment, true)
        .await?;
    std::fs::write(path, bytes).map_err(internal)
}

/// Android: asks UnifiedPush for this device's endpoint and registers it on my
/// instance (spec/federation.md). `no_distributor`: install ntfy (or another).
#[tauri::command]
async fn enable_push(app: AppHandle, state: State<'_, App>) -> Res<()> {
    #[cfg(target_os = "android")]
    {
        use tauri::plugin::mobile::PluginInvokeError;
        let session = session(&state).await?;
        let vapid = session.vapid_key().await?;
        let plugin = app.state::<push::Handle<tauri::Wry>>();
        let endpoint: chaton_core::push::Endpoint = plugin
            .0
            .run_mobile_plugin_async("register", serde_json::json!({ "vapid": vapid }))
            .await
            .map_err(|e| match e {
                PluginInvokeError::InvokeRejected(r)
                    if r.code.as_deref() == Some("no_distributor") =>
                {
                    CmdError {
                        code: "no_distributor",
                        message: r.message.unwrap_or_default(),
                    }
                }
                other => internal(other),
            })?;
        session.push_register(&endpoint).await?;
        Ok(())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, state);
        Err(CmdError {
            code: "unsupported",
            message: "push is for Android; desktop notifies while running".into(),
        })
    }
}

/// Registers the OS keychain as `keyring-core`'s default store (CLAUDE.md, Secrets).
fn keychain() -> Result<(), keyring_core::Error> {
    #[cfg(target_os = "android")]
    keyring_core::set_default_store(android_native_keyring_store::Store::new()?);
    #[cfg(target_os = "linux")]
    keyring_core::set_default_store(zbus_secret_service_keyring_store::store::Store::new()?);
    #[cfg(target_os = "windows")]
    keyring_core::set_default_store(windows_native_keyring_store::store::Store::new()?);
    #[cfg(target_os = "macos")]
    keyring_core::set_default_store(apple_native_keyring_store::keychain::Store::new()?);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // No keychain = no account can be saved; the UI shows the error on register.
    if let Err(e) = keychain() {
        eprintln!("keychain unavailable: {e}");
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(push::init())
        .plugin(voice::init())
        .manage(App::default())
        .invoke_handler(tauri::generate_handler![
            current_account,
            register,
            import_key,
            link_join,
            link_offer,
            link_confirm,
            link_cancel,
            export_key,
            logout,
            search_handle,
            peer,
            send_dm,
            name_group,
            conversations,
            messages,
            communities,
            create_community,
            join_community,
            leave_community,
            create_channel,
            voice_available,
            join_voice_channel,
            start_call,
            join_call,
            leave_voice,
            set_muted,
            set_deafened,
            voice_state,
            can_share_screen,
            screen_sources,
            start_screen_share,
            stop_screen_share,
            watch_screen,
            stop_watching,
            create_invite,
            members,
            set_roles,
            remove_member,
            channel_messages,
            send_channel,
            delete_message,
            send_dm_file,
            send_channel_file,
            set_profile_picture,
            set_community_picture,
            open_attachment,
            picture,
            save_attachment,
            enable_push,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
