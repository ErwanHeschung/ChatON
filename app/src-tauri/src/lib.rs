//! Tauri shell: thin commands over `chaton_core::session`. No logic, no crypto here.

use std::path::PathBuf;
use std::sync::Arc;

use chaton_core::Error;
use chaton_core::community::{ChannelMessage, Community, Member, Update};
use chaton_core::dm::Message;
use chaton_core::link::{self, LinkOffer};
use chaton_core::nostr::prelude::{EventId, PublicKey};
use chaton_core::session::{Account, Conversation, Peer, Session};
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
/// private messages, `community` events for the rest (see `Update`).
async fn start(app: &AppHandle, state: &App, session: Session) -> Account {
    let session = Arc::new(session);
    let account = session.account().clone();
    let (listener, handle) = (session.clone(), app.clone());
    tauri::async_runtime::spawn(async move {
        listener
            .listen(move |update| {
                let _ = match update {
                    Update::Dm { message } => handle.emit("dm", message),
                    other => handle.emit("community", other),
                };
            })
            .await;
    });
    *state.session.lock().await = Some(session);
    account
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
async fn send_dm(state: State<'_, App>, pubkey: PublicKey, text: String) -> Res<Message> {
    Ok(session(&state).await?.send_dm(pubkey, &text).await?)
}

#[tauri::command]
async fn conversations(state: State<'_, App>) -> Res<Vec<Conversation>> {
    Ok(session(&state).await?.conversations().await?)
}

#[tauri::command]
async fn messages(state: State<'_, App>, pubkey: PublicKey) -> Res<Vec<Message>> {
    Ok(session(&state).await?.messages(pubkey).await?)
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
async fn create_channel(state: State<'_, App>, community: String, name: String) -> Res<()> {
    session(&state)
        .await?
        .create_channel(&community, &name)
        .await?;
    Ok(())
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

/// Registers the OS keychain as `keyring-core`'s default store (CLAUDE.md, Secrets).
fn keychain() -> Result<(), keyring_core::Error> {
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
            conversations,
            messages,
            communities,
            create_community,
            join_community,
            leave_community,
            create_channel,
            create_invite,
            members,
            set_roles,
            remove_member,
            channel_messages,
            send_channel,
            delete_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
