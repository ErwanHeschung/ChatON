//! Who I don't hear from: my blocks (NIP-51 `10000`, private, spec protocol § 5)
//! and my instance's rules (`/.well-known/chaton/rules`, spec federation § 6).

use nostr::event::FinalizeEvent;
use nostr::nips::nip44::{self, Version};
use nostr_sdk::prelude::{Filter, Kind, PublicKey, Tag, Timestamp};
use serde::{Deserialize, Serialize};

use crate::session::{Session, http};
use crate::{Error, Result, event};

/// My blocks: people and whole instances. Nobody else can read them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Blocks {
    pub keys: Vec<PublicKey>,
    /// Domains, lowercase (`b.fr`, `localhost:8098`).
    pub instances: Vec<String>,
}

/// My instance's rules for other instances: what its server publishes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    /// `blacklist` (default) or `whitelist`.
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub instances: Vec<String>,
}

impl Rules {
    pub fn whitelist(&self) -> bool {
        self.mode == "whitelist"
    }

    /// May someone from `domain` (`None`: unknown) write to me?
    pub fn allows(&self, domain: Option<&str>) -> bool {
        match domain {
            None => !self.whitelist(),
            Some(d) => self.instances.iter().any(|i| i == d) == self.whitelist(),
        }
    }

    /// Fetched once per session; an instance without rules (or down) has none.
    pub(crate) async fn fetch(relay: &str) -> Self {
        let url = relay.replacen("ws", "http", 1) + "/.well-known/chaton/rules";
        let rules = async {
            http()
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await
        };
        let rules: std::result::Result<Self, reqwest::Error> = rules.await;
        let mut rules = rules.unwrap_or_default();
        for d in &mut rules.instances {
            *d = d.to_lowercase();
        }
        rules
    }
}

/// The instance part of a handle, lowercase.
pub(crate) fn domain(handle: &str) -> Option<String> {
    Some(handle.split_once('@')?.1.to_lowercase())
}

impl Blocks {
    fn to_tags(&self) -> Vec<Vec<String>> {
        let keys = self.keys.iter().map(|k| vec!["p".into(), k.to_hex()]);
        let instances = self
            .instances
            .iter()
            .map(|d| vec!["instance".into(), d.clone()]);
        keys.chain(instances).collect()
    }

    fn from_tags(tags: Vec<Vec<String>>) -> Self {
        let mut blocks = Self::default();
        for tag in tags {
            match tag.as_slice() {
                [k, v, ..] if k == "p" => blocks.keys.extend(PublicKey::from_hex(v).ok()),
                [k, v, ..] if k == "instance" => blocks.instances.push(v.to_lowercase()),
                _ => {}
            }
        }
        blocks
    }
}

impl Session {
    /// My blocks, from my latest `10000` (kept in sync by `sync_communities`).
    pub async fn blocks(&self) -> Result<Blocks> {
        Ok(self.blocks_event().await?.1)
    }

    /// My latest `10000` and what it says.
    async fn blocks_event(&self) -> Result<(Option<Timestamp>, Blocks)> {
        let mine = Filter::new()
            .author(self.account().pubkey)
            .kind(Kind::MuteList)
            .limit(1);
        let Some(ev) = self.client.database().query(mine).await?.first().cloned() else {
            return Ok((None, Blocks::default()));
        };
        let me = self.account().pubkey;
        let plain = nip44::decrypt(self.keys.secret_key(), &me, &ev.content)
            .map_err(|e| Error::Invalid(e.to_string()))?;
        Ok((
            Some(ev.created_at),
            Blocks::from_tags(serde_json::from_str(&plain)?),
        ))
    }

    pub async fn block_key(&self, key: PublicKey) -> Result<Blocks> {
        self.edit_blocks(|b| {
            if !b.keys.contains(&key) {
                b.keys.push(key);
            }
        })
        .await
    }

    pub async fn unblock_key(&self, key: PublicKey) -> Result<Blocks> {
        self.edit_blocks(|b| b.keys.retain(|k| *k != key)).await
    }

    pub async fn block_instance(&self, domain: &str) -> Result<Blocks> {
        let domain = domain.trim().to_lowercase();
        if domain.is_empty()
            || Some(&domain) == crate::block::domain(&self.account().handle).as_ref()
        {
            return Err(Error::Invalid("can't block your own instance".into()));
        }
        self.edit_blocks(|b| {
            if !b.instances.contains(&domain) {
                b.instances.push(domain.clone());
            }
        })
        .await
    }

    pub async fn unblock_instance(&self, domain: &str) -> Result<Blocks> {
        let domain = domain.trim().to_lowercase();
        self.edit_blocks(|b| b.instances.retain(|d| *d != domain))
            .await
    }

    /// My instance's rules, as fetched when the session opened.
    pub fn instance_rules(&self) -> &Rules {
        &self.rules
    }

    /// Republishes my whole list (replaceable), encrypted for myself only.
    async fn edit_blocks(&self, edit: impl FnOnce(&mut Blocks)) -> Result<Blocks> {
        let (last, mut blocks) = self.blocks_event().await?;
        edit(&mut blocks);
        // Replaceable: same second = lowest id wins, so always move forward.
        let at = last.map_or(Timestamp::now(), |t| Timestamp::now().max(t + 1));
        let me = self.account().pubkey;
        let content = nip44::encrypt(
            self.keys.secret_key(),
            &me,
            serde_json::to_string(&blocks.to_tags())?,
            Version::V2,
        )
        .map_err(|e| Error::Invalid(e.to_string()))?;
        let ev = event::builder(Kind::MuteList, Vec::<Tag>::new(), &content)?
            .custom_created_at(at)
            .finalize(&self.keys)?;
        let out = self
            .client
            .send_event(&ev)
            .to([self.account().relay.as_str()])
            .await?;
        if out.success.is_empty() {
            return Err(Error::NotDelivered(format!("{:?}", out.failed)));
        }
        Ok(blocks)
    }

    /// Not shown, not kept, not notified: a sender I block, from an instance I
    /// block, or one my instance's rules refuse (its verified handle decides).
    pub(crate) async fn hidden(&self, sender: PublicKey, blocks: &Blocks) -> bool {
        let me = self.account();
        if sender == me.pubkey {
            return false;
        }
        if blocks.keys.contains(&sender) {
            return true;
        }
        if blocks.instances.is_empty() && self.rules.instances.is_empty() && !self.rules.whitelist()
        {
            return false; // nothing about instances: don't look the sender up
        }
        let theirs = self.peer(sender).await.handle.as_deref().and_then(domain);
        if theirs.is_some() && theirs == domain(&me.handle) {
            return false; // my own instance: only my own blocks apply
        }
        theirs
            .as_ref()
            .is_some_and(|d| blocks.instances.contains(d))
            || !self.rules.allows(theirs.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::Keys;

    #[test]
    fn rules_blacklist_and_whitelist() {
        let black = Rules {
            mode: "blacklist".into(),
            instances: vec!["spam.example".into()],
        };
        assert!(!black.allows(Some("spam.example")));
        assert!(black.allows(Some("ok.example")));
        assert!(black.allows(None));
        assert!(Rules::default().allows(None));
        let white = Rules {
            mode: "whitelist".into(),
            instances: vec!["friend.example".into()],
        };
        assert!(white.allows(Some("friend.example")));
        assert!(!white.allows(Some("ok.example")));
        assert!(!white.allows(None));
    }

    #[test]
    fn blocks_round_trip_through_tags() {
        let blocks = Blocks {
            keys: vec![Keys::generate().public_key()],
            instances: vec!["b.example".into()],
        };
        let json = serde_json::to_string(&blocks.to_tags()).unwrap();
        assert_eq!(
            Blocks::from_tags(serde_json::from_str(&json).unwrap()),
            blocks
        );
        assert_eq!(domain("Bob@B.Example").as_deref(), Some("b.example"));
    }
}
