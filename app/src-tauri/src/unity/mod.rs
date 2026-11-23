//! Selected-file Unity bridge. No catalogue or credentials are exposed to Editors.
pub mod api;
pub mod discovery;
pub mod jobs;
pub mod wire;

use crate::{catalogue::Catalogue, storage::StorageService};
use anyhow::{bail, ensure, Result};
use api::UnityRequest;
use jobs::{RequestRecord, RequestStore};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tauri::Emitter;
use tokio::sync::Mutex;
use wire::{Manifest, ManifestEntry, PayloadFile, RequestIdentity};

pub struct UnityService {
    storage: Arc<StorageService>,
    requests: RequestStore,
    registry_dir: PathBuf,
    started: Mutex<bool>,
    workers: Mutex<HashSet<String>>,
    cancellations: Mutex<HashMap<String, Arc<AtomicBool>>>,
    updates: Mutex<()>,
    connections: Mutex<HashMap<String, wire::Registry>>,
}

/// The arguments of a submit request.
struct Submit {
    locale: Option<String>,
    product_ids: Vec<String>,
    leaf_refs: Vec<String>,
    instance_id: String,
    session_id: String,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn emit(app: &tauri::AppHandle) {
    let _ = app.emit("nna:changed", crate::ipc::Change::scope("jobs"));
}
/// The code a job records for a failure.
fn error_code(error: anyhow::Error) -> String {
    let text = error.to_string();
    let code = text.split(':').next().unwrap_or("");
    // A listener that closed unanswered reads as a timeout: the request may still run, and
    // UNITY_NO_RESPONSE's text asks to retry the native-import click.
    if code == "UNITY_NO_RESPONSE" {
        return "UNITY_TIMEOUT".into();
    }
    if code.len() <= 80
        && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && (code.starts_with("UNITY_")
            || code.starts_with("unity_")
            || code.starts_with("file_")
            || code.starts_with("root_")
            || code == "low_space")
    {
        code.into()
    } else {
        "UNITY_UNAVAILABLE".into()
    }
}
fn terminal(phase: &str) -> bool {
    matches!(phase, "completed" | "cancelled" | "failed" | "interrupted")
}
/// Whether Unity's own importer reads this package; anything unreadable takes the Bridge.
async fn native_header(path: &str) -> bool {
    use tokio::io::AsyncReadExt;
    let Ok(file) = tokio::fs::File::open(path).await else {
        return false;
    };
    let mut header = Vec::with_capacity(1024);
    file.take(1024).read_to_end(&mut header).await.is_ok() && wire::native_import_reads(&header)
}
/// Unity's importer takes an ordinary path, not the verbatim form of canonical roots.
fn plain_path(path: &str) -> String {
    match path.strip_prefix(r"\\?\") {
        Some(rest) => rest
            .strip_prefix(r"UNC\")
            .map_or_else(|| rest.to_owned(), |share| format!(r"\\{share}")),
        None => path.to_owned(),
    }
}
fn identity(record: &RequestRecord) -> RequestIdentity {
    RequestIdentity {
        request_id: record.request_id.clone(),
        instance_id: record.instance_id.clone(),
        session_id: record.session_id.clone(),
        fingerprint: record.fingerprint.clone(),
    }
}

pub fn registry_directory(data_dir: &std::path::Path) -> Result<PathBuf> {
    // Authored native verification cannot discover the user's private Editors.
    #[cfg(debug_assertions)]
    if std::env::var_os("NNA_DATA_DIR").is_some() {
        return Ok(std::env::var_os("NNA_UNITY_REGISTRY_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.join("unity-review-instances")));
    }
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|v| PathBuf::from(v).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|v| PathBuf::from(v).join(".local/share")))
    }
    .ok_or_else(|| anyhow::anyhow!("UNITY_REGISTRY_UNAVAILABLE"))?;
    Ok(base.join("NekoNekoAsset/UnityBridge/instances"))
}

impl UnityService {
    pub fn new(catalogue: Catalogue, storage: Arc<StorageService>) -> Result<Self> {
        let registry_dir = registry_directory(&catalogue.device_dir())?;
        Ok(Self {
            storage,
            requests: RequestStore::new(Arc::new(catalogue)),
            registry_dir,
            started: Mutex::new(false),
            workers: Mutex::new(HashSet::new()),
            cancellations: Mutex::new(HashMap::new()),
            updates: Mutex::new(()),
            connections: Mutex::new(HashMap::new()),
        })
    }
    pub async fn start(self: &Arc<Self>, app: &tauri::AppHandle) -> Result<()> {
        let _work = self.requests.begin_catalogue_work()?;
        let mut started = self.started.lock().await;
        if *started {
            return Ok(());
        }
        for mut record in self.requests.cleanup_pending()? {
            self.save_and_clean(&mut record, true)?;
        }
        for mut record in self.requests.pending()? {
            if !self.requests.is_local(&record) {
                self.requests.mark_other_device(&record)?;
                continue;
            }
            // No submit replay on restart, including a half-finished preparation.
            if !record.bytes_started {
                record.phase = "interrupted".into();
                record.error_code = Some("UNITY_INTERRUPTED".into());
                record.updated_at = now();
                self.save_and_clean(&mut record, true)?;
            } else {
                self.spawn_query(app.clone(), record.request_id.clone())
                    .await;
            }
        }
        *started = true;
        emit(app);
        Ok(())
    }
    pub async fn command(
        self: &Arc<Self>,
        app: &tauri::AppHandle,
        request: UnityRequest,
    ) -> Result<Value> {
        self.start(app).await?;
        let cancel = matches!(request, UnityRequest::Cancel { .. });
        match request {
            UnityRequest::ListEditors => Ok(serde_json::to_value(api::Editors {
                editors: discovery::discover(&self.registry_dir)
                    .await?
                    .iter()
                    .map(|e| e.summary().clone())
                    .collect(),
            })?),
            UnityRequest::Inventory { product_ids } => Ok(serde_json::to_value(api::Inventory {
                files: self.storage.list_unity_inventory(&product_ids).await?,
            })?),
            UnityRequest::Submit {
                locale,
                product_ids,
                leaf_refs,
                instance_id,
                session_id,
            } => {
                self.submit(app, locale, product_ids, leaf_refs, instance_id, session_id)
                    .await
            }
            UnityRequest::Details { job_id } => {
                let record = self
                    .requests
                    .get(&job_id)?
                    .ok_or_else(|| anyhow::anyhow!("UNITY_REQUEST_MISSING"))?;
                details(&record)
            }
            UnityRequest::Status { job_id } | UnityRequest::Cancel { job_id } => {
                self.status(app, job_id, cancel).await
            }
            UnityRequest::RetryInventory { job_id } => self.retry_inventory(job_id).await,
        }
    }

    /// Opens one package in Unity's own importer, or starts a Bridge request.
    async fn submit(
        self: &Arc<Self>,
        app: &tauri::AppHandle,
        locale: Option<String>,
        product_ids: Vec<String>,
        leaf_refs: Vec<String>,
        instance_id: String,
        session_id: String,
    ) -> Result<Value> {
        let work = self.requests.begin_catalogue_work()?;
        let args = Submit {
            locale,
            product_ids,
            leaf_refs,
            instance_id,
            session_id,
        };
        ensure!(
            !args.product_ids.is_empty() && !args.leaf_refs.is_empty(),
            "UNITY_SELECTION_EMPTY"
        );
        ensure!(
            args.locale
                .as_deref()
                .is_none_or(|locale| matches!(locale, "en" | "ja" | "ko" | "zh-Hans" | "zh-Hant")),
            "UNITY_INVALID_REQUEST"
        );
        let editors = discovery::discover(&self.registry_dir).await?;
        let editor = editors
            .into_iter()
            .find(|e| {
                e.summary().instance_id == args.instance_id
                    && e.summary().session_id == args.session_id
            })
            .ok_or_else(|| anyhow::anyhow!("UNITY_UNAVAILABLE"))?;
        ensure!(!editor.summary().busy, "UNITY_BUSY");
        // One package opens in Unity's own importer, which reads the managed file in
        // place, unless Features turns that off. An older Bridge, or a package Unity's
        // importer cannot open, uses the transfer.
        if let [leaf] = args.leaf_refs.as_slice() {
            let path = if self.requests.native_single_package()? {
                self.storage
                    .unity_package_path(&args.product_ids, leaf)
                    .await?
                    .and_then(|path| path.to_str().map(plain_path))
            } else {
                None
            };
            let path = match path {
                Some(path) if native_header(&path).await => Some(path),
                _ => None,
            };
            if let Some(path) = path {
                // The App holds the foreground after the click; let Unity show its window.
                #[cfg(windows)]
                let _ = unsafe {
                    windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(
                        editor.registry().process_id,
                    )
                };
                let response = wire::native_import(editor.registry(), &path)
                    .await
                    .map_err(|_| anyhow::anyhow!("UNITY_NO_RESPONSE"))?;
                match response.code.as_str() {
                    "accepted" => {
                        return Ok(serde_json::to_value(api::Submitted {
                            job_id: None,
                            native: Some(true),
                        })?)
                    }
                    "busy" => bail!("UNITY_BUSY"),
                    "disabled" => bail!("UNITY_DISABLED"),
                    _ => {}
                }
            }
        }
        let request_id = uuid::Uuid::new_v4().to_string();
        let stamp = now();
        let record = RequestRecord {
            request_id: request_id.clone(),
            origin_device_id: self.requests.device_id().to_owned(),
            fingerprint: hex::encode(Sha256::digest(b"")),
            instance_id: args.instance_id,
            session_id: args.session_id,
            project_name: editor.summary().project_name.clone(),
            project_root: editor.summary().project_root.clone(),
            unity_version: editor.summary().unity_version.clone(),
            product_ids: args.product_ids.clone(),
            manifest_json: String::new(),
            phase: "preparing".into(),
            bytes_started: false,
            payload_rejected: false,
            snapshot_ready: false,
            snapshot_removed: false,
            receipt: None,
            created_at: stamp.clone(),
            updated_at: stamp,
            error_code: None,
        };
        self.requests.create(&record)?;
        self.connections
            .lock()
            .await
            .insert(request_id.clone(), editor.registry().clone());
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancellations
            .lock()
            .await
            .insert(request_id.clone(), cancel.clone());
        let response_id = request_id.clone();
        let service = self.clone();
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let _work = work;
            let result = service
                .prepare_and_send(&handle, record, args.leaf_refs, args.locale, cancel, editor)
                .await;
            if let Err(error) = result {
                let _ = service.record_failure(&handle, &request_id, error).await;
            }
            if let Ok(Some(record)) = service.requests.get(&request_id) {
                if !terminal(&record.phase) && record.bytes_started {
                    service.spawn_query(handle, request_id.clone()).await;
                }
            }
            if service
                .requests
                .get(&request_id)
                .ok()
                .flatten()
                .is_some_and(|record| terminal(&record.phase))
            {
                service.cancellations.lock().await.remove(&request_id);
            }
        });
        emit(app);
        Ok(serde_json::to_value(api::Submitted {
            job_id: Some(response_id),
            native: None,
        })?)
    }

    /// Asks the Editor where a request stands, cancelling it first when asked to.
    async fn status(&self, app: &tauri::AppHandle, job_id: String, cancel: bool) -> Result<Value> {
        let _work = self.requests.begin_catalogue_work()?;
        let record = self
            .requests
            .get(&job_id)?
            .ok_or_else(|| anyhow::anyhow!("UNITY_REQUEST_MISSING"))?;
        ensure!(self.requests.is_local(&record), "UNITY_OTHER_DEVICE");
        if cancel && !terminal(&record.phase) {
            self.cancellations
                .lock()
                .await
                .entry(job_id.clone())
                .or_insert_with(|| Arc::new(AtomicBool::new(false)))
                .store(true, Ordering::Release);
        }
        if !record.bytes_started {
            return details(&record);
        }
        if let Err(error) = self.query_once(app, &job_id, cancel).await {
            if !record.keeps_stage_without_answer(&error.to_string()) {
                return Err(error);
            }
        }
        details(
            &self
                .requests
                .get(&job_id)?
                .ok_or_else(|| anyhow::anyhow!("UNITY_REQUEST_MISSING"))?,
        )
    }

    /// What a finished request did not import, to select again.
    async fn retry_inventory(&self, job_id: String) -> Result<Value> {
        let record = self
            .requests
            .get(&job_id)?
            .ok_or_else(|| anyhow::anyhow!("UNITY_REQUEST_MISSING"))?;
        ensure!(terminal(&record.phase), "UNITY_RESULT_UNRESOLVED");
        if !record.bytes_started || record.payload_rejected {
            return Ok(serde_json::to_value(api::RetrySelection {
                product_ids: record.product_ids,
                leaf_refs: vec![],
            })?);
        }
        let manifest: Manifest = serde_json::from_str(&record.manifest_json)
            .map_err(|_| anyhow::anyhow!("UNITY_RETRY_UNAVAILABLE"))?;
        let receipt = record
            .receipt
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("UNITY_RESULT_UNRESOLVED"))?;
        let inventory = self
            .storage
            .list_unity_inventory(&record.product_ids)
            .await?;
        let refs: Vec<String> = inventory
            .iter()
            .filter(|leaf| {
                manifest.entries.iter().any(|entry| {
                    entry.product_id == leaf.product_id
                        && entry.file_id == leaf.file_id
                        && entry.member_path == leaf.member_path
                        && !receipt["results"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|result| {
                                result["entryId"] == entry.entry_id
                                    && matches!(
                                        result["state"].as_str(),
                                        Some("applied" | "skipped")
                                    )
                            })
                })
            })
            .map(|leaf| leaf.leaf_ref.clone())
            .collect();
        ensure!(!refs.is_empty(), "UNITY_RETRY_UNAVAILABLE");
        Ok(serde_json::to_value(api::RetrySelection {
            product_ids: record.product_ids,
            leaf_refs: refs,
        })?)
    }

    async fn prepare_and_send(
        self: &Arc<Self>,
        app: &tauri::AppHandle,
        mut record: RequestRecord,
        refs: Vec<String>,
        locale: Option<String>,
        cancel: Arc<AtomicBool>,
        editor: discovery::DiscoveredEditor,
    ) -> Result<()> {
        let snapshot = self
            .storage
            .prepare_unity_snapshot(
                &record.request_id,
                &record.product_ids,
                &refs,
                cancel.clone(),
            )
            .await?;
        record.snapshot_ready = true;
        if cancel.load(Ordering::Acquire) {
            record.phase = "cancelled".into();
            record.updated_at = now();
            self.save_and_clean(&mut record, true)?;
            emit(app);
            return Ok(());
        }
        let manifest = Manifest {
            version: 1,
            locale,
            instance_id: record.instance_id.clone(),
            session_id: record.session_id.clone(),
            entries: snapshot
                .entries
                .iter()
                .map(|entry| ManifestEntry {
                    entry_id: entry.entry_id.clone(),
                    kind: entry.leaf.kind.clone(),
                    display_name: entry.leaf.display_name.clone(),
                    relative_path: entry.leaf.relative_path.clone(),
                    bytes: entry.leaf.bytes as i64,
                    sha256: entry.leaf.sha256.clone(),
                    product_id: entry.leaf.product_id.clone(),
                    file_id: entry.leaf.file_id.clone(),
                    revision: entry.leaf.revision.clone(),
                    member_path: entry.leaf.member_path.clone(),
                })
                .collect(),
        };
        record.manifest_json = serde_json::to_string(&manifest)?;
        record.fingerprint = hex::encode(Sha256::digest(record.manifest_json.as_bytes()));
        let files: Vec<_> = snapshot
            .entries
            .into_iter()
            .map(|entry| PayloadFile {
                entry_id: entry.entry_id,
                path: entry.path,
            })
            .collect();
        wire::preflight_submit(
            editor.registry(),
            &identity(&record),
            &record.manifest_json,
            &files,
        )?;
        record.phase = "staging".into();
        record.bytes_started = true;
        record.updated_at = now();
        self.requests.update(&record)?;
        emit(app);
        let response = wire::submit(
            editor.registry(),
            &identity(&record),
            &record.manifest_json,
            &files,
        )
        .await?;
        if response.receipt.is_some() {
            self.apply_response(app, &record.request_id, response)
                .await?;
        } else {
            // A rejecting initial response never accepted the raw payload.
            record.phase = "failed".into();
            record.payload_rejected = true;
            record.error_code = Some(
                match response.code.as_str() {
                    "busy" => "UNITY_BUSY",
                    "disabled" => "UNITY_DISABLED",
                    _ => "UNITY_INVALID_RESPONSE",
                }
                .into(),
            );
            record.updated_at = now();
            self.save_and_clean(&mut record, true)?;
            emit(app);
        }
        Ok(())
    }

    async fn record_failure(
        &self,
        app: &tauri::AppHandle,
        id: &str,
        error: anyhow::Error,
    ) -> Result<()> {
        let _gate = self.updates.lock().await;
        let Some(mut record) = self.requests.get(id)? else {
            return Ok(());
        };
        if terminal(&record.phase) {
            return Ok(());
        }
        let cancelled = self
            .cancellations
            .lock()
            .await
            .get(id)
            .is_some_and(|flag| flag.load(Ordering::Acquire));
        let phase: String = if record.bytes_started {
            "unresolved"
        } else if cancelled {
            "cancelled"
        } else {
            "failed"
        }
        .into();
        let error_code = if cancelled && !record.bytes_started {
            None
        } else {
            Some(error_code(error))
        };
        if record.phase == phase && record.error_code == error_code {
            return Ok(());
        }
        record.phase = phase;
        record.error_code = error_code;
        record.updated_at = now();
        let can_clean = !record.bytes_started;
        self.save_and_clean(&mut record, can_clean)?;
        emit(app);
        Ok(())
    }
    async fn apply_response(
        &self,
        app: &tauri::AppHandle,
        id: &str,
        response: wire::BridgeResponse,
    ) -> Result<()> {
        let _gate = self.updates.lock().await;
        let Some(mut record) = self.requests.get(id)? else {
            return Ok(());
        };
        let Some(receipt) = response.receipt else {
            bail!("UNITY_RESULT_UNRESOLVED");
        };
        validate_result_entries(&record.manifest_json, &receipt)?;
        let value = serde_json::to_value(&receipt)?;
        if record.receipt.as_ref() == Some(&value) && record.error_code.is_none() {
            return Ok(());
        }
        if let Some(previous) = record.receipt.as_ref() {
            if previous["updatedUtc"]
                .as_str()
                .and_then(|stamp| chrono::DateTime::parse_from_rfc3339(stamp).ok())
                .zip(chrono::DateTime::parse_from_rfc3339(&receipt.updated_utc).ok())
                .is_some_and(|(old, new)| old > new)
            {
                return Ok(());
            }
            if terminal(previous["stage"].as_str().unwrap_or("")) && !terminal(&receipt.stage) {
                return Ok(());
            }
        }
        record.phase = receipt.stage.clone();
        record.receipt = Some(value);
        record.error_code = None;
        record.updated_at = now();
        let can_clean = record.snapshot_cleanup_proved();
        self.save_and_clean(&mut record, can_clean)?;
        emit(app);
        Ok(())
    }
    fn save_and_clean(&self, record: &mut RequestRecord, proved: bool) -> Result<()> {
        // Persist the actual outcome even if Windows temporarily locks a staged file.
        self.requests.update(record)?;
        if proved
            && !record.snapshot_removed
            && self
                .storage
                .cleanup_unity_snapshot(&record.request_id)
                .is_ok()
        {
            record.snapshot_removed = true;
            self.requests.update(record)?;
        }
        Ok(())
    }
    async fn query_once(&self, app: &tauri::AppHandle, id: &str, cancel: bool) -> Result<()> {
        let record = self
            .requests
            .get(id)?
            .ok_or_else(|| anyhow::anyhow!("UNITY_REQUEST_MISSING"))?;
        // A disabled Bridge withdraws discovery while confirmed writes finish.
        // Keep only the already-handshaken listener in native memory for those queries.
        let remembered = self.connections.lock().await.get(id).cloned();
        if let Some(registry) = remembered {
            let response = if cancel {
                wire::cancel(&registry, &identity(&record)).await
            } else {
                wire::status(&registry, &identity(&record)).await
            };
            match response {
                Ok(response) if response.receipt.is_some() => {
                    return self.apply_response(app, id, response).await;
                }
                // Connected but unanswered: the Editor is busy, and its stale heartbeat would
                // only make discovery report it gone. Any reply that is not this request's,
                // as from another Editor on a reused port, goes through discovery.
                Err(error) if jobs::no_answer(&error.to_string()) => return Err(error),
                _ => {}
            }
        }
        let editors = discovery::discover(&self.registry_dir).await?;
        let editor = editors
            .into_iter()
            .find(|editor| editor.summary().instance_id == record.instance_id)
            .ok_or_else(|| anyhow::anyhow!("UNITY_UNAVAILABLE"))?;
        self.connections
            .lock()
            .await
            .insert(id.into(), editor.registry().clone());
        let response = if cancel {
            wire::cancel(editor.registry(), &identity(&record)).await?
        } else {
            wire::status(editor.registry(), &identity(&record)).await?
        };
        self.apply_response(app, id, response).await
    }
    async fn spawn_query(self: &Arc<Self>, app: tauri::AppHandle, id: String) {
        if !self.workers.lock().await.insert(id.clone()) {
            return;
        }
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                // An unresolved receipt may be polled indefinitely. Register each
                // query and its durable result, but leave restore free between polls.
                let work = match service.requests.begin_catalogue_work() {
                    Ok(permit) => permit,
                    Err(_) => {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                };
                let record = match service.requests.get(&id) {
                    Ok(Some(record)) => record,
                    _ => break,
                };
                if terminal(&record.phase) {
                    service.cancellations.lock().await.remove(&id);
                    service.connections.lock().await.remove(&id);
                    break;
                }
                let cancel = service
                    .cancellations
                    .lock()
                    .await
                    .get(&id)
                    .is_some_and(|flag| flag.load(Ordering::Acquire));
                if let Err(error) = service.query_once(&app, &id, cancel).await {
                    if !record.keeps_stage_without_answer(&error.to_string()) {
                        let _ = service.record_failure(&app, &id, error).await;
                    }
                }
                drop(work);
                tokio::time::sleep(Duration::from_secs(if record.phase == "unresolved" {
                    15
                } else {
                    2
                }))
                .await;
            }
            service.workers.lock().await.remove(&id);
        });
    }
}
fn details(record: &RequestRecord) -> Result<Value> {
    Ok(serde_json::to_value(api::UnityDetails {
        job_id: record.request_id.clone(),
        project_name: record.project_name.clone(),
        project_root: record.project_root.clone(),
        unity_version: record.unity_version.clone(),
        stage: record.presented_phase().to_owned(),
        error_code: record.presented_error_code().map(str::to_owned),
        receipt: record.presented_receipt(),
    })?)
}

fn validate_result_entries(manifest_json: &str, receipt: &wire::Receipt) -> Result<()> {
    let manifest: Manifest = serde_json::from_str(manifest_json)?;
    let entries: HashSet<_> = manifest
        .entries
        .iter()
        .map(|entry| entry.entry_id.as_str())
        .collect();
    ensure!(
        receipt
            .results
            .iter()
            .all(|result| entries.contains(result.entry_id.as_str())),
        "UNITY_INVALID_RESPONSE"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unity_receives_ordinary_paths_for_verbatim_roots() {
        for (verbatim, plain) in [
            (
                r"\\?\E:\Library\A.unitypackage",
                r"E:\Library\A.unitypackage",
            ),
            (
                r"\\?\UNC\nas\share\A.unitypackage",
                r"\\nas\share\A.unitypackage",
            ),
            (r"E:\Library\A.unitypackage", r"E:\Library\A.unitypackage"),
        ] {
            assert_eq!(plain_path(verbatim), plain);
        }
    }

    #[test]
    fn receipt_results_bind_to_selection_and_allow_package_children() {
        let entry = uuid::Uuid::new_v4().to_string();
        let manifest = serde_json::to_string(&json!({"version":1,"instanceId":"instance","sessionId":"session","entries":[{
            "entryId":entry,"kind":"unitypackage","displayName":"Authored.unitypackage","relativePath":"Authored.unitypackage",
            "bytes":1,"sha256":"a".repeat(64),"productId":"authored","fileId":"file","revision":"revision","memberPath":""
        }]})).unwrap();
        let mut receipt: wire::Receipt = serde_json::from_value(json!({
            "requestId":"request","fingerprint":"b".repeat(64),"instanceId":"instance","sessionId":"session",
            "stage":"completed","staged":true,"writesStarted":true,"updatedUtc":"2026-10-12T00:00:00Z","error":null,"compileErrors":[],
            "results":[{"entryId":entry,"path":"Assets/A.txt","state":"applied","message":null},{"entryId":entry,"path":"Assets/B.txt","state":"skipped","message":null}]
        })).unwrap();
        validate_result_entries(&manifest, &receipt).unwrap();
        receipt.results[0].entry_id = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            validate_result_entries(&manifest, &receipt)
                .unwrap_err()
                .to_string(),
            "UNITY_INVALID_RESPONSE"
        );
        receipt.results.clear();
        validate_result_entries(&manifest, &receipt).unwrap();
    }

    #[test]
    fn a_job_records_an_unanswered_query_as_a_timeout() {
        assert_eq!(
            error_code(anyhow::anyhow!("UNITY_NO_RESPONSE")),
            "UNITY_TIMEOUT"
        );
        assert_eq!(
            error_code(anyhow::anyhow!("UNITY_INVALID_RESPONSE")),
            "UNITY_INVALID_RESPONSE"
        );
    }
}
