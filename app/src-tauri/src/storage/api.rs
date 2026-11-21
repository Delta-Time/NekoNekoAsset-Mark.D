//! The actions the UI sends through `storage_action`.
use serde::Deserialize;
use serde_json::{Map, Value};
use ts_rs::TS;

/// One storage action. Downloads, imports and operations keep their arguments in their
/// jobs exactly as sent, so they travel as they came; only the action is typed here.
#[derive(Deserialize, TS)]
#[serde(tag = "action", rename_all = "snake_case")]
#[ts(export)]
pub enum StorageRequest {
    /// Copies local files into a new product, or several in batch mode.
    ImportLocal(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Adds and removes the files of a local product.
    LocalFiles(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    DeleteLocalProducts(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Downloads every file of `productIds`, or `selectedFileIds`.
    EnqueueProducts(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Downloads one file, `fileId`.
    EnqueueFile(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    CancelJob(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Cancels the downloads of `productIds`, or all of them.
    CancelDownloads(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    DeleteDownloads(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    RetryJob(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    RetryFile(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Sets the library folder.
    ConfigureRoot(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Points a root at the folder its files moved to.
    RemapRoot(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Moves the library to `path`.
    MigrateRoot(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    CleanupOldRoot(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    VerifyRoot(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    ExportBackup(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    RestoreBackup(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Packs the files of `productIds` into a ZIP at `path`.
    ExportZip(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Whether a product's folder and files can be opened.
    ProductAvailability(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Used and free bytes of a root's drive.
    RootUsage(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    OpenProductFolder(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    OpenFile(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    OpenRootFolder(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
    /// Initializes the App, deleting the library when `deleteLibrary`.
    ResetApp(#[ts(type = "Record<string, unknown>")] Map<String, Value>),
}

impl StorageRequest {
    pub fn parse(action: &str, args: Value) -> anyhow::Result<Self> {
        crate::ipc::parse(
            action,
            args,
            "unsupported_storage_action",
            "invalid_storage_request",
        )
    }
    pub fn into_args(self) -> Value {
        match self {
            Self::ImportLocal(args)
            | Self::LocalFiles(args)
            | Self::DeleteLocalProducts(args)
            | Self::EnqueueProducts(args)
            | Self::EnqueueFile(args)
            | Self::CancelJob(args)
            | Self::CancelDownloads(args)
            | Self::DeleteDownloads(args)
            | Self::RetryJob(args)
            | Self::RetryFile(args)
            | Self::ConfigureRoot(args)
            | Self::RemapRoot(args)
            | Self::MigrateRoot(args)
            | Self::CleanupOldRoot(args)
            | Self::VerifyRoot(args)
            | Self::ExportBackup(args)
            | Self::RestoreBackup(args)
            | Self::ExportZip(args)
            | Self::ProductAvailability(args)
            | Self::RootUsage(args)
            | Self::OpenProductFolder(args)
            | Self::OpenFile(args)
            | Self::OpenRootFolder(args)
            | Self::ResetApp(args) => Value::Object(args),
        }
    }
}
