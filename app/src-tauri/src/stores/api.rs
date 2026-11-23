//! The actions the UI sends through `booth_action`, which serves every store, and what
//! they answer.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

/// One store action. The store services read the arguments they need and some jobs keep
/// what was sent, so every argument struct keeps the other fields it was sent and gives
/// the arguments back exactly as they came (`args`).
#[derive(Serialize, Deserialize, TS)]
#[serde(tag = "action", rename_all = "snake_case")]
#[ts(export)]
pub enum StoreRequest {
    /// Adds an account of `store` (BOOTH by default).
    AccountAdd(AccountAdd),
    /// Opens the store's sign-in window for the account.
    AccountLogin(AccountRef),
    /// Signs the account out and keeps what it synchronized.
    AccountLogout(AccountRef),
    AccountRename(AccountRename),
    AccountRemove(AccountRemove),
    Sync(SyncRequest),
    CancelSync(SyncJob),
    RetrySync(SyncJob),
    /// Reads the current store pages of `productIds`.
    UpdateProducts(UpdateProducts),
    /// Adds BOOTH listings by what the account can download from them for free.
    ImportFree(ImportFree),
    /// Reads a public product page for a local product's metadata.
    FetchProductMetadata(ProductLink),
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountAdd {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// An older name of `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub store: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountRef {
    pub account_id: String,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountRename {
    pub account_id: String,
    pub name: String,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// Removes the account, and with `removeProducts` the products only it holds.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountRemove {
    pub account_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub remove_products: Option<bool>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// Synchronizes an account's `source` listings (every source by default).
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SyncRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(
        optional,
        type = "\"all\" | \"library\" | \"purchase\" | \"gift\" | \"free\""
    )]
    pub source: Option<String>,
    /// `incremental` stops at the first listing page it already knows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "\"incremental\" | \"full\" | \"forced\"")]
    pub mode: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SyncJob {
    pub job_id: String,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProducts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub product_ids: Option<Vec<String>>,
    /// An older name of `productIds`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ids: Option<Vec<String>>,
    /// Limits the update to this account's view of the products.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub account_id: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// BOOTH item IDs, in the order the user listed them, and the account that takes them.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImportFree {
    pub account_id: String,
    pub product_ids: Vec<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProductLink {
    pub url: String,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// The account an action added or signed out.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StoreAccount {
    pub account_id: String,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AccountRenamed {
    pub account_id: String,
    pub name: String,
}

/// The job a sign-in or synchronization started, or the one already doing it.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StoreJob {
    pub job_id: String,
}

/// The update jobs that cover the products; `jobId` is the first of them.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StoreJobs {
    pub job_id: Option<String>,
    pub job_ids: Vec<String>,
}

/// A public product page's metadata, for a local product.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProductMetadata {
    pub source_url: String,
    pub title: String,
    pub category: String,
    pub tags: Vec<String>,
    pub thumbnail_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub listing_price: Option<ListingPrice>,
}

/// The price a public listing states; never an owned quote.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ListingPrice {
    pub price: Option<serde_json::Number>,
    pub price_currency: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub price_formatted: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub price_source_currency: Option<Option<String>>,
}

/// The answer to each store action, for the typed store calls in the UI.
#[cfg(test)]
#[derive(Serialize, TS)]
#[ts(export)]
struct StoreResponses {
    account_add: StoreAccount,
    account_login: StoreJob,
    account_logout: StoreAccount,
    account_rename: AccountRenamed,
    account_remove: crate::catalogue::api::AccountRemoved,
    sync: StoreJob,
    cancel_sync: StoreJob,
    retry_sync: StoreJob,
    update_products: StoreJobs,
    import_free: StoreJob,
    fetch_product_metadata: ProductMetadata,
}

impl StoreRequest {
    pub fn parse(action: &str, args: Value) -> anyhow::Result<Self> {
        #[cfg(test)]
        let sent = args.clone();
        let request: Self = crate::ipc::parse(
            action,
            args,
            "UNKNOWN_STORE_ACTION",
            "INVALID_STORE_REQUEST",
        )?;
        #[cfg(test)]
        crate::ipc::assert_round_trip("store request", &sent, &request.args());
        Ok(request)
    }
    /// The arguments exactly as they were sent.
    pub fn args(&self) -> Value {
        crate::ipc::args_of(self)
    }
    /// The job a cancel or retry names.
    pub fn job_id(&self) -> Option<&str> {
        match self {
            Self::CancelSync(job) | Self::RetrySync(job) => Some(&job.job_id),
            _ => None,
        }
    }
    /// The account a sign-in names.
    pub fn login_account(&self) -> Option<&str> {
        match self {
            Self::AccountLogin(account) => Some(&account.account_id),
            _ => None,
        }
    }
}

/// An answer's JSON.
pub fn answer(value: impl Serialize) -> anyhow::Result<Value> {
    Ok(serde_json::to_value(value)?)
}

impl StoreJob {
    /// The answer naming a job record.
    pub fn of(job: &Value) -> anyhow::Result<Value> {
        answer(Self {
            job_id: job_id(&job["id"])?,
        })
    }
}

impl StoreJobs {
    /// The answer naming job ids, the first as `jobId`.
    pub fn of(ids: Vec<Value>) -> anyhow::Result<Value> {
        let job_ids = ids.iter().map(job_id).collect::<anyhow::Result<Vec<_>>>()?;
        answer(Self {
            job_id: job_ids.first().cloned(),
            job_ids,
        })
    }
}

fn job_id(id: &Value) -> anyhow::Result<String> {
    id.as_str()
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("JOB_UNAVAILABLE"))
}
