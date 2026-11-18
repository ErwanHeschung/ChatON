//! ChatON client core: identity, events, private messages and the relay session.
//!
//! Thin layer over `rust-nostr` that enforces `spec/protocol.md`. All crypto
//! lives here, never in the UI.
//!
//! Secrets go through `keyring-core`'s default store: the app must call
//! `keyring_core::set_default_store` with its platform store at startup.

pub mod block;
pub mod community;
pub mod dm;
pub mod event;
pub mod identity;
pub mod link;
pub mod media;
pub mod network;
pub mod perms;
pub mod push;
pub mod reads;
pub mod session;
pub mod voice;

pub use nostr;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("keychain: {0}")]
    Keychain(#[from] keyring_core::Error),
    #[error("nostr: {0}")]
    Nostr(#[from] nostr::error::Error),
    #[error("rejected: {0:?}")]
    Rejected(event::Reject),
    #[error("relay: {0}")]
    Relay(#[from] nostr_sdk::error::Error),
    #[error("local database: {0}")]
    Db(#[from] nostr_sqlite::prelude::Error),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid: {0}")]
    Invalid(String),
    #[error("server: {0}")]
    Server(String),
    #[error("handle already taken, or this key already has one")]
    HandleTaken,
    #[error("handle not found")]
    NotFound,
    #[error("this key doesn't match the handle")]
    KeyMismatch,
    #[error("no relay accepted the event: {0}")]
    NotDelivered(String),
    #[error("wrong link code")]
    WrongCode,
    #[error("link code expired, or no device is showing it")]
    CodeExpired,
    #[error("the other device refused the link")]
    LinkRefused,
    #[error("invalid or revoked invite")]
    InvalidInvite,
    #[error("file too large (25 MB max)")]
    TooLarge,
    #[error("this file is on a server you haven't allowed")]
    NotAllowed,
    #[error("voice is off on this instance")]
    VoiceDisabled,
    #[error("this voice channel is full")]
    Full,
    #[error("voice: {0}")]
    Voice(String),
    #[error("can't capture the screen here")]
    ScreenCapture,
}

pub type Result<T> = std::result::Result<T, Error>;
