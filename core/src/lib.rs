//! ChatON client core: identity, events and private messages.
//!
//! Thin layer over `rust-nostr` that enforces `spec/protocol.md`. All crypto
//! lives here, never in the UI.
//!
//! Secrets go through `keyring-core`'s default store: the app must call
//! `keyring_core::set_default_store` with its platform store at startup.

pub mod dm;
pub mod event;
pub mod identity;

pub use nostr;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("keychain: {0}")]
    Keychain(#[from] keyring_core::Error),
    #[error("nostr: {0}")]
    Nostr(#[from] nostr::error::Error),
    #[error("rejected: {0:?}")]
    Rejected(event::Reject),
}

pub type Result<T> = std::result::Result<T, Error>;
