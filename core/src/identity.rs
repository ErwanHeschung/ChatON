//! Identity = a secp256k1 key pair (CLAUDE.md, principle 1).

use keyring_core::Entry;
use nostr::nips::nip49::{EncryptedSecretKey, KeySecurity};
use nostr::prelude::{FromBech32, Keys, PublicKey, SecretKey, ToBech32};

use crate::Result;

const KEYCHAIN_SERVICE: &str = "chaton";

// ponytail: scrypt cost 2^16 (~64 MiB). Fine on desktop, re-tune for low-end phones at step 5.
const NCRYPTSEC_LOG_N: u8 = 16;

pub struct Identity {
    keys: Keys,
}

impl Identity {
    pub fn generate() -> Self {
        Self {
            keys: Keys::generate(),
        }
    }

    pub fn keys(&self) -> &Keys {
        &self.keys
    }

    pub fn public_key(&self) -> PublicKey {
        self.keys.public_key()
    }

    /// `npub1…`, for display and sharing only (NIP-19).
    pub fn npub(&self) -> String {
        match self.keys.public_key().to_bech32() {
            Ok(npub) => npub,
            Err(never) => match never {},
        }
    }

    /// Password-encrypted secret key (NIP-49), the blob stored in the vault.
    pub fn to_ncryptsec(&self, password: &str) -> Result<String> {
        let enc = EncryptedSecretKey::new(
            self.keys.secret_key(),
            password,
            NCRYPTSEC_LOG_N,
            KeySecurity::Medium,
        )?;
        Ok(enc.to_bech32()?)
    }

    pub fn from_ncryptsec(ncryptsec: &str, password: &str) -> Result<Self> {
        let secret = EncryptedSecretKey::from_bech32(ncryptsec)?.decrypt(password)?;
        Ok(Self {
            keys: Keys::new(secret),
        })
    }

    /// Stores the secret key in the OS keychain, never in a file.
    pub fn save(&self, account: &str) -> Result<()> {
        entry(account)?.set_secret(self.keys.secret_key().as_secret_bytes())?;
        Ok(())
    }

    pub fn load(account: &str) -> Result<Self> {
        let bytes = entry(account)?.get_secret()?;
        Ok(Self {
            keys: Keys::new(SecretKey::from_slice(&bytes)?),
        })
    }

    pub fn delete(account: &str) -> Result<()> {
        entry(account)?.delete_credential()?;
        Ok(())
    }
}

fn entry(account: &str) -> Result<Entry> {
    Ok(Entry::new(KEYCHAIN_SERVICE, account)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ncryptsec_round_trip() {
        let id = Identity::generate();
        let blob = id.to_ncryptsec("correct horse").unwrap();
        assert!(blob.starts_with("ncryptsec1"));
        let back = Identity::from_ncryptsec(&blob, "correct horse").unwrap();
        assert_eq!(back.public_key(), id.public_key());
        assert!(Identity::from_ncryptsec(&blob, "wrong").is_err());
    }

    // Official vector from the NIP-49 text.
    #[test]
    fn ncryptsec_official_vector() {
        let id = Identity::from_ncryptsec(
            "ncryptsec1qgg9947rlpvqu76pj5ecreduf9jxhselq2nae2kghhvd5g7dgjtcxfqtd67p9m0w57lspw8gsq6yphnm8623nsl8xn9j4jdzz84zm3frztj3z7s35vpzmqf6ksu8r89qk5z2zxfmu5gv8th8wclt0h4p",
            "nostr",
        )
        .unwrap();
        assert_eq!(
            id.keys().secret_key().to_secret_hex(),
            "3501454135014541350145413501453fefb02227e449e57cf4d3a3ce05378683"
        );
    }

    #[test]
    fn keychain_round_trip() {
        keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
        let id = Identity::generate();
        id.save("alice").unwrap();
        assert_eq!(
            Identity::load("alice").unwrap().public_key(),
            id.public_key()
        );
        Identity::delete("alice").unwrap();
        assert!(Identity::load("alice").is_err());
    }
}
