mod auth;
mod batch;
pub(crate) use batch::RecordBatch;
mod fetch;
mod free_item;
mod http;
mod incremental;
mod metadata;
pub(crate) use metadata::effective_public_page_availability;
pub(crate) use metadata::{listing_price_display, listing_starting_price, parse_jpy_price};
mod parser;
mod safety;
mod sync;
#[cfg(test)]
pub(crate) use sync::assert_declared as assert_sync_record;
#[cfg(test)]
mod tests;

use crate::catalogue::Catalogue;
use anyhow::{bail, ensure, Result};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter, Manager};

/// Ephemeral capability. No Debug/Serialize implementation: never persist signed URLs.
pub struct DownloadTicket {
    pub url: url::Url,
    pub client: reqwest::Client,
    pub expected_size: Option<u64>,
}
/// Storage validates the actual final streamed response again; a ticket is not proof of bytes.
pub fn validate_download_response(response: &reqwest::Response) -> Result<()> {
    if !safety::download_destination(response.url()) {
        bail!("UNSAFE_DOWNLOAD_URL");
    }
    // One file answering 401 or BOOTH's sign-in page is refused alone: only an account-level
    // listing can say the session is signed out.
    if safety::sign_in_page(response.url()) {
        bail!("PERMISSION_DENIED");
    }
    match response.status().as_u16() {
        401 => bail!("PERMISSION_DENIED"),
        403 if response.url().host_str().is_some_and(safety::booth_host) => {
            bail!("PERMISSION_DENIED")
        }
        403 => bail!("SIGNED_LINK_EXPIRED"),
        404 | 410 => bail!("FILE_UNAVAILABLE"),
        429 => bail!("RATE_LIMITED"),
        500..=599 => bail!("DOWNLOAD_RESOLVER_FAILED"),
        _ => {}
    }
    if !response.status().is_success() {
        bail!("DOWNLOAD_RESOLVER_FAILED");
    }
    let attachment = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().starts_with("attachment"));
    let html = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().contains("text/html"));
    if html && !attachment {
        bail!("DOWNLOAD_LINK_UNAVAILABLE");
    }
    Ok(())
}
/// A free item's page proves its offer ended: no file controls left, or no page.
fn free_offer_closed(status: u16, body: &str, id: &str) -> bool {
    matches!(status, 404 | 410)
        || (200..300).contains(&status)
            && free_item::parse(body, id).is_ok_and(|evidence| {
                evidence.error_code.as_deref() == Some("FREE_OFFER_UNAVAILABLE")
            })
}
/// What one readable page says about a file BOOTH refused: still listed (`Some(true)`),
/// missing from a complete listing of its product and variation (`Some(false)`), or
/// nothing. A file without a variation ID is matched by its variation's name.
fn page_offers(rows: &[parser::Row], file: &Value) -> Option<bool> {
    let id = file["sourceFileId"].as_str()?;
    let product = file["productId"].as_str()?;
    let rows: Vec<_> = rows
        .iter()
        .filter(|row| row.product_id.as_deref() == Some(product))
        .collect();
    if rows.iter().any(|row| row.files.iter().any(|f| f.id == id)) {
        return Some(true);
    }
    rows.iter()
        .any(|row| {
            row.manifest_complete
                && row.error_code.is_none()
                && match file["variantId"].as_str() {
                    Some(variant) => row
                        .files
                        .iter()
                        .any(|f| f.variant_id.as_deref() == Some(variant)),
                    None => row.variant_name.as_deref() == file["variantName"].as_str(),
                }
        })
        .then_some(false)
}
/// What a free item's page says about a file BOOTH refused from it: the offer ended, or
/// the file is gone while the offer goes on, as when a seller replaces a version.
fn free_refusal(status: u16, body: &str, id: &str, file: &Value) -> Option<&'static str> {
    if free_offer_closed(status, body, id) {
        return Some("FREE_OFFER_UNAVAILABLE");
    }
    let evidence = free_item::parse(body, id)
        .ok()
        .filter(|_| (200..300).contains(&status))?;
    let row = parser::Row {
        product_id: Some(evidence.product_id),
        files: evidence.files,
        manifest_complete: evidence.manifest_complete,
        error_code: evidence.error_code,
        ..Default::default()
    };
    (page_offers(&[row], file) == Some(false)).then_some("FILE_UNAVAILABLE")
}
/// Unsigned route for one catalogue file: its bound variant and, for an audio
/// track, the MP3 rendition.
fn download_route(file: &Value) -> Result<String> {
    let id = field(file, "sourceFileId")?;
    let (resolver_id, _, _) = safety::resolver(field(file, "sourceUrl")?)
        .ok_or_else(|| anyhow::anyhow!("INVALID_RESOLVER"))?;
    let variant = file["variantId"].as_str();
    let format = file["downloadFormat"].as_str();
    if !safety::numeric(id)
        || resolver_id != id
        || variant.is_some_and(|v| !safety::numeric(v))
        || format.is_some_and(|f| f != safety::TRACK_FORMAT)
    {
        bail!("INVALID_RESOLVER");
    }
    let query: Vec<_> = variant
        .map(|v| format!("variation_id={v}"))
        .into_iter()
        .chain(format.map(|f| format!("type={f}")))
        .collect();
    Ok(format!(
        "https://booth.pm/downloadables/{id}{}",
        if query.is_empty() {
            String::new()
        } else {
            format!("?{}", query.join("&"))
        }
    ))
}
#[derive(Clone)]
pub struct BoothService {
    inner: Arc<Inner>,
}
struct Inner {
    catalogue: Catalogue,
    http: http::MetadataHttp,
    running: Mutex<BTreeSet<String>>,
    execution: tokio::sync::Mutex<()>,
    recovering: Mutex<bool>,
    last_progress: Mutex<Option<(String, std::time::Instant)>>,
    _process_lock: Arc<std::fs::File>,
}
fn field<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args[key]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("INVALID_ARGUMENT_{key}"))
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
/// A reopened hidden profile can take a few seconds to load its saved cookies. Only a jar
/// still empty when the limit passes means there is no session to use.
async fn loaded_cookies<T, F, Fut>(limit: std::time::Duration, mut read: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        match read().await {
            Err(error)
                if code(&error) == "AUTH_REQUIRED" && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await
            }
            result => return result,
        }
    }
}
/// One job per account at a time; a store-wide job (a legacy update) runs alone.
fn blocked(running: &BTreeSet<String>, key: &str) -> bool {
    running.contains(key)
        || running.contains("booth-global")
        || (key == "booth-global" && !running.is_empty())
}
/// The fixed code of a BOOTH failure, never a message that could carry a URL.
pub(crate) fn error_code(error: &anyhow::Error) -> String {
    code(error)
}
fn code(error: &anyhow::Error) -> String {
    let value = error.to_string();
    if value.len() <= 100
        && value
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
    {
        value
    } else {
        "BOOTH_OPERATION_FAILED".into()
    }
}
impl BoothService {
    #[cfg(test)]
    pub(crate) fn execution_gate(&self) -> &tokio::sync::Mutex<()> {
        &self.inner.execution
    }
    pub fn new(catalogue: Catalogue) -> Result<Self> {
        let process_lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(catalogue.device_dir().join("booth.lock"))
            .map_err(|_| anyhow::anyhow!("BOOTH_LOCK_UNAVAILABLE"))?;
        fs2::FileExt::try_lock_exclusive(&process_lock)
            .map_err(|_| anyhow::anyhow!("BOOTH_ALREADY_RUNNING"))?;
        Self::with_process_lock(catalogue, Arc::new(process_lock))
    }
    pub(crate) fn rebind_catalogue(&self, catalogue: Catalogue) -> Result<Self> {
        ensure!(
            catalogue.device_dir() == self.inner.catalogue.device_dir(),
            "BOOTH_DEVICE_MISMATCH"
        );
        Self::with_process_lock(catalogue, self.inner._process_lock.clone())
    }
    fn with_process_lock(catalogue: Catalogue, process_lock: Arc<std::fs::File>) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(Inner {
                catalogue,
                http: http::MetadataHttp::new(),
                running: Mutex::new(BTreeSet::new()),
                execution: tokio::sync::Mutex::new(()),
                recovering: Mutex::new(false),
                last_progress: Mutex::new(None),
                _process_lock: process_lock,
            }),
        })
    }
    fn changed(&self, app: &AppHandle, job: Option<&str>) {
        let _ = app.emit(
            "nna:changed",
            crate::ipc::Change {
                job_id: job.map(str::to_owned),
                ..crate::ipc::Change::scope("booth")
            },
        );
    }
    fn known_account(&self, id: &str) -> Result<Value> {
        safety::account_id(id)?;
        self.inner
            .catalogue
            .get("account", id)?
            .filter(|a| a["store"] == "BOOTH")
            .ok_or_else(|| anyhow::anyhow!("ACCOUNT_UNAVAILABLE"))
    }
    fn account(&self, id: &str) -> Result<Value> {
        let account = self.known_account(id)?;
        if account["disconnected"] == true {
            bail!("ACCOUNT_UNAVAILABLE");
        }
        Ok(account)
    }
    /// Lead calls this once at startup; interrupted syncs resume through checkpoint validation.
    pub async fn start(&self, app: AppHandle) -> Result<()> {
        let _work = self.inner.catalogue.begin_catalogue_work()?;
        {
            let mut recovering = self
                .inner
                .recovering
                .lock()
                .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
            if *recovering {
                return Ok(());
            }
            *recovering = true;
        }
        for job in self.inner.catalogue.list("job")? {
            if !matches!(
                job["type"].as_str(),
                Some("boothSync" | "boothUpdate" | "boothLogin")
            ) || !matches!(job["state"].as_str(), Some("running" | "queued"))
            {
                continue;
            }
            let Some(job) = self.prepare_recovered_job(job)? else {
                continue;
            };
            if let Err(error) = self.spawn_job(app.clone(), job) {
                if code(&error) != "ACCOUNT_BUSY" {
                    return Err(error);
                }
            }
        }
        Ok(())
    }
    fn prepare_recovered_job(&self, mut job: Value) -> Result<Option<Value>> {
        let id = field(&job, "id")?.to_owned();
        let result = (|| -> Result<Option<Value>> {
            if self.inner.catalogue.library_root().is_some()
                && job["originDeviceId"] != self.inner.catalogue.device_id()
            {
                job["state"] = json!("needsAction");
                job["errorCode"] = json!("RETRY_REQUIRED");
                job["updatedAt"] = json!(now());
                self.inner.catalogue.put("job", &id, job)?;
                return Ok(None);
            }
            if job["type"] == "boothLogin" {
                job["state"] = json!("needsAction");
                job["errorCode"] = json!("RETRY_REQUIRED");
                job["updatedAt"] = json!(now());
                self.inner.catalogue.put("job", &id, job.clone())?;
                if let Some(account) = job["accountId"].as_str() {
                    crate::stores::session::discard_unverified_account(
                        &self.inner.catalogue,
                        crate::stores::StoreProvider::Booth,
                        account,
                    )?;
                }
                return Ok(None);
            }
            if let Some(account) = job["accountId"].as_str() {
                let record = self.known_account(account)?;
                if !crate::stores::has_local_session(&self.inner.catalogue, &record) {
                    job["state"] = json!("needsAction");
                    job["errorCode"] = json!("AUTH_REQUIRED");
                    job["updatedAt"] = json!(now());
                    self.inner.catalogue.put("job", &id, job)?;
                    return Ok(None);
                }
            }
            Ok(Some(job))
        })();
        match result {
            Ok(job) => Ok(job),
            Err(error) => {
                if let Some(before) = self
                    .inner
                    .catalogue
                    .get("job", &id)?
                    .filter(|job| matches!(job["state"].as_str(), Some("queued" | "running")))
                {
                    let mut failed = before.clone();
                    failed["state"] = json!("needsAction");
                    failed["errorCode"] = json!(code(&error));
                    failed["updatedAt"] = json!(now());
                    self.inner.catalogue.put_many_if_unchanged(
                        vec![("job".into(), id.clone(), failed)],
                        vec![("job".into(), id, Some(before))],
                    )?;
                }
                Ok(None)
            }
        }
    }
    pub async fn command(&self, app: &AppHandle, action: &str, args: Value) -> Result<Value> {
        let _work = if matches!(
            action,
            "account_add"
                | "account_login"
                | "account_logout"
                | "account_remove"
                | "sync"
                | "cancel_sync"
                | "retry_sync"
                | "update_products"
                | "import_free"
        ) {
            Some(self.inner.catalogue.begin_catalogue_work()?)
        } else {
            None
        };
        match action {
            "account_add" => {
                if args
                    .get("store")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s != "BOOTH")
                {
                    bail!("STORE_UNAVAILABLE");
                }
                let id = uuid::Uuid::new_v4().to_string();
                let name = args["name"]
                    .as_str()
                    .or_else(|| args["label"].as_str())
                    .map(safety::short)
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "BOOTH".into());
                self.inner.catalogue.put("account",&id,json!({"id":id,"store":"BOOTH","name":name,"label":name,"state":"needsAction","status":"disconnected","identityVerified":false,"createdAt":now()}))?;
                self.changed(app, None);
                crate::stores::api::answer(crate::stores::api::StoreAccount { account_id: id })
            }
            "account_login" => {
                let account = field(&args, "accountId")?.to_owned();
                self.known_account(&account)?;
                auth::window(app, &self.inner.catalogue.device_dir(), &account, true).await?;
                let job = self.make_job("boothLogin", json!({"accountId":account}))?;
                self.spawn_job(app.clone(), job.clone())?;
                crate::stores::api::StoreJob::of(&job)
            }
            "account_logout" | "account_remove" => {
                self.disconnect_account(app, action, &args).await
            }
            "sync" => {
                let id = field(&args, "accountId")?;
                self.account(id)?;
                if let Some(existing) = self.inner.catalogue.list("job")?.into_iter().find(|job| {
                    job["accountId"] == id
                        && matches!(job["type"].as_str(), Some("boothSync" | "boothUpdate"))
                        && matches!(job["state"].as_str(), Some("queued" | "running"))
                }) {
                    return crate::stores::api::StoreJob::of(&existing);
                }
                let source = args["source"].as_str().unwrap_or("all");
                if source != "all" {
                    safety::source_name(source)?;
                }
                let mode = args["mode"].as_str().unwrap_or("incremental");
                if !matches!(mode, "incremental" | "full" | "forced") {
                    bail!("INVALID_SYNC_MODE");
                }
                let job = self.make_job(
                    "boothSync",
                    json!({"accountId":id,"source":source,"mode":mode}),
                )?;
                self.spawn_job(app.clone(), job.clone())?;
                crate::stores::api::StoreJob::of(&job)
            }
            "cancel_sync" => self.cancel_job(app, &args),
            "retry_sync" => self.retry_job(app, &args).await,
            "update_products" => self.update_selected(app, &args),
            "import_free" => self.import_free(app, &args),
            _ => bail!("UNKNOWN_BOOTH_ACTION"),
        }
    }

    /// Logs an account out, or removes it with or without its products.
    async fn disconnect_account(
        &self,
        app: &AppHandle,
        action: &str,
        args: &Value,
    ) -> Result<Value> {
        let id = field(args, "accountId")?;
        self.known_account(id)?;
        self.inner.catalogue.require_account_idle(id)?;
        if self
            .inner
            .running
            .lock()
            .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?
            .contains(&format!("account:{id}"))
        {
            bail!("ACCOUNT_BUSY");
        }
        auth::disconnect(app, &self.inner.catalogue.device_dir(), id).await?;
        if action == "account_remove" {
            let result = self
                .inner
                .catalogue
                .remove_account(id, args["removeProducts"] == true)?;
            self.changed(app, None);
            return Ok(result);
        }
        let mut a = self.known_account(id)?;
        if self.inner.catalogue.library_root().is_none() {
            a["disconnected"] = json!(true);
            a["state"] = json!("disconnected");
            a["status"] = json!("disconnected");
            self.inner.catalogue.put("account", id, a)?;
        }
        self.changed(app, None);
        crate::stores::api::answer(crate::stores::api::StoreAccount {
            account_id: id.to_owned(),
        })
    }

    /// Cancels a queued or running BOOTH job.
    fn cancel_job(&self, app: &AppHandle, args: &Value) -> Result<Value> {
        let id = field(args, "jobId")?;
        let job = self
            .inner
            .catalogue
            .get("job", id)?
            .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))?;
        if !matches!(
            job["type"].as_str(),
            Some("boothSync" | "boothUpdate" | "boothLogin")
        ) {
            bail!("INVALID_JOB");
        }
        for attempt in 0..4 {
            let mut batch = batch::RecordBatch::new(&self.inner.catalogue);
            let mut latest = batch
                .get("job", id)?
                .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))?;
            if !matches!(latest["state"].as_str(), Some("queued" | "running")) {
                break;
            }
            latest["state"] = json!("cancelled");
            latest["updatedAt"] = json!(now());
            latest["completedAt"] = json!(now());
            batch.stage("job", id, latest)?;
            if batch.try_commit()?.is_some() {
                break;
            }
            ensure!(attempt < 3, "CONCURRENT_UPDATE");
        }
        self.changed(app, Some(id));
        crate::stores::api::answer(crate::stores::api::StoreJob {
            job_id: id.to_owned(),
        })
    }

    /// Runs a stopped BOOTH job again once its account is free.
    async fn retry_job(&self, app: &AppHandle, args: &Value) -> Result<Value> {
        let id = field(args, "jobId")?;
        let job = self
            .inner
            .catalogue
            .get("job", id)?
            .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))?;
        if !matches!(
            job["type"].as_str(),
            Some("boothSync" | "boothUpdate" | "boothLogin")
        ) {
            bail!("INVALID_JOB");
        }
        if !matches!(
            job["state"].as_str(),
            Some("failed" | "needsAction" | "cancelled")
        ) {
            bail!("JOB_BUSY");
        }
        let key = job["accountId"]
            .as_str()
            .map(|id| format!("account:{id}"))
            .unwrap_or_else(|| {
                if job["args"]["publicOnly"] == true {
                    format!("booth-public:{id}")
                } else {
                    "booth-global".into()
                }
            });
        // A retried update waits for the account like a new one.
        if job["type"] != "boothUpdate"
            && self
                .inner
                .running
                .lock()
                .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?
                .contains(&key)
        {
            bail!("ACCOUNT_BUSY");
        }
        if job["type"] == "boothLogin" {
            let account = field(&job, "accountId")?;
            self.known_account(account)?;
            auth::window(app, &self.inner.catalogue.device_dir(), account, true).await?;
        }
        let job = self.requeue_job(&job)?;
        self.spawn_job(app.clone(), job)?;
        crate::stores::api::answer(crate::stores::api::StoreJob {
            job_id: id.to_owned(),
        })
    }

    /// One update job per scope that holds the selected products, or the one already running.
    fn update_selected(&self, app: &AppHandle, args: &Value) -> Result<Value> {
        let ids = args["productIds"]
            .as_array()
            .or_else(|| args["ids"].as_array())
            .ok_or_else(|| anyhow::anyhow!("INVALID_PRODUCT_IDS"))?;
        let ids: BTreeSet<String> = ids
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .filter(|id| safety::numeric(id))
                    .map(str::to_owned)
                    .ok_or_else(|| anyhow::anyhow!("INVALID_PRODUCT_IDS"))
            })
            .collect::<Result<_>>()?;
        ensure!(!ids.is_empty(), "INVALID_PRODUCT_IDS");
        let mut jobs = Vec::new();
        for scope in self.update_scopes(&ids)? {
            if let Some(active) = self.active_update(&scope)? {
                jobs.push(active["id"].clone());
                continue;
            }
            let job = self.make_job("boothUpdate", scope)?;
            self.spawn_job(app.clone(), job.clone())?;
            jobs.push(job["id"].clone());
        }
        crate::stores::api::StoreJobs::of(jobs)
    }
    /// An identical update already waiting or running covers a repeated request.
    fn active_update(&self, scope: &Value) -> Result<Option<Value>> {
        Ok(self.inner.catalogue.list("job")?.into_iter().find(|job| {
            job["type"] == "boothUpdate"
                && job["args"] == *scope
                && matches!(job["state"].as_str(), Some("queued" | "running"))
        }))
    }
    fn update_scopes(&self, ids: &BTreeSet<String>) -> Result<Vec<Value>> {
        let mut sources = self.inner.catalogue.list("entitlement")?;
        sources.extend(
            self.inner
                .catalogue
                .list("acquisition")?
                .into_iter()
                .filter(|row| row["source"] == "free"),
        );
        let mut accounts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut owned = BTreeSet::new();
        for source in sources {
            let (Some(product), Some(account)) =
                (source["productId"].as_str(), source["accountId"].as_str())
            else {
                continue;
            };
            if ids.contains(product) && self.known_account(account).is_ok() {
                accounts
                    .entry(account.into())
                    .or_default()
                    .insert(product.into());
                owned.insert(product.to_owned());
            }
        }
        let mut scopes: Vec<_> = accounts
            .into_iter()
            .map(|(account, products)| json!({"accountId":account,"productIds":products}))
            .collect();
        let public: Vec<_> = ids.difference(&owned).cloned().collect();
        if !public.is_empty() {
            scopes.push(json!({"productIds":public,"publicOnly":true}));
        }
        Ok(scopes)
    }
    fn make_job(&self, kind: &str, args: Value) -> Result<Value> {
        let id = uuid::Uuid::new_v4().to_string();
        let account_name = args["accountId"]
            .as_str()
            .and_then(|account| self.known_account(account).ok())
            .and_then(|account| account["name"].as_str().map(str::to_owned));
        let mut job = json!({"id":id,"type":kind,"store":"BOOTH","accountId":args["accountId"],"accountName":account_name,"args":args,"state":"queued","progress":{"stage":"collecting","pages":0,"products":0},"createdAt":now(),"updatedAt":now(),"originDeviceId":self.inner.catalogue.device_id()});
        crate::support::reset_attempt(&mut job);
        self.inner.catalogue.put("job", &id, job.clone())?;
        Ok(job)
    }
    fn spawn_job(&self, app: AppHandle, mut job: Value) -> Result<()> {
        let work = self.inner.catalogue.begin_catalogue_work()?;
        let id = field(&job, "id")?.to_owned();
        let key = job["accountId"]
            .as_str()
            .map(|a| format!("account:{a}"))
            .unwrap_or_else(|| {
                if job["args"]["publicOnly"] == true {
                    format!("booth-public:{id}")
                } else {
                    "booth-global".into()
                }
            });
        {
            let mut running = self
                .inner
                .running
                .lock()
                .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
            if blocked(&running, &key) {
                // An update waits for the account's sync or update to end, as a sync waits
                // for its login or a cancelled task to finish; it stays queued meanwhile.
                if job["type"] == "boothUpdate"
                    || (job["type"] == "boothSync" && running.contains(&key))
                {
                    let service = self.clone();
                    tauri::async_runtime::spawn(async move {
                        let _work = work;
                        let error = match service.wait_for_turn(&job, &key).await {
                            Ok(true) => service
                                .spawn_job(app.clone(), job.clone())
                                .err()
                                .map(|error| code(&error)),
                            Ok(false) => Some("CANCELLED".into()),
                            Err(_) => Some("JOB_UNAVAILABLE".into()),
                        };
                        if let Some(error) = error {
                            if let Err(error) = service.finish_job(&job, Some(&error)) {
                                eprintln!("BOOTH terminal write failed: {}", code(&error));
                            }
                        }
                        service.changed(&app, Some(&id));
                    });
                    return Ok(());
                }
                job["state"] = json!("needsAction");
                job["errorCode"] = json!("ACCOUNT_BUSY");
                job["completedAt"] = json!(now());
                self.inner.catalogue.put("job", &id, job)?;
                bail!("ACCOUNT_BUSY");
            }
            running.insert(key.clone());
        }
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            let _work = work;
            // Accounts of this store share one execution queue. Other providers
            // own independent gates and continue concurrently.
            let _execution = if job["type"] == "boothLogin" {
                None
            } else {
                Some(service.inner.execution.lock().await)
            };
            crate::support::begin_attempt(&mut job);
            let (result, diagnostics) = crate::support_trace::capture(async {
                crate::support_trace::job_stage(&job);
                service.execute(&app, &mut job).await
            })
            .await;
            job["supportDiagnostics"] = diagnostics;
            crate::support::end_attempt(&mut job);
            let mut error_code = result.as_ref().err().map(code);
            // Complete login window handling before exposing the terminal job or
            // releasing this account. Verified credentials remain device-local.
            if job["type"] == "boothLogin" {
                if let Some(account) = job["accountId"].as_str() {
                    if crate::stores::session::finish_login(
                        &app,
                        crate::stores::StoreProvider::Booth,
                        account,
                        result.is_ok(),
                    )
                    .await
                    .is_err()
                    {
                        error_code = Some("AUTH_WINDOW_FAILED".into());
                    }
                }
            }
            if let Err(error) = service.finish_job(&job, error_code.as_deref()) {
                // Fixed code only: a failed durable terminal write is never
                // treated as successful completion or logged with HTTP data.
                eprintln!("BOOTH terminal write failed: {}", code(&error));
                let _ = app.emit(
                    "nna:changed",
                    crate::ipc::Change {
                        error_code: Some("TERMINAL_WRITE_FAILED".into()),
                        ..crate::ipc::Change::job("jobs", &id)
                    },
                );
            }
            if let Ok(mut running) = service.inner.running.lock() {
                running.remove(&key);
            }
            if job["type"] == "boothLogin" {
                if let Some(account) = job["accountId"].as_str() {
                    let _ = crate::stores::session::discard_unverified_account(
                        &service.inner.catalogue,
                        crate::stores::StoreProvider::Booth,
                        account,
                    );
                }
                if service
                    .inner
                    .catalogue
                    .get("job", &id)
                    .ok()
                    .flatten()
                    .is_some_and(|terminal| terminal["state"] == "completed")
                {
                    let _ = app.emit(
                        "nna:changed",
                        crate::ipc::Change {
                            session_changed: Some(true),
                            ..crate::ipc::Change::scope("booth")
                        },
                    );
                }
            }
            service.changed(&app, Some(&id));
        });
        Ok(())
    }
    fn requeue_job(&self, previous: &Value) -> Result<Value> {
        let id = field(previous, "id")?;
        ensure!(
            matches!(
                previous["state"].as_str(),
                Some("failed" | "needsAction" | "cancelled")
            ),
            "JOB_BUSY"
        );
        let mut job = previous.clone();
        crate::support::reset_attempt(&mut job);
        job["state"] = json!("queued");
        job["errorCode"] = Value::Null;
        job["completedAt"] = Value::Null;
        job["updatedAt"] = json!(now());
        job["originDeviceId"] = json!(self.inner.catalogue.device_id());
        // Login may await a native window. A second Retry must not overwrite
        // the attempt another caller has queued or started in the meantime.
        ensure!(
            self.inner.catalogue.put_many_if_unchanged(
                vec![("job".into(), id.into(), job.clone())],
                vec![("job".into(), id.into(), Some(previous.clone()))],
            )?,
            "JOB_BUSY"
        );
        Ok(job)
    }
    /// A sign-in signal keeps the protected session: only logout, deletion, a failed first
    /// login or a replacing login clear it. This device waits for a sign-in, which restores it.
    fn finish_job(&self, job: &Value, error: Option<&str>) -> Result<()> {
        let id = field(job, "id")?;
        if error == Some("AUTH_REQUIRED") && self.inner.catalogue.library_root().is_some() {
            if let Some(account) = job["accountId"].as_str() {
                crate::stores::session::mark_signed_out(
                    &self.inner.catalogue.device_dir(),
                    crate::stores::StoreProvider::Booth,
                    account,
                )?;
            }
        }
        for _ in 0..4 {
            let mut batch = batch::RecordBatch::new(&self.inner.catalogue);
            let current = batch.get("job", id)?;
            // Clear history can remove a cancelled job's row while its task winds down.
            // Its account and sources still end; the row stays cleared.
            let cleared = current.is_none();
            let current = current.unwrap_or_else(|| json!({"state":"cancelled"}));
            // Source scanners report through local job copies. Keep the latest
            // durable counters when cancellation/failure ends the parent task.
            let mut terminal = current.clone();
            if let Some(diagnostics) = job.get("supportDiagnostics") {
                terminal["supportDiagnostics"] = diagnostics.clone();
            }
            if let Some(capture) = job.get("supportCapture") {
                terminal["supportCapture"] = capture.clone();
            }
            // Assets this attempt left as they were; an earlier attempt's count goes.
            match job["progress"].get("incompleteProducts") {
                Some(count) => terminal["progress"]["incompleteProducts"] = count.clone(),
                None => {
                    if let Some(progress) = terminal["progress"].as_object_mut() {
                        progress.remove("incompleteProducts");
                    }
                }
            }
            terminal["supportLastStage"] = json!(crate::support::operation_stage(
                terminal["progress"]["stage"].as_str().unwrap_or("")
            ));
            let error = if current["state"] == "cancelled" {
                Some("CANCELLED")
            } else {
                error
            };
            terminal["state"] = json!(match error {
                None => "completed",
                Some("CANCELLED") => "cancelled",
                Some(
                    "AUTH_REQUIRED"
                    | "AUTH_CHALLENGE"
                    | "PERMISSION_DENIED"
                    | "RATE_LIMITED"
                    | "SYNC_INCOMPLETE"
                    | "COLLECTOR_UNSUPPORTED"
                    | "IMPORT_TOO_LARGE"
                    | "AUTH_TIMEOUT"
                    | "SOURCE_UNSUPPORTED"
                    | "CONCURRENT_UPDATE"
                    | "PUBLIC_METADATA_FAILED"
                    | "PUBLIC_METADATA_INVALID",
                ) => "needsAction",
                Some(_) => "failed",
            });
            terminal["errorCode"] = json!(error);
            terminal["updatedAt"] = json!(now());
            terminal["completedAt"] = json!(now());
            terminal["progress"]["stage"] = json!(terminal["state"].as_str().unwrap());
            if job["type"] == "boothSync" {
                if let Some(account_id) = job["accountId"].as_str() {
                    if let Some(mut account) = batch.get("account", account_id)? {
                        account["lastSyncAttemptAt"] = terminal["completedAt"].clone();
                        if error.is_none() {
                            account["lastSync"] = terminal["completedAt"].clone();
                            account["lastSyncAt"] = terminal["completedAt"].clone();
                            // BOOTH sign-in records authenticatedAt, never identityVerified.
                            let established = account["identityVerified"] == true
                                || account["authenticatedAt"]
                                    .as_str()
                                    .is_some_and(|at| !at.is_empty());
                            if account["disconnected"] != true && established {
                                account["state"] = json!("connected");
                                account["status"] = json!("connected");
                            }
                        }
                        account["syncState"] = json!(if error.is_none() {
                            "complete"
                        } else {
                            "incomplete"
                        });
                        account["lastSyncErrorCode"] = json!(error);
                        crate::stores::record_sync_incomplete(&mut account, job, error.is_none());
                        if error == Some("AUTH_REQUIRED")
                            && self.inner.catalogue.library_root().is_none()
                        {
                            account["state"] = json!("expired");
                            account["status"] = json!("expired");
                        }
                        batch.stage("account", account_id, account)?;
                    }
                    for source in ["library", "purchase", "gift", "free"] {
                        let key = format!("{account_id}:{source}");
                        if let Some(mut coverage) = batch.get("source_coverage", &key)? {
                            if error.is_some()
                                && matches!(
                                    coverage["status"].as_str(),
                                    Some("running" | "collected")
                                )
                            {
                                coverage["status"] = json!("incomplete");
                                coverage["coverageComplete"] = json!(false);
                                coverage["errorCode"] = json!(error);
                                coverage["completedAt"] = terminal["completedAt"].clone();
                                batch.stage("source_coverage", &key, coverage)?;
                            }
                        }
                    }
                }
            } else if error == Some("AUTH_CHALLENGE") {
                // A sync's sources record this themselves. The account keeps the sign-in it
                // needs, so clearing Activity does not hide it.
                if let Some(account_id) = job["accountId"].as_str() {
                    if let Some(mut account) = batch.get("account", account_id)? {
                        account["state"] = json!("needsAction");
                        account["status"] = json!("needsAction");
                        account["errorCode"] = json!("AUTH_CHALLENGE");
                        batch.stage("account", account_id, account)?;
                    }
                }
            }
            if !cleared {
                batch.stage("job", id, terminal)?;
            }
            if batch.try_commit()?.is_some() {
                if matches!(job["type"].as_str(), Some("boothSync" | "boothUpdate")) {
                    self.inner.catalogue.refresh_avatar_defaults()?;
                }
                return Ok(());
            }
        }
        bail!("CONCURRENT_UPDATE")
    }
    /// Clear history removes only finished rows, so a job whose row is gone was cancelled.
    fn cancelled(&self, job: &Value) -> Result<bool> {
        Ok(self
            .inner
            .catalogue
            .get("job", field(job, "id")?)?
            .is_none_or(|j| j["state"] == "cancelled"))
    }
    /// Waits until nothing blocks this job's account; false once the job is cancelled.
    async fn wait_for_turn(&self, job: &Value, key: &str) -> Result<bool> {
        loop {
            if self.cancelled(job)? {
                return Ok(false);
            }
            let busy = self
                .inner
                .running
                .lock()
                .map(|active| blocked(&active, key))
                .unwrap_or(true);
            if !busy {
                return Ok(true);
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
    /// Whether this job's last durable progress is at least one event interval old.
    fn progress_due(&self, job: &Value) -> Result<bool> {
        let id = field(job, "id")?;
        let last = self
            .inner
            .last_progress
            .lock()
            .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
        Ok(last.as_ref().is_none_or(|(previous, at)| {
            previous != id || at.elapsed() >= std::time::Duration::from_millis(250)
        }))
    }
    fn progress(&self, app: &AppHandle, job: &mut Value) -> Result<()> {
        crate::support_trace::job_stage(job);
        if let Some(diagnostics) = crate::support_trace::snapshot() {
            job["supportDiagnostics"] = diagnostics;
        }
        job["state"] = json!("running");
        job["updatedAt"] = json!(now());
        let mut committed = false;
        for _ in 0..4 {
            let mut batch = batch::RecordBatch::new(&self.inner.catalogue);
            let current = batch
                .get("job", field(job, "id")?)?
                .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))?;
            if current["state"] == "cancelled" {
                bail!("CANCELLED");
            }
            // Concurrent source scanners hold local copies of this same job.
            // Their publication order must not move durable aggregate counters back.
            if job["type"] == "boothSync" {
                for field in [
                    "pages",
                    "detailsCompleted",
                    "requestsCompleted",
                    "published",
                ] {
                    if let Some(saved) = current["progress"][field].as_u64() {
                        if job["progress"][field].as_u64().unwrap_or(0) < saved {
                            job["progress"][field] = json!(saved);
                        }
                    }
                }
            }
            batch.stage("job", field(job, "id")?, job.clone())?;
            if batch.try_commit()?.is_some() {
                committed = true;
                break;
            }
        }
        if !committed {
            bail!("CONCURRENT_UPDATE");
        }
        let id = field(job, "id")?;
        let should_emit = {
            let mut last = self
                .inner
                .last_progress
                .lock()
                .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
            let emit = last.as_ref().is_none_or(|(previous, at)| {
                previous != id || at.elapsed() >= std::time::Duration::from_millis(250)
            });
            if emit {
                *last = Some((id.to_owned(), std::time::Instant::now()));
            }
            emit
        };
        if should_emit {
            let _ = app.emit("nna:changed", crate::ipc::Change::job("jobs", id));
        }
        Ok(())
    }
    async fn execute(&self, app: &AppHandle, job: &mut Value) -> Result<()> {
        self.progress(app, job)?;
        match job["type"].as_str().unwrap_or("") {
            "boothSync" => self.sync_job(app, job).await,
            "boothUpdate" => self.update_job(app, job).await,
            "boothLogin" => self.login_job(app, job).await,
            _ => bail!("INVALID_JOB"),
        }
    }
    async fn login_job(&self, app: &AppHandle, job: &mut Value) -> Result<()> {
        let id = field(&job["args"], "accountId")?.to_owned();
        let window = app
            .get_webview_window(&format!("booth-auth-{id}"))
            .ok_or_else(|| anyhow::anyhow!("CANCELLED"))?;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(600);
        self.login_stage(app, job, "cookieAccess", None)?;
        while tokio::time::Instant::now() < deadline {
            if self.cancelled(job)? || app.get_webview_window(window.label()).is_none() {
                bail!("CANCELLED");
            }
            match auth::jar(window.clone()).await {
                Err(error) => {
                    let error_code = code(&error);
                    if error_code != "AUTH_REQUIRED" {
                        self.login_stage(app, job, "cookieAccess", Some("COOKIE_ACCESS_FAILED"))?;
                        bail!("COOKIE_ACCESS_FAILED");
                    }
                    self.login_stage(app, job, "authRequired", Some("AUTH_REQUIRED"))?;
                }
                Ok(jar) => {
                    let client = http::build_client(jar, false)?;
                    match self
                        .inner
                        .http
                        .get(&client, safety::route("/library?page=1", "library")?)
                        .await
                    {
                        Err(error) => {
                            let error_code = code(&error);
                            self.login_stage(
                                app,
                                job,
                                if matches!(error_code.as_str(), "AUTH_REQUIRED" | "AUTH_CHALLENGE")
                                {
                                    "authRequired"
                                } else {
                                    "cookieAccess"
                                },
                                Some(&error_code),
                            )?;
                            if !matches!(error_code.as_str(), "AUTH_REQUIRED" | "AUTH_CHALLENGE") {
                                return Err(error);
                            }
                        }
                        Ok((status, body)) => {
                            if !status.is_success() {
                                self.login_stage(
                                    app,
                                    job,
                                    "cookieAccess",
                                    Some("SOURCE_HTTP_FAILED"),
                                )?;
                                bail!("SOURCE_HTTP_FAILED");
                            }
                            match parser::parse(&body, "library", "/library?page=1") {
                                Ok(_) => {
                                    self.login_stage(app, job, "httpSuccess", None)?;
                                    self.login_stage(app, job, "parserRecognition", None)?;
                                    let mut batch = batch::RecordBatch::new(&self.inner.catalogue);
                                    let current_job = batch.get("job", field(job, "id")?)?;
                                    if current_job.is_none_or(|row| {
                                        !matches!(row["state"].as_str(), Some("queued" | "running"))
                                    }) || app.get_webview_window(window.label()).is_none()
                                    {
                                        bail!("CANCELLED");
                                    }
                                    let mut account = batch
                                        .get("account", &id)?
                                        .ok_or_else(|| anyhow::anyhow!("ACCOUNT_UNAVAILABLE"))?;
                                    let unverified = account["identityVerified"] != true
                                        && account["authenticatedAt"]
                                            .as_str()
                                            .is_none_or(str::is_empty)
                                        && account["identityHash"]
                                            .as_str()
                                            .is_none_or(str::is_empty);
                                    crate::stores::session::remember(
                                        window.clone(),
                                        &self.inner.catalogue.device_dir(),
                                        crate::stores::StoreProvider::Booth,
                                        &id,
                                    )
                                    .await?;
                                    if app.get_webview_window(window.label()).is_none() {
                                        if unverified {
                                            crate::stores::session::forget(
                                                &self.inner.catalogue.device_dir(),
                                                crate::stores::StoreProvider::Booth,
                                                &id,
                                            )?;
                                        }
                                        bail!("CANCELLED");
                                    }
                                    account["disconnected"] = json!(false);
                                    account["state"] = json!("connected");
                                    account["status"] = json!("connected");
                                    account["authenticatedAt"] = json!(now());
                                    batch.stage("account", &id, account)?;
                                    match batch.try_commit() {
                                        Ok(Some(_)) => {}
                                        result => {
                                            if unverified {
                                                crate::stores::session::forget(
                                                    &self.inner.catalogue.device_dir(),
                                                    crate::stores::StoreProvider::Booth,
                                                    &id,
                                                )?;
                                            }
                                            result?;
                                            if self.cancelled(job)? {
                                                bail!("CANCELLED");
                                            }
                                            bail!("CONCURRENT_UPDATE");
                                        }
                                    }
                                    return Ok(());
                                }
                                Err(error) => {
                                    let error_code = code(&error);
                                    if matches!(
                                        error_code.as_str(),
                                        "AUTH_REQUIRED" | "AUTH_CHALLENGE"
                                    ) {
                                        self.login_stage(
                                            app,
                                            job,
                                            "authRequired",
                                            Some(&error_code),
                                        )?;
                                    } else {
                                        // A successful HTTP response is not authentication evidence.
                                        // Unsupported DOM keeps the account unverified and exposes only a fixed code.
                                        self.login_stage(app, job, "httpSuccess", None)?;
                                        self.login_stage(
                                            app,
                                            job,
                                            "parserRecognition",
                                            Some("SOURCE_UNSUPPORTED"),
                                        )?;
                                        bail!("SOURCE_UNSUPPORTED");
                                    }
                                }
                            }
                        }
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
        bail!("AUTH_TIMEOUT")
    }
    fn login_stage(
        &self,
        app: &AppHandle,
        job: &mut Value,
        stage: &str,
        error: Option<&str>,
    ) -> Result<()> {
        if job["progress"]["loginStage"] == stage
            && job["progress"]["lastErrorCode"].as_str() == error
        {
            return Ok(());
        }
        job["progress"]["loginStage"] = json!(stage);
        job["progress"]["lastErrorCode"] = json!(error);
        self.progress(app, job)
    }
    async fn client(
        &self,
        app: &AppHandle,
        account: &str,
        download: bool,
    ) -> Result<reqwest::Client> {
        let record = self.account(account)?;
        if !crate::stores::has_local_session(&self.inner.catalogue, &record) {
            bail!("AUTH_REQUIRED");
        }
        let window = auth::window(app, &self.inner.catalogue.device_dir(), account, false).await?;
        let jar = loaded_cookies(std::time::Duration::from_secs(10), || {
            auth::jar(window.clone())
        })
        .await?;
        http::build_client(jar, download)
    }
    /// Why BOOTH refused a file, when a page says so: a free offer that ended, read as a
    /// sync counts it, or a file a complete listing no longer has. BOOTH refuses a file
    /// a seller removed instead of answering 404. Any other answer, including a page that
    /// still lists the file, leaves the refusal as it was.
    async fn refusal_reason(
        &self,
        client: &reqwest::Client,
        file: &Value,
        refusal: &str,
    ) -> Option<&'static str> {
        if file["source"] == "free" {
            let route = file["resolverPage"].as_str()?;
            let id = safety::free_item_id(route)?;
            let (status, body) = self
                .inner
                .http
                .get(client, url::Url::parse(route).ok()?)
                .await
                .ok()?;
            return free_refusal(status.as_u16(), &body, id, file);
        }
        if refusal != "PERMISSION_DENIED" {
            return None;
        }
        for (url, source) in self.refusal_pages(file) {
            let route = url.to_string();
            let (status, body) = match self.inner.http.get(client, url).await {
                Ok(read) => read,
                // A list page that moved, or lies past a verification run's cap, says
                // nothing; signing out, throttling or an outage end the check.
                Err(error)
                    if matches!(
                        code(&error).as_str(),
                        "RESPONSE_ROUTE_CHANGED" | "VERIFICATION_PAGE_LIMIT"
                    ) =>
                {
                    continue
                }
                Err(_) => return None,
            };
            let Some(page) = status
                .is_success()
                .then(|| parser::parse(&body, source, &route).ok())
                .flatten()
            else {
                continue;
            };
            if let Some(offered) = page_offers(&page.rows, file) {
                return (!offered).then_some("FILE_UNAVAILABLE");
            }
        }
        None
    }
    /// The account pages that can list a refused file: the one that last listed it,
    /// then the receipts its account bought the product on. Three at most.
    fn refusal_pages(&self, file: &Value) -> Vec<(url::Url, &'static str)> {
        let mut pages: Vec<(url::Url, &'static str)> = vec![];
        let mut add = |route: &str, source: &'static str| {
            if let Ok(url) = safety::route(route, source) {
                if !pages.iter().any(|(saved, _)| saved == &url) {
                    pages.push((url, source));
                }
            }
        };
        let source = match file["source"].as_str() {
            Some("library") => Some("library"),
            Some("purchase") => Some("purchase"),
            Some("gift") => Some("gift"),
            _ => None,
        };
        if let (Some(route), Some(source)) = (file["resolverPage"].as_str(), source) {
            add(route, source);
        }
        let product = file["productId"].as_str().unwrap_or_default();
        for acquisition in self
            .inner
            .catalogue
            .related("acquisition", product)
            .unwrap_or_default()
        {
            if acquisition["accountId"] == file["accountId"] && acquisition["source"] == "purchase"
            {
                if let Some(order) = acquisition["orderId"]
                    .as_str()
                    .filter(|id| safety::numeric(id))
                {
                    add(&format!("/orders/{order}"), "purchase");
                }
            }
        }
        pages.truncate(3);
        pages
    }
    /// A cookie-less client for public item pages, past the R-18 confirmation.
    pub fn public_client(&self) -> Result<reqwest::Client> {
        let jar = Arc::new(reqwest::cookie::Jar::default());
        jar.add_cookie_str(
            "adult=t; Domain=booth.pm; Path=/",
            &url::Url::parse("https://booth.pm/").unwrap(),
        );
        http::build_client(jar, false)
    }
    /// A public item page, paced with the store sync's own requests.
    pub async fn public_item_page(
        &self,
        client: &reqwest::Client,
        id: &str,
    ) -> Result<(u16, String)> {
        ensure!(safety::numeric(id), "INVALID_PRODUCT_ID");
        let url = url::Url::parse(&format!("https://booth.pm/ja/items/{id}"))?;
        let (status, body) = self.inner.http.get(client, url).await?;
        Ok((status.as_u16(), body))
    }
    /// The bytes of a 0-yen download control from a public item page, read with an account
    /// that holds the product. BOOTH records it in that account's free-download history.
    pub async fn free_license_file(
        &self,
        app: &AppHandle,
        account: &str,
        route: &str,
        limit: u64,
    ) -> Result<Vec<u8>> {
        let (_, variant, route) =
            safety::resolver(route).ok_or_else(|| anyhow::anyhow!("INVALID_RESOLVER"))?;
        ensure!(variant.is_some(), "INVALID_RESOLVER");
        let client = self.client(app, account, true).await?;
        let mut response = self
            .inner
            .http
            .resolve(&client, url::Url::parse(&route)?)
            .await?;
        validate_download_response(&response)?;
        if response.content_length().is_some_and(|n| n > limit) {
            bail!("LICENSE_DOCUMENT_TOO_LARGE");
        }
        let mut body = vec![];
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("NETWORK_UNAVAILABLE"))?
        {
            ensure!(
                body.len() + chunk.len() <= limit as usize,
                "LICENSE_DOCUMENT_TOO_LARGE"
            );
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
    pub async fn download_ticket(&self, app: &AppHandle, file_id: &str) -> Result<DownloadTicket> {
        let file = self
            .inner
            .catalogue
            .get("remote_file", file_id)?
            .ok_or_else(|| anyhow::anyhow!("FILE_UNAVAILABLE"))?;
        let account = field(&file, "accountId")?;
        let route = download_route(&file)?;
        let client = self.client(app, account, true).await?;
        // Resolve anew for each attempt; signed destinations are never catalogue fields.
        let resolved = self
            .inner
            .http
            .resolve(&client, url::Url::parse(&route).unwrap())
            .await
            .and_then(|response| validate_download_response(&response).map(|()| response));
        let response = match resolved {
            Err(error)
                if matches!(
                    code(&error).as_str(),
                    "PERMISSION_DENIED" | "FILE_UNAVAILABLE"
                ) =>
            {
                match self.refusal_reason(&client, &file, &code(&error)).await {
                    Some(reason) => bail!(reason),
                    None => return Err(error),
                }
            }
            resolved => resolved?,
        };
        let final_url = response.url().clone();
        // The first body is dropped; storage streams a fresh GET to the ephemeral destination.
        // Only this response proves the current size; saved listing bytes can
        // describe an older offered revision when delivery is chunked.
        let expected_size = response.content_length();
        let current = self
            .inner
            .catalogue
            .get("remote_file", file_id)?
            .ok_or_else(|| anyhow::anyhow!("FILE_UNAVAILABLE"))?;
        if current["accountId"] != file["accountId"]
            || current["sourceFileId"] != file["sourceFileId"]
            || current["variantId"] != file["variantId"]
            || current["productId"] != file["productId"]
        {
            bail!("SOURCE_CHANGED");
        }
        if current["revisionBasis"] != "source" {
            if let Some(etag) = response
                .headers()
                .get(reqwest::header::ETAG)
                .and_then(|v| v.to_str().ok())
                .filter(|v| v.starts_with('"') && v.ends_with('"') && v.len() <= 1024)
            {
                // Strong server content validator is positive revision evidence; its opaque value is hashed.
                let mut current = current.clone();
                current["revision"] = json!(format!("etag:{}", safety::hash(etag)));
                current["revisionBasis"] = json!("etag");
                current["requiresContentVerification"] = json!(false);
                current["sizeBytes"] = json!(expected_size);
                self.inner.catalogue.put("remote_file", file_id, current)?;
            }
        }
        Ok(DownloadTicket {
            url: final_url,
            client,
            expected_size,
        })
    }
}
