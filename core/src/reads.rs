//! What I've read and how each community notifies me (spec § 9, « Non-lus
//! et notifications »). Kept on this device only, in `reads.json`.

use std::collections::HashMap;
use std::path::Path;

use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Result;
use crate::session::Session;

const FILE: &str = "reads.json";

/// What notifies me in a community.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotifyLevel {
    /// Every message from someone else.
    All,
    /// Those that mention me.
    #[default]
    Mentions,
    None,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct Reads {
    /// Per channel: read up to this time (unix seconds).
    #[serde(default)]
    read: HashMap<String, u64>,
    #[serde(default)]
    notify: HashMap<String, NotifyLevel>,
}

impl Reads {
    pub(crate) fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(FILE))
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default()
    }
}

impl Session {
    /// `channel` read up to its newest message (or now), never backwards.
    pub async fn mark_read(&self, channel: &str) -> Result<()> {
        let newest = self
            .client
            .database()
            .query(
                Filter::new()
                    .kind(Kind::ChatMessage)
                    .custom_tag(SingleLetterTag::LOWERCASE_H, channel)
                    .limit(1),
            )
            .await?
            .first()
            .map_or(0, |ev| ev.created_at.as_secs());
        let at = newest.max(Timestamp::now().as_secs());
        self.edit_reads(|r| {
            let read = r.read.entry(channel.into()).or_default();
            *read = (*read).max(at);
        })
    }

    pub fn set_notify_level(&self, community: &str, level: NotifyLevel) -> Result<()> {
        self.edit_reads(|r| {
            r.notify.insert(community.into(), level);
        })
    }

    pub(crate) fn notify_level(&self, community: &str) -> NotifyLevel {
        self.reads(|r| r.notify.get(community).copied().unwrap_or_default())
    }

    /// Where `channel` was read up to; the first time, now: its history counts as read.
    pub(crate) fn last_read(&self, channel: &str) -> Result<u64> {
        match self.reads(|r| r.read.get(channel).copied()) {
            Some(at) => Ok(at),
            None => {
                let now = Timestamp::now().as_secs();
                self.edit_reads(|r| {
                    r.read.insert(channel.into(), now);
                })?;
                Ok(now)
            }
        }
    }

    fn edit_reads(&self, change: impl FnOnce(&mut Reads)) -> Result<()> {
        let mut reads = self.reads.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut reads);
        std::fs::write(self.dir().join(FILE), serde_json::to_vec(&*reads)?)?;
        Ok(())
    }

    fn reads<T>(&self, read: impl FnOnce(&Reads) -> T) -> T {
        read(&self.reads.lock().unwrap_or_else(|e| e.into_inner()))
    }
}
