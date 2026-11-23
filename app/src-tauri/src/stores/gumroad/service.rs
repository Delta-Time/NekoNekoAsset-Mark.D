//! Native Gumroad buyer collector. Capability routes are transient, never persisted.
use crate::stores::{
    api,
    gumroad::{self, ContentObservation, FileAccess, LibraryPage, LibraryPurchase},
    http, session, DownloadTicket, StoreProvider,
};
use crate::{booth::RecordBatch, catalogue::Catalogue};
use anyhow::{bail, ensure, Result};
use futures_util::{
    future::{BoxFuture, Shared},
    FutureExt, StreamExt,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use url::Url;
struct BundleMembers {
    purchases: Vec<LibraryPurchase>,
    pages: u64,
    head_signatures: [String; 2],
    complete: bool,
}

/// What a Gumroad library pass reads with; fixed when the pass starts.
#[derive(Clone, Copy)]
struct LibraryRead<'a> {
    app: &'a AppHandle,
    account: &'a str,
    client: &'a reqwest::Client,
    selected: &'a Option<BTreeSet<String>>,
    selected_bundles: &'a BTreeSet<String>,
    known_purchases: &'a BTreeMap<String, String>,
    buyer_identity: &'a str,
    checkpoint_id: &'a str,
    metadata_total: u64,
    page_limit: Option<u64>,
    metrics_before: [u64; 3],
    classified_before: [[u64; 13]; 2],
}

/// What a Gumroad library pass has read so far; its checkpoint saves it as the pass goes.
struct LibraryPass {
    verification_capped: bool,
    selected_purchases: BTreeSet<String>,
    signatures: [String; 2],
    owned_bundles: BTreeMap<String, gumroad::LibraryBundle>,
    incomplete: BTreeSet<String>,
    checkpoint: Value,
    seen: BTreeSet<String>,
    products: BTreeSet<String>,
    unresolved: u64,
    found: BTreeSet<String>,
    public_products: BTreeMap<String, Option<gumroad::PublicProductObservation>>,
    public_errors: BTreeMap<String, String>,
    public_unavailable: BTreeSet<String>,
    bundle_completed: BTreeSet<String>,
    bundled_purchases: BTreeSet<String>,
    bundle_memberships: BTreeMap<String, BundleMembers>,
}

type PublicRead<'a> =
    Shared<BoxFuture<'a, Result<Option<gumroad::PublicProductObservation>, String>>>;
fn shared_public_read<'a>(
    reads: &Mutex<BTreeMap<String, PublicRead<'a>>>,
    id: &str,
    make: impl FnOnce() -> PublicRead<'a>,
) -> Result<PublicRead<'a>> {
    let mut reads = reads
        .lock()
        .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
    Ok(reads.entry(id.into()).or_insert_with(make).clone())
}

#[derive(Clone)]
pub struct GumroadService {
    inner: Arc<Inner>,
}
struct Inner {
    catalogue: Catalogue,
    http: http::MetadataHttp,
    running: Mutex<BTreeSet<String>>,
    execution: tokio::sync::Mutex<()>,
    recovering: Mutex<bool>,
    downloads: Mutex<BTreeMap<String, DownloadSession>>,
    download_fill: tokio::sync::Mutex<()>,
    _process_lock: Arc<std::fs::File>,
}
#[derive(Clone)]
struct DownloadSession {
    client: reqwest::Client,
    jar: Arc<reqwest::cookie::Jar>,
    identity: String,
    verified_at: Instant,
    manifests: BTreeMap<String, DownloadManifest>,
    purchases: BTreeMap<String, LibraryPurchase>,
}
#[derive(Clone)]
struct DownloadManifest {
    purchase_ids: BTreeSet<String>,
    source_bundle_id: Option<String>,
    observed_at: Instant,
    files: Vec<gumroad::FileObservation>,
    revisions: BTreeMap<String, String>,
}
impl DownloadSession {
    fn valid(&self, account: &Value, now: Instant) -> bool {
        crate::stores::active_bound_account(account)
            && account["identityHash"] == self.identity
            && now.saturating_duration_since(self.verified_at) < Duration::from_secs(120)
    }
    fn snapshot(&self) -> Self {
        Self {
            client: self.client.clone(),
            jar: self.jar.clone(),
            identity: self.identity.clone(),
            verified_at: self.verified_at,
            manifests: BTreeMap::new(),
            purchases: BTreeMap::new(),
        }
    }
    fn index_purchases(&mut self, rows: &[LibraryPurchase]) {
        for row in rows {
            if self.purchases.len() < 10000 || self.purchases.contains_key(&row.purchase_id) {
                self.purchases.insert(row.purchase_id.clone(), row.clone());
            }
        }
    }
    fn indexed_purchases(
        &self,
        account: &Value,
        ids: &BTreeSet<&str>,
        now: Instant,
    ) -> Vec<LibraryPurchase> {
        if !self.valid(account, now) {
            return Vec::new();
        }
        ids.iter()
            .filter_map(|id| self.purchases.get(*id))
            .filter(|row| !row.excluded_non_asset())
            .cloned()
            .collect()
    }
}
fn source_fingerprint(file: &gumroad::FileObservation) -> String {
    fingerprint(&json!([file.name, file.folders, file.size_bytes]))
}
fn remote_file_id(account: &str, product: &str, file: &gumroad::FileObservation) -> String {
    format!(
        "gumroad:{account}:{}",
        fingerprint(&json!([product, file.source_product_id, file.file_id]))
    )
}
impl DownloadManifest {
    fn offered<'a>(
        &'a self,
        account: &str,
        product: &str,
        file: &Value,
        now: Instant,
    ) -> Option<&'a gumroad::FileObservation> {
        if now.saturating_duration_since(self.observed_at) >= Duration::from_secs(60)
            || file["productId"] != format!("gumroad:{product}")
            || file["accountId"] != account
            || file["sourceBundleId"].as_str() != self.source_bundle_id.as_deref()
            || !file["purchaseIds"].as_array().is_some_and(|ids| {
                ids.iter()
                    .filter_map(Value::as_str)
                    .any(|id| self.purchase_ids.contains(id))
            })
        {
            return None;
        }
        self.files.iter().find(|offered| {
            offered.access == FileAccess::Downloadable
                && file["sourceFileId"] == offered.file_id
                && file["sourceProductId"] == offered.source_product_id
                && file["sourcePurchaseId"] == offered.source_purchase_id
                && file["id"] == remote_file_id(account, product, offered)
                && file["sourceFingerprint"] == source_fingerprint(offered)
                && self
                    .revisions
                    .get(file["id"].as_str().unwrap_or(""))
                    .is_some_and(|revision| file["revision"] == *revision)
        })
    }
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("INVALID_ARGUMENT"))
}
fn fingerprint(value: &Value) -> String {
    hex::encode(Sha256::digest(value.to_string().as_bytes()))
}
fn safe_code(error: &anyhow::Error) -> String {
    let value = error.to_string();
    if value.len() <= 100
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        value
    } else {
        "GUMROAD_OPERATION_FAILED".into()
    }
}
fn library_url(page: u64, archived: bool) -> Url {
    let mut url = Url::parse("https://gumroad.com/library").unwrap();
    url.query_pairs_mut()
        .append_pair("sort", "purchase_date")
        .append_pair(
            "show_archived_only",
            if archived { "true" } else { "false" },
        )
        .append_pair("page", &page.to_string());
    url
}
/// Mid-run change detection compares membership only: the tab's purchase count and owned
/// bundles. Order, other counts and seller-editable fields are not membership.
fn signature(page: &LibraryPage) -> String {
    fingerprint(
        &json!({"count":page.count,"bundleIds":page.bundles.iter().map(|row| &row.product_id).collect::<BTreeSet<_>>()}),
    )
}
/// A fresh first page keeps membership when its signature is unchanged and it lists no
/// purchase that the pass has not read.
fn membership_kept(head: &LibraryPage, expected: &str, read: impl Fn(&str) -> bool) -> bool {
    signature(head) == expected && head.purchases.iter().all(|row| read(&row.purchase_id))
}
/// These failures are not one asset's own answer and stop the job. Any other failure of a
/// content, member or file page leaves only that asset as it was.
fn job_level(code: &str) -> bool {
    matches!(
        code,
        "NETWORK_UNAVAILABLE"
            | "RATE_LIMITED"
            | "HTTP_CLIENT_FAILED"
            | "CANCELLED"
            | "ACCOUNT_IDENTITY_MISMATCH"
    )
}
/// Only the account's library answer is sign-in evidence. A page that answers for one
/// asset turns its sign-in answer into that asset's own failure.
fn asset_failure(error: anyhow::Error, code: &str) -> anyhow::Error {
    if safe_code(&error) == "AUTH_REQUIRED" {
        anyhow::anyhow!("{code}")
    } else {
        error
    }
}
/// A `/d/` page answers only for its own purchase.
fn content_page(response: Result<(reqwest::StatusCode, String)>) -> Result<Value> {
    response
        .and_then(|(status, html)| {
            ensure!(status.is_success(), "GUMROAD_CONTENT_UNAVAILABLE");
            let page = gumroad::inertia_page(&html)?;
            ensure!(
                page["component"] == "UrlRedirects/DownloadPage",
                "GUMROAD_CONTENT_UNAVAILABLE"
            );
            Ok(page)
        })
        .map_err(|error| asset_failure(error, "PERMISSION_DENIED"))
}
/// The asset a purchase belongs to: its known product, otherwise the purchase itself.
fn purchase_asset(known_purchases: &BTreeMap<String, String>, purchase: &str) -> String {
    known_purchases
        .get(purchase)
        .cloned()
        .unwrap_or_else(|| format!("purchase:{purchase}"))
}
/// Assets that their own answers left as they were stay uncertified; the job still
/// completes and counts them.
fn record_incomplete(job: &mut Value, incomplete: &BTreeSet<String>) {
    if incomplete.is_empty() {
        if let Some(progress) = job["progress"].as_object_mut() {
            progress.remove("incompleteProducts");
        }
    } else {
        job["progress"]["incompleteProducts"] = json!(incomplete.len());
    }
}

async fn enumerate_bundle_members<'a>(
    buyer: &str,
    page_limit: Option<u64>,
    fetch: impl Fn(u64, bool) -> BoxFuture<'a, Result<LibraryPage>>,
) -> Result<BundleMembers> {
    let mut members = BundleMembers {
        purchases: Vec::new(),
        pages: 0,
        head_signatures: Default::default(),
        complete: true,
    };
    let mut seen = BTreeSet::new();
    let mut restarted = false;
    let mut tab = 0;
    'tabs: while tab < 2 {
        let archived = tab == 1;
        let first = fetch(1, archived).await?;
        let count = first.count;
        let pages = first.pages;
        members.head_signatures[tab] = signature(&first);
        let mut first = Some(first);
        let observed_pages = page_limit.map_or(pages, |limit| pages.min(limit));
        members.complete &= observed_pages == pages;
        for index in 1..=observed_pages {
            let page = if index == 1 {
                Some(first.take().unwrap())
            } else {
                match fetch(index, archived).await {
                    Ok(page) => Some(page),
                    Err(error) if page_gone(&error) => None,
                    Err(error) => return Err(error),
                }
            };
            if let Some(page) = &page {
                ensure!(page.buyer_identity == buyer, "ACCOUNT_IDENTITY_MISMATCH");
                members.pages += 1;
            }
            let Some(page) = page.filter(|page| page.count == count) else {
                // Membership changed while it was listed: list it once more.
                ensure!(!restarted, "GUMROAD_LIBRARY_CHANGED");
                restarted = true;
                tab = 0;
                seen.clear();
                members.purchases.clear();
                members.complete = true;
                continue 'tabs;
            };
            for row in page.purchases {
                // A purchase that moved to a later page is listed once.
                if seen.insert(row.purchase_id.clone()) {
                    members.purchases.push(row);
                }
            }
        }
        tab += 1;
    }
    Ok(members)
}
/// A later listing page that no longer exists: the list shrank while it was read.
fn page_gone(error: &anyhow::Error) -> bool {
    matches!(
        safe_code(error).as_str(),
        "GUMROAD_PAGINATION_UNPROVEN" | "GUMROAD_LIBRARY_UNAVAILABLE"
    )
}
fn assemble_observed_bundle(
    bundle: &gumroad::LibraryBundle,
    members: &BundleMembers,
    reuse: &BTreeMap<String, Option<ContentObservation>>,
) -> (LibraryPurchase, ContentObservation) {
    let (card, mut parent) = bundle_parent(bundle);
    for row in &members.purchases {
        parent.member_purchase_ids.insert(row.purchase_id.clone());
        if row.excluded_non_asset() {
            continue;
        }
        if let Some(Some(child)) = reuse.get(&row.purchase_id) {
            parent.unsupported_items += child.unsupported_items;
            merge_member_files(&mut parent, child.files.clone());
        } else {
            parent.unsupported_items += 1;
        }
    }
    parent.unsupported_items += usize::from(!members.complete);
    parent.inventory_complete = parent.unsupported_items == 0;
    (card, parent)
}

impl GumroadService {
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
            .open(catalogue.device_dir().join("gumroad.lock"))
            .map_err(|_| anyhow::anyhow!("GUMROAD_LOCK_UNAVAILABLE"))?;
        fs2::FileExt::try_lock_exclusive(&process_lock)
            .map_err(|_| anyhow::anyhow!("GUMROAD_ALREADY_RUNNING"))?;
        Self::with_process_lock(catalogue, Arc::new(process_lock))
    }
    pub(crate) fn rebind_catalogue(&self, catalogue: Catalogue) -> Result<Self> {
        ensure!(
            catalogue.device_dir() == self.inner.catalogue.device_dir(),
            "GUMROAD_DEVICE_MISMATCH"
        );
        Self::with_process_lock(catalogue, self.inner._process_lock.clone())
    }
    fn with_process_lock(catalogue: Catalogue, process_lock: Arc<std::fs::File>) -> Result<Self> {
        Ok(Self {
            inner: Arc::new(Inner {
                catalogue,
                http: http::MetadataHttp::new(StoreProvider::Gumroad),
                running: Mutex::new(BTreeSet::new()),
                execution: tokio::sync::Mutex::new(()),
                recovering: Mutex::new(false),
                downloads: Mutex::new(BTreeMap::new()),
                download_fill: tokio::sync::Mutex::new(()),
                _process_lock: process_lock,
            }),
        })
    }
    fn account(&self, id: &str) -> Result<Value> {
        uuid::Uuid::parse_str(id).map_err(|_| anyhow::anyhow!("INVALID_ACCOUNT_ID"))?;
        self.inner
            .catalogue
            .get("account", id)?
            .filter(|row| row["store"] == "Gumroad")
            .ok_or_else(|| anyhow::anyhow!("ACCOUNT_UNAVAILABLE"))
    }
    pub async fn start(&self, app: &AppHandle) -> Result<()> {
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
            if job["store"] != "Gumroad"
                || !matches!(job["state"].as_str(), Some("running" | "queued"))
            {
                continue;
            }
            let Some(job) = self.prepare_recovered_job(job)? else {
                continue;
            };
            if let Err(error) = self.spawn(app.clone(), job) {
                if safe_code(&error) != "ACCOUNT_BUSY" {
                    return Err(error);
                }
            }
        }
        Ok(())
    }
    fn prepare_recovered_job(&self, mut job: Value) -> Result<Option<Value>> {
        let id = field(&job, "id")?.to_owned();
        let result = (|| -> Result<Option<Value>> {
            let account_id = field(&job, "accountId")?;
            let account = self.account(account_id)?;
            if self.inner.catalogue.library_root().is_some()
                && (job["originDeviceId"] != self.inner.catalogue.device_id()
                    || (job["type"] != "gumroadLogin"
                        && !crate::stores::has_local_session(&self.inner.catalogue, &account)))
            {
                job["state"] = json!("needsAction");
                job["errorCode"] = json!("RETRY_REQUIRED");
                job["updatedAt"] = json!(now());
                self.inner.catalogue.put("job", &id, job)?;
                return Ok(None);
            }
            if job["type"] == "gumroadLogin" {
                job["state"] = json!("needsAction");
                job["errorCode"] = json!("RETRY_REQUIRED");
                self.inner.catalogue.put("job", &id, job.clone())?;
                session::discard_unverified_account(
                    &self.inner.catalogue,
                    StoreProvider::Gumroad,
                    field(&job, "accountId")?,
                )?;
                return Ok(None);
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
                    failed["errorCode"] = json!(safe_code(&error));
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
        let _work = self.inner.catalogue.begin_catalogue_work()?;
        match action {
            "account_login" => {
                let account = field(&args, "accountId")?;
                self.account(account)?;
                let active = self.inner.catalogue.list("job")?.into_iter().find(|job| {
                    job["accountId"] == account
                        && matches!(job["state"].as_str(), Some("queued" | "running"))
                });
                ensure!(
                    active
                        .as_ref()
                        .is_none_or(|job| job["type"] == "gumroadLogin"),
                    "ACCOUNT_BUSY"
                );
                self.clear_downloads(account, None);
                let opening = session::window(
                    app,
                    &self.inner.catalogue.device_dir(),
                    StoreProvider::Gumroad,
                    account,
                    Url::parse("https://gumroad.com/login").unwrap(),
                    true,
                )
                .await;
                if let Err(error) = opening {
                    session::discard_unverified_account(
                        &self.inner.catalogue,
                        StoreProvider::Gumroad,
                        account,
                    )?;
                    return Err(error);
                }
                if let Some(job) = active {
                    return api::StoreJob::of(&job);
                }
                let job = self.make_job("gumroadLogin", json!({"accountId":account}))?;
                self.spawn(app.clone(), job.clone())?;
                api::StoreJob::of(&job)
            }
            "account_remove" | "account_logout" => {
                self.disconnect_account(app, action, &args).await
            }
            "sync" => {
                let account = field(&args, "accountId")?;
                self.account(account)?;
                if let Some(existing) = self.inner.catalogue.list("job")?.into_iter().find(|job| {
                    job["accountId"] == account
                        && matches!(job["type"].as_str(), Some("gumroadSync" | "gumroadUpdate"))
                        && matches!(job["state"].as_str(), Some("queued" | "running"))
                }) {
                    return api::StoreJob::of(&existing);
                }
                ensure!(
                    matches!(args["source"].as_str(), None | Some("all" | "library")),
                    "INVALID_SOURCE"
                );
                ensure!(
                    matches!(
                        args["mode"].as_str(),
                        None | Some("incremental" | "full" | "forced")
                    ),
                    "INVALID_SYNC_MODE"
                );
                let job = self.make_job(
                    "gumroadSync",
                    json!({"accountId":account,"mode":args["mode"]}),
                )?;
                self.spawn(app.clone(), job.clone())?;
                api::StoreJob::of(&job)
            }
            "update_products" => self.update_selected(app, &args),
            "cancel_sync" | "retry_sync" => self.cancel_or_retry(app, action, &args).await,
            _ => bail!("UNKNOWN_STORE_ACTION"),
        }
    }

    /// Logs an account out, or removes it with or without its products.
    async fn disconnect_account(
        &self,
        app: &AppHandle,
        action: &str,
        args: &Value,
    ) -> Result<Value> {
        let account = field(args, "accountId")?;
        let mut row = self.account(account)?;
        let remove_products = if action == "account_remove" {
            Some(
                args["removeProducts"]
                    .as_bool()
                    .ok_or_else(|| anyhow::anyhow!("INVALID_ACCOUNT_ACTION"))?,
            )
        } else {
            None
        };
        self.clear_downloads(account, None);
        ensure!(
            !self
                .inner
                .running
                .lock()
                .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?
                .contains(account),
            "ACCOUNT_BUSY"
        );
        let window = session::window(
            app,
            &self.inner.catalogue.device_dir(),
            StoreProvider::Gumroad,
            account,
            Url::parse("https://gumroad.com/login").unwrap(),
            false,
        )
        .await?;
        session::forget(
            &self.inner.catalogue.device_dir(),
            StoreProvider::Gumroad,
            account,
        )?;
        session::disconnect(window).await?;
        if let Some(remove_products) = remove_products {
            let result = self
                .inner
                .catalogue
                .remove_account(account, remove_products)?;
            let _ = app.emit("nna:changed", crate::ipc::Change::scope("catalogue"));
            return Ok(result);
        }
        if self.inner.catalogue.library_root().is_none() {
            row["disconnected"] = json!(true);
            row["state"] = json!("disconnected");
            row["status"] = json!("disconnected");
            row["identityVerified"] = json!(false);
            self.inner.catalogue.put("account", account, row)?;
        }
        let _ = app.emit("nna:changed", crate::ipc::Change::scope("booth"));
        api::answer(api::StoreAccount {
            account_id: account.to_owned(),
        })
    }

    /// One update job per account that holds the selected products.
    fn update_selected(&self, app: &AppHandle, args: &Value) -> Result<Value> {
        let ids: BTreeSet<String> = args["productIds"]
            .as_array()
            .or_else(|| args["ids"].as_array())
            .ok_or_else(|| anyhow::anyhow!("INVALID_PRODUCT_IDS"))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .filter(|value| value.starts_with("gumroad:"))
                    .map(str::to_owned)
                    .ok_or_else(|| anyhow::anyhow!("INVALID_PRODUCT_IDS"))
            })
            .collect::<Result<_>>()?;
        ensure!(!ids.is_empty(), "INVALID_PRODUCT_IDS");
        let mut accounts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut matched = BTreeSet::new();
        for entitlement in self.inner.catalogue.list("entitlement")? {
            if entitlement["store"] == "Gumroad"
                && entitlement["productId"]
                    .as_str()
                    .is_some_and(|id| ids.contains(id))
            {
                let account = field(&entitlement, "accountId")?;
                if args["accountId"]
                    .as_str()
                    .is_some_and(|scope| scope != account)
                {
                    continue;
                }
                let record = self.account(account)?;
                if !crate::stores::active_bound_account(&record)
                    || !crate::stores::has_local_session(&self.inner.catalogue, &record)
                {
                    continue;
                }
                matched.insert(field(&entitlement, "productId")?.to_owned());
                accounts
                    .entry(account.into())
                    .or_default()
                    .insert(field(&entitlement, "productId")?.into());
            }
        }
        ensure!(matched.len() == ids.len(), "AUTH_REQUIRED");
        for account in accounts.keys() {
            self.account(account)?;
        }
        let running = self
            .inner
            .running
            .lock()
            .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
        ensure!(
            accounts.keys().all(|account| !running.contains(account)),
            "ACCOUNT_BUSY"
        );
        drop(running);
        let mut jobs = Vec::new();
        for (account, ids) in accounts {
            let job = self.make_job(
                "gumroadUpdate",
                json!({"accountId":account,"productIds":ids}),
            )?;
            self.spawn(app.clone(), job.clone())?;
            jobs.push(job["id"].clone());
        }
        api::StoreJobs::of(jobs)
    }

    /// Cancels a queued or running job, or runs a stopped one again.
    async fn cancel_or_retry(&self, app: &AppHandle, action: &str, args: &Value) -> Result<Value> {
        let id = field(args, "jobId")?;
        let job = self
            .inner
            .catalogue
            .get("job", id)?
            .filter(|job| job["store"] == "Gumroad")
            .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))?;
        if action == "cancel_sync" {
            for attempt in 0..4 {
                let mut batch = RecordBatch::new(&self.inner.catalogue);
                let mut latest = batch
                    .get("job", id)?
                    .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))?;
                if !matches!(latest["state"].as_str(), Some("queued" | "running")) {
                    break;
                }
                latest["state"] = json!("cancelled");
                latest["errorCode"] = json!("CANCELLED");
                latest["completedAt"] = json!(now());
                latest["progress"]["stage"] = json!("cancelled");
                latest["updatedAt"] = json!(now());
                batch.stage("job", id, latest)?;
                if batch.try_commit()?.is_some() {
                    break;
                }
                ensure!(attempt < 3, "CONCURRENT_UPDATE");
            }
        } else {
            ensure!(
                matches!(
                    job["state"].as_str(),
                    Some("failed" | "needsAction" | "cancelled")
                ),
                "JOB_BUSY"
            );
            ensure!(
                !self
                    .inner
                    .running
                    .lock()
                    .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?
                    .contains(field(&job, "accountId")?),
                "ACCOUNT_BUSY"
            );
            if job["type"] == "gumroadLogin" {
                self.clear_downloads(field(&job, "accountId")?, None);
                session::window(
                    app,
                    &self.inner.catalogue.device_dir(),
                    StoreProvider::Gumroad,
                    field(&job, "accountId")?,
                    Url::parse("https://gumroad.com/login").unwrap(),
                    true,
                )
                .await?;
            }
            let job = self.requeue_job(&job)?;
            self.spawn(app.clone(), job)?;
        }
        let _ = app.emit("nna:changed", crate::ipc::Change::job("jobs", id));
        api::answer(api::StoreJob {
            job_id: id.to_owned(),
        })
    }
    fn make_job(&self, kind: &str, args: Value) -> Result<Value> {
        let id = uuid::Uuid::new_v4().to_string();
        let mut job = json!({"id":id,"type":kind,"store":"Gumroad","accountId":args["accountId"],"args":args,"state":"queued","progress":{"stage":"collecting","pages":0,"products":0},"createdAt":now(),"updatedAt":now(),"originDeviceId":self.inner.catalogue.device_id()});
        job["accountName"] = self.account(field(&args, "accountId")?)?["name"].clone();
        self.inner.catalogue.put("job", &id, job.clone())?;
        Ok(job)
    }
    fn spawn(&self, app: AppHandle, mut job: Value) -> Result<()> {
        let work = self.inner.catalogue.begin_catalogue_work()?;
        let account = field(&job, "accountId")?.to_owned();
        let id = field(&job, "id")?.to_owned();
        if !self
            .inner
            .running
            .lock()
            .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?
            .insert(account.clone())
        {
            // A sync waits for its login or a cancelled task to finish; it stays queued.
            if job["type"] == "gumroadSync" {
                let service = self.clone();
                tauri::async_runtime::spawn(async move {
                    let _work = work;
                    loop {
                        if let Err(error) = service.cancelled(&job) {
                            if let Err(error) = service.finish_job(&job, Some(&safe_code(&error))) {
                                eprintln!("Gumroad terminal write failed: {}", safe_code(&error));
                            }
                            let _ =
                                app.emit("nna:changed", crate::ipc::Change::job("catalogue", id));
                            return;
                        }
                        if service
                            .inner
                            .running
                            .lock()
                            .map(|active| !active.contains(&account))
                            .unwrap_or(false)
                        {
                            if let Err(error) = service.spawn(app.clone(), job.clone()) {
                                if let Err(error) =
                                    service.finish_job(&job, Some(&safe_code(&error)))
                                {
                                    eprintln!(
                                        "Gumroad terminal write failed: {}",
                                        safe_code(&error)
                                    );
                                }
                            }
                            let _ =
                                app.emit("nna:changed", crate::ipc::Change::job("catalogue", id));
                            return;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                });
                return Ok(());
            }
            job["state"] = json!("needsAction");
            job["errorCode"] = json!("ACCOUNT_BUSY");
            self.inner.catalogue.put("job", &id, job)?;
            bail!("ACCOUNT_BUSY");
        }
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            let _work = work;
            let _execution = if job["type"] == "gumroadLogin" {
                None
            } else {
                Some(service.inner.execution.lock().await)
            };
            let result = service.execute(&app, &mut job).await;
            let error = result.as_ref().err().map(safe_code);
            if let Err(error) = service.finish_job(&job, error.as_deref()) {
                eprintln!("Gumroad terminal write failed: {}", safe_code(&error));
                let _ = app.emit(
                    "nna:changed",
                    crate::ipc::Change {
                        error_code: Some("TERMINAL_WRITE_FAILED".into()),
                        ..crate::ipc::Change::job("jobs", &id)
                    },
                );
            } else if error.as_deref() == Some("AUTH_REQUIRED") {
                let _ = app.emit("nna:changed", crate::ipc::Change::scope("booth"));
            }
            if job["type"] == "gumroadLogin" {
                let verified = service
                    .inner
                    .catalogue
                    .get("job", &id)
                    .ok()
                    .flatten()
                    .is_some_and(|terminal| terminal["state"] == "completed");
                if let Err(error) =
                    session::finish_login(&app, StoreProvider::Gumroad, &account, verified).await
                {
                    eprintln!("Gumroad login window close failed: {}", safe_code(&error));
                }
                if let Err(error) = session::discard_unverified_account(
                    &service.inner.catalogue,
                    StoreProvider::Gumroad,
                    &account,
                ) {
                    eprintln!("Gumroad login cleanup failed: {}", safe_code(&error));
                }
                let _ = app.emit(
                    "nna:changed",
                    if verified {
                        crate::ipc::Change {
                            session_changed: Some(true),
                            ..crate::ipc::Change::scope("booth")
                        }
                    } else {
                        crate::ipc::Change::scope("booth")
                    },
                );
            }
            if let Ok(mut running) = service.inner.running.lock() {
                running.remove(&account);
            }
            let scope = if matches!(job["type"].as_str(), Some("gumroadSync" | "gumroadUpdate")) {
                "catalogue"
            } else {
                "jobs"
            };
            let _ = app.emit("nna:changed", crate::ipc::Change::job(scope, id));
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
        ensure!(
            self.inner.catalogue.put_many_if_unchanged(
                vec![("job".into(), id.into(), job.clone())],
                vec![("job".into(), id.into(), Some(previous.clone()))],
            )?,
            "JOB_BUSY"
        );
        Ok(job)
    }
    fn finish_job(&self, job: &Value, error: Option<&str>) -> Result<()> {
        let id = field(job, "id")?;
        // A sign-in answer keeps the protected session: only logout, deletion, a failed first
        // login or a new login replace it. This device waits for a sign-in, which restores it.
        if error == Some("AUTH_REQUIRED") && self.inner.catalogue.library_root().is_some() {
            session::mark_signed_out(
                &self.inner.catalogue.device_dir(),
                StoreProvider::Gumroad,
                field(job, "accountId")?,
            )?;
        }
        for _ in 0..3 {
            let mut batch = RecordBatch::new(&self.inner.catalogue);
            // Clear history can remove a cancelled job's row while its task winds down.
            // Its account and source still end; the row stays cleared.
            let current = batch.get("job", id)?;
            let cleared = current.is_none();
            let error = if current.is_none_or(|current| current["state"] == "cancelled") {
                Some("CANCELLED")
            } else {
                error
            };
            let mut terminal = job.clone();
            terminal["supportLastStage"] = json!(crate::support::operation_stage(
                job["progress"]["stage"].as_str().unwrap_or("")
            ));
            terminal["state"] = json!(match error {
                None => "completed",
                Some("CANCELLED") => "cancelled",
                Some(_) => "needsAction",
            });
            terminal["errorCode"] = json!(error);
            terminal["updatedAt"] = json!(now());
            terminal["completedAt"] = json!(now());
            terminal["progress"]["stage"] = terminal["state"].clone();
            if job["type"] == "gumroadSync" {
                let account_id = field(job, "accountId")?;
                if let Some(mut account) = batch.get("account", account_id)? {
                    account["lastSyncAttemptAt"] = terminal["completedAt"].clone();
                    if error.is_none() {
                        account["lastSync"] = terminal["completedAt"].clone();
                        account["lastSyncAt"] = terminal["completedAt"].clone();
                    }
                    account["syncState"] = json!(if error.is_none() {
                        "complete"
                    } else {
                        "incomplete"
                    });
                    account["lastSyncErrorCode"] = json!(error);
                    crate::stores::record_sync_incomplete(&mut account, job, error.is_none());
                    batch.stage("account", account_id, account)?;
                }
                if error.is_some() {
                    let coverage_id = format!("gumroad:{account_id}:library");
                    if let Some(mut coverage) =
                        batch.get("source_coverage", &coverage_id)?.filter(|value| {
                            matches!(value["status"].as_str(), Some("running" | "collected"))
                        })
                    {
                        coverage["status"] = json!("incomplete");
                        coverage["coverageComplete"] = json!(false);
                        coverage["errorCode"] = json!(error);
                        coverage["completedAt"] = terminal["completedAt"].clone();
                        batch.stage("source_coverage", &coverage_id, coverage)?;
                    }
                }
            }
            if error == Some("AUTH_REQUIRED") && self.inner.catalogue.library_root().is_none() {
                let account_id = field(job, "accountId")?;
                if let Some(mut account) = batch.get("account", account_id)? {
                    account["state"] = json!("expired");
                    account["status"] = json!("expired");
                    batch.stage("account", account_id, account)?;
                }
            }
            if !cleared {
                batch.stage("job", id, terminal)?;
            }
            if batch.try_commit()?.is_some() {
                if matches!(job["type"].as_str(), Some("gumroadSync" | "gumroadUpdate")) {
                    self.inner.catalogue.refresh_avatar_defaults()?;
                }
                return Ok(());
            }
        }
        bail!("CONCURRENT_UPDATE")
    }
    fn publish_journals(
        &self,
        app: &AppHandle,
        job: &mut Value,
        account: &str,
        checkpoint: &mut Value,
        known_purchases: &BTreeMap<String, String>,
        unlisted: &BTreeSet<String>,
    ) -> Result<()> {
        let (groups, blocked, unbound) = grouped_journals(
            &self.inner.catalogue,
            job,
            account,
            checkpoint,
            known_purchases,
        )?;
        let total = groups.len();
        for (product, records) in groups {
            if unbound || blocked.contains(&product) || unlisted.contains(&product) {
                continue;
            }
            self.cancelled(job)?;
            job["progress"]["stage"] = json!("publishing");
            job["progress"]["metadataTotal"] = json!(total);
            job["progress"]["metadataCompleted"] = json!(total);
            self.save_running_job(job)?;
            let _ = app.emit(
                "nna:changed",
                crate::ipc::Change::job("jobs", job["id"].as_str().unwrap_or_default()),
            );
            commit_gumroad_product(&self.inner.catalogue, job, &product, &records, checkpoint)?;
            self.clear_downloads(account, Some(&product));
            let _ = app.emit(
                "nna:changed",
                crate::ipc::Change::products("catalogue", [product]),
            );
        }
        job["progress"]["stage"] = json!("finalizing");
        self.save_running_job(job)?;
        Ok(())
    }
    fn publish_complete_journal(
        &self,
        app: &AppHandle,
        job: &Value,
        checkpoint: &mut Value,
        journal_id: &str,
    ) -> Result<()> {
        for product in self.publish_complete_journal_records(job, checkpoint, journal_id)? {
            let _ = app.emit(
                "nna:changed",
                crate::ipc::Change::products("catalogue", [product]),
            );
        }
        Ok(())
    }
    fn publish_complete_journal_records(
        &self,
        job: &Value,
        checkpoint: &mut Value,
        journal_id: &str,
    ) -> Result<Vec<String>> {
        let mut published = Vec::new();
        let journal = self
            .inner
            .catalogue
            .get("checkpoint", journal_id)?
            .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
        let account = field(&journal, "accountId")?;
        let mut single = checkpoint.clone();
        single["journalIds"] = json!([journal_id]);
        let (groups, blocked, unbound) = grouped_journals(
            &self.inner.catalogue,
            job,
            account,
            &single,
            &BTreeMap::new(),
        )?;
        if unbound {
            return Ok(published);
        }
        for (product, records) in groups {
            if blocked.contains(&product) {
                continue;
            }
            self.cancelled(job)?;
            commit_gumroad_product_mode(
                &self.inner.catalogue,
                job,
                &product,
                &records,
                checkpoint,
                false,
            )?;
            self.clear_downloads(account, Some(&product));
            published.push(product);
        }
        Ok(published)
    }
    fn consume_complete_observation(
        &self,
        job: &Value,
        account: &str,
        row: &LibraryPurchase,
        observation: &ContentObservation,
        public: Option<&gumroad::PublicProductObservation>,
        error: Option<&String>,
        unavailable: bool,
        checkpoint: &mut Value,
    ) -> Result<Vec<String>> {
        if !observation.inventory_complete {
            return Ok(vec![]);
        }
        let id = observation.catalogue_product_id();
        let mut prepared = RecordBatch::new(&self.inner.catalogue);
        project(&mut prepared, account, row, observation)?;
        project_public(&mut prepared, observation, public)?;
        if error.is_some() || unavailable {
            let mut product = prepared.get("product", &id)?.unwrap();
            if let Some(error) = error {
                product["priceRefreshErrorCode"] = json!(error);
            }
            if unavailable {
                product["publicPageAvailability"] = json!("unavailable");
            }
            prepared.stage("product", &id, product)?;
        }
        let grant_journal = format!(
            "gumroad:{account}:{}:grant:{}",
            field(job, "id")?,
            fingerprint(&json!(row.purchase_id))
        );
        append_journal(checkpoint, &grant_journal)?;
        self.inner.catalogue.put("checkpoint",&grant_journal,json!({"id":grant_journal,"store":"Gumroad","accountId":account,"runId":job["id"],"source":"library","observations":[{"productId":id,"purchaseId":row.purchase_id,"complete":true}],"records":prepared.staged_records()}))?;
        self.publish_complete_journal_records(job, checkpoint, &grant_journal)
    }
    fn cancelled(&self, job: &Value) -> Result<()> {
        ensure!(
            self.inner
                .catalogue
                .get("job", field(job, "id")?)?
                .is_some_and(|row| row["state"] != "cancelled"),
            "CANCELLED"
        );
        Ok(())
    }
    fn save_running_job(&self, job: &Value) -> Result<()> {
        let id = field(job, "id")?;
        for _ in 0..3 {
            let current = self
                .inner
                .catalogue
                .get("job", id)?
                .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))?;
            ensure!(current["state"] != "cancelled", "CANCELLED");
            if self.inner.catalogue.put_many_if_unchanged(
                vec![("job".into(), id.into(), job.clone())],
                vec![("job".into(), id.into(), Some(current))],
            )? {
                return Ok(());
            }
        }
        bail!("CONCURRENT_UPDATE")
    }
    async fn client(&self, app: &AppHandle, account: &str) -> Result<reqwest::Client> {
        Ok(self.client_with_jar(app, account).await?.0)
    }
    async fn client_with_jar(
        &self,
        app: &AppHandle,
        account: &str,
    ) -> Result<(reqwest::Client, Arc<reqwest::cookie::Jar>)> {
        let window = session::window(
            app,
            &self.inner.catalogue.device_dir(),
            StoreProvider::Gumroad,
            account,
            Url::parse("https://gumroad.com/library").unwrap(),
            false,
        )
        .await?;
        // session::jar already waits for a reopened hidden WebView to load its profile.
        let jar = session::jar(window, StoreProvider::Gumroad).await?;
        Ok((
            http::build_client(StoreProvider::Gumroad, jar.clone())?,
            jar,
        ))
    }
    async fn public_product(
        &self,
        client: &reqwest::Client,
        url: &Url,
        product_id: &str,
    ) -> Result<Option<gumroad::PublicProductObservation>> {
        let mut url = url.clone();
        url.set_path(&format!("{}.json", url.path().trim_end_matches('/')));
        let (status, body) = self
            .inner
            .http
            .get_classified(client, url, http::RequestKind::Public)
            .await
            // A cookie-less public page cannot speak for the account's sign-in.
            .map_err(|error| asset_failure(error, "GUMROAD_PUBLIC_METADATA_UNAVAILABLE"))?;
        if matches!(status.as_u16(), 404 | 410) {
            return Ok(None);
        }
        ensure!(status.is_success(), "GUMROAD_PUBLIC_METADATA_UNAVAILABLE");
        let value: Value = serde_json::from_str(&body)
            .map_err(|_| anyhow::anyhow!("GUMROAD_PUBLIC_SCHEMA_UNSUPPORTED"))?;
        gumroad::parse_public_product(&value, Some(product_id)).map(Some)
    }
    async fn library(
        &self,
        client: &reqwest::Client,
        page: u64,
        archived: bool,
    ) -> Result<LibraryPage> {
        self.library_scope(client, page, archived, None).await
    }
    async fn library_scope(
        &self,
        client: &reqwest::Client,
        page: u64,
        archived: bool,
        bundle: Option<&str>,
    ) -> Result<LibraryPage> {
        let mut url = library_url(page, archived);
        if let Some(bundle) = bundle {
            url.query_pairs_mut().append_pair("bundles", bundle);
        }
        let (status, html) = self.inner.http.get(client, url.clone()).await?;
        ensure!(status.is_success(), "GUMROAD_LIBRARY_UNAVAILABLE");
        let page_data = gumroad::inertia_page(&html)?;
        ensure!(
            page_data["component"] == "Library/Index",
            "SOURCE_UNSUPPORTED"
        );
        gumroad::parse_library_scope(&page_data["props"], &url, page, archived, bundle)
    }
    async fn content(
        &self,
        client: &reqwest::Client,
        row: &LibraryPurchase,
        expected_product: Option<&str>,
    ) -> Result<ContentObservation> {
        let url = row
            .content_route
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("GUMROAD_CONTENT_UNAVAILABLE"))?
            .url()
            .clone();
        let page = content_page(
            self.inner
                .http
                .get_classified(client, url.clone(), http::RequestKind::Content)
                .await,
        )?;
        gumroad::parse_content_props(&page["props"], &url, &row.purchase_id, expected_product)
    }
    /// A bundle remains one product unit. Member identity is established by its parent listing
    /// and fresh bound content page; neither a generated ZIP nor a bearer token is identity.
    async fn expanded_content(
        &self,
        client: &reqwest::Client,
        row: &LibraryPurchase,
        expected_product: Option<&str>,
    ) -> Result<ContentObservation> {
        let mut parent = self.content(client, row, expected_product).await?;
        let ancestors = BTreeSet::from([parent.product_id.clone()]);
        let mut queue: VecDeque<_> = std::mem::take(&mut parent.bundles)
            .into_iter()
            .map(|member| (member, ancestors.clone()))
            .collect();
        let mut visited = 0usize;
        while let Some((member, mut ancestors)) = queue.pop_front() {
            visited += 1;
            ensure!(visited <= 10000, "GUMROAD_INVENTORY_TOO_LARGE");
            if ancestors.len() >= 32 || !ancestors.insert(member.product_id.clone()) {
                continue;
            }
            let Some(route) = member.content_route else {
                continue;
            };
            let url = route.url().clone();
            let result = async {
                let page = content_page(
                    self.inner
                        .http
                        .get_classified(client, url.clone(), http::RequestKind::Content)
                        .await,
                )?;
                let purchase = field(&page["props"]["purchase"], "id")?;
                gumroad::parse_content_props(
                    &page["props"],
                    &url,
                    purchase,
                    Some(&member.product_id),
                )
            }
            .await;
            match result {
                Ok(mut child) => {
                    // Each listed member contributes one unresolved placeholder until bound.
                    parent.unsupported_items =
                        parent.unsupported_items.saturating_sub(1) + child.unsupported_items;
                    for nested in std::mem::take(&mut child.bundles) {
                        queue.push_back((nested, ancestors.clone()));
                    }
                    merge_member_files(&mut parent, child.files);
                }
                Err(error) if job_level(&safe_code(&error)) => return Err(error),
                Err(_) => {} // Preserve parent financial facts and previously verified files.
            }
        }
        parent.inventory_complete = parent.unsupported_items == 0;
        Ok(parent)
    }
    async fn authenticate(
        &self,
        app: &AppHandle,
        account: &str,
        client: &reqwest::Client,
        login_job: Option<&Value>,
    ) -> Result<LibraryPage> {
        self.clear_downloads(account, None);
        let page = self.library(client, 1, false).await?;
        let identity = fingerprint(&json!(["gumroad", page.buyer_identity]));
        let mut batch = RecordBatch::new(&self.inner.catalogue);
        let mut row = batch
            .get("account", account)?
            .ok_or_else(|| anyhow::anyhow!("ACCOUNT_UNAVAILABLE"))?;
        if let Some(job) = login_job {
            ensure!(
                session::existing_window(app, StoreProvider::Gumroad, account)?.is_some(),
                "CANCELLED"
            );
            ensure!(
                batch
                    .get("job", field(job, "id")?)?
                    .is_some_and(|job| matches!(job["state"].as_str(), Some("queued" | "running"))),
                "CANCELLED"
            );
        }
        if let Some(saved) = row["identityHash"].as_str() {
            ensure!(saved == identity, "ACCOUNT_IDENTITY_MISMATCH");
        }
        let unverified = row["identityVerified"] != true
            && row["authenticatedAt"].as_str().is_none_or(str::is_empty)
            && row["identityHash"].as_str().is_none_or(str::is_empty);
        if login_job.is_some() {
            let window = session::existing_window(app, StoreProvider::Gumroad, account)?
                .ok_or_else(|| anyhow::anyhow!("CANCELLED"))?;
            session::remember(
                window,
                &self.inner.catalogue.device_dir(),
                StoreProvider::Gumroad,
                account,
            )
            .await?;
            if session::existing_window(app, StoreProvider::Gumroad, account)?.is_none() {
                if unverified {
                    session::forget(
                        &self.inner.catalogue.device_dir(),
                        StoreProvider::Gumroad,
                        account,
                    )?;
                }
                bail!("CANCELLED");
            }
        }
        row["identityHash"] = json!(identity);
        row["identityVerified"] = json!(true);
        row["disconnected"] = json!(false);
        row["state"] = json!("connected");
        row["status"] = json!("connected");
        row["updatedAt"] = json!(now());
        row["authenticatedAt"] = json!(now());
        batch.stage("account", account, row)?;
        match batch.try_commit() {
            Ok(Some(_)) => {}
            result => {
                if login_job.is_some() && unverified {
                    session::forget(
                        &self.inner.catalogue.device_dir(),
                        StoreProvider::Gumroad,
                        account,
                    )?;
                }
                result?;
                bail!("CONCURRENT_UPDATE");
            }
        }
        let _ = app.emit("nna:changed", crate::ipc::Change::scope("booth"));
        Ok(page)
    }
    async fn bundle_members(
        &self,
        client: &reqwest::Client,
        bundle: &gumroad::LibraryBundle,
        buyer: &str,
        job: Option<&Value>,
    ) -> Result<BundleMembers> {
        let page_limit = job
            .and_then(|_| crate::verification::source_page_limit())
            .map(|v| v as u64);
        enumerate_bundle_members(buyer, page_limit, |index, archived| {
            async move {
                if let Some(job) = job {
                    self.cancelled(job)?;
                }
                self.library_scope(client, index, archived, Some(&bundle.product_id))
                    .await
            }
            .boxed()
        })
        .await
    }
    async fn assemble_bundle(
        &self,
        client: &reqwest::Client,
        bundle: &gumroad::LibraryBundle,
        buyer: &str,
        mut members: BundleMembers,
        reuse: &mut BTreeMap<String, Option<ContentObservation>>,
        job: Option<&Value>,
    ) -> Result<(LibraryPurchase, ContentObservation, u64)> {
        let mut pages = members.pages;
        let mut restarted = false;
        while !self
            .read_bundle(client, bundle, buyer, &members, reuse, job)
            .await?
        {
            // Membership changed while members were read: list the bundle once more.
            ensure!(!restarted, "GUMROAD_LIBRARY_CHANGED");
            restarted = true;
            members = self.bundle_members(client, bundle, buyer, job).await?;
            pages += members.pages;
        }
        let (card, parent) = assemble_observed_bundle(bundle, &members, reuse);
        Ok((card, parent, pages))
    }
    /// Reads unread member content, then reports whether a single fresh scoped head check
    /// per tab still shows the listed membership.
    async fn read_bundle(
        &self,
        client: &reqwest::Client,
        bundle: &gumroad::LibraryBundle,
        buyer: &str,
        members: &BundleMembers,
        reuse: &mut BTreeMap<String, Option<ContentObservation>>,
        job: Option<&Value>,
    ) -> Result<bool> {
        let missing: Vec<_> = members
            .purchases
            .iter()
            .filter(|row| !row.excluded_non_asset() && !reuse.contains_key(&row.purchase_id))
            .cloned()
            .collect();
        let mut reads = futures_util::stream::iter(missing.into_iter().map(|row| async move {
            if let Some(job) = job {
                self.cancelled(job)?;
            }
            Ok::<_, anyhow::Error>((
                row.purchase_id.clone(),
                self.expanded_content(client, &row, None).await,
            ))
        }))
        .buffer_unordered(2);
        while let Some(result) = reads.next().await {
            let (purchase, result) = result?;
            match result {
                Ok(child) => {
                    reuse.insert(purchase, Some(child));
                }
                Err(error) if job_level(&safe_code(&error)) => return Err(error),
                Err(_) => {
                    reuse.insert(purchase, None);
                }
            }
        }
        // A single fresh scoped head check per tab, after all member content.
        let listed: BTreeSet<_> = members
            .purchases
            .iter()
            .map(|row| row.purchase_id.as_str())
            .collect();
        for (tab, archived) in [false, true].into_iter().enumerate() {
            if let Some(job) = job {
                self.cancelled(job)?;
            }
            let head = self
                .library_scope(client, 1, archived, Some(&bundle.product_id))
                .await?;
            ensure!(head.buyer_identity == buyer, "ACCOUNT_IDENTITY_MISMATCH");
            if !membership_kept(&head, &members.head_signatures[tab], |id| {
                listed.contains(id)
            }) {
                return Ok(false);
            }
        }
        Ok(true)
    }
    async fn bundle_content(
        &self,
        client: &reqwest::Client,
        bundle: &gumroad::LibraryBundle,
        buyer: &str,
        reuse: &mut BTreeMap<String, Option<ContentObservation>>,
        job: Option<&Value>,
    ) -> Result<(LibraryPurchase, ContentObservation, u64)> {
        let members = self.bundle_members(client, bundle, buyer, job).await?;
        self.assemble_bundle(client, bundle, buyer, members, reuse, job)
            .await
    }
    async fn execute(&self, app: &AppHandle, job: &mut Value) -> Result<()> {
        let account = field(job, "accountId")?.to_owned();
        let existing = self.account(&account)?;
        job["state"] = json!("running");
        self.save_running_job(job)?;
        if job["type"] == "gumroadLogin" {
            let started = Instant::now();
            loop {
                self.cancelled(job)?;
                let window = session::existing_window(app, StoreProvider::Gumroad, &account)?
                    .ok_or_else(|| anyhow::anyhow!("CANCELLED"))?;
                let result = async {
                    let jar = session::jar(window, StoreProvider::Gumroad).await?;
                    let client = http::build_client(StoreProvider::Gumroad, jar)?;
                    self.authenticate(app, &account, &client, Some(job)).await
                }
                .await;
                match result {
                    Ok(_) => return Ok(()),
                    Err(error) if safe_code(&error) == "AUTH_REQUIRED" => {}
                    Err(error) => return Err(error),
                }
                ensure!(started.elapsed() < Duration::from_secs(600), "AUTH_TIMEOUT");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
        ensure!(
            existing["identityVerified"] == true && existing["disconnected"] != true,
            "AUTH_REQUIRED"
        );
        let client = self.client(app, &account).await?;
        // The identity-verified response is also the first library page.
        let first = self.authenticate(app, &account, &client, None).await?;
        self.collect(app, job, &account, &client, first).await
    }
    async fn collect(
        &self,
        app: &AppHandle,
        job: &mut Value,
        account: &str,
        client: &reqwest::Client,
        first: LibraryPage,
    ) -> Result<()> {
        let metrics_before = self.inner.http.metrics();
        let classified_before = self.inner.http.classified_metrics();
        let page_limit = crate::verification::source_page_limit().map(|v| v as u64);
        let verification_capped = false;
        if let Some(limit) = page_limit {
            job["verification"] = json!({"pageLimit":limit,"sourcePages":{"unarchived":0,"archived":0},"bundlePages":0,"capped":false});
        }
        let (selected, selected_purchases, selected_bundles) =
            self.library_selection(job, account)?;
        let heads = [first, self.library(client, 1, true).await?];
        let buyer_identity = heads[0].buyer_identity.clone();
        ensure!(
            heads[1].buyer_identity == buyer_identity,
            "ACCOUNT_IDENTITY_MISMATCH"
        );
        let signatures = [signature(&heads[0]), signature(&heads[1])];
        let mut owned_bundles = BTreeMap::new();
        for bundle in heads.iter().flat_map(|head| &head.bundles) {
            owned_bundles
                .entry(bundle.product_id.clone())
                .or_insert_with(|| bundle.clone());
        }
        // Distinct assets that their own definitive answer left as they were. A selected
        // bundle the library no longer lists is one of them.
        let incomplete: BTreeSet<String> = selected_bundles
            .iter()
            .filter(|id| !owned_bundles.contains_key(*id))
            .map(|id| format!("gumroad:{id}"))
            .collect();
        let checkpoint_id = if selected.is_some() {
            format!("gumroad:{account}:selected:{}", field(job, "id")?)
        } else {
            format!("gumroad:{account}:library")
        };
        let saved = self.inner.catalogue.get("checkpoint", &checkpoint_id)?;
        let checkpoint = saved.filter(|row| selected.is_none() && row["publicationVersion"] == 1 && row["jobId"] == job["id"] && row["headSignatures"] == json!(signatures) && row["unresolvedRows"].as_u64() == Some(0))
            .unwrap_or_else(|| json!({"id":checkpoint_id,"publicationVersion":1,"journalIds":[],"publishedProducts":[],"store":"Gumroad","accountId":account,"jobId":job["id"],"headSignatures":signatures,"nextPages":[1,1],"seenPurchaseIds":[],"productIds":[],"unresolvedRows":0,"pages":0,"complete":false}));
        self.inner
            .catalogue
            .prune_store_checkpoints("Gumroad", account)?;
        let seen: BTreeSet<String> = checkpoint["seenPurchaseIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        let products: BTreeSet<String> = checkpoint["productIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        let unresolved = checkpoint["unresolvedRows"].as_u64().unwrap_or(0);
        let found = BTreeSet::new();
        let known_purchases: BTreeMap<String, String> = self
            .inner
            .catalogue
            .list("acquisition")?
            .into_iter()
            .filter(|row| row["store"] == "Gumroad" && row["accountId"] == account)
            .filter_map(|row| {
                Some((
                    row["purchaseId"].as_str()?.to_owned(),
                    row["productId"].as_str()?.to_owned(),
                ))
            })
            .collect();
        let public_client = http::build_client(
            StoreProvider::Gumroad,
            Arc::new(reqwest::cookie::Jar::default()),
        )?;
        let public_products = BTreeMap::new();
        let public_reads: Mutex<BTreeMap<String, PublicRead<'_>>> = Mutex::new(BTreeMap::new());
        let public_errors = BTreeMap::new();
        let public_unavailable = BTreeSet::new();
        let metadata_total = heads.iter().map(|head| head.count).sum::<u64>()
            + owned_bundles
                .values()
                .filter(|bundle| {
                    selected.is_none() || selected_bundles.contains(&bundle.product_id)
                })
                .count() as u64;
        let bundle_completed: BTreeSet<String> = checkpoint["completedBundles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        let bundled_purchases: BTreeSet<String> = checkpoint["bundlePurchaseIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        let bundle_memberships = BTreeMap::new();
        let read = LibraryRead {
            app,
            account,
            client,
            selected: &selected,
            selected_bundles: &selected_bundles,
            known_purchases: &known_purchases,
            buyer_identity: &buyer_identity,
            checkpoint_id: &checkpoint_id,
            metadata_total,
            page_limit,
            metrics_before,
            classified_before,
        };
        let mut pass = LibraryPass {
            verification_capped,
            selected_purchases,
            signatures,
            owned_bundles,
            incomplete,
            checkpoint,
            seen,
            products,
            unresolved,
            found,
            public_products,
            public_errors,
            public_unavailable,
            bundle_completed,
            bundled_purchases,
            bundle_memberships,
        };
        self.read_bundle_memberships(read, job, &mut pass).await?;
        let (listed, observed) = self
            .read_listing(read, &public_client, &public_reads, job, heads, &mut pass)
            .await?;
        self.assemble_bundles(read, job, &mut pass).await?;
        self.finish_collection(read, job, listed, observed, &mut pass)
    }

    /// Reads which purchases each owned bundle holds, before the listing pass skips them.
    async fn read_bundle_memberships(
        &self,
        read: LibraryRead<'_>,
        job: &mut Value,
        pass: &mut LibraryPass,
    ) -> Result<()> {
        let LibraryRead {
            app,
            client,
            selected,
            selected_bundles,
            buyer_identity,
            metadata_total,
            page_limit,
            metrics_before,
            classified_before,
            ..
        } = read;
        let LibraryPass {
            verification_capped,
            owned_bundles,
            checkpoint,
            seen,
            products,
            unresolved,
            bundle_completed,
            bundled_purchases,
            bundle_memberships,
            ..
        } = pass;
        for bundle in owned_bundles
            .values()
            .filter(|bundle| selected.is_none() || selected_bundles.contains(&bundle.product_id))
        {
            if bundle_completed.contains(&bundle.product_id) {
                continue;
            }
            let members = self
                .bundle_members(client, bundle, buyer_identity, Some(job))
                .await?;
            *verification_capped |= !members.complete;
            if page_limit.is_some() {
                job["verification"]["bundlePages"] =
                    json!(job["verification"]["bundlePages"].as_u64().unwrap_or(0) + members.pages);
            }
            bundled_purchases.extend(members.purchases.iter().map(|row| row.purchase_id.clone()));
            checkpoint["bundlePurchaseIds"] = json!(bundled_purchases);
            checkpoint["pages"] = json!(checkpoint["pages"].as_u64().unwrap_or(0) + members.pages);
            bundle_memberships.insert(bundle.product_id.clone(), members);
            job["progress"] = json!({"stage":"collecting","pages":checkpoint["pages"],"products":products.len(),"published":checkpoint["publishedProducts"].as_array().map_or(0,Vec::len),"metadataCompleted":seen.len()+bundle_completed.len(),"metadataTotal":metadata_total,"unresolvedRows":unresolved});
            self.set_collection_metrics(job, metrics_before, classified_before);
            self.save_running_job(job)?;
            let _ = app.emit(
                "nna:changed",
                crate::ipc::Change::job("jobs", job["id"].as_str().unwrap_or_default()),
            );
        }
        Ok(())
    }

    /// Reads both library tabs page by page, and once more from the start if membership
    /// changed during the pass; returns what the closing pass listed and observed.
    async fn read_listing<'a>(
        &'a self,
        read: LibraryRead<'a>,
        public_client: &'a reqwest::Client,
        public_reads: &Mutex<BTreeMap<String, PublicRead<'a>>>,
        job: &mut Value,
        heads: [LibraryPage; 2],
        pass: &mut LibraryPass,
    ) -> Result<(BTreeSet<String>, BTreeMap<String, String>)> {
        let LibraryRead {
            client,
            selected,
            selected_bundles,
            buyer_identity,
            page_limit,
            ..
        } = read;
        let LibraryPass {
            selected_purchases,
            bundled_purchases,
            checkpoint,
            ..
        } = &mut *pass;
        selected_purchases.retain(|id| !bundled_purchases.contains(id));
        // Mid-run change detection compares membership only. The first change restarts the
        // listing pass from its first pages; purchases already read are not read again.
        let mut restarted = checkpoint["listingRestarted"] == true;
        let mut membership_changed = false;
        // Purchases the current pass listed, and the product each observed purchase bound.
        let mut listed: BTreeSet<String> =
            serde_json::from_value(checkpoint["listedPurchaseIds"].clone()).unwrap_or_default();
        let mut observed: BTreeMap<String, String> =
            serde_json::from_value(checkpoint["observedPurchases"].clone()).unwrap_or_default();
        let mut pending: VecDeque<_> = heads.into_iter().enumerate().collect();
        while let Some((tab, head)) = pending.pop_front() {
            let LibraryPass {
                verification_capped,
                checkpoint,
                ..
            } = &mut *pass;
            let archived = tab == 1;
            let pages = head.pages;
            let observed_pages = page_limit.map_or(pages, |limit| pages.min(limit));
            *verification_capped |= observed_pages != pages;
            let count = head.count;
            let mut first = Some(head);
            for page_index in checkpoint["nextPages"][tab].as_u64().unwrap_or(1)..=observed_pages {
                if self
                    .read_library_page(
                        read,
                        public_client,
                        public_reads,
                        job,
                        pass,
                        &mut listed,
                        &mut observed,
                        restarted,
                        &mut first,
                        tab,
                        archived,
                        count,
                        page_index,
                    )
                    .await?
                {
                    membership_changed = true;
                    pending.clear();
                    break;
                }
            }
            let LibraryPass {
                signatures,
                seen,
                owned_bundles,
                verification_capped,
                bundled_purchases,
                checkpoint,
                bundle_memberships,
                ..
            } = &mut *pass;
            if !pending.is_empty() {
                continue;
            }
            ensure!(
                !(membership_changed && restarted),
                "GUMROAD_LIBRARY_CHANGED"
            );
            let current = [
                self.library(client, 1, false).await?,
                self.library(client, 1, true).await?,
            ];
            ensure!(
                current
                    .iter()
                    .all(|head| head.buyer_identity == buyer_identity),
                "ACCOUNT_IDENTITY_MISMATCH"
            );
            if !membership_changed
                && current
                    .iter()
                    .zip(&*signatures)
                    .all(|(head, expected)| membership_kept(head, expected, |id| seen.contains(id)))
            {
                continue;
            }
            ensure!(!restarted, "GUMROAD_LIBRARY_CHANGED");
            restarted = true;
            membership_changed = false;
            listed.clear();
            // A bundle bought during the pass lists its members before the pass restarts.
            for bundle in current.iter().flat_map(|head| &head.bundles) {
                if owned_bundles.contains_key(&bundle.product_id)
                    || (selected.is_some() && !selected_bundles.contains(&bundle.product_id))
                {
                    continue;
                }
                let members = self
                    .bundle_members(client, bundle, buyer_identity, Some(job))
                    .await?;
                *verification_capped |= !members.complete;
                bundled_purchases
                    .extend(members.purchases.iter().map(|row| row.purchase_id.clone()));
                checkpoint["pages"] =
                    json!(checkpoint["pages"].as_u64().unwrap_or(0) + members.pages);
                bundle_memberships.insert(bundle.product_id.clone(), members);
                owned_bundles.insert(bundle.product_id.clone(), bundle.clone());
            }
            *signatures = [signature(&current[0]), signature(&current[1])];
            checkpoint["headSignatures"] = json!(signatures);
            checkpoint["listingRestarted"] = json!(true);
            checkpoint["listedPurchaseIds"] = json!(listed);
            checkpoint["bundlePurchaseIds"] = json!(bundled_purchases);
            checkpoint["nextPages"] = json!([1, 1]);
            pending.extend(current.into_iter().enumerate());
        }
        Ok((listed, observed))
    }

    /// Reads one library page and publishes its complete purchases; returns true when the
    /// tab's membership changed and the pass must stop reading it.
    async fn read_library_page<'a>(
        &'a self,
        read: LibraryRead<'a>,
        public_client: &'a reqwest::Client,
        public_reads: &Mutex<BTreeMap<String, PublicRead<'a>>>,
        job: &mut Value,
        pass: &mut LibraryPass,
        listed: &mut BTreeSet<String>,
        observed: &mut BTreeMap<String, String>,
        restarted: bool,
        first: &mut Option<LibraryPage>,
        tab: usize,
        archived: bool,
        count: u64,
        page_index: u64,
    ) -> Result<bool> {
        let LibraryRead {
            app,
            client,
            selected,
            known_purchases,
            buyer_identity,
            metadata_total,
            ..
        } = read;
        let LibraryPass {
            checkpoint,
            seen,
            products,
            unresolved,
            found,
            bundle_completed,
            bundled_purchases,
            ..
        } = &mut *pass;
        self.cancelled(job)?;
        let page = if page_index == 1 {
            // The initial response has already been validated.
            first.take().unwrap()
        } else {
            match self.library(client, page_index, archived).await {
                Ok(page) => page,
                Err(error) if page_gone(&error) => {
                    return Ok(true);
                }
                Err(error) => return Err(error),
            }
        };
        job["progress"] = json!({"stage":"collecting","pages":checkpoint["pages"],"products":products.len(),"published":checkpoint["publishedProducts"].as_array().map_or(0,Vec::len),"metadataCompleted":seen.len()+bundle_completed.len(),"metadataTotal":metadata_total,"unresolvedRows":unresolved});
        self.save_running_job(job)?;
        let _ = app.emit(
            "nna:changed",
            crate::ipc::Change::job("jobs", job["id"].as_str().unwrap_or_default()),
        );
        ensure!(
            page.buyer_identity == buyer_identity,
            "ACCOUNT_IDENTITY_MISMATCH"
        );
        if page.count != count {
            // The tab's membership changed: this pass cannot close.
            return Ok(true);
        }
        let mut observations = Vec::new();
        let mut page_unresolved = 0;
        let mut journal = Vec::new();
        let completed_before_page = seen.len() + bundle_completed.len();
        let mut page_processed = 0usize;
        // Poll two complete content/public acquisitions together. The consumer
        // only stages finished results, retaining page order and grant grouping.
        let mut eligible = Vec::new();
        for row in page.purchases {
            listed.insert(row.purchase_id.clone());
            // A purchase already read on an earlier page or pass is read once.
            if !seen.insert(row.purchase_id.clone()) {
                continue;
            }
            if !parentless_purchase(&row, bundled_purchases) {
                page_processed += 1;
                continue;
            }
            if selected.as_ref().is_some_and(|ids| {
                known_purchases
                    .get(&row.purchase_id)
                    .is_some_and(|id| !ids.contains(id))
            }) {
                page_processed += 1;
                continue;
            }
            found.insert(row.purchase_id.clone());
            eligible.push(row);
        }
        let content_job = job.clone();
        let mut contents = futures_util::stream::iter(eligible.into_iter().map(|row| {
            let content_job = &content_job;
            let public_reads = &public_reads;
            let selected = &selected;
            async move {
                self.cancelled(content_job)?;
                let content = self.expanded_content(client, &row, None).await;
                let public = if let Ok(observation) = &content {
                    if selected
                        .as_ref()
                        .is_none_or(|ids| ids.contains(&observation.catalogue_product_id()))
                    {
                        if let Some(url) = &observation.public_product_url {
                            let future =
                                shared_public_read(public_reads, &observation.product_id, || {
                                    let id = observation.product_id.clone();
                                    let url = url.clone();
                                    async move {
                                        self.public_product(public_client, &url, &id)
                                            .await
                                            .map_err(|error| safe_code(&error))
                                    }
                                    .boxed()
                                    .shared()
                                })?;
                            Some(future.await)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                self.cancelled(content_job)?;
                Ok::<_, anyhow::Error>((row, content, public))
            }
        }))
        .buffer_unordered(2);
        while let Some(result) = contents.next().await {
            let (row, content, public) = result?;
            self.consume_content(
                read,
                job,
                pass,
                observed,
                &mut observations,
                &mut page_unresolved,
                &mut journal,
                &mut page_processed,
                completed_before_page,
                row,
                content,
                public,
            )?;
        }
        self.commit_library_page(
            read,
            job,
            pass,
            listed,
            observed,
            restarted,
            tab,
            archived,
            page_index,
            &observations,
            journal,
            page_unresolved,
        )?;
        Ok(false)
    }

    /// Takes one purchase's read content: publishes it when complete, and journals it.
    fn consume_content(
        &self,
        read: LibraryRead<'_>,
        job: &mut Value,
        pass: &mut LibraryPass,
        observed: &mut BTreeMap<String, String>,
        observations: &mut Vec<(LibraryPurchase, ContentObservation)>,
        page_unresolved: &mut u64,
        journal: &mut Vec<Value>,
        page_processed: &mut usize,
        completed_before_page: usize,
        row: LibraryPurchase,
        content: Result<ContentObservation>,
        public: Option<Result<Option<gumroad::PublicProductObservation>, String>>,
    ) -> Result<()> {
        let LibraryRead {
            app,
            account,
            selected,
            known_purchases,
            ..
        } = read;
        let LibraryPass {
            incomplete,
            checkpoint,
            products,
            public_products,
            public_errors,
            public_unavailable,
            ..
        } = pass;
        *page_processed += 1;
        self.cancelled(job)?;
        match content {
            Ok(observation) => {
                let id = observation.catalogue_product_id();
                if selected.as_ref().is_some_and(|ids| !ids.contains(&id)) {
                    return Ok(());
                }
                if let Some(public) = public {
                    let public = match public {
                        Ok(Some(public)) => Some(public),
                        Ok(None) => {
                            public_unavailable.insert(observation.product_id.clone());
                            None
                        }
                        Err(code) => {
                            public_errors.insert(observation.product_id.clone(), code);
                            None
                        }
                    };
                    public_products.insert(observation.product_id.clone(), public);
                }
                ensure!(
                    selected.as_ref().is_none_or(|ids| ids.contains(&id)),
                    "GUMROAD_PRODUCT_BINDING_UNPROVEN"
                );
                let complete = observation.inventory_complete;
                if !complete {
                    *page_unresolved += 1;
                    incomplete.insert(id.clone());
                }
                observed.insert(row.purchase_id.clone(), id.clone());
                products.insert(id.clone());
                journal.push(json!({"purchaseId":row.purchase_id,"productId":id,"complete":complete,"errorCode":public_errors.get(&observation.product_id).filter(|_|needs_public_quote(&observation)).cloned().or_else(|| (!observation.inventory_complete).then(|| "GUMROAD_INVENTORY_UNPROVEN".into()))}));
                // Publish this complete acquisition while other content
                // futures and later listing pages remain outstanding.
                if complete {
                    let published = self.consume_complete_observation(
                        job,
                        account,
                        &row,
                        &observation,
                        public_products
                            .get(&observation.product_id)
                            .and_then(Option::as_ref),
                        public_errors
                            .get(&observation.product_id)
                            .filter(|_| needs_public_quote(&observation)),
                        needs_public_quote(&observation)
                            && public_unavailable.contains(&observation.product_id),
                        checkpoint,
                    )?;
                    for product in published {
                        let _ = app.emit(
                            "nna:changed",
                            crate::ipc::Change::products("catalogue", [product]),
                        );
                    }
                }
                observations.push((row, observation));
            }
            Err(error) => {
                let code = safe_code(&error);
                if job_level(&code) {
                    return Err(error);
                }
                // This purchase's own answer: it stays as it was.
                *page_unresolved += 1;
                incomplete.insert(purchase_asset(known_purchases, &row.purchase_id));
                journal
                    .push(json!({"purchaseId":row.purchase_id,"complete":false,"errorCode":code}));
            }
        }
        job["progress"]["stage"] = json!("enriching");
        job["progress"]["metadataCompleted"] = json!(completed_before_page + *page_processed);
        job["progress"]["published"] = json!(checkpoint["publishedProducts"]
            .as_array()
            .map_or(0, Vec::len));
        self.save_running_job(job)?;
        let _ = app.emit(
            "nna:changed",
            crate::ipc::Change::job("jobs", job["id"].as_str().unwrap_or_default()),
        );
        Ok(())
    }

    /// Records the page in the pass checkpoint and commits its journal and records together.
    fn commit_library_page(
        &self,
        read: LibraryRead<'_>,
        job: &mut Value,
        pass: &mut LibraryPass,
        listed: &BTreeSet<String>,
        observed: &BTreeMap<String, String>,
        restarted: bool,
        tab: usize,
        archived: bool,
        page_index: u64,
        observations: &[(LibraryPurchase, ContentObservation)],
        journal: Vec<Value>,
        page_unresolved: u64,
    ) -> Result<()> {
        let LibraryRead {
            app,
            account,
            selected,
            checkpoint_id,
            metadata_total,
            page_limit,
            metrics_before,
            classified_before,
            ..
        } = read;
        let LibraryPass {
            checkpoint,
            seen,
            products,
            unresolved,
            public_products,
            public_errors,
            public_unavailable,
            bundle_completed,
            ..
        } = pass;
        *unresolved += page_unresolved;
        checkpoint["nextPages"][tab] = json!(page_index + 1);
        checkpoint["seenPurchaseIds"] = json!(seen);
        checkpoint["listedPurchaseIds"] = json!(listed);
        checkpoint["observedPurchases"] = json!(observed);
        checkpoint["productIds"] = json!(products);
        checkpoint["unresolvedRows"] = json!(unresolved);
        checkpoint["pages"] = json!(checkpoint["pages"].as_u64().unwrap_or(0) + 1);
        if page_limit.is_some() {
            job["verification"]["sourcePages"][if archived { "archived" } else { "unarchived" }] =
                json!(page_index);
        }
        checkpoint["updatedAt"] = json!(now());
        // A restarted pass keeps the first pass's page journals.
        let journal_id = format!(
            "gumroad:{account}:{}:{}{tab}:{page_index}",
            field(job, "id")?,
            if restarted { "restart:" } else { "" }
        );
        append_journal(checkpoint, &journal_id)?;
        let mut committed = false;
        for _ in 0..3 {
            self.cancelled(job)?;
            let mut batch = RecordBatch::new(&self.inner.catalogue);
            let mut prepared = RecordBatch::new(&self.inner.catalogue);
            for (row, observation) in observations {
                project(&mut prepared, account, row, observation)?;
                project_public(
                    &mut prepared,
                    observation,
                    public_products
                        .get(&observation.product_id)
                        .and_then(Option::as_ref),
                )?;
                if let Some(error) = public_errors
                    .get(&observation.product_id)
                    .filter(|_| needs_public_quote(observation))
                {
                    let id = observation.catalogue_product_id();
                    let mut product = prepared.get("product", &id)?.unwrap();
                    product["priceRefreshErrorCode"] = json!(error);
                    prepared.stage("product", &id, product)?;
                }
                if needs_public_quote(observation)
                    && public_unavailable.contains(&observation.product_id)
                {
                    let id = observation.catalogue_product_id();
                    let mut product = prepared.get("product", &id)?.unwrap();
                    product["publicPageAvailability"] = json!("unavailable");
                    prepared.stage("product", &id, product)?;
                }
            }
            batch.stage("checkpoint", &journal_id, json!({"id":journal_id,"store":"Gumroad","accountId":account,"runId":job["id"],"source":"library","archived":archived,"page":page_index,"observations":journal,"records":prepared.staged_records()}))?;
            if selected.is_none() {
                batch.stage("checkpoint", checkpoint_id, checkpoint.clone())?;
                batch.stage("source_coverage", checkpoint_id, json!({"id":checkpoint_id,"store":"Gumroad","accountId":account,"source":"library","status":"running","coverageComplete":false,"pages":checkpoint["pages"],"unresolvedRows":unresolved,"updatedAt":now()}))?;
            }
            if batch.try_commit()?.is_some() {
                committed = true;
                break;
            }
        }
        ensure!(committed, "CONCURRENT_UPDATE");
        job["progress"] = json!({"stage":"enriching","pages":checkpoint["pages"],"products":products.len(),"published":checkpoint["publishedProducts"].as_array().map_or(0,Vec::len),"metadataCompleted":seen.len()+bundle_completed.len(),"metadataTotal":metadata_total,"unresolvedRows":unresolved});
        job["updatedAt"] = json!(now());
        self.set_collection_metrics(job, metrics_before, classified_before);
        self.save_running_job(job)?;
        let _ = app.emit(
            "nna:changed",
            crate::ipc::Change::job("jobs", job["id"].as_str().unwrap_or_default()),
        );
        Ok(())
    }

    /// Assembles and publishes each owned bundle from the purchases it holds.
    async fn assemble_bundles(
        &self,
        read: LibraryRead<'_>,
        job: &mut Value,
        pass: &mut LibraryPass,
    ) -> Result<()> {
        let LibraryRead {
            app,
            account,
            client,
            selected,
            selected_bundles,
            buyer_identity,
            checkpoint_id,
            metadata_total,
            metrics_before,
            classified_before,
            ..
        } = read;
        let LibraryPass {
            owned_bundles,
            incomplete,
            checkpoint,
            seen,
            products,
            unresolved,
            bundle_completed,
            bundled_purchases,
            bundle_memberships,
            ..
        } = pass;
        let mut reused_children = BTreeMap::new();
        for bundle in owned_bundles
            .values()
            .filter(|bundle| selected.is_none() || selected_bundles.contains(&bundle.product_id))
        {
            if bundle_completed.contains(&bundle.product_id) {
                continue;
            }
            let members = bundle_memberships
                .remove(&bundle.product_id)
                .ok_or_else(|| anyhow::anyhow!("GUMROAD_BUNDLE_OWNERSHIP_UNPROVEN"))?;
            let assembled = self
                .assemble_bundle(
                    client,
                    bundle,
                    buyer_identity,
                    members,
                    &mut reused_children,
                    Some(job),
                )
                .await;
            let (card, observation, pages) = match assembled {
                // A bundle the library stopped listing stays as it was.
                Err(error) if safe_code(&error) == "GUMROAD_BUNDLE_OWNERSHIP_UNPROVEN" => {
                    *unresolved += 1;
                    incomplete.insert(format!("gumroad:{}", bundle.product_id));
                    continue;
                }
                assembled => assembled?,
            };
            let id = observation.catalogue_product_id();
            bundled_purchases.extend(observation.member_purchase_ids.iter().cloned());
            products.insert(id.clone());
            *unresolved += u64::from(!observation.inventory_complete);
            if !observation.inventory_complete {
                incomplete.insert(id.clone());
            }
            bundle_completed.insert(bundle.product_id.clone());
            checkpoint["completedBundles"] = json!(bundle_completed);
            checkpoint["bundlePurchaseIds"] = json!(bundled_purchases);
            checkpoint["productIds"] = json!(products);
            checkpoint["unresolvedRows"] = json!(unresolved);

            checkpoint["updatedAt"] = json!(now());
            let journal_id = format!(
                "gumroad:{account}:{}:bundle:{}",
                field(job, "id")?,
                fingerprint(&json!(bundle.product_id))
            );
            append_journal(checkpoint, &journal_id)?;
            let mut committed = false;
            for _ in 0..3 {
                self.cancelled(job)?;
                let mut batch = RecordBatch::new(&self.inner.catalogue);
                let mut prepared = RecordBatch::new(&self.inner.catalogue);
                project(&mut prepared, account, &card, &observation)?;
                batch.stage("checkpoint", &journal_id, json!({"id":journal_id,"store":"Gumroad","accountId":account,"runId":job["id"],"source":"library","sourceBundleId":bundle.product_id,"purchaseIds":observation.member_purchase_ids,"complete":observation.inventory_complete,"pages":pages,"observations":[{"productId":id,"complete":observation.inventory_complete}],"records":prepared.staged_records()}))?;
                if selected.is_none() {
                    batch.stage("checkpoint", checkpoint_id, checkpoint.clone())?;
                    batch.stage("source_coverage", checkpoint_id, json!({"id":checkpoint_id,"store":"Gumroad","accountId":account,"source":"library","status":"running","coverageComplete":false,"pages":checkpoint["pages"],"unresolvedRows":unresolved,"updatedAt":now()}))?;
                }
                if batch.try_commit()?.is_some() {
                    committed = true;
                    break;
                }
            }
            ensure!(committed, "CONCURRENT_UPDATE");
            self.publish_complete_journal(app, job, checkpoint, &journal_id)?;
            job["progress"] = json!({"stage":"enriching","pages":checkpoint["pages"],"products":products.len(),"published":checkpoint["publishedProducts"].as_array().map_or(0,Vec::len),"metadataCompleted":seen.len()+bundle_completed.len(),"metadataTotal":metadata_total,"unresolvedRows":unresolved});
            self.set_collection_metrics(job, metrics_before, classified_before);
            self.save_running_job(job)?;
            let _ = app.emit(
                "nna:changed",
                crate::ipc::Change::job("jobs", job["id"].as_str().unwrap_or_default()),
            );
        }
        Ok(())
    }

    /// Settles the pass: what stays as it was, certification of what the closing pass
    /// listed, and the library's coverage.
    fn finish_collection(
        &self,
        read: LibraryRead<'_>,
        job: &mut Value,
        listed: BTreeSet<String>,
        observed: BTreeMap<String, String>,
        pass: &mut LibraryPass,
    ) -> Result<()> {
        let LibraryRead {
            app,
            account,
            selected,
            known_purchases,
            checkpoint_id,
            metrics_before,
            classified_before,
            ..
        } = read;
        let LibraryPass {
            verification_capped,
            selected_purchases,
            incomplete,
            checkpoint,
            products,
            unresolved,
            found,
            ..
        } = pass;
        // A selected purchase the library no longer lists stays as it was.
        for id in selected_purchases.difference(found) {
            incomplete.insert(purchase_asset(known_purchases, id));
        }
        record_incomplete(job, incomplete);
        self.set_collection_metrics(job, metrics_before, classified_before);
        if *verification_capped {
            // Observed complete grants were already published additively. A
            // verification stop is not source closure or a replacement manifest.
            job["verification"]["capped"] = json!(true);
            self.save_running_job(job)?;
            bail!("VERIFICATION_PAGE_LIMIT");
        }
        // Only products whose purchases the closing pass listed are certified.
        let unlisted: BTreeSet<String> = observed
            .into_iter()
            .filter(|(purchase, _)| !listed.contains(purchase))
            .map(|(_, product)| product)
            .collect();
        self.publish_journals(app, job, account, checkpoint, known_purchases, &unlisted)?;
        if selected.is_some() {
            return Ok(());
        }
        let coverage_id = format!("gumroad:{account}:library");
        checkpoint["complete"] = json!(*unresolved == 0);
        let coverage = json!({"id":coverage_id,"store":"Gumroad","accountId":account,"source":"library","coverageComplete":*unresolved == 0,"status":if *unresolved == 0 { "complete" } else { "incomplete" },"terminalSeen":true,"pages":checkpoint["pages"],"unresolvedRows":unresolved,"productCount":products.len(),"incrementalBoundary":"fullTraversalWhenOrderingUnproven","completedAt":now()});
        let mut finalized = false;
        for _ in 0..3 {
            let mut batch = RecordBatch::new(&self.inner.catalogue);
            let current = batch
                .get("job", field(job, "id")?)?
                .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))?;
            ensure!(current["state"] != "cancelled", "CANCELLED");
            batch.stage("checkpoint", checkpoint_id, checkpoint.clone())?;
            batch.stage("source_coverage", &coverage_id, coverage.clone())?;
            if batch.try_commit()?.is_some() {
                finalized = true;
                break;
            }
        }
        ensure!(finalized, "CONCURRENT_UPDATE");
        Ok(())
    }

    /// The products an update job selected, and the purchases and bundles that hold them.
    fn library_selection(
        &self,
        job: &Value,
        account: &str,
    ) -> Result<(Option<BTreeSet<String>>, BTreeSet<String>, BTreeSet<String>)> {
        let selected: Option<BTreeSet<String>> = job["args"]["productIds"].as_array().map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        });
        let selected_purchases: BTreeSet<String> = if let Some(ids) = &selected {
            self.inner
                .catalogue
                .list("acquisition")?
                .into_iter()
                .filter(|row| {
                    row["accountId"] == account
                        && row["store"] == "Gumroad"
                        && row["productId"].as_str().is_some_and(|id| ids.contains(id))
                })
                .filter_map(|row| row["purchaseId"].as_str().map(str::to_owned))
                .collect()
        } else {
            BTreeSet::new()
        };
        let selected_bundles: BTreeSet<String> = self
            .inner
            .catalogue
            .list("acquisition")?
            .into_iter()
            .filter(|row| {
                row["accountId"] == account
                    && row["store"] == "Gumroad"
                    && selected.as_ref().is_some_and(|ids| {
                        row["productId"].as_str().is_some_and(|id| ids.contains(id))
                    })
            })
            .filter_map(|row| row["sourceBundleId"].as_str().map(str::to_owned))
            .collect();
        ensure!(
            selected.is_none() || !selected_purchases.is_empty() || !selected_bundles.is_empty(),
            "ENTITLEMENT_UNAVAILABLE"
        );
        Ok((selected, selected_purchases, selected_bundles))
    }
    fn set_collection_metrics(
        &self,
        job: &mut Value,
        before: [u64; 3],
        classified_before: [[u64; 13]; 2],
    ) {
        let metrics = self.inner.http.metrics();
        let by_kind = self.inner.http.classified_since(classified_before);
        job["requestMetrics"] = json!({"requests":metrics[0]-before[0],"pacingWaitMs":metrics[1]-before[1],"responseWaitMs":metrics[2]-before[2],"activeRequestLimit":2,"startSpacingMs":self.inner.http.start_spacing_ms(),"byKind":by_kind,"scope":"providerCollectionInterval"});
        job["progress"]["requestsCompleted"] = json!(metrics[0] - before[0]);
        job["progress"]["detailsCompleted"] = by_kind["content"]["requests"].clone();
    }
    fn clear_downloads(&self, account: &str, product: Option<&str>) {
        if let Ok(mut cache) = self.inner.downloads.lock() {
            if let Some(product) = product {
                if let Some(session) = cache.get_mut(account) {
                    session.manifests.remove(product);
                    session.purchases.clear();
                }
            } else {
                cache.remove(account);
            }
        }
    }
    fn cached_download(
        &self,
        account: &str,
        product: &str,
        file: &Value,
    ) -> Result<Option<(DownloadSession, gumroad::FileObservation)>> {
        let record = self.account(account)?;
        let cache = self
            .inner
            .downloads
            .lock()
            .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
        Ok(cache
            .get(account)
            .filter(|session| session.valid(&record, Instant::now()))
            .and_then(|session| {
                session
                    .manifests
                    .get(product)
                    .and_then(|manifest| {
                        manifest.offered(
                            account,
                            product.strip_prefix("gumroad:").unwrap_or(""),
                            file,
                            Instant::now(),
                        )
                    })
                    .map(|file| (session.snapshot(), file.clone()))
            }))
    }
    async fn fill_download(
        &self,
        app: &AppHandle,
        account: &str,
        product: &str,
        file: &Value,
    ) -> Result<(DownloadSession, gumroad::FileObservation)> {
        // Only cache misses wait here; map locks never cover native cookies or HTTP.
        let _fill = self.inner.download_fill.lock().await;
        if let Some(cached) = self.cached_download(account, product, file)? {
            return Ok(cached);
        }
        let record = self.account(account)?;
        ensure!(
            crate::stores::active_bound_account(&record),
            "AUTH_REQUIRED"
        );
        let identity = field(&record, "identityHash")?.to_owned();
        let cached_session = {
            let cache = self
                .inner
                .downloads
                .lock()
                .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
            cache
                .get(account)
                .filter(|session| session.valid(&record, Instant::now()))
                .map(DownloadSession::snapshot)
        };
        let mut session = if let Some(session) = cached_session {
            session
        } else {
            let (client, jar) = self.client_with_jar(app, account).await?;
            self.authenticate(app, account, &client, None).await?;
            let record = self.account(account)?;
            ensure!(
                crate::stores::active_bound_account(&record) && record["identityHash"] == identity,
                "ACCOUNT_IDENTITY_MISMATCH"
            );
            DownloadSession {
                client,
                jar,
                identity,
                verified_at: Instant::now(),
                manifests: BTreeMap::new(),
                purchases: BTreeMap::new(),
            }
        };
        let source_product = product
            .strip_prefix("gumroad:")
            .ok_or_else(|| anyhow::anyhow!("PRODUCT_UNAVAILABLE"))?;
        let purchase_ids: BTreeSet<_> = file["purchaseIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        ensure!(!purchase_ids.is_empty(), "ENTITLEMENT_UNAVAILABLE");
        if let Some(bundle) = file["sourceBundleId"].as_str() {
            ensure!(bundle == source_product, "GUMROAD_PRODUCT_BINDING_UNPROVEN");
            let head = self.library(&session.client, 1, false).await?;
            ensure!(
                fingerprint(&json!(["gumroad", head.buyer_identity])) == session.identity,
                "ACCOUNT_IDENTITY_MISMATCH"
            );
            let owned = head
                .bundles
                .into_iter()
                .find(|owned| owned.product_id == bundle)
                .ok_or_else(|| anyhow::anyhow!("GUMROAD_BUNDLE_OWNERSHIP_UNPROVEN"))?;
            let (_, observation, _) = self
                .bundle_content(
                    &session.client,
                    &owned,
                    &head.buyer_identity,
                    &mut BTreeMap::new(),
                    None,
                )
                .await?;
            session.verified_at = Instant::now();
            return self
                .retain_download(account, product, file, &session, &observation)?
                .ok_or_else(|| anyhow::anyhow!("FILE_UNAVAILABLE"));
        }
        let indexed = {
            let cache = self
                .inner
                .downloads
                .lock()
                .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
            cache
                .get(account)
                .map(|session| session.indexed_purchases(&record, &purchase_ids, Instant::now()))
                .unwrap_or_default()
        };
        let mut attempted = BTreeSet::new();
        for row in indexed {
            match self
                .expanded_content(&session.client, &row, Some(source_product))
                .await
            {
                Ok(observation) => {
                    attempted.insert(row.purchase_id.clone());
                    if let Some(ticket) =
                        self.retain_download(account, product, file, &session, &observation)?
                    {
                        return Ok(ticket);
                    }
                }
                Err(error)
                    if matches!(
                        safe_code(&error).as_str(),
                        "AUTH_REQUIRED"
                            | "PERMISSION_DENIED"
                            | "RATE_LIMITED"
                            | "ACCOUNT_IDENTITY_MISMATCH"
                    ) =>
                {
                    return Err(error)
                }
                Err(_) => {
                    self.clear_downloads(account, Some(product));
                }
            }
        }
        for archived in [false, true] {
            let first = self.library(&session.client, 1, archived).await?;
            let pages = first.pages;
            let mut first = Some(first);
            for index in 1..=pages {
                let page = if index == 1 {
                    first.take().unwrap()
                } else {
                    self.library(&session.client, index, archived).await?
                };
                ensure!(
                    fingerprint(&json!(["gumroad", page.buyer_identity])) == session.identity,
                    "ACCOUNT_IDENTITY_MISMATCH"
                );
                session.verified_at = Instant::now();
                self.index_download_page(account, &session, &page.purchases)?;
                for row in page.purchases.into_iter().filter(|row| {
                    !row.excluded_non_asset()
                        && purchase_ids.contains(row.purchase_id.as_str())
                        && !attempted.contains(&row.purchase_id)
                }) {
                    let observation = self
                        .expanded_content(&session.client, &row, Some(source_product))
                        .await?;
                    if let Some(ticket) =
                        self.retain_download(account, product, file, &session, &observation)?
                    {
                        return Ok(ticket);
                    }
                }
            }
        }
        bail!("FILE_UNAVAILABLE")
    }
    fn index_download_page(
        &self,
        account: &str,
        session: &DownloadSession,
        rows: &[LibraryPurchase],
    ) -> Result<()> {
        let record = self.account(account)?;
        ensure!(
            crate::stores::active_bound_account(&record)
                && record["identityHash"] == session.identity,
            "STORE_IDENTITY_CHANGED"
        );
        let mut cache = self
            .inner
            .downloads
            .lock()
            .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
        if !cache.contains_key(account) && cache.len() >= 16 {
            if let Some(oldest) = cache
                .iter()
                .min_by_key(|(_, session)| session.verified_at)
                .map(|(id, _)| id.clone())
            {
                cache.remove(&oldest);
            }
        }
        let cached = cache
            .entry(account.to_owned())
            .or_insert_with(|| session.snapshot());
        if !cached.valid(&record, Instant::now()) {
            *cached = session.snapshot();
        }
        cached.index_purchases(rows);
        Ok(())
    }
    fn retain_download(
        &self,
        account: &str,
        product: &str,
        file: &Value,
        session: &DownloadSession,
        observation: &ContentObservation,
    ) -> Result<Option<(DownloadSession, gumroad::FileObservation)>> {
        let source_product = &observation.product_id;
        ensure!(
            product == observation.catalogue_product_id(),
            "GUMROAD_PRODUCT_BINDING_UNPROVEN"
        );
        let mut manifest = DownloadManifest {
            purchase_ids: if observation.source_bundle_id.is_some() {
                observation.member_purchase_ids.clone()
            } else {
                BTreeSet::from([observation.purchase_id.clone()])
            },
            source_bundle_id: observation.source_bundle_id.clone(),
            observed_at: Instant::now(),
            files: observation.files.clone(),
            revisions: BTreeMap::new(),
        };
        for offered in &manifest.files {
            let id = remote_file_id(account, source_product, offered);
            if let Some(saved) = self.inner.catalogue.get("remote_file", &id)? {
                if saved["sourceFingerprint"] == source_fingerprint(offered) {
                    if let Some(revision) = saved["revision"]
                        .as_str()
                        .filter(|revision| !revision.is_empty())
                    {
                        manifest.revisions.insert(id, revision.to_owned());
                    }
                }
            }
        }
        let Some(offered) = manifest
            .offered(account, source_product, file, Instant::now())
            .cloned()
        else {
            return Ok(None);
        };
        let record = self.account(account)?;
        ensure!(
            crate::stores::active_bound_account(&record)
                && record["identityHash"] == session.identity,
            "STORE_IDENTITY_CHANGED"
        );
        let mut cache = self
            .inner
            .downloads
            .lock()
            .map_err(|_| anyhow::anyhow!("STATE_LOCK_FAILED"))?;
        if !cache.contains_key(account) && cache.len() >= 16 {
            if let Some(oldest) = cache
                .iter()
                .min_by_key(|(_, session)| session.verified_at)
                .map(|(id, _)| id.clone())
            {
                cache.remove(&oldest);
            }
        }
        let cached = cache
            .entry(account.to_owned())
            .or_insert_with(|| session.snapshot());
        if !cached.valid(&record, Instant::now()) {
            *cached = session.snapshot();
        }
        if observation.inventory_complete {
            if cached.manifests.len() >= 8 {
                if let Some(oldest) = cached
                    .manifests
                    .iter()
                    .min_by_key(|(_, manifest)| manifest.observed_at)
                    .map(|(id, _)| id.clone())
                {
                    cached.manifests.remove(&oldest);
                }
            }
            cached.manifests.insert(product.to_owned(), manifest);
        }
        Ok(Some((session.snapshot(), offered)))
    }
    pub async fn download_ticket(&self, app: &AppHandle, file_id: &str) -> Result<DownloadTicket> {
        let file = self
            .inner
            .catalogue
            .get("remote_file", file_id)?
            .filter(|file| file["store"] == "Gumroad")
            .ok_or_else(|| anyhow::anyhow!("FILE_UNAVAILABLE"))?;
        let account = field(&file, "accountId")?;
        let saved_account = self.account(account)?;
        let product = field(&file, "productId")?;
        let result = async {
            ensure!(
                crate::stores::active_bound_account(&saved_account),
                "AUTH_REQUIRED"
            );
            let (session, offered) =
                if let Some(cached) = self.cached_download(account, product, &file)? {
                    cached
                } else {
                    self.fill_download(app, account, product, &file).await?
                };
            let head_client = http::build_head_client(StoreProvider::Gumroad, session.jar.clone())?;
            let url = offered
                .resolver
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("DOWNLOAD_LINK_UNAVAILABLE"))?
                .url()
                .clone();
            // A file resolver answers only for its file.
            let response = self
                .inner
                .http
                .resolve_head(&head_client, url)
                .await
                .map_err(|error| asset_failure(error, "DOWNLOAD_RESOLVER_FAILED"))?;
            match response.status().as_u16() {
                401 => bail!("DOWNLOAD_RESOLVER_FAILED"),
                403 => bail!("SIGNED_LINK_EXPIRED"),
                404 | 410 => bail!("FILE_UNAVAILABLE"),
                // Any redirect with a usable Location resolves the file, not only 302.
                _ => ensure!(
                    response.status().is_redirection(),
                    "DOWNLOAD_LINK_UNAVAILABLE"
                ),
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| response.url().join(value).ok())
                .filter(http::gumroad_file_destination)
                .ok_or_else(|| anyhow::anyhow!("UNSAFE_DOWNLOAD_URL"))?;
            let account_record = self.account(account)?;
            ensure!(
                crate::stores::active_bound_account(&account_record)
                    && account_record["identityHash"] == session.identity,
                "STORE_IDENTITY_CHANGED"
            );
            let current = self
                .inner
                .catalogue
                .get("remote_file", file_id)?
                .ok_or_else(|| anyhow::anyhow!("FILE_UNAVAILABLE"))?;
            ensure!(
                [
                    "id",
                    "productId",
                    "accountId",
                    "sourceProductId",
                    "sourcePurchaseId",
                    "sourceFileId",
                    "purchaseIds",
                    "sourceBundleId",
                    "revision",
                    "sourceFingerprint"
                ]
                .iter()
                .all(|key| current[*key] == file[*key]),
                "FILE_CHANGED"
            );
            Ok(DownloadTicket {
                provider: StoreProvider::Gumroad,
                expected_origin: None,
                url: location,
                client: http::build_download_client(StoreProvider::Gumroad, session.jar.clone())?,
                expected_size: offered.size_bytes,
            })
        }
        .await;
        if let Err(error) = &result {
            // A refused page or file link re-verifies the account on the next ticket.
            match safe_code(error).as_str() {
                "AUTH_REQUIRED"
                | "PERMISSION_DENIED"
                | "DOWNLOAD_RESOLVER_FAILED"
                | "ACCOUNT_IDENTITY_MISMATCH"
                | "STORE_IDENTITY_CHANGED" => self.clear_downloads(account, None),
                _ => self.clear_downloads(account, Some(product)),
            }
        }
        result
    }
}

fn append_journal(checkpoint: &mut Value, id: &str) -> Result<()> {
    if checkpoint["journalIds"].is_null() {
        checkpoint["journalIds"] = json!([]);
    }
    let ids = checkpoint["journalIds"]
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
    if !ids.iter().any(|value| value == id) {
        ids.push(json!(id));
    }
    Ok(())
}
type PreparedRecords = Vec<(String, String, Value)>;
fn grouped_journals(
    catalogue: &Catalogue,
    job: &Value,
    account: &str,
    checkpoint: &Value,
    known_purchases: &BTreeMap<String, String>,
) -> Result<(BTreeMap<String, PreparedRecords>, BTreeSet<String>, bool)> {
    let mut groups = BTreeMap::<String, PreparedRecords>::new();
    let mut blocked = BTreeSet::new();
    let mut unbound = false;
    for id in checkpoint["journalIds"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?
    {
        let journal = catalogue
            .get("checkpoint", field_value(id)?)?
            .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
        ensure!(
            journal["accountId"] == account
                && journal["runId"] == job["id"]
                && journal["store"] == "Gumroad",
            "CHECKPOINT_INVALID"
        );
        for observation in journal["observations"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))?
        {
            if observation["complete"] != true {
                if let Some(product) = observation["productId"]
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| {
                        observation["purchaseId"]
                            .as_str()
                            .and_then(|id| known_purchases.get(id))
                            .cloned()
                    })
                {
                    blocked.insert(product);
                } else {
                    unbound = true;
                }
            }
        }
        let records: PreparedRecords = serde_json::from_value(journal["records"].clone())
            .map_err(|_| anyhow::anyhow!("CHECKPOINT_INVALID"))?;
        for (kind, id, value) in records {
            ensure!(
                matches!(
                    kind.as_str(),
                    "product" | "acquisition" | "remote_file" | "entitlement"
                ),
                "CHECKPOINT_INVALID"
            );
            let product = if kind == "product" {
                id.clone()
            } else {
                field(&value, "productId")?.to_owned()
            };
            groups.entry(product).or_default().push((kind, id, value));
        }
    }
    Ok((groups, blocked, unbound))
}
fn field_value(value: &Value) -> Result<&str> {
    value
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow::anyhow!("CHECKPOINT_INVALID"))
}
fn commit_gumroad_product(
    catalogue: &Catalogue,
    job: &Value,
    product: &str,
    records: &PreparedRecords,
    checkpoint: &mut Value,
) -> Result<()> {
    commit_gumroad_product_mode(catalogue, job, product, records, checkpoint, true)
}
fn commit_gumroad_product_mode(
    catalogue: &Catalogue,
    job: &Value,
    product: &str,
    records: &PreparedRecords,
    checkpoint: &mut Value,
    authoritative: bool,
) -> Result<()> {
    let key = field(checkpoint, "id")?;
    let mut published: BTreeSet<String> = checkpoint["publishedProducts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let version = fingerprint(&json!([records, authoritative]));
    if checkpoint["publishedVersions"][product] == version {
        return Ok(());
    }
    published.insert(product.into());
    let mut updated = checkpoint.clone();
    updated["publishedProducts"] = json!(published);
    if !updated["publishedVersions"].is_object() {
        updated["publishedVersions"] = json!({});
    }
    updated["publishedVersions"][product] = json!(version);
    for _ in 0..3 {
        let mut batch = RecordBatch::new(catalogue);
        let current = batch
            .get("job", field(job, "id")?)?
            .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))?;
        ensure!(current["state"] != "cancelled", "CANCELLED");
        for (kind, id, value) in records {
            let mut value = value.clone();
            if kind == "entitlement" && !authoritative {
                let unchanged = batch.get(kind, id)?.is_some_and(|previous| {
                    previous["manifestComplete"] == true
                        && previous["fileIds"] == value["fileIds"]
                        && previous["variantIds"] == value["variantIds"]
                });
                value["manifestComplete"] = json!(unchanged);
            }
            if kind == "remote_file" {
                if let Some(previous) = batch.get(kind, id)? {
                    let purchases: BTreeSet<_> = previous["purchaseIds"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .chain(value["purchaseIds"].as_array().into_iter().flatten())
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect();
                    value["purchaseIds"] = json!(purchases);
                }
            }
            batch.stage(kind, id, value)?;
        }
        // A later base-product purchase must not overwrite an unresolved owned
        // variant quote from another grant of this same product.
        if batch
            .related("acquisition", product)?
            .iter()
            .any(|row| row["variantId"].as_str().is_some_and(|id| !id.is_empty()))
        {
            let mut value = batch
                .get("product", product)?
                .ok_or_else(|| anyhow::anyhow!("PRODUCT_UNAVAILABLE"))?;
            clear_unbound_variant_price(&mut value);
            batch.stage("product", product, value)?;
        }
        batch.stage("checkpoint", key, updated.clone())?;
        if batch.try_commit()?.is_some() {
            *checkpoint = updated;
            return Ok(());
        }
    }
    bail!("CONCURRENT_UPDATE")
}
fn parentless_purchase(card: &LibraryPurchase, bundle_purchase_ids: &BTreeSet<String>) -> bool {
    !card.excluded_non_asset() && !bundle_purchase_ids.contains(&card.purchase_id)
}

fn bundle_parent(bundle: &gumroad::LibraryBundle) -> (LibraryPurchase, ContentObservation) {
    (
        LibraryPurchase {
            purchase_id: String::new(),
            title: bundle.label.clone(),
            author: None,
            thumbnail_url: None,
            native_type: Some("bundle".into()),
            variant_name: None,
            archived: false,
            content_route: None,
        },
        ContentObservation {
            purchase_id: String::new(),
            product_id: bundle.product_id.clone(),
            variant_id: None,
            title: bundle.label.clone(),
            acquired_at: None,
            has_membership: false,
            files: Vec::new(),
            inventory_complete: false,
            unsupported_items: 0,
            public_product_url: None,
            bundles: Vec::new(),
            source_bundle_id: Some(bundle.product_id.clone()),
            member_purchase_ids: BTreeSet::new(),
        },
    )
}

fn project(
    batch: &mut RecordBatch<'_>,
    account: &str,
    card: &LibraryPurchase,
    observation: &ContentObservation,
) -> Result<()> {
    let product_id = observation.catalogue_product_id();
    let mut product = batch.get("product", &product_id)?.unwrap_or_else(|| json!({"id":product_id,"store":"Gumroad","provider":"gumroad","sourceTags":[],"tags":[]}));
    for (key, value) in [
        ("title", observation.title.as_ref().or(card.title.as_ref())),
        (
            "sourceTitle",
            observation.title.as_ref().or(card.title.as_ref()),
        ),
        ("author", card.author.as_ref()),
        ("shop", card.author.as_ref()),
        ("previewUrl", card.thumbnail_url.as_ref()),
    ] {
        if let Some(value) = value {
            product[key] = json!(value);
        }
    }
    if let Some(url) = &observation.public_product_url {
        product["sourceUrl"] = json!(url.as_str());
    }
    product["observedAt"] = json!(now());
    batch.stage("product", &product_id, product)?;
    let acquisition_id = format!(
        "gumroad:{account}:{}",
        fingerprint(&if let Some(bundle) = &observation.source_bundle_id {
            json!(["bundle", bundle])
        } else {
            json!(observation.purchase_id)
        })
    );
    let mut acquisition = batch.get("acquisition", &acquisition_id)?.unwrap_or_else(|| json!({"id":acquisition_id,"productId":product_id,"accountId":account,"store":"Gumroad","provider":"gumroad","source":"library"}));
    if let Some(bundle) = &observation.source_bundle_id {
        acquisition["sourceBundleId"] = json!(bundle);
        acquisition["purchaseIds"] = json!(observation.member_purchase_ids);
        acquisition.as_object_mut().unwrap().remove("purchaseId");
    } else {
        acquisition["purchaseId"] = json!(observation.purchase_id);
    }
    if let Some(date) = &observation.acquired_at {
        acquisition["acquiredDateText"] = json!(date.to_rfc3339());
        acquisition["acquiredAt"] = json!(date.to_rfc3339());
    }
    acquisition["variantId"] = json!(observation.variant_id);
    acquisition["variantName"] = json!(card.variant_name);
    acquisition["isArchived"] = json!(card.archived);
    acquisition["membership"] = json!(observation.has_membership);
    acquisition["observedAt"] = json!(now());
    batch.stage("acquisition", &acquisition_id, acquisition)?;
    let mut file_ids = BTreeSet::new();
    for file in &observation.files {
        if file.access != FileAccess::Downloadable || file.name.is_none() {
            continue;
        }
        let id = format!(
            "gumroad:{account}:{}",
            fingerprint(&json!([
                observation.product_id,
                file.source_product_id,
                file.file_id
            ]))
        );
        let previous = batch.get("remote_file", &id)?;
        let mut purchase_ids: BTreeSet<String> = previous
            .as_ref()
            .and_then(|row| row["purchaseIds"].as_array())
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        if observation.source_bundle_id.is_some() {
            purchase_ids.extend(observation.member_purchase_ids.iter().cloned());
        } else {
            purchase_ids.insert(observation.purchase_id.clone());
        }
        let evidence = fingerprint(&json!([file.name, file.folders, file.size_bytes]));
        let revision = previous
            .as_ref()
            .filter(|row| row["sourceFingerprint"] == evidence)
            .and_then(|row| row["revision"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("observed-{evidence}"));
        batch.stage("remote_file", &id, json!({"id":id,"productId":product_id,"accountId":account,"store":"Gumroad","provider":"gumroad","source":"library","sourceBundleId":observation.source_bundle_id,"sourceFileId":file.file_id,"sourceProductId":file.source_product_id,"sourcePurchaseId":file.source_purchase_id,"purchaseIds":purchase_ids,"name":file.name,"sourceFolders":file.folders,"sizeBytes":file.size_bytes,"revision":revision,"sourceFingerprint":evidence,"revisionBasis":"unverifiedSourceMetadata","requiresContentVerification":true,"observedAt":now()}))?;
        file_ids.insert(id);
    }
    let mut entitlement = batch
        .get("entitlement", &acquisition_id)?
        .unwrap_or_else(|| json!({"id":acquisition_id}));
    if !observation.inventory_complete {
        file_ids.extend(
            entitlement["fileIds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
    }
    let mut fresh = json!({"id":acquisition_id,"productId":product_id,"accountId":account,"store":"Gumroad","provider":"gumroad","source":"library","fileIds":file_ids,"manifestComplete":observation.inventory_complete,"currentManifest":true,"unresolvedItems":observation.unsupported_items,"observedAt":now()});
    if let Some(bundle) = &observation.source_bundle_id {
        fresh["sourceBundleId"] = json!(bundle);
        fresh["purchaseIds"] = json!(observation.member_purchase_ids);
        entitlement.as_object_mut().unwrap().remove("purchaseId");
    } else {
        fresh["purchaseId"] = json!(observation.purchase_id);
    }
    entitlement
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("ENTITLEMENT_INVALID"))?
        .extend(fresh.as_object().unwrap().clone());
    batch.stage("entitlement", &acquisition_id, entitlement)
}

fn needs_public_quote(observation: &ContentObservation) -> bool {
    // Public bodies enrich every product. Listing quotes and billing flags do not
    // establish an owned option's current price. Paid evidence is independent.
    observation.variant_id.is_none()
}

fn project_public(
    batch: &mut RecordBatch<'_>,
    observation: &ContentObservation,
    public: Option<&gumroad::PublicProductObservation>,
) -> Result<()> {
    let id = observation.catalogue_product_id();
    let mut product = batch
        .get("product", &id)?
        .ok_or_else(|| anyhow::anyhow!("PRODUCT_UNAVAILABLE"))?;
    product["priceCheckedAt"] = json!(now());
    product["priceRefreshErrorCode"] = Value::Null;
    product["priceRefreshState"] = json!(if public.is_some() && needs_public_quote(observation) {
        "observed"
    } else {
        "unavailable"
    });
    if let Some(public) = public {
        ensure!(
            public.product_id == observation.product_id,
            "GUMROAD_PRODUCT_BINDING_UNPROVEN"
        );
        if let Some(description) = &public.description {
            product["sourceDescription"] = json!(description);
        }
    }
    if let Some(public) = public.filter(|_| needs_public_quote(observation)) {
        let amount = public
            .current_price
            .as_ref()
            .and_then(gumroad::PublicPrice::usd_amount);
        product["price"] = json!(amount);
        product["priceCurrency"] = json!(amount.map(|_| "USD"));
        product["priceBasis"] = json!(amount.map(|_| "gumroadPublicPriceCents"));
        product["priceFormatted"] = json!(public.price_formatted);
        product["priceSourceCurrency"] = json!(public.price_source_currency);
        product["sourcePriceCents"] = json!(public
            .current_price
            .as_ref()
            .filter(|price| price.price_cents <= 9_007_199_254_740_991)
            .map(|price| price.price_cents));
        product["isPayWhatYouWant"] = json!(public
            .current_price
            .as_ref()
            .map(|price| price.pay_what_you_want));
        product["isRecurringBilling"] =
            json!(public.current_price.as_ref().map(|price| price.recurring));
        product["priceObservedAt"] = json!(now());
    }
    // The current public observation contains only a product-level quote, not
    // an exact option-ID price. It cannot establish the owned variant's price.
    if observation.variant_id.is_some() {
        clear_unbound_variant_price(&mut product);
    }
    batch.stage("product", &id, product)
}
fn clear_unbound_variant_price(product: &mut Value) {
    for key in ["price", "priceCurrency", "priceBasis", "priceFormatted"] {
        product[key] = Value::Null;
    }
}

fn merge_member_files(parent: &mut ContentObservation, files: Vec<gumroad::FileObservation>) {
    for file in files {
        if let Some(previous) = parent.files.iter_mut().find(|previous| {
            previous.file_id == file.file_id && previous.source_product_id == file.source_product_id
        }) {
            if previous.name != file.name
                || previous.size_bytes != file.size_bytes
                || previous.access != file.access
            {
                previous.access = FileAccess::Unavailable;
                previous.resolver = None;
                parent.unsupported_items += 1;
            }
        } else {
            parent.files.push(file);
        }
    }
}

pub fn validate_download_response(response: &reqwest::Response) -> Result<()> {
    ensure!(
        http::gumroad_file_destination(response.url()),
        "UNSAFE_DOWNLOAD_URL"
    );
    match response.status().as_u16() {
        // A signed file URL answers only for that file, never for the account's sign-in.
        401 | 403 => bail!("SIGNED_LINK_EXPIRED"),
        404 | 410 => bail!("FILE_UNAVAILABLE"),
        429 => bail!("RATE_LIMITED"),
        _ => ensure!(response.status().is_success(), "DOWNLOAD_RESOLVER_FAILED"),
    }
    Ok(())
}

#[cfg(test)]
#[path = "service_tests.rs"]
mod tests;
