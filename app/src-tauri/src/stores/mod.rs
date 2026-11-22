//! Small store dispatch boundary. Provider collectors retain their own source contracts.
pub(crate) mod api;
pub mod gumroad;
pub mod http;
pub mod jinxxy;
mod public_metadata;
pub mod session;
pub(crate) mod session_cache;

use crate::{booth::BoothService, catalogue::Catalogue};
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum StoreProvider {
    #[serde(rename = "BOOTH", alias = "booth")]
    Booth,
    #[serde(rename = "Gumroad", alias = "gumroad")]
    Gumroad,
    #[serde(rename = "Jinxxy", alias = "jinxxy")]
    Jinxxy,
}

impl StoreProvider {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "BOOTH" | "booth" => Ok(Self::Booth),
            "Gumroad" | "gumroad" => Ok(Self::Gumroad),
            "Jinxxy" | "jinxxy" => Ok(Self::Jinxxy),
            _ => bail!("STORE_UNAVAILABLE"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Booth => "BOOTH",
            Self::Gumroad => "Gumroad",
            Self::Jinxxy => "Jinxxy",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Booth => "booth",
            Self::Gumroad => "gumroad",
            Self::Jinxxy => "jinxxy",
        }
    }
}

/// Transient capability only. Signed URLs and clients never enter source records or IPC.
/// Intentionally has no Debug or Serialize implementation.
pub struct DownloadTicket {
    pub provider: StoreProvider,
    pub url: url::Url,
    pub client: reqwest::Client,
    pub expected_size: Option<u64>,
    /// Origin-only binding for a provider-issued compiled download; never persisted.
    pub expected_origin: Option<url::Url>,
}

pub fn validate_download_response(
    provider: StoreProvider,
    response: &reqwest::Response,
    expected_origin: Option<&url::Url>,
) -> Result<()> {
    match provider {
        StoreProvider::Booth => crate::booth::validate_download_response(response),
        StoreProvider::Gumroad => gumroad::service::validate_download_response(response),
        StoreProvider::Jinxxy => jinxxy::validate_download_response(response, expected_origin),
    }
}

pub(super) fn active_bound_account(account: &Value) -> bool {
    account["identityVerified"] == true
        && account["disconnected"] != true
        && account["status"] == "connected"
        && account["state"] == "connected"
        && account["identityHash"]
            .as_str()
            .is_some_and(|hash| !hash.is_empty())
}

/// A shared account binding is not a credential for the current device.
pub(crate) fn has_local_session(catalogue: &Catalogue, account: &Value) -> bool {
    if catalogue.library_root().is_none() {
        return true;
    }
    let Some(id) = account["id"].as_str() else {
        return false;
    };
    let Some(provider) = account["store"]
        .as_str()
        .and_then(|provider| StoreProvider::parse(provider).ok())
    else {
        return false;
    };
    session::profile_path(&catalogue.device_dir(), provider, id)
        .ok()
        .filter(|profile| !session_cache::signed_out(profile))
        .and_then(|profile| session_cache::load(&profile, provider, id).ok())
        .is_some_and(|cookies| !cookies.is_empty())
}

#[derive(Clone)]
pub struct StoreService {
    catalogue: Catalogue,
    booth: Arc<BoothService>,
    gumroad: gumroad::service::GumroadService,
    jinxxy: jinxxy::JinxxyService,
    command_gate: Arc<tokio::sync::Mutex<()>>,
    public_gumroad: http::MetadataHttp,
    public_jinxxy: http::MetadataHttp,
}

fn requested_provider(args: &Value) -> Result<Option<StoreProvider>> {
    let parse = |value: &Value| {
        StoreProvider::parse(
            value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("STORE_UNAVAILABLE"))?,
        )
    };
    let store = args.get("store").map(parse).transpose()?;
    let provider = args.get("provider").map(parse).transpose()?;
    ensure!(
        store.is_none() || provider.is_none() || store == provider,
        "STORE_ACCOUNT_MISMATCH"
    );
    Ok(store.or(provider))
}

fn require_provider(
    requested: Option<StoreProvider>,
    actual: StoreProvider,
) -> Result<StoreProvider> {
    ensure!(
        requested.is_none_or(|value| value == actual),
        "STORE_ACCOUNT_MISMATCH"
    );
    Ok(actual)
}

impl StoreService {
    pub fn has_local_session(&self, account: &Value) -> bool {
        has_local_session(&self.catalogue, account)
    }

    pub fn account_views(&self, accounts: Vec<Value>) -> Vec<Value> {
        accounts
            .into_iter()
            .map(|mut account| {
                let available = self.has_local_session(&account);
                account["localSessionAvailable"] = json!(available);
                if !available && account["status"] == "connected" {
                    account["status"] = json!("expired");
                    account["state"] = json!("expired");
                }
                account
            })
            .collect()
    }

    pub fn new(catalogue: Catalogue, booth: Arc<BoothService>) -> Result<Self> {
        let gumroad = gumroad::service::GumroadService::new(catalogue.clone())?;
        let jinxxy = jinxxy::JinxxyService::new(catalogue.clone())?;
        Ok(Self {
            catalogue,
            booth,
            gumroad,
            jinxxy,
            command_gate: Arc::new(tokio::sync::Mutex::new(())),
            public_gumroad: http::MetadataHttp::new(StoreProvider::Gumroad),
            public_jinxxy: http::MetadataHttp::new(StoreProvider::Jinxxy),
        })
    }

    pub(crate) fn booth(&self) -> BoothService {
        (*self.booth).clone()
    }

    pub(crate) fn rebind_catalogue(&self, catalogue: Catalogue) -> Result<Self> {
        ensure!(
            catalogue.device_dir() == self.catalogue.device_dir(),
            "STORE_DEVICE_MISMATCH"
        );
        Ok(Self {
            booth: Arc::new(self.booth.rebind_catalogue(catalogue.clone())?),
            gumroad: self.gumroad.rebind_catalogue(catalogue.clone())?,
            jinxxy: jinxxy::JinxxyService::new(catalogue.clone())?,
            catalogue,
            command_gate: Arc::new(tokio::sync::Mutex::new(())),
            public_gumroad: http::MetadataHttp::new(StoreProvider::Gumroad),
            public_jinxxy: http::MetadataHttp::new(StoreProvider::Jinxxy),
        })
    }

    pub async fn start(&self, app: AppHandle) -> Result<()> {
        let _gate = self.command_gate.lock().await;
        let booth = self.booth.start(app.clone()).await;
        let gumroad = self.gumroad.start(&app).await;
        let jinxxy = self.jinxxy.start(&app).await;
        // A provider's failed recovery cannot prevent independent workers from
        // starting. Report the first real service error after trying all three.
        booth.and(gumroad).and(jinxxy)
    }

    pub fn provider_for_account(&self, account_id: &str) -> Result<StoreProvider> {
        uuid::Uuid::parse_str(account_id).map_err(|_| anyhow::anyhow!("INVALID_ACCOUNT_ID"))?;
        let account = self
            .catalogue
            .get("account", account_id)?
            .ok_or_else(|| anyhow::anyhow!("ACCOUNT_UNAVAILABLE"))?;
        StoreProvider::parse(
            account["store"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("STORE_UNAVAILABLE"))?,
        )
    }
    fn rename_account(&self, id: &str, name: &str) -> Result<()> {
        self.provider_for_account(id)?;
        for _ in 0..4 {
            let previous = self
                .catalogue
                .get("account", id)?
                .ok_or_else(|| anyhow::anyhow!("ACCOUNT_UNAVAILABLE"))?;
            let mut renamed = previous.clone();
            renamed["name"] = json!(name);
            renamed["label"] = json!(name);
            if self.catalogue.put_many_if_unchanged(
                vec![("account".into(), id.into(), renamed)],
                vec![("account".into(), id.into(), Some(previous))],
            )? {
                return Ok(());
            }
        }
        bail!("CONCURRENT_UPDATE")
    }

    fn provider_for_job(&self, job_id: &str) -> Result<StoreProvider> {
        let job = self
            .catalogue
            .get("job", job_id)?
            .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))?;
        let booth_job = matches!(
            job["type"].as_str(),
            Some("boothSync" | "boothUpdate" | "boothLogin")
        );
        let provider = StoreProvider::parse(
            job["store"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("INVALID_JOB"))?,
        )?;
        if let Some(account) = job["accountId"].as_str() {
            require_provider(Some(provider), self.provider_for_account(account)?)?;
        }
        ensure!(
            !booth_job || provider == StoreProvider::Booth,
            "STORE_ACCOUNT_MISMATCH"
        );
        Ok(provider)
    }

    fn selected_groups(&self, args: &Value) -> Result<BTreeMap<StoreProvider, BTreeSet<String>>> {
        let ids = args["productIds"]
            .as_array()
            .or_else(|| args["ids"].as_array())
            .ok_or_else(|| anyhow::anyhow!("INVALID_PRODUCT_IDS"))?;
        ensure!(!ids.is_empty(), "INVALID_PRODUCT_IDS");
        let mut providers: BTreeMap<StoreProvider, BTreeSet<String>> = BTreeMap::new();
        for id in ids {
            let id = id
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| anyhow::anyhow!("INVALID_PRODUCT_IDS"))?;
            let provider = match self.catalogue.get("product", id)? {
                Some(product) if product["provider"] == "local" => {
                    ensure!(
                        product["store"] == "Local Files"
                            && id.strip_prefix("local:").is_some_and(|id| !id.is_empty()),
                        "STORE_ACCOUNT_MISMATCH"
                    );
                    continue;
                }
                Some(product) => StoreProvider::parse(
                    product["store"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("STORE_UNAVAILABLE"))?,
                )?,
                // BOOTH allows explicitly selected numeric public products before ingestion.
                None if !id.is_empty()
                    && id.len() <= 24
                    && id.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    StoreProvider::Booth
                }
                None => bail!("PRODUCT_UNAVAILABLE"),
            };
            require_provider(requested_provider(args)?, provider)?;
            providers.entry(provider).or_default().insert(id.into());
        }
        Ok(providers)
    }

    fn preflight_updates(
        &self,
        groups: &BTreeMap<StoreProvider, BTreeSet<String>>,
        args: &Value,
    ) -> Result<()> {
        let entitlements = self.catalogue.list("entitlement")?;
        let mut accounts = BTreeSet::new();
        for (provider, products) in groups {
            if *provider == StoreProvider::Booth {
                continue;
            }
            for product in products {
                let mut found = false;
                for entitlement in entitlements
                    .iter()
                    .filter(|row| row["productId"] == *product)
                {
                    let account_id = entitlement["accountId"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("ACCOUNT_UNAVAILABLE"))?;
                    if args["accountId"]
                        .as_str()
                        .is_some_and(|scope| scope != account_id)
                    {
                        continue;
                    }
                    let account = self
                        .catalogue
                        .get("account", account_id)?
                        .ok_or_else(|| anyhow::anyhow!("ACCOUNT_UNAVAILABLE"))?;
                    if !active_bound_account(&account) || !self.has_local_session(&account) {
                        continue;
                    }
                    require_provider(Some(*provider), self.provider_for_account(account_id)?)?;
                    accounts.insert(account_id.to_owned());
                    found = true;
                }
                ensure!(found, "AUTH_REQUIRED");
            }
        }
        // BOOTH queues an update behind its account's running work and reuses an identical one.
        for job in self
            .catalogue
            .list("job")?
            .into_iter()
            .filter(|row| matches!(row["state"].as_str(), Some("queued" | "running")))
        {
            ensure!(
                !job["accountId"]
                    .as_str()
                    .is_some_and(|id| accounts.contains(id)),
                "ACCOUNT_BUSY"
            );
        }
        Ok(())
    }

    /// A public listing's facts. No session, gate or catalogue write.
    async fn public_product(&self, raw: &str) -> Result<Value> {
        let Some((provider, url)) = public_metadata::product_url(raw)? else {
            return self.booth.fetch_product_metadata(raw).await;
        };
        let transport = match provider {
            StoreProvider::Gumroad => &self.public_gumroad,
            StoreProvider::Jinxxy => &self.public_jinxxy,
            StoreProvider::Booth => unreachable!(),
        };
        public_metadata::fetch(transport, provider, url).await
    }

    /// What a pasted store link asks now. Any other link never reaches the network.
    pub(crate) async fn listing_price(&self, raw: &str) -> Result<Option<Value>> {
        let product = self
            .public_product(&public_metadata::bare_url(raw)?)
            .await?;
        Ok(product.get("listingPrice").cloned())
    }

    pub async fn command(&self, app: &AppHandle, action: &str, mut args: Value) -> Result<Value> {
        if action == "fetch_product_metadata" {
            let raw = args["url"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("INVALID_PRODUCT_URL"))?;
            return self.public_product(raw).await;
        }
        // Finish interrupted-job recovery before accepting a new login/sync.
        // Provider start is idempotent; the same gate prevents startup from
        // mistaking a freshly created login for an interrupted one.
        self.start(app.clone()).await?;
        let _work = self.catalogue.begin_catalogue_work()?;
        // Serialize command preparation only; collectors run independently after job creation.
        let _gate = self.command_gate.lock().await;
        let requested = requested_provider(&args)?;
        if action == "update_products" {
            let groups = self.selected_groups(&args)?;
            self.preflight_updates(&groups, &args)?;
            let mut jobs = BTreeSet::new();
            for (provider, ids) in groups {
                let mut provider_args = json!({"productIds":ids});
                if let Some(scope) = args.get("accountId") {
                    provider_args["accountId"] = scope.clone();
                }
                let result = match provider {
                    StoreProvider::Booth => self.booth.command(app, action, provider_args).await?,
                    StoreProvider::Gumroad => {
                        self.gumroad.command(app, action, provider_args).await?
                    }
                    StoreProvider::Jinxxy => {
                        self.jinxxy.command(app, action, provider_args).await?
                    }
                };
                if let Some(ids) = result["jobIds"].as_array() {
                    jobs.extend(ids.iter().filter_map(Value::as_str).map(str::to_owned));
                }
                if let Some(id) = result["jobId"].as_str() {
                    jobs.insert(id.into());
                }
            }
            return api::answer(api::StoreJobs {
                job_id: jobs.first().cloned(),
                job_ids: jobs.into_iter().collect(),
            });
        }
        let provider = if matches!(action, "cancel_sync" | "retry_sync") {
            let job = args["jobId"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))?;
            require_provider(requested, self.provider_for_job(job)?)?
        } else if let Some(account) = args["accountId"].as_str() {
            require_provider(requested, self.provider_for_account(account)?)?
        } else {
            requested.unwrap_or(StoreProvider::Booth)
        };
        let account_id = args["accountId"].as_str().map(str::to_owned);
        let retried = if action == "retry_sync" {
            self.catalogue
                .get("job", args["jobId"].as_str().unwrap_or(""))?
        } else {
            None
        };
        if action == "sync"
            || (action == "retry_sync"
                && retried
                    .as_ref()
                    .is_some_and(|job| !job["type"].as_str().unwrap_or("").ends_with("Login")))
        {
            let id = if let Some(id) = account_id.as_deref() {
                Some(id.to_owned())
            } else {
                retried.and_then(|job| job["accountId"].as_str().map(str::to_owned))
            };
            if let Some(id) = id {
                let account = self
                    .catalogue
                    .get("account", &id)?
                    .ok_or_else(|| anyhow::anyhow!("ACCOUNT_UNAVAILABLE"))?;
                ensure!(self.has_local_session(&account), "AUTH_REQUIRED");
            }
        }
        if action == "account_rename" {
            let id = account_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("ACCOUNT_UNAVAILABLE"))?;
            let name = normalized_account_name(&args["name"])?;
            self.rename_account(id, &name)?;
            let _ = app.emit("nna:changed", crate::ipc::Change::scope("booth"));
            return api::answer(api::AccountRenamed {
                account_id: id.to_owned(),
                name,
            });
        }
        if matches!(action, "account_logout" | "account_remove") {
            self.catalogue.require_account_idle(
                account_id
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("ACCOUNT_UNAVAILABLE"))?,
            )?;
        }
        if action == "account_add" && provider != StoreProvider::Booth {
            let id = uuid::Uuid::new_v4().to_string();
            let name = args["name"]
                .as_str()
                .or_else(|| args["label"].as_str())
                .map(|name| {
                    name.chars()
                        .filter(|c| !c.is_control())
                        .take(200)
                        .collect::<String>()
                })
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| provider.label().into());
            self.catalogue.put(
                "account",
                &id,
                json!({
                    "id":id,"store":provider.label(),"name":name,"label":name,
                    "state":"needsAction","status":"disconnected","identityVerified":false,
                    "createdAt":chrono::Utc::now().to_rfc3339()
                }),
            )?;
            let _ = app.emit("nna:changed", crate::ipc::Change::scope("booth"));
            return api::answer(api::StoreAccount { account_id: id });
        }
        let result = match provider {
            StoreProvider::Booth => {
                if let Some(object) = args.as_object_mut() {
                    object.remove("provider");
                    if action == "account_add" && object.contains_key("store") {
                        object.insert("store".into(), json!("BOOTH"));
                    } else {
                        object.remove("store");
                    }
                }
                self.booth.command(app, action, args).await
            }
            StoreProvider::Gumroad => self.gumroad.command(app, action, args).await,
            StoreProvider::Jinxxy => self.jinxxy.command(app, action, args).await,
        };
        if result.is_err() && matches!(action, "account_login" | "login") {
            if let Some(id) = account_id {
                if self.catalogue.discard_unverified_account(&id)? {
                    let _ = app.emit("nna:changed", crate::ipc::Change::scope("booth"));
                }
            }
        }
        result
    }

    pub async fn download_ticket(&self, app: &AppHandle, file_id: &str) -> Result<DownloadTicket> {
        let file = self
            .catalogue
            .get("remote_file", file_id)?
            .ok_or_else(|| anyhow::anyhow!("FILE_UNAVAILABLE"))?;
        let account = file["accountId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("ACCOUNT_UNAVAILABLE"))?;
        let provider = self.provider_for_account(account)?;
        let record = self
            .catalogue
            .get("account", account)?
            .ok_or_else(|| anyhow::anyhow!("ACCOUNT_UNAVAILABLE"))?;
        ensure!(self.has_local_session(&record), "AUTH_REQUIRED");
        let product_id = file["productId"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("PRODUCT_UNAVAILABLE"))?;
        let product = self
            .catalogue
            .get("product", product_id)?
            .ok_or_else(|| anyhow::anyhow!("PRODUCT_UNAVAILABLE"))?;
        require_provider(
            Some(provider),
            StoreProvider::parse(
                product["store"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("STORE_UNAVAILABLE"))?,
            )?,
        )?;
        let result = match provider {
            StoreProvider::Booth => self
                .booth
                .download_ticket(app, file_id)
                .await
                .map(|ticket| DownloadTicket {
                    provider,
                    url: ticket.url,
                    client: ticket.client,
                    expected_size: ticket.expected_size,
                    expected_origin: None,
                }),
            StoreProvider::Gumroad => self.gumroad.download_ticket(app, file_id).await,
            StoreProvider::Jinxxy => self.jinxxy.download_ticket(app, file_id).await,
        };
        // Queued downloads for this account wait for sign-in; the session itself is kept.
        if self.catalogue.library_root().is_some()
            && result
                .as_ref()
                .err()
                .is_some_and(|error| error.to_string() == "AUTH_REQUIRED")
        {
            session::mark_signed_out(&self.catalogue.device_dir(), provider, account)?;
        }
        result
    }
}

fn normalized_account_name(value: &Value) -> Result<String> {
    let name: String = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("INVALID_ACCOUNT_NAME"))?
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    let name: String = name.trim().chars().take(200).collect();
    ensure!(!name.is_empty(), "INVALID_ACCOUNT_NAME");
    Ok(name)
}

#[cfg(test)]
mod tests;
