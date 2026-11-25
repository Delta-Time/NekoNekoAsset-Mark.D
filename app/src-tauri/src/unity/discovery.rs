use super::wire::{hello, is_uuid, LiveEditor, Registry, MAX_CONTROL_BYTES};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use futures_util::{stream, StreamExt};
use std::path::Path;
use tokio::io::AsyncReadExt;

// Only its public summary is eligible for a frontend DTO.
pub struct DiscoveredEditor {
    registry: Registry,
    summary: LiveEditor,
}

impl DiscoveredEditor {
    pub fn summary(&self) -> &LiveEditor {
        &self.summary
    }
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

fn fresh(registry: &Registry, now: DateTime<Utc>) -> bool {
    let Ok(heartbeat) = DateTime::parse_from_rfc3339(&registry.heartbeat_utc) else {
        return false;
    };
    let age = now.signed_duration_since(heartbeat);
    registry.enabled && age >= chrono::Duration::zero() && age < chrono::Duration::seconds(10)
}

fn linked(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

pub async fn discover(registry_dir: &Path) -> Result<Vec<DiscoveredEditor>> {
    let directory = match tokio::fs::symlink_metadata(registry_dir).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err(anyhow!("UNITY_REGISTRY_INVALID")),
    };
    if !directory.is_dir() || linked(&directory) {
        return Err(anyhow!("UNITY_REGISTRY_INVALID"));
    }
    let mut entries = tokio::fs::read_dir(registry_dir)
        .await
        .map_err(|_| anyhow!("UNITY_REGISTRY_INVALID"))?;
    let mut registries = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|_| anyhow!("UNITY_REGISTRY_INVALID"))?
    {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Some(session) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !is_uuid(session) {
            continue;
        }
        let Ok(metadata) = tokio::fs::symlink_metadata(&path).await else {
            continue;
        };
        if !metadata.is_file()
            || linked(&metadata)
            || metadata.len() == 0
            || metadata.len() > MAX_CONTROL_BYTES as u64
        {
            continue;
        }
        // Atomic replacement gives either published generation; partial/invalid files are skipped.
        let Ok(file) = tokio::fs::File::open(&path).await else {
            continue;
        };
        let mut bytes = Vec::new();
        if file
            .take(MAX_CONTROL_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .await
            .is_err()
            || bytes.len() > MAX_CONTROL_BYTES
        {
            continue;
        }
        let Ok(registry) = serde_json::from_slice::<Registry>(&bytes) else {
            continue;
        };
        if registry.session_id != session
            || registry.validate().is_err()
            || !fresh(&registry, Utc::now())
        {
            continue;
        }
        registries.push(registry);
    }
    let mut live: Vec<_> = stream::iter(registries)
        .map(|registry| async move {
            let summary = hello(&registry).await.ok()?;
            if !fresh(&registry, Utc::now()) {
                return None;
            }
            Some(DiscoveredEditor { registry, summary })
        })
        .buffer_unordered(4)
        .filter_map(|editor| async move { editor })
        .collect()
        .await;
    live.sort_by(|a, b| {
        a.summary
            .instance_id
            .cmp(&b.summary.instance_id)
            .then(a.summary.session_id.cmp(&b.summary.session_id))
    });
    Ok(live)
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
