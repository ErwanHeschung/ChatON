//! The instances I know (mine, my contacts', my communities'), and whether they
//! answer: what the app shows as a map. No directory: only what I already talk to.

use std::collections::BTreeMap;
use std::time::Instant;

use nostr_sdk::prelude::PublicKey;
use serde::{Deserialize, Serialize};

use crate::Result;
use crate::block::domain;
use crate::media;
use crate::session::{Session, http, instance_url};

#[derive(Debug, Clone, Default, Serialize)]
pub struct Instance {
    /// `b.fr`, `localhost:8098`.
    pub domain: String,
    pub mine: bool,
    /// My contacts there (verified handles).
    pub people: Vec<PublicKey>,
    /// How many of my communities it hosts.
    pub communities: usize,
    /// By me or by my instance's rules.
    pub blocked: bool,
    /// It answered its NIP-11 document.
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    /// NIP-11 `name`.
    pub name: Option<String>,
    /// NIP-11 `relay_countries` (ISO 3166 alpha-2), as its admin says.
    pub countries: Vec<String>,
}

#[derive(Deserialize)]
struct Nip11 {
    name: Option<String>,
    #[serde(default)]
    relay_countries: Vec<String>,
}

impl Session {
    /// Every instance I know, mine first, each asked for its NIP-11 document (at once).
    pub async fn instances(&self) -> Result<Vec<Instance>> {
        let mine = domain(&self.account().handle).unwrap_or_default();
        let mut known: BTreeMap<String, Instance> = BTreeMap::new();
        fn entry<'a>(
            known: &'a mut BTreeMap<String, Instance>,
            d: String,
            mine: &str,
        ) -> &'a mut Instance {
            known.entry(d.clone()).or_insert_with(|| Instance {
                mine: d == mine,
                domain: d,
                ..Default::default()
            })
        }
        entry(&mut known, mine.clone(), &mine);
        let mut people: Vec<PublicKey> = Vec::new();
        for c in self.conversations().await? {
            people.extend(c.room);
        }
        people.sort();
        people.dedup();
        for pk in people.into_iter().filter(|pk| *pk != self.account().pubkey) {
            if let Some(d) = self.peer(pk).await.handle.as_deref().and_then(domain) {
                entry(&mut known, d, &mine).people.push(pk);
            }
        }
        for relay in self.communities_relays().await? {
            if let Some(d) = media::host(&relay) {
                entry(&mut known, d.to_lowercase(), &mine).communities += 1;
            }
        }
        let blocks = self.blocks().await?;
        let rules = self.instance_rules();
        let mut probes = tokio::task::JoinSet::new();
        for (i, (d, instance)) in known.iter_mut().enumerate() {
            instance.blocked =
                !instance.mine && (blocks.instances.contains(d) || !rules.allows(Some(d)));
            let url = instance_url(&format!("@{d}")).unwrap_or_default();
            probes.spawn(async move { (i, probe(&url).await) });
        }
        let mut all: Vec<Instance> = known.into_values().collect();
        while let Some(Ok((i, answer))) = probes.join_next().await {
            if let Some((ms, doc)) = answer {
                let instance = &mut all[i];
                (instance.reachable, instance.latency_ms) = (true, Some(ms));
                // Free text from its admin: cleaned like a person's display name.
                instance.name = doc.name.as_deref().and_then(crate::session::display_name);
                instance.countries = doc
                    .relay_countries
                    .into_iter()
                    .map(|c| c.to_uppercase())
                    .filter(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_uppercase()))
                    .collect();
            }
        }
        all.sort_by_key(|i| !i.mine);
        Ok(all)
    }
}

/// Its NIP-11 document, and how long it took; `None`: unreachable.
async fn probe(url: &str) -> Option<(u64, Nip11)> {
    let start = Instant::now();
    let resp = http()
        .get(url)
        .header("Accept", "application/nostr+json")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?;
    let ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    Some((ms, resp.json().await.ok()?))
}
