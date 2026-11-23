//! The actions the UI sends through `storage_action`, and what they answer.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

/// One storage action. Downloads, imports and operations keep their arguments in their
/// jobs, and a local import fingerprints them, so every argument struct keeps the other
/// fields it was sent and gives the arguments back exactly as they came (`args`).
#[derive(Serialize, Deserialize, TS)]
#[serde(tag = "action", rename_all = "snake_case")]
#[ts(export)]
pub enum StorageRequest {
    /// Copies local files into a new product, or one product per source in batch mode.
    ImportLocal(LocalImport),
    /// Adds and removes the files of a product's own copies.
    LocalFiles(LocalFiles),
    DeleteLocalProducts(ProductIds),
    /// Downloads every file of `productIds`, or `selectedFileIds` of one product.
    EnqueueProducts(Downloads),
    EnqueueFile(FileDownload),
    CancelJob(StorageJob),
    /// Cancels the downloads of `productIds`, or all of them.
    CancelDownloads(OptionalProductIds),
    DeleteDownloads(DownloadRemoval),
    RetryJob(Retry),
    /// Sets the library folder.
    ConfigureRoot(FolderPath),
    /// Points a root at the folder its files moved to.
    RemapRoot(RootMove),
    /// Moves the library to `path`.
    MigrateRoot(RootMove),
    CleanupOldRoot(RootRef),
    ExportBackup(FolderPath),
    RestoreBackup(FolderPath),
    /// Packs the files of `productIds` into a ZIP at `path`.
    ExportZip(ZipExport),
    /// Whether a product's folder and files can be opened.
    ProductAvailability(ProductRef),
    RootUsage(RootRef),
    OpenProductFolder(ProductRef),
    OpenFile(FileRef),
    OpenRootFolder(RootRef),
    /// Clears this device's settings, and with `deleteLibrary` the library too.
    ResetApp(Reset),
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LocalImport {
    /// The draft a repeated submission resumes.
    pub draft_id: String,
    #[ts(type = "\"single\" | \"batch\"")]
    pub mode: String,
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub root_id: Option<String>,
    #[serde(flatten)]
    pub draft: LocalDraft,
    /// Batch mode's drafts, one per source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub products: Option<Vec<BatchDraft>>,
}

/// A batch draft for one source.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BatchDraft {
    pub source: String,
    #[serde(flatten)]
    pub draft: LocalDraft,
}

/// What a local product starts with.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LocalDraft {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub category_manual: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_url: Option<String>,
    /// A chosen image, relative to the sources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub thumbnail_path: Option<String>,
    /// The source page's image, or `null` for none.
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub thumbnail_url: Option<Option<String>>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LocalFiles {
    pub product_id: String,
    pub draft_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub add_sources: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub remove_file_ids: Option<Vec<String>>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProductIds {
    pub product_ids: Vec<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OptionalProductIds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub product_ids: Option<Vec<String>>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Downloads {
    pub product_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub root_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub selected_file_ids: Option<Option<Vec<String>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub redownload: Option<bool>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileDownload {
    pub file_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub redownload: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub root_id: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StorageJob {
    pub job_id: String,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// Deletes the downloaded files of `productIds`, or one file.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRemoval {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub product_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub file_id: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// Retries a job, or a file's download.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Retry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub file_id: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FolderPath {
    pub path: String,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RootMove {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub root_id: Option<String>,
    pub path: String,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// A root, the active one when left out.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RootRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub root_id: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ZipExport {
    pub product_ids: Vec<String>,
    pub path: String,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProductRef {
    pub product_id: String,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileRef {
    pub file_id: String,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Reset {
    pub delete_library: bool,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// The import or file-change job and the products it writes.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LocalJob {
    pub job_id: String,
    pub product_ids: Vec<String>,
    /// The job's state when a repeated submission resumed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub state: Option<String>,
}

/// The download jobs that cover the request, new or already queued.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DownloadJobs {
    pub job_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DeletedProducts {
    pub deleted_product_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DeletedFiles {
    pub deleted_file_ids: Vec<String>,
}

/// Whether the job was still running and is now asked to stop.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CancelRequested {
    pub cancel_requested: bool,
}

/// How many downloads were asked to stop.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct DownloadsCancelled {
    pub cancelled: usize,
}

/// The job a storage operation started.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OperationJob {
    pub job_id: String,
}

/// A portable library moved at once; the App restarts into it.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LibraryMoved {
    pub library_path: String,
    pub restart_required: bool,
    pub file_count: usize,
    pub total_bytes: u64,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProductAvailability {
    pub folder_available: bool,
    pub openable_file_ids: Vec<String>,
    #[ts(type = "\"online\" | \"offline\" | \"unconfigured\"")]
    pub root_status: &'static str,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RootUsage {
    pub used_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct Opened {
    pub opened: bool,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RestartRequired {
    pub restart_required: bool,
}

/// What a retry answers: downloads for a download or file, the import job for an import,
/// and the deletion for a local product deletion.
#[cfg(test)]
#[derive(Serialize, TS)]
#[serde(untagged)]
#[allow(dead_code)] // Only describes the answer to TypeScript.
enum Retried {
    Downloads(DownloadJobs),
    Import(LocalJob),
    Deleted(DeletedProducts),
}

/// What moving the library answers: its job, or a moved portable library.
#[cfg(test)]
#[derive(Serialize, TS)]
#[serde(untagged)]
#[allow(dead_code)] // Only describes the answer to TypeScript.
enum Moved {
    Job(OperationJob),
    Library(LibraryMoved),
}

/// The answer to each storage action, for the typed storage calls in the UI.
#[cfg(test)]
#[derive(Serialize, TS)]
#[ts(export)]
struct StorageResponses {
    import_local: LocalJob,
    local_files: LocalJob,
    delete_local_products: DeletedProducts,
    enqueue_products: DownloadJobs,
    enqueue_file: DownloadJobs,
    cancel_job: CancelRequested,
    cancel_downloads: DownloadsCancelled,
    delete_downloads: DeletedFiles,
    retry_job: Retried,
    configure_root: crate::catalogue::api::Root,
    remap_root: OperationJob,
    migrate_root: Moved,
    cleanup_old_root: OperationJob,
    export_backup: OperationJob,
    restore_backup: OperationJob,
    export_zip: OperationJob,
    product_availability: ProductAvailability,
    root_usage: RootUsage,
    open_product_folder: Opened,
    open_file: Opened,
    open_root_folder: Opened,
    reset_app: RestartRequired,
}

impl StorageRequest {
    pub fn parse(action: &str, args: Value) -> anyhow::Result<Self> {
        #[cfg(test)]
        let sent = args.clone();
        let request: Self = crate::ipc::parse(
            action,
            args,
            "unsupported_storage_action",
            "invalid_storage_request",
        )?;
        #[cfg(test)]
        crate::ipc::assert_round_trip("storage request", &sent, &request.args());
        Ok(request)
    }
    /// The arguments exactly as they were sent.
    pub fn args(&self) -> Value {
        crate::ipc::args_of(self)
    }
}

/// An answer's JSON.
pub fn answer(value: impl Serialize) -> anyhow::Result<Value> {
    Ok(serde_json::to_value(value)?)
}

/// Strings from a JSON array of ids, as job records hold them.
pub fn ids(values: &Value) -> Vec<String> {
    values
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
