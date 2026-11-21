//! The actions the UI sends through `booth_action`, which serves every store.
use serde::Deserialize;
use serde_json::{Map, Value};
use ts_rs::TS;

/// One store action. The store services read its arguments and keep some in their jobs
/// exactly as sent, so they travel as they came; only the action is typed here.
#[derive(Deserialize, TS)]
#[serde(tag = "action", rename_all = "snake_case")]
#[ts(export)]
pub enum StoreRequest {
    /// Adds an account of `store` (BOOTH by default), named `name`.
    AccountAdd(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Opens the sign-in of `accountId`.
    AccountLogin(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// The same as `account_login`.
    Login(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    AccountLogout(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    AccountRename(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Removes `accountId`; `removeProducts` also removes its own products.
    AccountRemove(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Reads `accountId`'s purchases: `source` and `mode` choose what and how far.
    Sync(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    CancelSync(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    RetrySync(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Reads the listings of `productIds` again.
    UpdateProducts(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Reads a public product page at `url`.
    FetchProductMetadata(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Debug builds only, like the other `diagnose_` actions.
    DiagnoseOrdering(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    DiagnoseManifests(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    DiagnoseFileStructure(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    DiagnoseFreeItemEntry(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    DiagnoseFreeItemRole(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    DiagnoseFreeItemControls(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    DiagnoseFreeItemIdentity(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    DiagnoseReceiptTemplate(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    DiagnoseReceiptNoticeEnvelope(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    DiagnoseReceiptContactEnvelope(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    DiagnoseProductLineFields(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
}

impl StoreRequest {
    pub fn parse(action: &str, args: Value) -> anyhow::Result<Self> {
        crate::ipc::parse(
            action,
            args,
            "UNKNOWN_STORE_ACTION",
            "INVALID_STORE_REQUEST",
        )
    }
    /// The arguments as sent.
    pub fn args(&self) -> &Map<String, Value> {
        match self {
            Self::AccountAdd(args)
            | Self::AccountLogin(args)
            | Self::Login(args)
            | Self::AccountLogout(args)
            | Self::AccountRename(args)
            | Self::AccountRemove(args)
            | Self::Sync(args)
            | Self::CancelSync(args)
            | Self::RetrySync(args)
            | Self::UpdateProducts(args)
            | Self::FetchProductMetadata(args)
            | Self::DiagnoseOrdering(args)
            | Self::DiagnoseManifests(args)
            | Self::DiagnoseFileStructure(args)
            | Self::DiagnoseFreeItemEntry(args)
            | Self::DiagnoseFreeItemRole(args)
            | Self::DiagnoseFreeItemControls(args)
            | Self::DiagnoseFreeItemIdentity(args)
            | Self::DiagnoseReceiptTemplate(args)
            | Self::DiagnoseReceiptNoticeEnvelope(args)
            | Self::DiagnoseReceiptContactEnvelope(args)
            | Self::DiagnoseProductLineFields(args) => args,
        }
    }
    pub fn into_args(self) -> Value {
        match self {
            Self::AccountAdd(args)
            | Self::AccountLogin(args)
            | Self::Login(args)
            | Self::AccountLogout(args)
            | Self::AccountRename(args)
            | Self::AccountRemove(args)
            | Self::Sync(args)
            | Self::CancelSync(args)
            | Self::RetrySync(args)
            | Self::UpdateProducts(args)
            | Self::FetchProductMetadata(args)
            | Self::DiagnoseOrdering(args)
            | Self::DiagnoseManifests(args)
            | Self::DiagnoseFileStructure(args)
            | Self::DiagnoseFreeItemEntry(args)
            | Self::DiagnoseFreeItemRole(args)
            | Self::DiagnoseFreeItemControls(args)
            | Self::DiagnoseFreeItemIdentity(args)
            | Self::DiagnoseReceiptTemplate(args)
            | Self::DiagnoseReceiptNoticeEnvelope(args)
            | Self::DiagnoseReceiptContactEnvelope(args)
            | Self::DiagnoseProductLineFields(args) => Value::Object(args),
        }
    }
}
