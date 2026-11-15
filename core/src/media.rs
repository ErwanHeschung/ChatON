//! Files (spec protocol § 10): Blossom upload to my instance, download with a
//! size cap and hash check. Attachments are AES-256-GCM encrypted here, with a
//! fresh key per file carried by the message; pictures are public, in clear.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use nostr::prelude::{Keys, Tag, Url};
use nostr_blossom::client::BlossomClient;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::session::{TIMEOUT, is_local_host};
use crate::{Error, Result};

/// Max file size, plaintext or ciphertext (spec § 10).
pub const MAX_SIZE: usize = 25 * 1024 * 1024;
const ALGORITHM: &str = "aes-gcm";
/// AES-GCM tag: the ciphertext is this much longer than the file.
const TAG_LEN: usize = 16;

/// An encrypted file in a message. Handed to the UI as an opaque value, which
/// gives it back to open or save the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub url: String,
    pub mime: String,
    pub name: String,
    /// Plaintext size in bytes.
    pub size: u64,
    /// `x`: sha256 of the ciphertext, its Blossom address.
    pub sha256: String,
    /// `ox`: sha256 of the plaintext.
    pub plain_sha256: String,
    pub key: String,
    pub nonce: String,
}

impl Attachment {
    /// NIP-17 kind 15 tags (the URL is the content).
    pub(crate) fn to_dm_tags(&self) -> Result<Vec<Tag>> {
        let size = self.size.to_string();
        [
            ["file-type", self.mime.as_str()],
            ["encryption-algorithm", ALGORITHM],
            ["decryption-key", &self.key],
            ["decryption-nonce", &self.nonce],
            ["x", &self.sha256],
            ["ox", &self.plain_sha256],
            ["size", &size],
            ["name", &self.name],
        ]
        .into_iter()
        .map(|t| Ok(Tag::parse(t)?))
        .collect()
    }

    pub(crate) fn from_dm_tags<'a>(url: &str, tags: impl Iterator<Item = &'a Tag>) -> Option<Self> {
        Self::from_fields(
            tags.filter_map(|t| match t.as_slice() {
                [k, v, ..] => Some((k.as_str(), v.as_str())),
                _ => None,
            })
            .chain([("url", url)]),
        )
    }

    /// NIP-92 `imeta` tag, for a channel message.
    pub(crate) fn to_imeta(&self) -> Result<Tag> {
        let fields = [
            ("url", self.url.as_str()),
            ("m", &self.mime),
            ("x", &self.sha256),
            ("ox", &self.plain_sha256),
            ("size", &self.size.to_string()),
            ("name", &self.name),
            ("encryption-algorithm", ALGORITHM),
            ("decryption-key", &self.key),
            ("decryption-nonce", &self.nonce),
        ]
        .map(|(k, v)| format!("{k} {v}"));
        Ok(Tag::parse(
            std::iter::once("imeta".to_string()).chain(fields),
        )?)
    }

    pub(crate) fn from_imeta(tag: &Tag) -> Option<Self> {
        let [kind, fields @ ..] = tag.as_slice() else {
            return None;
        };
        if kind != "imeta" {
            return None;
        }
        Self::from_fields(fields.iter().filter_map(|f| f.split_once(' ')))
    }

    fn from_fields<'a>(fields: impl Iterator<Item = (&'a str, &'a str)>) -> Option<Self> {
        let (mut url, mut mime, mut name, mut size, mut x, mut ox, mut key, mut nonce, mut algo) =
            Default::default();
        for (k, v) in fields {
            match k {
                "url" => url = Some(v),
                "m" | "file-type" => mime = Some(v),
                "name" => name = Some(v),
                "size" => size = v.parse().ok(),
                "x" => x = Some(v),
                "ox" => ox = Some(v),
                "decryption-key" => key = Some(v),
                "decryption-nonce" => nonce = Some(v),
                "encryption-algorithm" => algo = Some(v),
                _ => {}
            }
        }
        let is_hex =
            |s: &str, len: usize| s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit());
        let (x, ox, key, nonce) = (x?, ox?, key?, nonce?);
        if algo != Some(ALGORITHM)
            || !is_hex(x, 64)
            || !is_hex(ox, 64)
            || !is_hex(key, 64)
            || !is_hex(nonce, 24)
        {
            return None;
        }
        Some(Self {
            url: url?.to_string(),
            mime: mime.unwrap_or("application/octet-stream").to_string(),
            name: name.unwrap_or("file").to_string(),
            size: size?,
            sha256: x.to_lowercase(),
            plain_sha256: ox.to_lowercase(),
            key: key.to_lowercase(),
            nonce: nonce.to_lowercase(),
        })
    }
}

/// Encrypts `plain` with a fresh key: `(ciphertext, attachment without url)`.
pub(crate) fn encrypt(plain: &[u8], mime: &str, name: &str) -> Result<(Vec<u8>, Attachment)> {
    let (mut key, mut nonce) = ([0u8; 32], [0u8; 12]);
    getrandom::fill(&mut key).map_err(|e| Error::Invalid(e.to_string()))?;
    getrandom::fill(&mut nonce).map_err(|e| Error::Invalid(e.to_string()))?;
    let cipher = Aes256Gcm::new(&key.into())
        .encrypt(Nonce::from_slice(&nonce), plain)
        .map_err(|_| Error::Invalid("encryption failed".into()))?;
    let attachment = Attachment {
        url: String::new(),
        mime: mime.to_string(),
        name: name.to_string(),
        size: plain.len() as u64,
        sha256: sha256_hex(&cipher),
        plain_sha256: sha256_hex(plain),
        key: hex::encode(key),
        nonce: hex::encode(nonce),
    };
    Ok((cipher, attachment))
}

/// Checks the ciphertext hash, decrypts, checks the plaintext hash.
pub(crate) fn decrypt(attachment: &Attachment, cipher: &[u8]) -> Result<Vec<u8>> {
    let corrupt = || Error::Invalid("file doesn't match its message".into());
    if sha256_hex(cipher) != attachment.sha256 {
        return Err(corrupt());
    }
    let key: [u8; 32] = hex::decode(&attachment.key)
        .ok()
        .and_then(|k| k.try_into().ok())
        .ok_or_else(corrupt)?;
    let nonce: [u8; 12] = hex::decode(&attachment.nonce)
        .ok()
        .and_then(|n| n.try_into().ok())
        .ok_or_else(corrupt)?;
    let plain = Aes256Gcm::new(&key.into())
        .decrypt(Nonce::from_slice(&nonce), cipher)
        .map_err(|_| corrupt())?;
    if sha256_hex(&plain) != attachment.plain_sha256 {
        return Err(corrupt());
    }
    Ok(plain)
}

/// Uploads to the Blossom server of `relay`'s instance; returns the blob URL.
pub(crate) async fn upload(keys: &Keys, relay: &str, bytes: Vec<u8>, mime: &str) -> Result<String> {
    if bytes.len() > MAX_SIZE + TAG_LEN {
        return Err(Error::TooLarge);
    }
    let base = Url::parse(&http_base(relay)).map_err(|e| Error::Invalid(e.to_string()))?;
    let descriptor = BlossomClient::new(base)
        .upload_blob(bytes, Some(mime.to_string()), None, Some(keys))
        .await
        .map_err(|e| Error::Server(format!("upload: {e}")))?;
    Ok(descriptor.url.to_string())
}

/// Downloads `url` (at most `MAX_SIZE`, plus the AES tag) and checks its sha256.
pub(crate) async fn download(url: &str, sha256: &str) -> Result<Vec<u8>> {
    let mut resp = reqwest::Client::builder()
        .timeout(TIMEOUT * 6)
        .build()?
        .get(url)
        .send()
        .await?
        .error_for_status()?;
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        if body.len() + chunk.len() > MAX_SIZE + TAG_LEN {
            return Err(Error::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    if sha256_hex(&body) != sha256 {
        return Err(Error::Invalid("file doesn't match its hash".into()));
    }
    Ok(body)
}

/// `ws://h` → `http://h`, `wss://h` → `https://h`.
pub(crate) fn http_base(relay: &str) -> String {
    relay.replacen("ws", "http", 1)
}

/// `host[:port]` of a URL.
pub(crate) fn host(url: &str) -> Option<String> {
    let url = Url::parse(url).ok()?;
    let host = url.host_str()?;
    Some(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

/// The hash a Blossom URL names (`/<sha256>[.ext]`), if it's on one of `hosts`
/// over HTTPS (HTTP for a local dev instance). Nothing else is auto-downloaded.
pub(crate) fn allowed<S: AsRef<str>>(url: &str, hosts: &[S]) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let host = host(url)?;
    let scheme_ok =
        parsed.scheme() == "https" || (parsed.scheme() == "http" && is_local_host(&host));
    if !scheme_ok || !hosts.iter().any(|h| h.as_ref() == host) {
        return None;
    }
    let file = parsed.path().strip_prefix('/')?;
    let hash = file.split_once('.').map_or(file, |(h, _)| h);
    (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then(|| hash.to_lowercase())
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (Vec<u8>, Attachment) {
        let (cipher, mut att) =
            encrypt(b"un chat \xF0\x9F\x90\xB1", "image/png", "chat.png").unwrap();
        att.url = format!("https://chaton.example/{}", att.sha256);
        (cipher, att)
    }

    #[test]
    fn encrypts_and_checks_both_hashes() {
        let (cipher, att) = sample();
        assert_eq!(decrypt(&att, &cipher).unwrap(), "un chat 🐱".as_bytes());
        assert_eq!(cipher.len(), att.size as usize + TAG_LEN);

        let mut tampered = cipher.clone();
        tampered[0] ^= 1;
        assert!(decrypt(&att, &tampered).is_err(), "ciphertext changed");
        // Hash updated to the tampered bytes: GCM still refuses them.
        let forged = Attachment {
            sha256: sha256_hex(&tampered),
            ..att.clone()
        };
        assert!(decrypt(&forged, &tampered).is_err(), "GCM tag");
        let wrong_ox = Attachment {
            plain_sha256: "0".repeat(64),
            ..att.clone()
        };
        assert!(decrypt(&wrong_ox, &cipher).is_err(), "plaintext hash");
        let (_, other) = encrypt(b"x", "a/b", "x").unwrap();
        assert_ne!(other.key, att.key, "one key per file");
    }

    #[test]
    fn tags_round_trip() {
        let (_, att) = sample();
        let tags = att.to_dm_tags().unwrap();
        assert_eq!(
            Attachment::from_dm_tags(&att.url, tags.iter()),
            Some(att.clone())
        );
        assert_eq!(
            Attachment::from_imeta(&att.to_imeta().unwrap()),
            Some(att.clone())
        );
        // Missing key, other algorithm, or not an imeta: no attachment.
        let no_key: Vec<Tag> = tags
            .iter()
            .filter(|t| t.kind() != "decryption-key")
            .cloned()
            .collect();
        assert_eq!(Attachment::from_dm_tags(&att.url, no_key.iter()), None);
        let other_algo = Tag::parse(["imeta", "url u", "encryption-algorithm aes-cbc"]).unwrap();
        assert_eq!(Attachment::from_imeta(&other_algo), None);
        assert_eq!(
            Attachment::from_imeta(&Tag::parse(["h", "x"]).unwrap()),
            None
        );
    }

    #[test]
    fn only_known_hosts_are_auto_downloaded() {
        let hash = "ab".repeat(32);
        let hosts = ["chaton.example", "localhost:8080"];
        assert_eq!(
            allowed(&format!("https://chaton.example/{hash}.png"), &hosts),
            Some(hash.clone())
        );
        assert_eq!(
            allowed(&format!("http://localhost:8080/{hash}"), &hosts),
            Some(hash.clone())
        );
        for bad in [
            format!("http://chaton.example/{hash}"), // plain HTTP, not local
            format!("https://evil.example/{hash}"),  // other host
            format!("https://chaton.example.evil/{hash}"),
            "https://chaton.example/not-a-hash".to_string(),
            format!("https://chaton.example/x/{hash}"),
        ] {
            assert_eq!(allowed(&bad, &hosts), None, "{bad}");
        }
        assert_eq!(http_base("wss://chaton.example"), "https://chaton.example");
        assert_eq!(http_base("ws://localhost:8080"), "http://localhost:8080");
    }
}
