//! Tauri shell: thin commands over `chaton_core::session`. No logic, no crypto here.

use std::path::PathBuf;
use std::sync::Arc;

use chaton_core::Error;
use chaton_core::dm::Message;
use chaton_core::nostr::prelude::PublicKey;
use chaton_core::session::{Account, Conversation, Peer, Session};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;

#[derive(Default)]
struct App {
    session: Mutex<Option<Arc<Session>>>,
}

/// What the UI gets on failure: a stable `code` to branch on, and a message.
#[derive(Debug, Serialize)]
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
            Error::Invalid(_) | Error::Nostr(_) => "invalid",
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

/// Keeps the session and forwards incoming DMs to the UI as `dm` events.
async fn start(app: &AppHandle, state: &App, session: Session) -> Account {
    let session = Arc::new(session);
    let account = session.account().clone();
    let (listener, handle) = (session.clone(), app.clone());
    tauri::async_runtime::spawn(async move {
        listener
            .listen(move |message| drop(handle.emit("dm", message)))
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
async fn login(
    app: AppHandle,
    state: State<'_, App>,
    handle: String,
    nsec: String,
) -> Res<Account> {
    let session = Session::login(&data_dir(&app)?, &handle, &nsec).await?;
    Ok(start(&app, &state, session).await)
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
            login,
            search_handle,
            peer,
            send_dm,
            conversations,
            messages,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
