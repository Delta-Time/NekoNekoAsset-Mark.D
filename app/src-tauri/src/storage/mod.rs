//! Durable product-level downloads and guarded, verified local storage.
mod ai_evidence;
pub(crate) mod api;
pub(crate) use ai_evidence::{
    ai_public_urls_in_text, redact_ai_evidence_text, safe_ai_evidence_name,
};
mod archive;
mod download_delete;
mod io;
mod local;
mod local_delete;
mod operations;
mod portable;
mod shared;
#[cfg(test)]
mod tests;
mod transfer;
mod unity_snapshot;
pub(crate) use unity_snapshot::UnityLeaf;

pub(crate) fn checked_library_root(path: &Path) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "root_offline");
    let metadata = std::fs::metadata(path)
        .map_err(|error| anyhow::Error::new(error).context("root_offline"))?;
    ensure!(metadata.is_dir(), "library_path_not_directory");
    archive::validate_no_links(path)?;
    Ok(path.canonicalize()?)
}

pub(crate) fn validate_library_path(path: &Path) -> Result<()> {
    archive::validate_no_links(path)
}

/// Startup queues these again: a download restarts only its unfinished file and a
/// local import only its uncopied ones, so an App update need not wait for them.
pub(crate) const RESUMED_AT_STARTUP: [&str; 2] = ["download", "storage:import_local"];
/// A queued download whose store refused a file, waiting for that store's list to be read
/// again; its product stays in progress meanwhile.
const STORE_UPDATE: &str = "storeUpdate";

use crate::{catalogue::Catalogue, stores::StoreService};
use anyhow::{bail, ensure, Result};
use futures_util::{stream::FuturesUnordered, StreamExt};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs::File,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{Emitter, Manager};
use tokio::{
    io::AsyncWriteExt,
    sync::{Mutex as AsyncMutex, RwLock, Semaphore},
};

#[derive(Clone)]
pub struct StorageService {
    catalogue: Catalogue,
    stores: Arc<StoreService>,
    started: Arc<AtomicBool>,
    queue_gate: Arc<AsyncMutex<()>>,
    operations: Arc<RwLock<()>>,
    extraction: Arc<Semaphore>,
    cancellations: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    /// Products whose downloaded files are being removed.
    deleting: Arc<Mutex<HashSet<String>>>,
    process_lock: Arc<Mutex<Option<File>>>,
    automatic_pending: Arc<Mutex<AutomaticPending>>,
}

#[derive(Default)]
struct AutomaticPending {
    full: bool,
    products: HashSet<String>,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
fn artifact_id(file: &Value) -> String {
    io::key(&format!(
        "{}\0{}",
        file["id"].as_str().unwrap_or(""),
        file["revision"].as_str().unwrap_or("unknown")
    ))
}
fn download_count(v: &Value) -> usize {
    match v.as_i64() {
        Some(n) if n > 6 => 6,
        Some(n) if n < 1 => 1,
        Some(n) => n as usize,
        None => 3,
    }
}
fn adaptive_download_count(
    configured: &Value,
    provider: Option<crate::stores::StoreProvider>,
    jobs: &[Value],
) -> usize {
    use crate::stores::StoreProvider;
    let syncing = provider.is_some_and(|provider| {
        jobs.iter().any(|job| {
            matches!(job["state"].as_str(), Some("queued" | "running"))
                && matches!(
                    (provider, job["type"].as_str()),
                    (StoreProvider::Booth, Some("boothSync" | "boothUpdate"))
                        | (
                            StoreProvider::Gumroad,
                            Some("gumroadSync" | "gumroadUpdate")
                        )
                        | (StoreProvider::Jinxxy, Some("jinxxySync" | "jinxxyUpdate"))
                )
        })
    });
    if syncing {
        1
    } else {
        download_count(configured)
    }
}
fn file_manifest(files: Vec<Value>, entitlements: &[Value]) -> (Vec<Value>, bool) {
    let complete =
        !entitlements.is_empty() && entitlements.iter().all(|e| e["manifestComplete"] == true);
    let ids: HashSet<_> = entitlements
        .iter()
        .flat_map(|e| strings(&e["fileIds"]))
        .collect();
    let files: Vec<_> = files
        .into_iter()
        .filter(|f| f["id"].as_str().is_some_and(|id| ids.contains(id)))
        .collect();
    let complete = complete && files.len() == ids.len();
    (files, complete)
}
fn fingerprint(files: &[Value]) -> String {
    let mut fields: Vec<_> = files
        .iter()
        .map(|f| {
            json!([
                f["id"],
                f["revision"],
                f["requiresContentVerification"] == true
            ])
        })
        .collect();
    fields.sort_by(|a, b| a[0].as_str().cmp(&b[0].as_str()));
    io::key(&serde_json::to_string(&fields).unwrap_or_default())
}
/// Last download outcomes a product download leaves out. A file its store refused as
/// no longer offered is never asked for again, and the list drops it unless it was
/// downloaded; a closed free offer is tried again when its product is downloaded alone.
fn skipped_failures(alone: bool) -> &'static [&'static str] {
    if alone {
        &["store_file_unavailable"]
    } else {
        &["store_file_unavailable", "free_offer_unavailable"]
    }
}
/// Files a free item page read at `read_at` offers again: the record a closed offer left on
/// each one's last download before then goes, so its row shows it as offered and Download
/// takes it. A failed automatic download stays failed, as any does until the user retries.
/// A record goes only while unchanged, so a download that started since keeps its own.
/// Returns the products whose files changed.
pub(crate) fn reopen_free_offers(
    catalogue: &Catalogue,
    file_ids: impl IntoIterator<Item = String>,
    read_at: &str,
) -> Result<BTreeSet<String>> {
    let time = |value: &str| chrono::DateTime::parse_from_rfc3339(value).ok();
    let read_at = time(read_at);
    let mut products = BTreeSet::new();
    for file_id in file_ids {
        let id = format!("pending-{}", io::key(&file_id));
        let Some(record) = catalogue.get("artifact", &id)? else {
            continue;
        };
        // Records from before failures carried their time are older than any read.
        let ended_before = match record["updatedAt"].as_str().map(time) {
            Some(ended) => ended.zip(read_at).is_some_and(|(ended, read)| ended < read),
            None => true,
        };
        if record["errorCode"] == "free_offer_unavailable"
            && ended_before
            && catalogue.write_many_if_unchanged(
                vec![("artifact".into(), id.clone(), None)],
                vec![("artifact".into(), id, Some(record.clone()))],
            )?
        {
            products.extend(record["productId"].as_str().map(str::to_owned));
        }
    }
    Ok(products)
}
fn selected_download_files(files: Vec<Value>, selected: &Value) -> Result<Vec<Value>> {
    if selected.is_null() {
        return Ok(files);
    }
    let ids = strings(selected);
    ensure!(
        selected.as_array().is_some_and(|values| values.len() == 1) && ids.len() == 1,
        "invalid_arguments"
    );
    let files: Vec<_> = files
        .into_iter()
        .filter(|file| file["id"] == ids[0])
        .collect();
    ensure!(files.len() == 1, "file_missing");
    Ok(files)
}
/// A download with nothing to fetch: after the store's update, which only a refusal asks for,
/// because the store no longer offers any of its files; otherwise because the list is not
/// known yet.
fn ensure_files(job: &Value, files: &[Value]) -> Result<()> {
    if !files.is_empty() {
        return Ok(());
    }
    if job["storeUpdated"] == true {
        bail!("store_file_unavailable");
    }
    bail!("manifest_incomplete")
}
#[cfg(test)]
fn native_seven_zip_test_tool() -> PathBuf {
    let path = std::env::var_os("NNA_7ZIP")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/tools/7zip/7z.exe")
        });
    assert!(
        path.is_file(),
        "native archive regression requires bundled 7-Zip or explicit NNA_7ZIP"
    );
    path
}
impl StorageService {
    pub fn new(catalogue: Catalogue, stores: Arc<StoreService>) -> Result<Self> {
        Ok(Self {
            catalogue,
            stores,
            started: Arc::new(AtomicBool::new(false)),
            queue_gate: Arc::new(AsyncMutex::new(())),
            operations: Arc::new(RwLock::new(())),
            extraction: Arc::new(Semaphore::new(1)),
            cancellations: Arc::new(Mutex::new(HashMap::new())),
            deleting: Arc::new(Mutex::new(HashSet::new())),
            process_lock: Arc::new(Mutex::new(None)),
            automatic_pending: Arc::new(Mutex::new(AutomaticPending::default())),
        })
    }
    fn changed(&self, app: &tauri::AppHandle, id: Option<&str>) {
        let _ = app.emit(
            "nna:changed",
            crate::ipc::Change {
                job_id: id.map(str::to_owned),
                ..crate::ipc::Change::scope("products")
            },
        );
    }
    fn jobs_changed(&self, app: &tauri::AppHandle, id: &str) {
        let _ = app.emit("nna:changed", crate::ipc::Change::job("jobs", id));
    }
    fn root(&self, id: Option<&str>) -> Result<Value> {
        let configured = self.preferences()?["libraryRootId"]
            .as_str()
            .map(str::to_owned);
        let root = if let Some(id) = id.or(configured.as_deref()) {
            self.catalogue.get("root", id)?
        } else {
            self.catalogue
                .list("root")?
                .into_iter()
                .find(|r| r["active"] != false)
        };
        root.ok_or_else(|| anyhow::anyhow!("root_unconfigured"))
    }
    fn root_path(&self, root: &Value) -> Result<PathBuf> {
        self.catalogue.check_library_access()?;
        let p = if root["libraryRelative"] == true {
            let library = self
                .catalogue
                .library_root()
                .ok_or_else(|| anyhow::anyhow!("root_offline"))?;
            if root["path"] == "." {
                library.to_path_buf()
            } else {
                io::safe_join(library, io::required_string(root, "path")?)?
            }
        } else {
            PathBuf::from(io::required_string(root, "path")?)
        };
        ensure!(p.is_dir(), "root_offline");
        io::reject_link(&p)?;
        Ok(p)
    }
    fn preferences(&self) -> Result<Value> {
        Ok(self
            .catalogue
            .get("preferences", "app")?
            .unwrap_or(json!({})))
    }
    fn managed_product_directory(&self, product: &str) -> Result<String> {
        if let Some(directory) = self
            .catalogue
            .related("artifact", product)?
            .iter()
            .find_map(|a| a["productDirectory"].as_str().map(str::to_owned))
        {
            archive::validate_relative_path(&directory)?;
            return Ok(directory);
        }
        let source = self
            .catalogue
            .get("product", product)?
            .ok_or_else(|| anyhow::anyhow!("product_missing"))?;
        Ok(format!(
            "assets/{}",
            io::readable_component(
                source["sourceTitle"]
                    .as_str()
                    .or(source["title"].as_str())
                    .unwrap_or("Asset"),
                product
            )
        ))
    }
    fn put_job(&self, job: &mut Value, state: &str, error: Option<&str>) -> Result<()> {
        if state == "queued" {
            // A retry reuses this record: it shows in Activity again, and its failure
            // must raise Attention again.
            let job = job.as_object_mut().unwrap();
            job.remove("attentionDismissed");
            job.remove("activityCleared");
        }
        job["state"] = json!(state);
        job["updatedAt"] = json!(now());
        job["errorCode"] = json!(error);
        self.catalogue
            .put("job", io::required_string(job, "id")?, job.clone())
    }
    fn cancellation(&self, id: &str) -> Arc<AtomicBool> {
        self.cancellations
            .lock()
            .unwrap()
            .entry(id.to_owned())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone()
    }
    fn cancel_job(&self, id: &str) -> Result<Value> {
        loop {
            let before = self
                .catalogue
                .get("job", id)?
                .ok_or_else(|| anyhow::anyhow!("job_missing"))?;
            ensure!(
                before["type"] == "download"
                    || before["type"]
                        .as_str()
                        .is_some_and(|t| t.starts_with("storage:")),
                "invalid_job"
            );
            if !matches!(before["state"].as_str(), Some("queued" | "running")) {
                return api::answer(api::CancelRequested {
                    cancel_requested: false,
                });
            }
            let mut job = before.clone();
            job["cancelRequested"] = json!(true);
            job["updatedAt"] = json!(now());
            if job["state"] == "queued" {
                job["state"] = json!("cancelled");
                job["errorCode"] = Value::Null;
            }
            if self.catalogue.put_many_if_unchanged(
                vec![("job".into(), id.to_owned(), job)],
                vec![("job".into(), id.to_owned(), Some(before))],
            )? {
                self.cancellation(id).store(true, Ordering::Release);
                return api::answer(api::CancelRequested {
                    cancel_requested: true,
                });
            }
        }
    }
    /// Cancels every active download of these products, or of all products.
    async fn cancel_downloads(&self, args: &Value) -> Result<Value> {
        let products = args.get("productIds").map(strings);
        // Enqueue holds this gate until its jobs are written, so none is missed.
        let _gate = self.queue_gate.lock().await;
        loop {
            let active: Vec<_> = self
                .catalogue
                .list("job")?
                .into_iter()
                .filter(|job| {
                    job["type"] == "download"
                        && job["cancelRequested"] != true
                        && matches!(job["state"].as_str(), Some("queued" | "running"))
                        && products
                            .as_ref()
                            .is_none_or(|ids| ids.iter().any(|id| job["productId"] == id.as_str()))
                })
                .collect();
            let mut records = Vec::new();
            let mut expected = Vec::new();
            for before in &active {
                let id = io::required_string(before, "id")?.to_owned();
                let mut job = before.clone();
                job["cancelRequested"] = json!(true);
                job["updatedAt"] = json!(now());
                if job["state"] == "queued" {
                    job["state"] = json!("cancelled");
                    job["errorCode"] = Value::Null;
                }
                records.push(("job".into(), id.clone(), job));
                expected.push(("job".into(), id, Some(before.clone())));
            }
            if active.is_empty() || self.catalogue.put_many_if_unchanged(records, expected)? {
                for job in active.iter().filter(|job| job["state"] == "running") {
                    self.cancellation(job["id"].as_str().unwrap_or(""))
                        .store(true, Ordering::Release);
                }
                return api::answer(api::DownloadsCancelled {
                    cancelled: active.len(),
                });
            }
        }
    }
    fn remote_files(&self, product: &str) -> Result<(Vec<Value>, bool)> {
        if let Some(source) = self
            .catalogue
            .get("product", product)?
            .filter(|p| p["provider"] == "local")
        {
            let ids: HashSet<_> = strings(&source["localManifest"]["fileIds"])
                .into_iter()
                .collect();
            let files: Vec<_> = self
                .catalogue
                .related("remote_file", product)?
                .into_iter()
                .filter(|f| {
                    f["id"].as_str().is_some_and(|id| ids.contains(id))
                        && f["originKind"] == "local"
                })
                .collect();
            let complete = source["localManifest"]["complete"] == true
                && !ids.is_empty()
                && files.len() == ids.len();
            return Ok((files, complete));
        }
        let entitlements = self.catalogue.effective_entitlements(product)?;
        Ok(file_manifest(
            self.catalogue.related("remote_file", product)?,
            &entitlements,
        ))
    }
    /// Attached files plus store files kept after the store stopped offering
    /// them. Callers still require a usable artifact at the file's revision. An
    /// older descriptor of a downloadable still offered is that same file.
    fn kept_files(&self, product: &str) -> Result<Vec<Value>> {
        let (mut files, _) = self.remote_files(product)?;
        if self
            .catalogue
            .get("product", product)?
            .is_some_and(|p| p["provider"] != "local")
        {
            let attached: HashSet<String> = files
                .iter()
                .filter_map(|f| f["id"].as_str().map(str::to_owned))
                .collect();
            let source = |f: &Value| {
                (
                    f["accountId"].as_str().map(str::to_owned),
                    f["sourceFileId"].as_str().map(str::to_owned),
                )
            };
            let offered: HashSet<_> = files
                .iter()
                .filter(|f| {
                    f["originKind"] != "local"
                        && f["sourceFileId"].as_str().is_some_and(|id| !id.is_empty())
                })
                .map(source)
                .collect();
            files.extend(
                self.catalogue
                    .related("remote_file", product)?
                    .into_iter()
                    .filter(|f| {
                        f["id"].as_str().is_some_and(|id| !attached.contains(id))
                            && !offered.contains(&source(f))
                    }),
            );
        }
        Ok(files)
    }
    fn enabled_files(&self, files: Vec<Value>) -> Result<Vec<Value>> {
        let enabled = self.available_download_accounts()?;
        Ok(files
            .into_iter()
            .filter(|f| f["accountId"].as_str().is_some_and(|a| enabled.contains(a)))
            .collect())
    }
    /// Leaves out files whose last download ended with one of `codes`.
    fn without_last_failure(&self, files: Vec<Value>, codes: &[&str]) -> Result<Vec<Value>> {
        let mut remaining = Vec::with_capacity(files.len());
        for file in files {
            let last = format!("pending-{}", io::key(file["id"].as_str().unwrap_or("")));
            if self
                .catalogue
                .get("artifact", &last)?
                .is_none_or(|artifact| !codes.iter().any(|code| artifact["errorCode"] == *code))
            {
                remaining.push(file);
            }
        }
        Ok(remaining)
    }
    /// Every file's artifact at its current revision has one of `statuses`. Automatic
    /// downloads leave such files alone; checking their bytes again is left to a
    /// Download the user asks for.
    fn all_at_revision(&self, files: &[Value], statuses: &[&str]) -> Result<bool> {
        for file in files {
            let known = file["revision"]
                .as_str()
                .is_some_and(|revision| !revision.is_empty() && revision != "unknown");
            if !known || file["requiresContentVerification"] == true {
                return Ok(false);
            }
            let Some(artifact) = self.catalogue.get("artifact", &artifact_id(file))? else {
                return Ok(false);
            };
            if !statuses.contains(&artifact["status"].as_str().unwrap_or("")) {
                return Ok(false);
            }
        }
        Ok(!files.is_empty())
    }
    /// The intent remembers which list a product's last download asked for, so
    /// clearing Activity never makes an automatic download ask for it again.
    fn remember_attempt(&self, job: &Value) -> Result<()> {
        let Some(product) = job["productId"].as_str() else {
            return Ok(());
        };
        if job["type"] != "download" || !job["selectedFileIds"].is_null() {
            return Ok(());
        }
        // Every product download writes the intent; one removed since stays removed.
        while let Some(before) = self.catalogue.get("download_intent", product)? {
            if before["manifestFingerprint"] == job["manifestFingerprint"] {
                break;
            }
            let mut intent = before.clone();
            intent["manifestFingerprint"] = job["manifestFingerprint"].clone();
            if self.catalogue.put_many_if_unchanged(
                vec![("download_intent".into(), product.to_owned(), intent)],
                vec![("download_intent".into(), product.to_owned(), Some(before))],
            )? {
                break;
            }
        }
        Ok(())
    }
    /// Intents written before 0.4.0 kept no list; only their download jobs did. Copy each
    /// product's latest one while Activity still lists it, so a failed or cancelled download
    /// keeps waiting for a retry after the first Clear history.
    fn remember_listed_attempts(&self) -> Result<()> {
        let mut jobs: Vec<_> = self
            .catalogue
            .list("job")?
            .into_iter()
            .filter(|job| {
                job["type"] == "download"
                    && job["selectedFileIds"].is_null()
                    && job["manifestFingerprint"].is_string()
                    && job["productId"].is_string()
            })
            .collect();
        if jobs.is_empty() {
            return Ok(());
        }
        // In the order they ran, so each product keeps its latest.
        jobs.sort_by(|a, b| a["createdAt"].as_str().cmp(&b["createdAt"].as_str()));
        let latest: HashMap<&str, &Value> = jobs
            .iter()
            .map(|job| (job["productId"].as_str().unwrap_or(""), job))
            .collect();
        // One write, read again should another change an intent first.
        for _ in 0..4 {
            let (mut writes, mut expected) = (Vec::new(), Vec::new());
            for intent in self.catalogue.list("download_intent")? {
                let Some(id) = intent["id"].as_str().map(str::to_owned) else {
                    continue;
                };
                let Some(job) = latest.get(id.as_str()) else {
                    continue;
                };
                if !intent["manifestFingerprint"].is_null() {
                    continue;
                }
                let mut remembered = intent.clone();
                remembered["manifestFingerprint"] = job["manifestFingerprint"].clone();
                writes.push(("download_intent".to_owned(), id.clone(), remembered));
                expected.push(("download_intent".to_owned(), id, Some(intent)));
            }
            if writes.is_empty() || self.catalogue.put_many_if_unchanged(writes, expected)? {
                break;
            }
        }
        Ok(())
    }
    /// What a download job covers of its product's current list.
    fn job_files(&self, job: &Value, files: Vec<Value>) -> Result<Vec<Value>> {
        let files = if job["automatic"] == true {
            self.enabled_files(files)?
        } else {
            files
        };
        if job["selectedFileIds"].is_null() {
            self.without_last_failure(files, skipped_failures(job["skipEndedOffers"] != true))
        } else {
            Ok(files)
        }
    }
    fn available_download_accounts(&self) -> Result<HashSet<String>> {
        Ok(self
            .catalogue
            .list("account")?
            .into_iter()
            .filter(|a| {
                a["disconnected"] != true && crate::stores::has_local_session(&self.catalogue, a)
            })
            .filter_map(|a| a["id"].as_str().map(str::to_owned))
            .collect())
    }
    fn download_has_local_session(&self, job: &Value, accounts: &HashSet<String>) -> Result<bool> {
        if self.catalogue.library_root().is_none() {
            return Ok(true);
        }
        for id in strings(&job["fileIds"]) {
            let Some(file) = self.catalogue.get("remote_file", &id)? else {
                continue;
            };
            if !accounts.contains(file["accountId"].as_str().unwrap_or("")) {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn put_queued_download(
        &self,
        before: &Value,
        job: &mut Value,
        state: &str,
        error: Option<&str>,
    ) -> Result<bool> {
        ensure!(
            before["type"] == "download" && before["state"] == "queued",
            "invalid_job"
        );
        let id = io::required_string(before, "id")?;
        job["state"] = json!(state);
        job["updatedAt"] = json!(now());
        job["errorCode"] = json!(error);
        self.catalogue.put_many_if_unchanged(
            vec![("job".into(), id.to_owned(), job.clone())],
            vec![("job".into(), id.to_owned(), Some(before.clone()))],
        )
    }
    fn queued_download(&self) -> Result<(Option<Value>, bool)> {
        let jobs = self.catalogue.queued_jobs(Some("download"), None)?;
        if jobs.is_empty() {
            return Ok((None, false));
        }
        let accounts = self.available_download_accounts()?;
        let mut changed = false;
        for mut job in jobs {
            let mut before = job.clone();
            if job["stage"] == STORE_UPDATE {
                if !job["updateJobIds"].is_array() || self.store_update_active(&job)? {
                    continue;
                }
                let fields = job.as_object_mut().unwrap();
                fields.remove("stage");
                fields.remove("updateJobIds");
                // One refused file has nothing left to fetch; a product goes on with its new list.
                if !job["selectedFileIds"].is_null() {
                    changed |= self.put_queued_download(
                        &before,
                        &mut job,
                        "needsAction",
                        Some("store_file_unavailable"),
                    )?;
                    continue;
                }
                if !self.put_queued_download(&before, &mut job, "queued", None)? {
                    continue;
                }
                changed = true;
                before = job.clone();
            }
            if !self.download_has_local_session(&job, &accounts)? {
                if job["stage"] != "authRequired" || job["errorCode"] != "auth_required" {
                    job["stage"] = json!("authRequired");
                    changed |= self.put_queued_download(
                        &before,
                        &mut job,
                        "queued",
                        Some("auth_required"),
                    )?;
                }
                continue;
            }
            if job["stage"] == "authRequired" {
                job.as_object_mut().unwrap().remove("stage");
                if !self.put_queued_download(&before, &mut job, "queued", None)? {
                    continue;
                }
                changed = true;
            }
            return Ok((Some(job), changed));
        }
        Ok((None, changed))
    }
    fn store_update_active(&self, job: &Value) -> Result<bool> {
        for id in strings(&job["updateJobIds"]) {
            if self.catalogue.get("job", &id)?.is_some_and(|update| {
                matches!(update["state"].as_str(), Some("queued" | "running"))
            }) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    /// A file its store refused may have been replaced, so the download goes back to the
    /// queue, once, until its store's list has been read again. A cancel requested
    /// meanwhile ends it instead.
    fn wait_for_store_list(
        &self,
        job: &mut Value,
        result: &Result<()>,
        cancel: &AtomicBool,
    ) -> Result<bool> {
        let refused = result
            .as_ref()
            .is_err_and(|error| io::stable_error(error) == "store_file_unavailable")
            || job["failures"].as_object().is_some_and(|failures| {
                failures
                    .values()
                    .any(|failure| failure["errorCode"] == "store_file_unavailable")
            });
        if !refused || job["storeUpdated"] == true || cancel.load(Ordering::Acquire) {
            return Ok(false);
        }
        let id = io::required_string(job, "id")?.to_owned();
        let Some(before) = self
            .catalogue
            .get("job", &id)?
            .filter(|stored| stored["state"] == "running" && stored["cancelRequested"] != true)
        else {
            return Ok(false);
        };
        job["stage"] = json!(STORE_UPDATE);
        job["storeUpdated"] = json!(true);
        job["state"] = json!("queued");
        job["errorCode"] = Value::Null;
        job["updatedAt"] = json!(now());
        self.catalogue.put_many_if_unchanged(
            vec![("job".into(), id.clone(), job.clone())],
            vec![("job".into(), id, Some(before))],
        )
    }
    /// Once nothing else can download, one update per store reads again the lists of the
    /// products waiting for one: an update slows its store's transfers to one, and one
    /// store's busy or signed-out account must not fail another's.
    async fn start_store_updates(&self, app: &tauri::AppHandle) -> Result<bool> {
        let waiting: Vec<_> = self
            .catalogue
            .queued_jobs(Some("download"), None)?
            .into_iter()
            .filter(|job| job["stage"] == STORE_UPDATE && !job["updateJobIds"].is_array())
            .collect();
        if waiting.is_empty() {
            return Ok(false);
        }
        let mut stores: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for job in &waiting {
            let product = job["productId"].as_str().unwrap_or("");
            // Store product IDs carry their store; BOOTH's are bare numbers.
            let store = product.split_once(':').map_or("booth", |(store, _)| store);
            stores.entry(store).or_default().insert(product);
        }
        let mut updates = HashMap::new();
        for products in stores.values() {
            // The files the download just verified need no second verification.
            let jobs = self
                .stores
                .command(
                    app,
                    "update_products",
                    json!({"productIds":products,"keepVerified":true}),
                )
                .await
                .map_or_else(|_| json!([]), |answer| answer["jobIds"].clone());
            for product in products {
                updates.insert(*product, jobs.clone());
            }
        }
        for before in &waiting {
            let mut job = before.clone();
            job["updateJobIds"] = updates
                .get(before["productId"].as_str().unwrap_or(""))
                .cloned()
                .unwrap_or_else(|| json!([]));
            self.put_queued_download(before, &mut job, "queued", None)?;
        }
        Ok(true)
    }
    /// Held for the App lifetime in the library data folder. Windows denies other
    /// handles' reads of this byte-range lock, so a library move skips it.
    fn lock_process(&self) -> Result<()> {
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.catalogue.data_dir().join("storage.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .map_err(|_| anyhow::anyhow!("storage_already_running"))?;
        *self.process_lock.lock().unwrap() = Some(lock);
        Ok(())
    }
    pub async fn start(&self, app: tauri::AppHandle) -> Result<()> {
        let _startup = self.catalogue.begin_catalogue_work()?;
        if self.started.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let setup = (|| -> Result<()> {
            self.lock_process()?;
            // First, before anything slower lets a Clear history remove those jobs.
            let _ = self.remember_listed_attempts();
            for mut job in self.catalogue.list("job")? {
                let other_device = job["type"] == "storage:import_local"
                    && self.catalogue.library_root().is_some()
                    && job["originDeviceId"] != self.catalogue.device_id();
                // Earlier versions marked it with the code for sources nobody recorded.
                if other_device
                    && job["state"] == "needsAction"
                    && job["errorCode"] == "local_source_required"
                {
                    self.put_job(&mut job, "needsAction", Some("local_other_device"))?;
                    continue;
                }
                if matches!(job["state"].as_str(), Some("running" | "queued")) {
                    if other_device {
                        self.put_job(&mut job, "needsAction", Some("local_other_device"))?;
                        continue;
                    }
                    if RESUMED_AT_STARTUP.contains(&job["type"].as_str().unwrap_or("")) {
                        self.put_job(&mut job, "queued", None)?;
                    } else if job["type"]
                        .as_str()
                        .is_some_and(|t| t.starts_with("storage:"))
                    {
                        self.put_job(&mut job, "needsAction", Some("interrupted"))?;
                    }
                }
            }
            self.reconcile_archive_cleanup()?;
            // Should it fail, those products keep their sources in their jobs, as before.
            let _ = self.record_listed_local_sources();
            Ok(())
        })();
        if let Err(e) = setup {
            *self.process_lock.lock().unwrap() = None;
            self.started.store(false, Ordering::Release);
            return Err(e);
        }
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            // Recovery evaluates the existing catalogue once. Source ingestion and
            // preference changes subsequently provide explicit, bounded invalidation.
            let _ = service.reevaluate_automatic().await;
            loop {
                let _ = service.drain_automatic_changes().await;
                let work = match service.catalogue.begin_catalogue_work() {
                    Ok(permit) => permit,
                    Err(_) => {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };
                let local = service
                    .catalogue
                    .queued_jobs(Some("storage:import_local"), None)
                    .ok()
                    .and_then(|jobs| {
                        jobs.into_iter().find(|job| {
                            service.catalogue.library_root().is_none()
                                || job["originDeviceId"] == service.catalogue.device_id()
                        })
                    });
                if let Some(job) = local {
                    service.process_local_job(&app, job).await;
                    continue;
                }
                let operation = service.operations.read().await;
                let (next, changed) = service.queued_download().unwrap_or((None, false));
                if changed {
                    service.changed(&app, None);
                }
                if let Some(mut job) = next {
                    let before = job.clone();
                    let id = job["id"].as_str().unwrap_or("").to_owned();
                    let cancel = service.cancellation(&id);
                    if job["cancelRequested"] == true {
                        cancel.store(true, Ordering::Release);
                    }
                    if service
                        .put_queued_download(&before, &mut job, "running", None)
                        .unwrap_or(false)
                    {
                        service.changed(&app, Some(&id));
                        let result = service.run_download(&app, &mut job, &cancel).await;
                        if service
                            .wait_for_store_list(&mut job, &result, &cancel)
                            .unwrap_or(false)
                        {
                            service.changed(&app, Some(&id));
                            service.cancellations.lock().unwrap().remove(&id);
                            continue;
                        }
                        let (state, error) = match &result {
                            Ok(()) => ("completed", None),
                            Err(e) if io::stable_error(e) == "cancelled" => ("cancelled", None),
                            Err(e) => {
                                let code = io::stable_error(e);
                                (
                                    if matches!(
                                        code,
                                        "root_offline"
                                            | "low_space"
                                            | "auth_required"
                                            | "permission_denied"
                                            | "store_access_denied"
                                            | "manifest_incomplete"
                                            | "store_file_unavailable"
                                            | "free_offer_unavailable"
                                            | "archive_password_required"
                                            | "archive_missing_volume"
                                            | "source_tree_incomplete"
                                    ) {
                                        "needsAction"
                                    } else {
                                        "failed"
                                    },
                                    Some(code),
                                )
                            }
                        };
                        // Before the job ends, so a delete of its files cannot start in between.
                        let _ = service.remember_attempt(&job);
                        let _ = service.put_job(&mut job, state, error);
                        service.changed(&app, Some(&id));
                        if let Some(product) = job["productId"].as_str() {
                            // A source update received while this job was active
                            // may have been deduplicated against it.
                            let _ = service.source_products_changed(&[product.to_owned()]).await;
                        }
                    }
                    service.cancellations.lock().unwrap().remove(&id);
                } else {
                    if service.start_store_updates(&app).await.unwrap_or(false) {
                        service.changed(&app, None);
                    }
                    drop(operation);
                    drop(work);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        });
        Ok(())
    }
    /// Startup or an automatic-download/root/account preference change only.
    pub async fn reevaluate_automatic(&self) -> Result<()> {
        self.automatic_pending.lock().unwrap().full = true;
        self.drain_automatic_changes().await
    }
    /// Reevaluate only products whose durable source manifest changed.
    pub async fn source_products_changed(&self, product_ids: &[String]) -> Result<()> {
        if product_ids.is_empty() {
            return Ok(());
        }
        self.automatic_pending
            .lock()
            .unwrap()
            .products
            .extend(product_ids.iter().cloned());
        self.drain_automatic_changes().await
    }
    async fn drain_automatic_changes(&self) -> Result<()> {
        {
            let pending = self.automatic_pending.lock().unwrap();
            if !pending.full && pending.products.is_empty() {
                return Ok(());
            }
        }
        let Ok(_operation) = self.operations.try_read() else {
            return Ok(());
        };
        // Retain the lightweight invalidation while offline/busy. In particular,
        // don't scan a whole startup catalogue repeatedly until its root returns.
        let root = match self.root(None) {
            Ok(root) => root,
            Err(error) if error.to_string() == "root_unconfigured" => return Ok(()),
            Err(error) => return Err(error),
        };
        if let Err(error) = self.root_path(&root) {
            if error.to_string() == "root_offline" {
                return Ok(());
            }
            return Err(error);
        }
        let pending = std::mem::take(&mut *self.automatic_pending.lock().unwrap());
        let result = async {
            let mut ids = pending.products.clone();
            if pending.full {
                for file in self.catalogue.list("remote_file")? {
                    if let Some(product) = file["productId"].as_str() {
                        ids.insert(product.to_owned());
                    }
                }
            }
            self.evaluate_automatic(&ids).await
        }
        .await;
        if result.is_err() {
            let mut retained = self.automatic_pending.lock().unwrap();
            retained.full |= pending.full;
            retained.products.extend(pending.products);
        }
        result
    }
    async fn evaluate_automatic(&self, product_ids: &HashSet<String>) -> Result<()> {
        let prefs = self.preferences()?;
        let all = prefs["autoDownloadAll"].as_bool().unwrap_or(false);
        let accounts = self.available_download_accounts()?;
        let mut pending = Vec::new();
        for id in product_ids {
            let intent = self.catalogue.get("download_intent", id)?;
            // Deleting downloaded files opts out of download-all until the next Download.
            let wanted = intent.as_ref().map(|intent| intent["wantDownload"].clone());
            if wanted == Some(json!(false)) || (!all && wanted != Some(json!(true))) {
                continue;
            }
            let files = self.catalogue.related("remote_file", id)?;
            let entitlements = self.catalogue.effective_entitlements(id)?;
            let (files, _) = file_manifest(files, &entitlements);
            let files: Vec<_> = files
                .into_iter()
                .filter(|f| {
                    f["accountId"]
                        .as_str()
                        .is_some_and(|a| accounts.contains(a))
                })
                .collect();
            // The job that found an offer closed or a file refused recorded the list
            // still holding that file.
            let with_ended = fingerprint(&files);
            let files = self.without_last_failure(files, skipped_failures(false))?;
            // Bytes already here leave nothing to fetch, as Downloaded counts them.
            let here = ["ready", "downloaded", "extracting", "extractionFailed"];
            if files.is_empty() || self.all_at_revision(&files, &here)? {
                continue;
            }
            let fingerprint = fingerprint(&files);
            let asked = |value: &Value| *value == fingerprint || *value == with_ended;
            // Failed/cancelled work requires an explicit retry; only a changed manifest
            // queues again. Jobs from before the intent remembered its list still count.
            if intent
                .as_ref()
                .is_some_and(|intent| asked(&intent["manifestFingerprint"]))
                || self
                    .catalogue
                    .related("job", id)?
                    .iter()
                    .any(|job| job["type"] == "download" && asked(&job["manifestFingerprint"]))
            {
                continue;
            }
            pending.push(id.clone());
        }
        if !pending.is_empty() {
            self.enqueue(&json!({"productIds":pending,"automatic":true}))
                .await?;
        }
        Ok(())
    }
    /// Account logout/removal holds this existing gate through session cleanup
    /// and record mutation; queued work rechecks current account facts afterward.
    pub async fn pause_download_queue(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.queue_gate.lock().await
    }
    async fn enqueue(&self, args: &Value) -> Result<Value> {
        let _work = self.catalogue.begin_catalogue_work()?;
        let _operation = self
            .operations
            .try_read()
            .map_err(|_| anyhow::anyhow!("storage_busy"))?;
        let _gate = self.queue_gate.lock().await;
        let automatic = args["automatic"] == true;
        let root = self.root(args["rootId"].as_str())?;
        self.root_path(&root)?;
        let ids = strings(&args["productIds"]);
        ensure!(!ids.is_empty(), "invalid_arguments");
        // Several products at once skip any with nothing to download; one explicit
        // product keeps its actionable incomplete-manifest result.
        let bulk = ids.iter().collect::<HashSet<_>>().len() > 1;
        let selected = args.get("selectedFileIds").cloned().unwrap_or(Value::Null);
        ensure!(selected.is_null() || ids.len() == 1, "invalid_arguments");
        let mut result = Vec::new();
        let jobs = self.catalogue.list("job")?;
        let active: Vec<_> = jobs
            .iter()
            .filter(|j| {
                j["type"] == "download"
                    && j["cancelRequested"] != true
                    && matches!(j["state"].as_str(), Some("queued" | "running"))
            })
            .collect();
        let mut writes = Vec::new();
        let mut source_required = false;
        for id in ids.into_iter().collect::<HashSet<_>>() {
            let Some(product) = self.catalogue.get("product", &id)? else {
                if automatic {
                    continue;
                }
                bail!("product_missing");
            };
            if self.deleting.lock().unwrap().contains(&id) {
                if automatic {
                    continue;
                }
                bail!("storage_busy");
            }
            if product["provider"] == "local" {
                ensure!(selected.is_null(), "local_source_required");
                match self.retry_local_product_locked(&id, &jobs) {
                    Ok(job) => result.extend(job.map(Value::from)),
                    // Several products at once go on without one whose source must be
                    // chosen again; asked for alone, or with nothing else to do, it says so.
                    Err(error) if bulk && error.to_string() == "local_source_required" => {
                        source_required = true;
                    }
                    Err(error) => return Err(error),
                }
                continue;
            }
            let (files, complete) = self.remote_files(&id)?;
            let listed = !files.is_empty();
            let files = if automatic {
                self.enabled_files(files)?
            } else {
                files
            };
            let files = if selected.is_null() {
                self.without_last_failure(files, skipped_failures(!(automatic || bulk)))?
            } else {
                files
            };
            // Only an unknown list is worth a job that says so; files the store refused
            // leave nothing to ask for.
            if (automatic || bulk || listed) && files.is_empty() {
                continue;
            }
            let files = selected_download_files(files, &selected)?;
            let fingerprint = fingerprint(&files);
            let running = active.iter().find(|job| {
                job["productId"] == id
                    && if selected.is_null() {
                        job["selectedFileIds"].is_null()
                    } else {
                        job["selectedFileIds"] == selected
                            || (job["selectedFileIds"].is_null()
                                && strings(&job["fileIds"]).contains(&strings(&selected)[0]))
                    }
            });
            if selected.is_null() {
                // The list a job asks for; one already active asks for its own.
                let asked =
                    running.map_or(json!(fingerprint), |job| job["manifestFingerprint"].clone());
                writes.push((
                    "download_intent".into(),
                    id.clone(),
                    json!({"id":id,"wantDownload":true,"manifestFingerprint":asked}),
                ));
            }
            if let Some(job) = running {
                ensure!(
                    args["redownload"] != true || job["redownload"] == true,
                    "storage_busy"
                );
                result.push(job["id"].clone());
                continue;
            }
            let job_id = uuid::Uuid::new_v4().to_string();
            let job = json!({"id":job_id,"type":"download","productId":id,"rootId":root["id"],"automatic":args["automatic"]==true,"skipEndedOffers":automatic||bulk,"selectedFileIds":selected,"redownload":args["redownload"]==true,"fileIds":files.iter().map(|f|f["id"].clone()).collect::<Vec<_>>(),"manifestComplete":complete,"manifestFingerprint":fingerprint,"state":if files.is_empty(){"needsAction"}else{"queued"},"errorCode":if files.is_empty(){Some("manifest_incomplete")}else{None},"progress":0,"createdAt":now(),"updatedAt":now()});
            writes.push(("job".into(), job_id.clone(), job));
            result.push(json!(job_id));
        }
        if source_required && result.is_empty() && !automatic {
            bail!("local_source_required");
        }
        self.catalogue.put_many(writes)?;
        api::answer(api::DownloadJobs {
            job_ids: result
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
        })
    }
    #[cfg(test)]
    pub(crate) async fn enqueue_for_test(&self, args: &Value) -> Result<Value> {
        self.enqueue(args).await
    }
    async fn enqueue_file(&self, args: &Value) -> Result<Value> {
        let id = io::required_string(args, "fileId")?;
        ensure!(
            args.get("redownload").is_none() || args["redownload"].is_boolean(),
            "invalid_arguments"
        );
        let file = self
            .catalogue
            .get("remote_file", id)?
            .ok_or_else(|| anyhow::anyhow!("file_missing"))?;
        self.enqueue(&json!({"productIds":[file["productId"]],"selectedFileIds":[id],"rootId":args["rootId"],"redownload":args["redownload"]==true})).await
    }
    pub async fn command(
        &self,
        app: &tauri::AppHandle,
        action: &str,
        args: Value,
    ) -> Result<Value> {
        let deleting = action == "delete_local_products"
            || (action == "retry_job"
                && args["jobId"].as_str().is_some_and(|id| {
                    self.catalogue
                        .get("job", id)
                        .ok()
                        .flatten()
                        .is_some_and(|job| job["type"] == "storage:delete_local_products")
                }));
        let _work = if deleting
            || matches!(
                action,
                "remap_root"
                    | "migrate_root"
                    | "cleanup_old_root"
                    | "export_backup"
                    | "restore_backup"
                    | "export_zip"
                    | "product_availability"
                    | "root_usage"
                    | "open_product_folder"
                    | "open_file"
                    | "open_root_folder"
                    | "cancel_job"
                    | "cancel_downloads"
            ) {
            None
        } else {
            Some(self.catalogue.begin_catalogue_work()?)
        };
        let result = match action {
            "import_local" => self.enqueue_local(&args).await,
            "local_files" => self.mutate_local_files(&args).await,
            "delete_local_products" => self.delete_local_products(&args, None).await,
            "enqueue_products" => self.enqueue(&args).await,
            "enqueue_file" => self.enqueue_file(&args).await,
            "cancel_job" => {
                let id = io::required_string(&args, "jobId")?;
                self.cancel_job(id)
            }
            "cancel_downloads" => self.cancel_downloads(&args).await,
            "delete_downloads" => self.delete_downloads(&args).await,
            "retry_job" => {
                let mut retry = args.clone();
                if let Some(id) = args["jobId"].as_str() {
                    let job = self
                        .catalogue
                        .get("job", id)?
                        .ok_or_else(|| anyhow::anyhow!("job_missing"))?;
                    if job["type"] == "storage:delete_local_products" {
                        let result = self.delete_local_products(&args, Some(id)).await;
                        self.changed(app, Some(id));
                        return result;
                    }
                    if job["type"] == "storage:import_local" {
                        let result = self.retry_local_import(id).await;
                        self.changed(app, Some(id));
                        return result;
                    }
                    ensure!(job["type"] == "download", "invalid_job");
                    retry["productIds"] = json!([job["productId"]]);
                    retry["rootId"] = job["rootId"].clone();
                    retry["selectedFileIds"] = job["selectedFileIds"].clone();
                    retry["redownload"] = job["redownload"].clone();
                } else if let Some(id) = args["fileId"].as_str() {
                    let _ = id;
                    return self.enqueue_file(&args).await;
                }
                self.enqueue(&retry).await
            }
            "discard_import" => {
                let id = io::required_string(&args, "jobId")?;
                self.discard_local_import(id).await
            }
            "configure_root" => self.configure_root(&args).await,
            "remap_root" | "migrate_root" | "cleanup_old_root" | "export_backup"
            | "restore_backup" | "export_zip" => self.launch_operation(app, action, args).await,
            "product_availability" => {
                let service = self.clone();
                tokio::task::spawn_blocking(move || service.product_availability(&args)).await?
            }
            "root_usage" => {
                let service = self.clone();
                tokio::task::spawn_blocking(move || service.root_usage(&args)).await?
            }
            "open_product_folder" | "open_file" | "open_root_folder" => {
                let service = self.clone();
                let action = action.to_owned();
                tokio::task::spawn_blocking(move || service.open_artifact(&action, &args)).await?
            }
            _ => bail!("unsupported_storage_action"),
        };
        if !matches!(
            action,
            "product_availability"
                | "root_usage"
                | "open_product_folder"
                | "open_file"
                | "open_root_folder"
        ) {
            self.changed(app, None);
        }
        result
    }
    async fn configure_root(&self, args: &Value) -> Result<Value> {
        if let Some(path) = self.catalogue.library_root() {
            let selected = checked_library_root(Path::new(io::required_string(args, "path")?))?;
            ensure!(selected == path, "use_remap_or_migrate");
            let mut root = self.root(None)?;
            root["path"] = json!(path);
            return Ok(root);
        }
        let _lock = self.operations.write().await;
        let path = PathBuf::from(io::required_string(args, "path")?);
        let path = tokio::task::spawn_blocking(move || io::absolute_dir(&path, true)).await??;
        for root in self.catalogue.list("root")? {
            if Path::new(root["path"].as_str().unwrap_or("")) == path {
                return Ok(root);
            }
        }
        ensure!(
            self.catalogue.list("root")?.is_empty(),
            "use_remap_or_migrate"
        );
        let id = uuid::Uuid::new_v4().to_string();
        let root = json!({"id":id,"path":path,"status":"online","active":true});
        self.catalogue.put("root", &id, root.clone())?;
        self.catalogue
            .command("preferences", json!({"patch":{"libraryRootId":id}}))?;
        Ok(root)
    }
}
