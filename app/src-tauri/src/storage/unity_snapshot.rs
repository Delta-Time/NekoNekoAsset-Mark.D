//! Exact catalogue leaf selection and verified, App-owned Bridge payload copies.
use super::{archive, artifact_id, io, StorageService};
use anyhow::{anyhow, ensure, Result};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};
use uuid::Uuid;

/// A file Unity can import: a whole package, or one member of an extracted archive.
#[derive(Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct UnityLeaf {
    pub leaf_ref: String,
    pub product_id: String,
    pub product_title: String,
    pub file_id: String,
    pub source_name: String,
    pub variant_name: String,
    pub revision: String,
    pub member_path: String,
    pub display_name: String,
    pub relative_path: String,
    #[ts(type = "'unitypackage' | 'file'")]
    pub kind: String,
    pub bytes: u64,
    pub sha256: String,
}

// Local source/snapshot paths are never IPC DTOs or diagnostic output.
pub struct UnitySnapshotEntry {
    pub entry_id: String,
    pub leaf: UnityLeaf,
    pub path: PathBuf,
}
pub struct UnitySnapshot {
    // Only tests read it, to check where the snapshot was written.
    #[cfg_attr(not(test), allow(dead_code))]
    pub directory: PathBuf,
    pub entries: Vec<UnitySnapshotEntry>,
}
struct ResolvedLeaf {
    leaf: UnityLeaf,
    source: PathBuf,
    remote: Value,
    artifact: Arc<Value>,
    root: Value,
}

impl StorageService {
    pub async fn list_unity_inventory(&self, product_ids: &[String]) -> Result<Vec<UnityLeaf>> {
        let guard = self.operations.clone().read_owned().await;
        let service = self.clone();
        let products = product_ids.to_vec();
        tokio::task::spawn_blocking(move || {
            let _guard = guard;
            Ok(service
                .resolve_unity_leaves(&products)?
                .into_iter()
                .map(|entry| entry.leaf)
                .collect())
        })
        .await?
    }

    pub async fn prepare_unity_snapshot(
        &self,
        request_id: &str,
        product_ids: &[String],
        leaf_refs: &[String],
        cancel: Arc<AtomicBool>,
    ) -> Result<UnitySnapshot> {
        let directory = self.unity_snapshot_directory(request_id)?;
        ensure!(!leaf_refs.is_empty(), "unity_selection_empty");
        let requested: HashSet<_> = leaf_refs.iter().collect();
        ensure!(
            requested.len() == leaf_refs.len(),
            "unity_selection_duplicate"
        );
        let guard = self.operations.clone().read_owned().await;
        let service = self.clone();
        let products = product_ids.to_vec();
        let refs = leaf_refs.to_vec();
        tokio::task::spawn_blocking(move || -> Result<UnitySnapshot> {
            // An aborted async waiter cannot release this gate while the copy
            // worker is still reading managed storage.
            let _guard = guard;
            io::check_cancel(&cancel)?;
            let mut available: HashMap<_, _> = service
                .resolve_unity_leaves(&products)?
                .into_iter()
                .map(|entry| (entry.leaf.leaf_ref.clone(), entry))
                .collect();
            let selected: Vec<_> = refs
                .iter()
                .map(|reference| {
                    available
                        .remove(reference)
                        .ok_or_else(|| anyhow!("unity_leaf_stale"))
                })
                .collect::<Result<_>>()?;
            require_distinct_paths(&selected)?;
            let required = selected.iter().try_fold(0u64, |total, entry| {
                total
                    .checked_add(entry.leaf.bytes)
                    .ok_or_else(|| anyhow!("unity_payload_too_large"))
            })?;
            let parent = directory
                .parent()
                .ok_or_else(|| anyhow!("unity_snapshot_invalid"))?;
            archive::validate_no_links(parent)?;
            fs::create_dir_all(parent)?;
            archive::validate_no_links(parent)?;
            io::space(parent, required)?;
            // A reused request ID never overwrites an existing request payload.
            fs::create_dir(&directory)?;
            let mut owned = SnapshotGuard {
                directory: directory.clone(),
                armed: true,
            };
            archive::validate_no_links(&directory)?;
            let mut entries = Vec::with_capacity(selected.len());
            for selected in &selected {
                io::check_cancel(&cancel)?;
                // Recheck the complete managed source chain at the read boundary.
                let source = service.unity_source_path(selected)?;
                ensure!(source == selected.source, "unity_leaf_stale");
                let entry_id = Uuid::new_v4().to_string();
                let target = io::safe_join(&directory, &entry_id)?;
                copy_snapshot(
                    &source,
                    &target,
                    &selected.leaf.sha256,
                    selected.leaf.bytes,
                    &cancel,
                )?;
                entries.push(UnitySnapshotEntry {
                    entry_id,
                    leaf: selected.leaf.clone(),
                    path: target,
                });
            }
            // The storage read gate permits ordinary download/source publication.
            // Do not publish a selection whose observed records changed during I/O.
            let current: HashMap<_, _> = service
                .resolve_unity_leaves(&products)?
                .into_iter()
                .map(|entry| (entry.leaf.leaf_ref.clone(), entry))
                .collect();
            for selected in &selected {
                let fresh = current
                    .get(&selected.leaf.leaf_ref)
                    .ok_or_else(|| anyhow!("unity_leaf_stale"))?;
                ensure!(
                    fresh.remote == selected.remote
                        && fresh.artifact == selected.artifact
                        && fresh.root == selected.root,
                    "unity_leaf_stale"
                );
            }
            io::check_cancel(&cancel)?;
            owned.armed = false;
            Ok(UnitySnapshot { directory, entries })
        })
        .await?
    }

    /// The managed file of one selected unitypackage, which Unity's own importer reads
    /// in place. Other kinds return None; they always use the verified transfer.
    pub async fn unity_package_path(
        &self,
        product_ids: &[String],
        leaf_ref: &str,
    ) -> Result<Option<PathBuf>> {
        let guard = self.operations.clone().read_owned().await;
        let service = self.clone();
        let products = product_ids.to_vec();
        let leaf_ref = leaf_ref.to_owned();
        tokio::task::spawn_blocking(move || {
            let _guard = guard;
            let selected = service
                .resolve_unity_leaves(&products)?
                .into_iter()
                .find(|entry| entry.leaf.leaf_ref == leaf_ref)
                .ok_or_else(|| anyhow!("unity_leaf_stale"))?;
            Ok((selected.leaf.kind == "unitypackage").then_some(selected.source))
        })
        .await?
    }

    /// Caller supplies a request ID only, after protocol-approved receipt proof.
    pub fn cleanup_unity_snapshot(&self, request_id: &str) -> Result<()> {
        clean_snapshot(&self.unity_snapshot_directory(request_id)?)
    }

    fn unity_snapshot_directory(&self, request_id: &str) -> Result<PathBuf> {
        let id = Uuid::parse_str(request_id).map_err(|_| anyhow!("unity_request_invalid"))?;
        ensure!(id.to_string() == request_id, "unity_request_invalid");
        io::safe_join(
            &self.catalogue.data_dir(),
            &format!("unity/requests/{id}/snapshot"),
        )
    }

    fn resolve_unity_leaves(&self, products: &[String]) -> Result<Vec<ResolvedLeaf>> {
        let mut result = Vec::new();
        for product in products.iter().collect::<BTreeSet<_>>() {
            ensure!(
                self.catalogue.get("product", product)?.is_some(),
                "product_missing"
            );
            let title = self
                .catalogue
                .command("product_details", json!({"id":product}))?["title"]
                .as_str()
                .unwrap_or(product)
                .to_owned();
            let start = result.len();
            for remote in self.kept_files(product)? {
                let revision = remote["revision"].as_str().unwrap_or("");
                if revision.is_empty() || revision == "unknown" {
                    continue;
                }
                let Some(artifact) = self.catalogue.get("artifact", &artifact_id(&remote))? else {
                    continue;
                };
                if artifact["productId"].as_str() != Some(product.as_str())
                    || artifact["fileId"] != remote["id"]
                    || artifact["revision"] != remote["revision"]
                    || !matches!(
                        artifact["status"].as_str(),
                        Some("ready" | "downloaded" | "extractionFailed")
                    )
                {
                    continue;
                }
                let Some(root_id) = artifact["rootId"].as_str() else {
                    continue;
                };
                let Some(root) = self.catalogue.get("root", root_id)? else {
                    continue;
                };
                if self.root_path(&root).is_err() {
                    continue;
                }
                // Every member retains the same complete inventory for stale checks.
                let artifact = Arc::new(artifact);
                if artifact["originalAvailable"] == true {
                    let name = remote["sourceRelativePath"]
                        .as_str()
                        .or(remote["name"].as_str())
                        .unwrap_or("");
                    // A store name Windows cannot store was saved under its io::managed_name.
                    let name = if archive::validate_relative_path(name).is_ok() {
                        Some(name.to_owned())
                    } else if remote["sourceRelativePath"].is_string() {
                        super::transfer::managed_source_path(&remote).ok()
                    } else {
                        (!name.is_empty()).then_some(io::managed_name(name))
                    };
                    if let Some(leaf) = name.and_then(|name| {
                        self.unity_leaf(&remote, &artifact, &root, "", &name, &artifact)
                            .ok()
                    }) {
                        result.push(leaf);
                    }
                }
                if artifact["status"] == "ready" {
                    for entry in artifact["extractedInventory"]
                        .as_array()
                        .into_iter()
                        .flatten()
                    {
                        if entry["isDirectory"] == true {
                            continue;
                        }
                        let Some(member) = entry["relativePath"].as_str() else {
                            continue;
                        };
                        if let Ok(leaf) =
                            self.unity_leaf(&remote, &artifact, &root, member, member, entry)
                        {
                            result.push(leaf);
                        }
                    }
                }
            }
            for entry in &mut result[start..] {
                entry.leaf.product_title = title.clone();
            }
        }
        result.sort_by(|a, b| {
            (&a.leaf.product_id, &a.leaf.file_id, &a.leaf.member_path).cmp(&(
                &b.leaf.product_id,
                &b.leaf.file_id,
                &b.leaf.member_path,
            ))
        });
        Ok(result)
    }

    fn unity_leaf(
        &self,
        remote: &Value,
        artifact: &Arc<Value>,
        root: &Value,
        member: &str,
        name: &str,
        fact: &Value,
    ) -> Result<ResolvedLeaf> {
        let relative_path = archive::validate_relative_path(name)?
            .to_str()
            .ok_or_else(|| anyhow!("unity_leaf_invalid"))?
            .replace('\\', "/");
        let bytes = fact["sizeBytes"]
            .as_u64()
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(|| anyhow!("unity_leaf_invalid"))?;
        let hash = io::required_string(fact, "hash")?;
        ensure!(
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "unity_leaf_invalid"
        );
        let display_name = relative_path.rsplit('/').next().unwrap_or("").to_owned();
        let kind = if display_name.to_ascii_lowercase().ends_with(".unitypackage") {
            "unitypackage"
        } else {
            "file"
        }
        .to_owned();
        let leaf_ref = io::key(&serde_json::to_string(&json!([
            remote["productId"],
            remote["id"],
            remote["revision"],
            artifact["id"],
            root["id"],
            root["path"],
            artifact["relativePath"],
            artifact["extractedPath"],
            member,
            relative_path,
            hash,
            bytes
        ]))?);
        let mut selected = ResolvedLeaf {
            leaf: UnityLeaf {
                leaf_ref,
                product_id: io::required_string(remote, "productId")?.to_owned(),
                product_title: String::new(),
                file_id: io::required_string(remote, "id")?.to_owned(),
                source_name: remote["name"].as_str().unwrap_or("").to_owned(),
                variant_name: remote["variantName"].as_str().unwrap_or("").to_owned(),
                revision: io::required_string(remote, "revision")?.to_owned(),
                member_path: member.to_owned(),
                display_name,
                relative_path,
                kind,
                bytes,
                sha256: hash.to_owned(),
            },
            source: PathBuf::new(),
            remote: remote.clone(),
            artifact: Arc::clone(artifact),
            root: root.clone(),
        };
        selected.source = self.unity_source_path(&selected)?;
        io::ensure_regular(&selected.source)?;
        ensure!(
            fs::metadata(&selected.source)?.len() == bytes,
            "file_integrity_failed"
        );
        Ok(selected)
    }

    fn unity_source_path(&self, selected: &ResolvedLeaf) -> Result<PathBuf> {
        let root = self.root_path(&selected.root)?;
        if selected.leaf.member_path.is_empty() {
            io::safe_join(
                &root,
                io::required_string(&selected.artifact, "relativePath")?,
            )
        } else {
            let base = io::safe_join(
                &root,
                io::required_string(&selected.artifact, "extractedPath")?,
            )?;
            io::safe_join(&base, &selected.leaf.member_path)
        }
    }
}

// Hash the source buffers while copying, rather than rereading the entire copy.
// The sender hashes this immutable snapshot again as it sends it; the receiver
// checks the same digest before staging or allowing any project writes.
fn copy_snapshot(
    source: &Path,
    target: &Path,
    expected_hash: &str,
    expected_size: u64,
    cancel: &AtomicBool,
) -> Result<()> {
    io::ensure_regular(source)?;
    let mut input = fs::File::open(source)?;
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(target)?;
    let mut hash = Sha256::new();
    let mut copied = 0u64;
    let mut buffer = [0u8; 128 * 1024];
    loop {
        io::check_cancel(cancel)?;
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        copied = copied
            .checked_add(count as u64)
            .ok_or_else(|| anyhow!("file_integrity_failed"))?;
        ensure!(copied <= expected_size, "file_integrity_failed");
        hash.update(&buffer[..count]);
        output.write_all(&buffer[..count])?;
    }
    ensure!(
        copied == expected_size && hex::encode(hash.finalize()) == expected_hash,
        "file_integrity_failed"
    );
    io::check_cancel(cancel)?;
    output.sync_all()?;
    Ok(())
}

fn require_distinct_paths(selected: &[ResolvedLeaf]) -> Result<()> {
    let mut paths = BTreeSet::new();
    for entry in selected {
        let path = entry.leaf.relative_path.to_lowercase();
        ensure!(paths.insert(path), "unity_path_collision");
    }
    for path in &paths {
        for (index, _) in path.match_indices('/') {
            ensure!(!paths.contains(&path[..index]), "unity_path_collision");
        }
    }
    Ok(())
}

struct SnapshotGuard {
    directory: PathBuf,
    armed: bool,
}
impl Drop for SnapshotGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = clean_snapshot(&self.directory);
        }
    }
}
fn clean_snapshot(directory: &Path) -> Result<()> {
    archive::validate_no_links(directory)?;
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow!("unity_snapshot_invalid"))?;
        let id = Uuid::parse_str(&name).map_err(|_| anyhow!("unity_snapshot_invalid"))?;
        ensure!(id.to_string() == name, "unity_snapshot_invalid");
        let path = io::safe_join(directory, &name)?;
        io::ensure_regular(&path)?;
        paths.push(path);
    }
    for path in paths {
        archive::validate_no_links(&path)?;
        io::ensure_regular(&path)?;
        fs::remove_file(path)?;
    }
    fs::remove_dir(directory)?;
    // The request directory may contain a root-owned receipt; retain it then.
    if let Some(parent) = directory.parent() {
        let _ = fs::remove_dir(parent);
    }
    Ok(())
}

#[cfg(test)]
#[path = "unity_snapshot_tests.rs"]
mod tests;
