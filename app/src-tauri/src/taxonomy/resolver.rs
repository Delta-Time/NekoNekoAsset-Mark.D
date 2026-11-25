//! Pure exact compatibility lookup over a caller-supplied canonical snapshot.
//! The catalogue owns snapshot lifetime, overlays and re-projection. No DB here.
use super::{names, normalize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct AvatarTagResolver {
    entries: BTreeMap<String, Value>,
    keys: BTreeMap<String, BTreeSet<String>>,
}

fn usable_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.chars().any(char::is_alphabetic)
        && !["booth:", "product:", "avatar:"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        && !name.contains("://")
}

fn name_key(name: &str) -> String {
    normalize(&names::cleaned_title(name))
}

impl AvatarTagResolver {
    /// Supply canonical dictionary + effective owned Base entries after explicit
    /// identity links and human name/alias overrides. Distinct IDs are never
    /// merged by spelling. Duplicate entries of one ID contribute their aliases.
    pub fn from_options(options: &[Value]) -> Self {
        let mut resolver = Self::default();
        for entry in options {
            let Some(id) = entry["id"].as_str().filter(|id| !id.is_empty()) else {
                continue;
            };
            resolver.entries.insert(id.to_owned(), entry.clone());
            // Uncertain labels remain available through explicit IDs; their
            // words and bundle components do not establish one avatar name.
            if matches!(entry["nameKind"].as_str(), Some("bundle" | "fallback")) {
                continue;
            }
            let names: BTreeSet<_> = ["officialName", "englishName", "displayName"]
                .into_iter()
                .filter_map(|field| entry[field].as_str())
                .chain(
                    entry["aliases"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str),
                )
                .map(name_key)
                .filter(|name| usable_name(name))
                .collect();
            for name in &names {
                resolver.insert(name, id);
            }
            // Generate complete same-identity pairs, never split an input tag
            // and accept only its recognizable half. This keeps qualifiers and
            // mixed-avatar labels from turning into an automatic association.
            let simple: Vec<_> = names
                .iter()
                .filter(|name| !name.contains(['/', '(', ')', '「', '」', '『', '』', '"']))
                .collect();
            for left in &simple {
                for right in &simple {
                    for pair in [
                        format!("{left}/{right}"),
                        format!("{left} / {right}"),
                        format!("{left} {right}"),
                        format!("{left}-{right}"),
                        format!("{left} -{right}-"),
                        format!("{left} - {right}"),
                        format!("{left}({right})"),
                        format!("{left} ({right})"),
                        format!("{left}「{right}」"),
                        format!("{left} 「{right}」"),
                        format!("{left}『{right}』"),
                        format!("「{left}」 {right}"),
                    ] {
                        resolver.insert(&pair, id);
                    }
                }
            }
        }
        // A special edition does not make its plain product's name ambiguous.
        let editions: BTreeSet<_> = options
            .iter()
            .filter(|entry| super::avatar_edition(entry))
            .filter_map(|entry| entry["id"].as_str())
            .collect();
        for ids in resolver.keys.values_mut() {
            if ids.iter().any(|id| !editions.contains(id.as_str())) {
                ids.retain(|id| !editions.contains(id.as_str()));
            }
        }
        resolver
    }

    fn insert(&mut self, key: &str, id: &str) {
        self.keys
            .entry(key.to_owned())
            .or_default()
            .insert(id.to_owned());
    }

    fn tag_candidates<'a>(&'a self, key: &str, candidates: &mut BTreeSet<&'a String>) {
        if let Some(ids) = self.keys.get(key) {
            candidates.extend(ids.iter());
        }
        // Accept exactly one conventional honorific after a complete known
        // name. Keep its full spelling as a candidate too: an actual name
        // ending in ちゃん must not be silently assigned to another identity.
        if let Some(name) = key.strip_suffix("ちゃん").filter(|name| {
            !name.is_empty()
                && !name.ends_with("ちゃん")
                && !name.chars().next_back().is_some_and(char::is_whitespace)
        }) {
            if let Some(ids) = self.keys.get(name) {
                candidates.extend(ids.iter());
            }
        }
    }

    fn identity(&self, tag: &str) -> Option<&str> {
        let full = name_key(tag);
        if !usable_name(&full) {
            return None;
        }
        let mut candidates = BTreeSet::new();
        self.tag_candidates(&full, &mut candidates);
        for suffix in [
            "対応", "専用", "用", "對應", "对应", "向け", "전용", "대응", "용",
        ] {
            if let Some(bare) = full.strip_suffix(suffix) {
                self.tag_candidates(&name_key(bare.trim()), &mut candidates);
            }
        }
        if candidates.len() == 1 {
            candidates.into_iter().next().map(String::as_str)
        } else {
            None
        }
    }

    /// Exact compatible-avatar evidence only. Includes no purchase, entitlement,
    /// ownership or file-access inference. Ambiguous shared names return None.
    pub fn resolve_tag(&self, tag: &str) -> Option<Value> {
        let id = self.identity(tag)?;
        let mut candidate = self.entries.get(id)?.clone();
        candidate["basis"] = json!("exactTag");
        candidate["matchedAliases"] = json!([tag]);
        candidate["entitlementEstablished"] = json!(false);
        Some(candidate)
    }

    /// Explicit human choices may use a canonical ID even when names collide.
    /// Automatic tag matching continues to reject ID-shaped text.
    pub fn resolve_manual_avatar(&self, name: &str) -> Option<Value> {
        self.entries
            .get(name)
            .cloned()
            .or_else(|| self.resolve_tag(name))
    }

    /// Use for rejection equivalence within this same snapshot. A unique name
    /// returns its ID; ambiguous/unrecognized text remains plain normalized
    /// text, with no dictionary fallback that could conceal a collision.
    pub fn canonical_tag(&self, tag: &str) -> String {
        self.identity(tag)
            .map(str::to_owned)
            .unwrap_or_else(|| normalize(tag))
    }
}

#[cfg(test)]
#[path = "resolver_tests.rs"]
mod tests;
