//! Push registration (spec/federation.md, "Notifications push"): this device's
//! WebPush endpoint, kept by my own instance, which wakes the device when a DM
//! arrives. Requests are NIP-98 signed.

use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};

use crate::session::{Session, http};
use crate::{Error, Result, media};

const PATH: &str = "/api/push";
/// The endpoint registered from this device, to unregister it on logout.
pub(crate) const PUSH_FILE: &str = "push.json";

/// A WebPush subscription, as UnifiedPush gives it (keys in base64url).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

impl Session {
    /// My instance's VAPID public key (base64url), for the push distributor.
    pub async fn vapid_key(&self) -> Result<String> {
        #[derive(Deserialize)]
        struct Info {
            vapid: String,
        }
        let resp = http().get(self.push_url()).send().await?;
        if !resp.status().is_success() {
            return Err(Error::Server(resp.status().to_string()));
        }
        Ok(resp.json::<Info>().await?.vapid)
    }

    /// Asks my instance to wake this device at `endpoint` when a DM arrives.
    pub async fn push_register(&self, endpoint: &Endpoint) -> Result<()> {
        self.push_call(Method::POST, endpoint).await?;
        std::fs::write(self.dir().join(PUSH_FILE), serde_json::to_vec(endpoint)?)?;
        Ok(())
    }

    /// Forgets this device's endpoint, if it registered one.
    pub async fn push_unregister(&self) -> Result<()> {
        let path = self.dir().join(PUSH_FILE);
        let raw = match std::fs::read(&path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        self.push_call(Method::DELETE, &serde_json::from_slice(&raw)?)
            .await?;
        std::fs::remove_file(path)?;
        Ok(())
    }

    fn push_url(&self) -> String {
        format!("{}{PATH}", media::http_base(&self.account().relay))
    }

    async fn push_call(&self, method: Method, endpoint: &Endpoint) -> Result<()> {
        let body = serde_json::to_string(endpoint)?;
        let resp = self.nip98(method, &self.push_url(), body).await?;
        match resp.status() {
            StatusCode::NO_CONTENT => Ok(()),
            StatusCode::FORBIDDEN => Err(Error::NotAllowed),
            StatusCode::BAD_REQUEST => Err(Error::Invalid(resp.text().await?)),
            status => Err(Error::Server(format!("{status}: {}", resp.text().await?))),
        }
    }
}
