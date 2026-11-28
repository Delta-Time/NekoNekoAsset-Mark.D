// Functions outside tests stay under the line limit in clippy.toml; test scenarios may run long.
#![cfg_attr(not(test), warn(clippy::too_many_lines))]

mod ai;
mod app_reset;
mod booth;
mod catalogue;
mod ipc;
mod library_lock;
mod licenses;
mod native;
mod preview;
mod profile;
mod setup_sessions;
#[cfg(test)]
mod setup_tests;
mod storage;
mod stores;
mod support;
mod support_trace;
mod taxonomy;
#[cfg(test)]
mod test_dir;
mod unity;
mod updates;
mod verification;
mod window_state;

use anyhow::Context;
use booth::BoothService;
use catalogue::Catalogue;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use storage::StorageService;
use stores::StoreService;
use tauri::{Emitter, Listener, Manager};

#[derive(Clone)]
struct Services {
    setup_only: bool,
    ai: Arc<ai::AiService>,
    ai_jobs: Arc<ai::jobs::AiJobs>,
    ai_suggestions: Arc<ai::suggestions::AiSuggestions>,
    catalogue: Catalogue,
    stores: Arc<StoreService>,
    storage: Arc<StorageService>,
    previews: Arc<preview::PreviewCache>,
    unity: Arc<unity::UnityService>,
    licenses: licenses::LicenseService,
}

impl Services {
    fn new(catalogue: Catalogue, device_dir: &std::path::Path) -> anyhow::Result<Self> {
        Self::build(catalogue, device_dir, None)
    }

    fn build(
        catalogue: Catalogue,
        device_dir: &std::path::Path,
        previous: Option<&StoreService>,
    ) -> anyhow::Result<Self> {
        let setup_only = catalogue
            .get("preferences", "app")?
            .is_none_or(|preferences| preferences["setupComplete"] != true);
        let ai = Arc::new(ai::AiService::new(device_dir)?);
        let ai_suggestions = Arc::new(ai::suggestions::AiSuggestions::new(
            catalogue.clone(),
            ai.clone(),
        )?);
        let stores = Arc::new(match previous {
            Some(previous) => previous.rebind_catalogue(catalogue.clone())?,
            None => StoreService::new(
                catalogue.clone(),
                Arc::new(BoothService::new(catalogue.clone())?),
            )?,
        });
        let storage = Arc::new(StorageService::new(catalogue.clone(), stores.clone())?);
        let unity = Arc::new(unity::UnityService::new(
            catalogue.clone(),
            storage.clone(),
        )?);
        let previews = Arc::new(preview::PreviewCache::new(catalogue.data_dir())?);
        let ai_jobs = Arc::new(ai::jobs::AiJobs::new(
            catalogue.clone(),
            ai.clone(),
            storage.clone(),
            previews.clone(),
        ));
        let licenses = licenses::LicenseService::new(catalogue.clone(), stores.booth());
        Ok(Self {
            setup_only,
            ai,
            ai_jobs,
            ai_suggestions,
            catalogue,
            stores,
            storage,
            previews,
            unity,
            licenses,
        })
    }
}

/// Whether a library is open, being set up, missing or blocked, for the shell to show.
#[derive(Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
struct LibraryStatus {
    #[ts(type = "\"ready\" | \"setup\" | \"unconfigured\" | \"blocked\"")]
    state: String,
    path: Option<PathBuf>,
    default_path: Option<PathBuf>,
    error_code: Option<String>,
    locale: String,
    /// Connected accounts this device has no sign-in for.
    missing_credentials: Vec<LibraryCredential>,
}

#[derive(Clone, Serialize, ts_rs::TS)]
#[ts(export)]
struct LibraryCredential {
    id: String,
    name: Option<String>,
    store: Option<String>,
}

/// Whether opening the library restarts the App into it.
#[derive(Serialize, ts_rs::TS)]
#[ts(export)]
struct LibraryOpened {
    restarting: bool,
}

struct LibraryHost {
    device_dir: PathBuf,
    status: Mutex<LibraryStatus>,
    services: Mutex<Option<Services>>,
    setup: Mutex<Option<SetupState>>,
    opening: AtomicBool,
    // Keep both the destination and source protected until restart ends all workers.
    next_catalogue: Mutex<Option<Catalogue>>,
    switching: Mutex<Option<catalogue::CatalogueWorkPermit>>,
}

#[derive(Clone)]
struct SetupState {
    catalogue: Catalogue,
    stores: Arc<StoreService>,
    candidate: Arc<Mutex<PathBuf>>,
    initial_preferences: Value,
    sessions: Arc<setup_sessions::SetupSessions>,
}

impl SetupState {
    fn new(device_dir: &std::path::Path, candidate: PathBuf, locale: &str) -> anyhow::Result<Self> {
        let catalogue = Catalogue::open_setup(device_dir)?;
        let sessions = Arc::new(setup_sessions::SetupSessions::new(device_dir)?);
        catalogue.command("preferences", json!({"patch":{"locale":locale}}))?;
        let initial_preferences = catalogue
            .get("preferences", "app")?
            .context("catalogue.preferences_missing")?;
        let stores = Arc::new(StoreService::new(
            catalogue.clone(),
            Arc::new(BoothService::new(catalogue.clone())?),
        )?);
        Ok(Self {
            catalogue,
            stores,
            candidate: Arc::new(Mutex::new(candidate)),
            initial_preferences,
            sessions,
        })
    }

    fn candidate(&self) -> Result<PathBuf, UiError> {
        self.candidate
            .lock()
            .map(|candidate| candidate.clone())
            .map_err(|_| UiError {
                code: "library_unavailable".into(),
            })
    }
}

impl LibraryHost {
    fn status_snapshot(&self) -> Result<LibraryStatus, UiError> {
        self.status
            .lock()
            .map(|status| status.clone())
            .map_err(|_| UiError {
                code: "library_unavailable".into(),
            })
    }
    fn begin_opening(&self) -> Result<(), UiError> {
        if self.opening.swap(true, Ordering::AcqRel) {
            return Err(UiError {
                code: "library_opening".into(),
            });
        }
        Ok(())
    }

    fn active_services(&self) -> Result<Option<Services>, UiError> {
        self.services
            .lock()
            .map(|services| services.clone())
            .map_err(|_| UiError {
                code: "library_unavailable".into(),
            })
    }

    fn active_setup(&self) -> Result<Option<SetupState>, UiError> {
        self.setup
            .lock()
            .map(|setup| setup.clone())
            .map_err(|_| UiError {
                code: "library_unavailable".into(),
            })
    }

    fn services(&self) -> Result<Services, UiError> {
        if self.opening.load(Ordering::Acquire) {
            return Err(UiError {
                code: "library_opening".into(),
            });
        }
        self.active_services()?.ok_or_else(|| UiError {
            code: self
                .status_snapshot()
                .ok()
                .and_then(|status| status.error_code)
                .unwrap_or_else(|| "library_not_open".into()),
        })
    }
}

#[derive(Debug, Serialize)]
struct UiError {
    code: String,
}
impl UiError {
    /// Library setup exposes a cause and recovery action, without raw OS/SQLite
    /// messages or paths. Inspect typed sources before falling back to stable codes.
    fn from_library_error(error: &anyhow::Error) -> Self {
        if error
            .chain()
            .any(|cause| cause.to_string() == "library_preferences_write_failed")
        {
            return Self {
                code: "library_preferences_write_failed".into(),
            };
        }
        let io_code =
            error
                .downcast_ref::<std::io::Error>()
                .and_then(|source| match source.kind() {
                    std::io::ErrorKind::PermissionDenied => Some("library_permission_denied"),
                    std::io::ErrorKind::StorageFull => Some("library_disk_full"),
                    std::io::ErrorKind::NotADirectory => Some("library_path_not_directory"),
                    _ => None,
                });
        let sqlite_code = error
            .downcast_ref::<rusqlite::Error>()
            .map(|source| match source {
                rusqlite::Error::SqliteFailure(failure, _) => match failure.code {
                    rusqlite::ErrorCode::PermissionDenied | rusqlite::ErrorCode::ReadOnly => {
                        "library_permission_denied"
                    }
                    rusqlite::ErrorCode::DiskFull => "library_disk_full",
                    _ => "library_database_unavailable",
                },
                _ => "library_database_unavailable",
            });
        if let Some(code) = io_code.or(sqlite_code) {
            return Self { code: code.into() };
        }
        let mut result = Self::from_error(anyhow::anyhow!(error.to_string()));
        if matches!(
            result.code.as_str(),
            "storage.create_directory_failed" | "storage_path_unavailable" | "operation_failed"
        ) {
            result.code = "library_create_failed".into();
        }
        result
    }

    /// A library move names the failed side and a recovery action, without raw
    /// OS messages or paths. Typed I/O causes outrank the operation's code.
    fn from_move_error(error: &anyhow::Error) -> Self {
        let code = Self::from_error(anyhow::anyhow!(error.to_string())).code;
        let destination = matches!(
            code.as_str(),
            "storage.destination_unavailable"
                | "storage_link_forbidden"
                | "storage_path_unavailable"
                | "library_lock_unavailable"
                | "storage.create_directory_failed"
                | "storage.migration_create_failed"
                | "storage.write_failed"
                | "storage.migration_flush_failed"
                | "storage.export_publish_failed"
        );
        let source = matches!(
            code.as_str(),
            "storage.read_failed"
                | "storage.file_unavailable"
                | "storage.file_missing"
                | "storage.metadata_failed"
                | "storage.not_regular_file"
                | "storage.source_changed"
                | "storage.source_hash_mismatch"
        );
        let cause = error
            .downcast_ref::<std::io::Error>()
            .map(|source| (source.kind(), source.raw_os_error()));
        let mapped = match cause {
            // Windows sharing and byte-range lock violations.
            Some((_, Some(32 | 33))) if cfg!(windows) => "storage.move_file_in_use",
            Some((std::io::ErrorKind::StorageFull, _)) => "library_disk_full",
            Some((std::io::ErrorKind::PermissionDenied, _)) if destination => {
                "library_permission_denied"
            }
            _ if code == "storage.low_space" => "library_disk_full",
            _ if destination => "library_create_failed",
            _ if source => "storage.move_source_unreadable",
            _ => return Self { code },
        };
        Self {
            code: mapped.into(),
        }
    }

    fn from_error(error: anyhow::Error) -> Self {
        let raw = error.to_string();
        let first = raw.split(':').next().unwrap_or("operation_failed").trim();
        let code = if !first.is_empty()
            && first.len() <= 80
            && first
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            first.to_owned()
        } else {
            "operation_failed".to_owned()
        };
        Self { code }
    }
}
#[derive(Default)]
struct NativeOperationGate(Arc<tokio::sync::RwLock<()>>);

fn trusted_access(window: &tauri::WebviewWindow) -> Result<(), UiError> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err(UiError {
            code: "access_denied".into(),
        })
    }
}
fn trusted(
    window: &tauri::WebviewWindow,
) -> Result<tokio::sync::OwnedRwLockReadGuard<()>, UiError> {
    trusted_access(window)?;
    window
        .app_handle()
        .state::<NativeOperationGate>()
        .0
        .clone()
        .try_read_owned()
        .map_err(|_| UiError {
            code: "updates_busy".into(),
        })
}
fn changed(app: &tauri::AppHandle, scope: &str) {
    let _ = app.emit("nna:changed", crate::ipc::Change::scope(scope));
}

async fn sync_connected_accounts(
    app: &tauri::AppHandle,
    catalogue: &Catalogue,
    stores: &StoreService,
) -> Result<(), UiError> {
    let _work = catalogue
        .begin_catalogue_work()
        .map_err(UiError::from_error)?;
    for account in catalogue.list("account").map_err(UiError::from_error)? {
        if account["disconnected"] == true
            || account["status"] != "connected"
            || !stores.has_local_session(&account)
        {
            continue;
        }
        let Some(id) = account["id"].as_str() else {
            continue;
        };
        let active = catalogue
            .list("job")
            .map_err(UiError::from_error)?
            .iter()
            .any(|job| {
                let kind = job["type"].as_str().unwrap_or("").to_ascii_lowercase();
                job["accountId"] == id
                    && (kind.contains("sync") || kind.contains("update"))
                    && matches!(job["state"].as_str(), Some("queued" | "running"))
            });
        if active {
            continue;
        }
        if let Err(error) = stores
            .command(
                app,
                "sync",
                json!({"accountId":id,"source":"all","mode":"incremental"}),
            )
            .await
        {
            // Setup has already committed. Retain an actionable sync failure
            // rather than falsely reporting that configuring the library failed.
            let provider = account["store"]
                .as_str()
                .unwrap_or("BOOTH")
                .to_ascii_lowercase();
            let (kind, store) = match provider.as_str() {
                "gumroad" => ("gumroadSync", "Gumroad"),
                "jinxxy" => ("jinxxySync", "Jinxxy"),
                _ => ("boothSync", "BOOTH"),
            };
            let job_id = uuid::Uuid::new_v4().to_string();
            let timestamp = chrono::Utc::now().to_rfc3339();
            catalogue.put("job", &job_id, json!({"id":job_id,"type":kind,"store":store,"accountId":id,"args":{"accountId":id,"source":"all","mode":"incremental"},"state":"failed","progress":{"stage":"failed"},"errorCode":UiError::from_error(error).code,"createdAt":timestamp,"updatedAt":timestamp,"completedAt":timestamp})).map_err(UiError::from_error)?;
        }
    }
    changed(app, "jobs");
    Ok(())
}

#[tauri::command]
async fn ai_action(
    window: tauri::WebviewWindow,
    services: tauri::State<'_, LibraryHost>,
    request: ai::AiRequest,
) -> Result<ai::AiResponse, UiError> {
    let _operation = trusted(&window)?;
    let services = services.services()?;
    if services
        .catalogue
        .get("preferences", "app")
        .map_err(UiError::from_error)?
        .is_none_or(|preferences| preferences["aiFeaturesEnabled"] != true)
    {
        return Err(UiError {
            code: "ai_features_disabled".into(),
        });
    }
    services
        .ai
        .command(request)
        .await
        .map_err(UiError::from_error)
}

#[tauri::command]
async fn ai_analysis_action(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    services: tauri::State<'_, LibraryHost>,
    request: ai::jobs::AiJobRequest,
) -> Result<Value, UiError> {
    let _operation = trusted(&window)?;
    let services = services.services()?;
    services
        .ai_jobs
        .command(&app, request)
        .await
        .map_err(UiError::from_error)
}

#[tauri::command]
async fn ai_suggestion_action(
    window: tauri::WebviewWindow,
    services: tauri::State<'_, LibraryHost>,
    request: ai::suggestions::AiSuggestionRequest,
) -> Result<Value, UiError> {
    let _operation = trusted(&window)?;
    let services = services.services()?;
    services
        .ai_suggestions
        .command(request)
        .await
        .map_err(UiError::from_error)
}

#[tauri::command]
async fn catalogue_action(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    services: tauri::State<'_, LibraryHost>,
    action: String,
    args: Value,
) -> Result<Value, UiError> {
    use catalogue::api::CatalogueRequest as Request;
    let _operation = trusted(&window)?;
    let mut request = Request::parse(&action, args).map_err(UiError::from_error)?;
    // A preference change, and whether it finishes setup.
    let patch = match &request {
        Request::Preferences { patch } => Some(patch.clone()),
        _ => None,
    };
    let finishing = patch
        .as_ref()
        .is_some_and(|patch| patch.get("setupComplete") == Some(&Value::Bool(true)));
    if let Some(setup) = services.active_setup()? {
        return staged_setup_action(app, &services, setup, request, &patch, finishing).await;
    }
    let status = services.status_snapshot()?;
    if status.state == "unconfigured"
        && services.active_services()?.is_none()
        && matches!(request, Request::Bootstrap { .. } | Request::State)
    {
        return unconfigured_bootstrap(status.locale);
    }
    let host = services;
    let services = host.services()?;
    if services.setup_only && finishing {
        return finish_setup(app, &host, services, json!({"patch":patch})).await;
    }
    let catalogue = services.catalogue.clone();
    let invalidates = !request.reads_only();
    let _work = if invalidates {
        Some(
            services
                .catalogue
                .begin_catalogue_work()
                .map_err(UiError::from_error)?,
        )
    } else {
        None
    };
    let previous_preferences = if patch.is_some() {
        catalogue
            .get("preferences", "app")
            .map_err(UiError::from_error)?
            .unwrap_or(json!({}))
    } else {
        Value::Null
    };
    let patched = |key: &str| patch.as_ref().and_then(|patch| patch.get(key));
    let reevaluate_automatic = patched("autoDownloadAll")
        .and_then(Value::as_bool)
        .is_some_and(|next| {
            next != previous_preferences["autoDownloadAll"]
                .as_bool()
                .unwrap_or(false)
        });
    let completes_setup = finishing && previous_preferences["setupComplete"] != true;
    let avatar_settings_changed = patched("avatarSettings")
        .is_some_and(|next| *next != previous_preferences["avatarSettings"]);
    let mutation_scope = match request {
        Request::Collection(_) => "collections",
        Request::ClearActivity => "catalogue",
        Request::Preferences { .. } if !avatar_settings_changed => "state",
        _ => "catalogue",
    };
    let account_view = matches!(request, Request::Bootstrap { .. } | Request::State);
    if let Request::Query(query) = &mut request {
        // The license database answers which products a license filter keeps.
        if let Some(ids) = services
            .licenses
            .matching(&json!(query.license))
            .map_err(UiError::from_error)?
        {
            query.license_ids = Some(ids);
        }
    }
    let mut result = tauri::async_runtime::spawn_blocking(move || catalogue.execute(request))
        .await
        .map_err(|_| UiError {
            code: "worker_failed".into(),
        })?
        .map_err(UiError::from_error)?;
    if account_view {
        if let Some(accounts) = result["accounts"].as_array() {
            result["accounts"] = json!(services.stores.account_views(accounts.clone()));
        }
    }
    if let Some(links) = result
        .as_object_mut()
        .and_then(|result| result.remove("changedLinks"))
    {
        let links: Vec<String> = serde_json::from_value(links).unwrap_or_default();
        if !links.is_empty() {
            services.storage.read_changed_link_prices(&app, links);
        }
    }
    if previous_preferences.is_object()
        && (previous_preferences["aiEnabled"] != result["aiEnabled"]
            || previous_preferences["aiFeaturesEnabled"] != result["aiFeaturesEnabled"])
    {
        services
            .ai_jobs
            .reset_automatic_baseline()
            .await
            .map_err(UiError::from_error)?;
    }
    if previous_preferences["licenseFilter"] == true && result["licenseFilter"] == false {
        services.licenses.cancel(None);
    }
    if previous_preferences["aiFeaturesEnabled"] == true && result["aiFeaturesEnabled"] == false {
        services
            .ai_jobs
            .cancel_all(&app)
            .await
            .map_err(UiError::from_error)?;
        services.ai_suggestions.cancel_all().await;
    }
    if invalidates {
        changed(&app, mutation_scope);
    }
    if completes_setup {
        sync_connected_accounts(&app, &services.catalogue, &services.stores).await?;
    }
    if reevaluate_automatic && !services.setup_only {
        let storage = services.storage.clone();
        tauri::async_runtime::spawn(async move {
            let _ = storage.reevaluate_automatic().await;
        });
    }
    Ok(result)
}

/// A catalogue request while setup is staged: only its reads and preferences answer.
async fn staged_setup_action(
    app: tauri::AppHandle,
    services: &LibraryHost,
    setup: SetupState,
    request: catalogue::api::CatalogueRequest,
    patch: &Option<serde_json::Map<String, Value>>,
    finishing: bool,
) -> Result<Value, UiError> {
    use catalogue::api::CatalogueRequest as Request;
    if services.opening.load(Ordering::Acquire) {
        return Err(UiError {
            code: "library_opening".into(),
        });
    }
    if finishing {
        return finish_staged_setup(app, services, setup, json!({"patch":patch})).await;
    }
    if !matches!(
        request,
        Request::Bootstrap { .. } | Request::State | Request::Preferences { .. }
    ) {
        return Err(UiError {
            code: "library_not_open".into(),
        });
    }
    let catalogue = setup.catalogue.clone();
    let mutation = patch.is_some();
    let _work = mutation
        .then(|| setup.catalogue.begin_catalogue_work())
        .transpose()
        .map_err(UiError::from_error)?;
    let mut result = tauri::async_runtime::spawn_blocking(move || catalogue.execute(request))
        .await
        .map_err(|_| UiError {
            code: "worker_failed".into(),
        })?
        .map_err(UiError::from_error)?;
    if let Some(accounts) = result["accounts"].as_array() {
        result["accounts"] = json!(setup.stores.account_views(accounts.clone()));
    }
    if mutation {
        changed(&app, "state");
    }
    Ok(result)
}

/// What the App shows before any library exists.
fn unconfigured_bootstrap(locale: String) -> Result<Value, UiError> {
    let empty = catalogue::api::Bootstrap {
        version: 1,
        total: 0,
        counts: Default::default(),
        products: vec![],
        state: catalogue::api::StateSnapshot {
            accounts: vec![],
            roots: vec![],
            jobs: vec![],
            preferences: Some(
                json!({"locale":locale,"setupComplete":false,"autoDownloadAll":false}),
            ),
            source_coverage: vec![],
        },
        collections: vec![],
        saved_searches: vec![],
        custom_categories: vec![],
        tag_options: json!([]),
        avatar_options: vec![],
        owned_base_options: vec![],
    };
    serde_json::to_value(empty).map_err(|error| UiError::from_error(error.into()))
}
#[tauri::command]
async fn booth_action(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    services: tauri::State<'_, LibraryHost>,
    action: String,
    args: Value,
) -> Result<Value, UiError> {
    use stores::api::StoreRequest as Request;
    let _operation = trusted(&window)?;
    let request = Request::parse(&action, args).map_err(UiError::from_error)?;
    if let Some(setup) = services.active_setup()? {
        if services.opening.load(Ordering::Acquire) {
            return Err(UiError {
                code: "library_opening".into(),
            });
        }
        if !matches!(
            request,
            Request::AccountAdd(_)
                | Request::AccountLogin(_)
                | Request::AccountRename(_)
                | Request::AccountRemove(_)
                | Request::AccountLogout(_)
                | Request::CancelSync(_)
                | Request::RetrySync(_)
        ) {
            return Err(UiError {
                code: "library_not_open".into(),
            });
        }
        if matches!(request, Request::CancelSync(_) | Request::RetrySync(_)) {
            let job = setup
                .catalogue
                .get("job", request.job_id().unwrap_or(""))
                .map_err(UiError::from_error)?;
            if !job.is_some_and(|job| {
                job["type"]
                    .as_str()
                    .is_some_and(|kind| kind.ends_with("Login"))
            }) {
                return Err(UiError {
                    code: "library_not_open".into(),
                });
            }
        }
        if matches!(request, Request::AccountLogin(_) | Request::RetrySync(_)) {
            let account_id = if matches!(request, Request::RetrySync(_)) {
                setup
                    .catalogue
                    .get("job", request.job_id().unwrap_or(""))
                    .map_err(UiError::from_error)?
                    .and_then(|job| job["accountId"].as_str().map(str::to_owned))
            } else {
                request.login_account().map(str::to_owned)
            };
            if let Some(account_id) = account_id {
                let provider = setup
                    .stores
                    .provider_for_account(&account_id)
                    .map_err(UiError::from_error)?;
                if let Err(error) = setup.sessions.arm(provider, &account_id) {
                    // No native login/profile write has begun. Discard only the
                    // unused memory account; an existing unmarked profile stays.
                    let _ = setup.catalogue.discard_unverified_account(&account_id);
                    changed(&app, "booth");
                    return Err(UiError::from_error(error));
                }
            }
        }
        let result = setup
            .stores
            .command(&app, &action, request.args())
            .await
            .map_err(UiError::from_error)?;
        changed(&app, "booth");
        return Ok(result);
    }
    let services = services.services()?;
    let quiet = matches!(request, Request::FetchProductMetadata(_));
    let job_change = matches!(
        request,
        Request::Sync(_)
            | Request::CancelSync(_)
            | Request::RetrySync(_)
            | Request::UpdateProducts(_)
    );
    let account_change = matches!(
        request,
        Request::AccountAdd(_) | Request::AccountRemove(_) | Request::AccountLogout(_)
    );
    let result = {
        // Keep new downloads out between the idle check and session clearing.
        let _queue = if matches!(
            request,
            Request::AccountLogout(_) | Request::AccountRemove(_)
        ) {
            Some(services.storage.pause_download_queue().await)
        } else {
            None
        };
        services
            .stores
            .command(&app, &action, request.args())
            .await
            .map_err(UiError::from_error)?
    };
    if !quiet {
        changed(&app, if job_change { "jobs" } else { "booth" });
        if !services.setup_only && account_change {
            let storage = services.storage.clone();
            tauri::async_runtime::spawn(async move {
                let _ = storage.reevaluate_automatic().await;
            });
        }
    }
    Ok(result)
}
#[tauri::command]
async fn storage_action(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    services: tauri::State<'_, LibraryHost>,
    action: String,
    args: Value,
) -> Result<Value, UiError> {
    use storage::api::StorageRequest as Request;
    let _operation = trusted(&window)?;
    let request = Request::parse(&action, args).map_err(UiError::from_error)?;
    let device_dir = services.device_dir.clone();
    if let Request::ResetApp(reset) = &request {
        let delete_library = reset.delete_library;
        let active = services.services()?;
        if services.opening.swap(true, Ordering::AcqRel) {
            return Err(UiError {
                code: "library_opening".into(),
            });
        }
        let result = async {
            let permit = active.catalogue.begin_restore_work()?;
            profile::prepare_restart_handoff()?;
            let catalogue = active.catalogue.clone();
            tauri::async_runtime::spawn_blocking(move || {
                app_reset::schedule(&device_dir, &catalogue, delete_library)
            })
            .await
            .map_err(|_| anyhow::anyhow!("worker_failed"))??;
            *services
                .switching
                .lock()
                .map_err(|_| anyhow::anyhow!("catalogue.lock"))? = Some(permit);
            Ok::<_, anyhow::Error>(())
        }
        .await;
        if let Err(error) = result {
            services.opening.store(false, Ordering::Release);
            return Err(UiError::from_error(error));
        }
        restart_for_library(app);
        return storage::api::answer(storage::api::RestartRequired {
            restart_required: true,
        })
        .map_err(UiError::from_error);
    }
    let services = services.services()?;
    if matches!(request, Request::MigrateRoot(_)) && services.catalogue.library_root().is_some() {
        let storage = services.storage.clone();
        let args = request.args();
        let result = tauri::async_runtime::spawn_blocking(move || {
            storage.move_library(&args, &AtomicBool::new(false))
        })
        .await
        .map_err(|_| UiError {
            code: "worker_failed".into(),
        })?
        .map_err(|error| UiError::from_move_error(&error))?;
        let handoff = (|| -> anyhow::Result<()> {
            let path = result["libraryPath"]
                .as_str()
                .context("library_unavailable")?;
            let locale = services
                .catalogue
                .get("preferences", "app")?
                .unwrap_or(json!({}))["locale"]
                .as_str()
                .unwrap_or("en")
                .to_owned();
            profile::prepare_restart_handoff()?;
            profile::select_library(
                &device_dir,
                &profile::LibrarySelection {
                    path: PathBuf::from(path),
                    locale,
                },
            )
        })();
        if let Err(error) = handoff {
            services
                .catalogue
                .release_move_hold()
                .map_err(UiError::from_error)?;
            return Err(UiError::from_error(error));
        }
        restart_for_library(app);
        return Ok(result);
    }
    let quiet = matches!(
        request,
        Request::ProductAvailability(_)
            | Request::RootUsage(_)
            | Request::OpenFile(_)
            | Request::OpenProductFolder(_)
            | Request::OpenRootFolder(_)
    );
    let job_change = matches!(
        request,
        Request::EnqueueProducts(_)
            | Request::EnqueueFile(_)
            | Request::DeleteDownloads(_)
            | Request::CancelJob(_)
            | Request::RetryJob(_)
            | Request::DiscardImport(_)
            | Request::ExtractWithPassword(_)
    );
    let root_change = matches!(request, Request::ConfigureRoot(_) | Request::RemapRoot(_));
    let result = services
        .storage
        .command(&app, &action, request.args())
        .await
        .map_err(UiError::from_error)?;
    if !quiet {
        changed(&app, if job_change { "jobs" } else { "storage" });
        if root_change {
            let storage = services.storage.clone();
            tauri::async_runtime::spawn(async move {
                let _ = storage.reevaluate_automatic().await;
            });
        }
    }
    Ok(result)
}
#[tauri::command]
async fn licenses_action(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    services: tauri::State<'_, LibraryHost>,
    action: String,
    args: Value,
) -> Result<Value, UiError> {
    let _operation = trusted(&window)?;
    let request =
        licenses::api::LicensesRequest::parse(&action, args).map_err(UiError::from_error)?;
    let services = services.services()?;
    services
        .licenses
        .command(&app, request)
        .await
        .map_err(UiError::from_error)
}
#[tauri::command]
async fn unity_action(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    services: tauri::State<'_, LibraryHost>,
    action: String,
    args: Value,
) -> Result<Value, UiError> {
    let _operation = trusted(&window)?;
    let request = unity::api::UnityRequest::parse(&action, args).map_err(UiError::from_error)?;
    let services = services.services()?;
    services
        .unity
        .command(&app, request)
        .await
        .map_err(UiError::from_error)
}
#[tauri::command]
async fn native_dialog(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    kind: native::DialogKind,
    options: Option<native::DialogOptions>,
) -> Result<Option<native::DialogPick>, UiError> {
    let _operation = trusted(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        native::dialog(&app, kind, options.unwrap_or_default())
    })
    .await
    .map_err(|_| UiError {
        code: "worker_failed".into(),
    })?
    .map_err(UiError::from_error)
}
#[tauri::command]
async fn native_file_action(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    services: tauri::State<'_, LibraryHost>,
    action: String,
    args: Value,
) -> Result<Value, UiError> {
    let _operation = trusted(&window)?;
    let request = native::FileRequest::parse(&action, args).map_err(UiError::from_error)?;
    let services = services.services()?;
    let catalogue = services.catalogue.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        native::file_action(&app, &catalogue, request)
    })
    .await
    .map_err(|_| UiError {
        code: "worker_failed".into(),
    })?
    .map_err(UiError::from_error)?;
    Ok(result)
}
#[tauri::command]
fn external_open(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    url: String,
) -> Result<(), UiError> {
    let _operation = trusted(&window)?;
    native::external_open(&app, &url).map_err(UiError::from_error)
}

#[tauri::command]
fn native_zoom(window: tauri::WebviewWindow, scale: f64) -> Result<native::Zoomed, UiError> {
    let _operation = trusted(&window)?;
    let factor = native::zoom_factor(scale).map_err(UiError::from_error)?;
    window.set_zoom(factor).map_err(|_| UiError {
        code: "ui_scale_failed".into(),
    })?;
    Ok(native::Zoomed { scale })
}

#[tauri::command]
async fn preview_cache(
    window: tauri::WebviewWindow,
    services: tauri::State<'_, LibraryHost>,
    url: String,
) -> Result<preview::CachedPreview, UiError> {
    let _operation = trusted(&window)?;
    let services = services.services()?;
    let _work = services
        .catalogue
        .begin_catalogue_work()
        .map_err(UiError::from_error)?;
    let path = services
        .previews
        .get(url)
        .await
        .map_err(UiError::from_error)?;
    Ok(preview::CachedPreview { path })
}

#[tauri::command]
async fn library_status(window: tauri::WebviewWindow) -> Result<LibraryStatus, UiError> {
    let _operation = trusted(&window)?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || active_library_status(&app.state::<LibraryHost>()))
        .await
        .map_err(|_| UiError {
            code: "worker_failed".into(),
        })?
}

fn active_library_status(host: &LibraryHost) -> Result<LibraryStatus, UiError> {
    let mut status = host.status_snapshot()?;
    if status.error_code.as_deref() == Some("reset_incomplete")
        && host.opening.load(Ordering::Acquire)
    {
        status.error_code = Some("reset_in_progress".into());
        return Ok(status);
    }
    if let Some(setup) = host.active_setup()? {
        status.state = "setup".into();
        status.path = Some(setup.candidate()?);
        status.default_path = None;
        status.error_code = None;
        status.locale = setup
            .catalogue
            .get("preferences", "app")
            .map_err(UiError::from_error)?
            .and_then(|prefs| prefs["locale"].as_str().map(str::to_owned))
            .unwrap_or(status.locale);
        status.missing_credentials = vec![];
        return Ok(status);
    }
    if let Some(services) = host.active_services()? {
        status.state = "ready".into();
        status.path = services
            .catalogue
            .library_root()
            .map(std::path::Path::to_path_buf);
        status.default_path = None;
        status.error_code = None;
        if let Some(locale) = services
            .catalogue
            .get("preferences", "app")
            .map_err(UiError::from_error)?
            .and_then(|p| p["locale"].as_str().map(str::to_owned))
        {
            status.locale = locale;
        }
        status.missing_credentials = services
            .catalogue
            .list("account")
            .map_err(UiError::from_error)?
            .into_iter()
            .filter(|account| {
                account["disconnected"] != true
                    && account["status"] == "connected"
                    && !services.stores.has_local_session(account)
            })
            .map(|account| LibraryCredential {
                id: account["id"].as_str().unwrap_or_default().to_owned(),
                name: account["name"].as_str().map(str::to_owned),
                store: account["store"].as_str().map(str::to_owned),
            })
            .collect();
    } else if status.state == "unconfigured" {
        status.default_path = profile::default_library_path(&host.device_dir).ok();
    }
    Ok(status)
}

fn validate_setup_candidate(path: PathBuf, new_library: bool) -> anyhow::Result<PathBuf> {
    let target = if new_library {
        profile::new_library_path(&path)
    } else {
        path
    };
    anyhow::ensure!(target.is_absolute(), "root_not_absolute");
    storage::validate_library_path(&target)?;
    if target.exists() {
        return storage::checked_library_root(&target);
    }
    let parent = storage::checked_library_root(target.parent().context("root_offline")?)?;
    Ok(parent.join(target.file_name().context("root_offline")?))
}

fn choose_setup_candidate(
    setup: &SetupState,
    path: PathBuf,
    new_library: bool,
    locale: Option<&str>,
) -> anyhow::Result<bool> {
    let candidate = validate_setup_candidate(path, new_library)?;
    let existing = if candidate.exists() {
        Catalogue::probe_setup_preferences(&candidate)?
    } else {
        None
    };
    if existing.is_some_and(|preferences| preferences["setupComplete"] == true) {
        return Ok(false);
    }
    if let Some(locale) = locale {
        setup
            .catalogue
            .command("preferences", json!({"patch":{"locale":locale}}))?;
    }
    *setup
        .candidate
        .lock()
        .map_err(|_| anyhow::anyhow!("library_unavailable"))? = candidate;
    Ok(true)
}

fn setup_finish_patch(setup: &SetupState, args: &Value) -> anyhow::Result<Value> {
    let mut patch = args.get("patch").unwrap_or(args).clone();
    let fields = patch
        .as_object_mut()
        .context("catalogue.invalid_preferences")?;
    fields.remove("setupComplete");
    fields.remove("libraryRootId");
    // Validate all user choices before touching the target. Failed Finish keeps
    // the same memory accounts and draft choices available for ordinary retry.
    setup
        .catalogue
        .command("preferences", json!({"patch":patch}))?;
    let preferences = setup
        .catalogue
        .get("preferences", "app")?
        .context("catalogue.preferences_missing")?;
    let mut patch = serde_json::Map::new();
    for (key, value) in preferences
        .as_object()
        .context("catalogue.invalid_preferences")?
    {
        if !matches!(key.as_str(), "id" | "libraryRootId" | "setupComplete")
            && setup.initial_preferences.get(key) != Some(value)
        {
            patch.insert(key.clone(), value.clone());
        }
    }
    // Explicit Finish values also apply when they equal this build's defaults.
    for (key, value) in args
        .get("patch")
        .unwrap_or(args)
        .as_object()
        .context("catalogue.invalid_preferences")?
    {
        if !matches!(key.as_str(), "id" | "libraryRootId" | "setupComplete") {
            patch.insert(key.clone(), value.clone());
        }
    }
    patch.insert("setupComplete".into(), json!(true));
    Ok(Value::Object(patch))
}

fn materialize_setup(
    setup: &SetupState,
    device_dir: &std::path::Path,
    patch: Value,
) -> anyhow::Result<(Catalogue, Value)> {
    let candidate = validate_setup_candidate(
        setup
            .candidate()
            .map_err(|error| anyhow::anyhow!(error.code))?,
        false,
    )?;
    if candidate.exists() {
        Catalogue::probe_setup_preferences(&candidate)?;
    }
    let previous_selection = profile::selected_library(device_dir)?;
    if !candidate.exists() {
        std::fs::create_dir(&candidate).context("storage.create_directory_failed")?;
    }
    let target = Catalogue::open_bound(&candidate, device_dir)?;
    let root = target
        .library_root()
        .context("library_unavailable")?
        .to_path_buf();
    let existing = target.get("preferences", "app")?;
    if let Some(preferences) = existing.filter(|preferences| preferences["setupComplete"] == true) {
        // An existing completed library has its own accounts and policy. Opening
        // it never applies the preceding new-library setup draft.
        profile::select_library(
            device_dir,
            &profile::LibrarySelection {
                path: root,
                locale: preferences["locale"].as_str().unwrap_or("en").into(),
            },
        )?;
        return Ok((target, preferences));
    }
    let accounts = setup.catalogue.list("account")?;
    if let Err(error) = setup.sessions.release(&accounts) {
        rearm_unpublished_setup_sessions(setup, &target, &accounts);
        return Err(error);
    }
    let mut selected = false;
    let result = target.commit_setup(patch.clone(), accounts.clone(), || {
        profile::select_library(
            device_dir,
            &profile::LibrarySelection {
                path: root,
                locale: patch["locale"].as_str().unwrap_or("en").into(),
            },
        )?;
        selected = true;
        Ok(())
    });
    if result.is_err() {
        rearm_unpublished_setup_sessions(setup, &target, &accounts);
    }
    if result.is_err() && selected {
        // SQL commit failed after the locator write. Restore only this exact
        // device locator; the target transaction rolls back without publication.
        match previous_selection {
            Some(selection) => profile::select_library(device_dir, &selection)?,
            None => match std::fs::remove_file(device_dir.join("library.json")) {
                Ok(()) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(_) => anyhow::bail!("library_restart_failed"),
            },
        }
    }
    Ok((target, result?))
}

fn rearm_unpublished_setup_sessions(setup: &SetupState, target: &Catalogue, accounts: &[Value]) {
    let unpublished: Vec<_> = accounts
        .iter()
        .filter(|account| {
            let Some(id) = account["id"].as_str() else {
                return false;
            };
            // Any uncertainty retains the released marker. In particular, an SQL
            // commit error is never authority to delete a possibly committed login.
            matches!(target.get("account", id), Ok(None))
                && setup
                    .catalogue
                    .get("account", id)
                    .is_ok_and(|current| current.as_ref() == Some(*account))
        })
        .cloned()
        .collect();
    let _ = setup.sessions.rearm(&unpublished);
}

async fn finish_staged_setup(
    app: tauri::AppHandle,
    host: &LibraryHost,
    setup: SetupState,
    args: Value,
) -> Result<Value, UiError> {
    host.begin_opening()?;
    let result = async {
        let permit = setup
            .catalogue
            .begin_restore_work()
            .map_err(UiError::from_error)?;
        let patch = setup_finish_patch(&setup, &args).map_err(UiError::from_error)?;
        profile::prepare_restart_handoff().map_err(UiError::from_error)?;
        let device_dir = host.device_dir.clone();
        let (catalogue, preferences) = tauri::async_runtime::spawn_blocking(move || {
            materialize_setup(&setup, &device_dir, patch)
        })
        .await
        .map_err(|_| UiError {
            code: "worker_failed".into(),
        })?
        .map_err(|error| UiError::from_library_error(&error))?;
        *host.next_catalogue.lock().map_err(|_| UiError {
            code: "library_unavailable".into(),
        })? = Some(catalogue);
        let preferences = retain_setup_finish(host, preferences, permit)?;
        restart_for_library(app);
        Ok(preferences)
    }
    .await;
    if result.is_err() {
        host.opening.store(false, Ordering::Release);
    }
    result
}

async fn finish_setup(
    app: tauri::AppHandle,
    host: &LibraryHost,
    services: Services,
    args: Value,
) -> Result<Value, UiError> {
    host.begin_opening()?;
    let result = async {
        let permit = services
            .catalogue
            .begin_restore_work()
            .map_err(UiError::from_error)?;
        // Prepare the protected replacement before committing completion. A failed
        // handoff keeps setup incomplete and permits an ordinary retry.
        profile::prepare_restart_handoff().map_err(UiError::from_error)?;
        let catalogue = services.catalogue.clone();
        let preferences =
            tauri::async_runtime::spawn_blocking(move || catalogue.command("preferences", args))
                .await
                .map_err(|_| UiError {
                    code: "worker_failed".into(),
                })?
                .map_err(UiError::from_error)?;
        let preferences = retain_setup_finish(host, preferences, permit)?;
        restart_for_library(app);
        Ok(preferences)
    }
    .await;
    if result.is_err() {
        host.opening.store(false, Ordering::Release);
    }
    result
}

fn retain_setup_finish(
    host: &LibraryHost,
    mut preferences: Value,
    permit: catalogue::CatalogueWorkPermit,
) -> Result<Value, UiError> {
    *host.switching.lock().map_err(|_| UiError {
        code: "library_unavailable".into(),
    })? = Some(permit);
    preferences["restartRequired"] = json!(true);
    Ok(preferences)
}

fn open_selected_catalogue(
    path: PathBuf,
    device_dir: PathBuf,
    current: Option<Catalogue>,
    new_library: bool,
) -> anyhow::Result<Catalogue> {
    let path = if new_library {
        let destination = profile::new_library_path(&path);
        if !destination.exists() {
            let parent = destination.parent().context("root_offline")?;
            storage::checked_library_root(parent)?;
            storage::validate_library_path(&destination)?;
            std::fs::create_dir(&destination).context("storage.create_directory_failed")?;
        }
        destination
    } else {
        // Open/Retry must never adopt an unrelated folder as a new library.
        // Initialization belongs to an explicit new-library choice or Finish.
        let root = storage::checked_library_root(&path)?;
        let database = root.join(".nna/data/catalogue.sqlite3");
        storage::validate_library_path(&database)?;
        anyhow::ensure!(database.is_file(), "library_database_unavailable");
        root
    };
    if let Some(current) = current {
        if std::fs::canonicalize(&path).ok().as_deref() == current.library_root() {
            current.check_library_available()?;
            return Ok(current);
        }
    }
    Catalogue::open_bound(&path, &device_dir)
}

fn restart_for_library(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        window_state::flush_app(&app).await;
        app.restart();
    });
}

#[tauri::command]
async fn library_open(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    host: tauri::State<'_, LibraryHost>,
    path: PathBuf,
    locale: Option<String>,
    new_library: Option<bool>,
) -> Result<LibraryOpened, UiError> {
    let _operation = trusted(&window)?;
    host.begin_opening()?;
    let result = async {
        let status = host.status_snapshot()?;
        if app_reset::pending(&host.device_dir) {
            if status.path.as_ref() != Some(&path) {
                return Err(UiError {
                    code: "reset_pending".into(),
                });
            }
            let device = host.device_dir.clone();
            tauri::async_runtime::spawn_blocking(move || app_reset::apply_pending(&device))
                .await
                .map_err(|_| UiError {
                    code: "worker_failed".into(),
                })?
                .map_err(UiError::from_error)?;
            let mut reset_status = host.status.lock().map_err(|_| UiError {
                code: "library_unavailable".into(),
            })?;
            reset_status.state = "unconfigured".into();
            reset_status.path = None;
            reset_status.error_code = None;
            drop(reset_status);
            host.opening.store(false, Ordering::Release);
            changed(&app, "library");
            return Ok(LibraryOpened { restarting: false });
        }
        let setup = match host.active_setup()? {
            Some(setup) => Some(setup),
            None if host.active_services()?.is_none() && status.state == "unconfigured" => {
                let candidate = validate_setup_candidate(path.clone(), new_library == Some(true))
                    .map_err(|error| UiError::from_library_error(&error))?;
                let existing = if candidate.exists() {
                    Catalogue::probe_setup_preferences(&candidate)
                        .map_err(|error| UiError::from_library_error(&error))?
                } else {
                    None
                };
                if existing.is_some_and(|preferences| preferences["setupComplete"] == true) {
                    None
                } else {
                    let setup = SetupState::new(
                        &host.device_dir,
                        candidate,
                        locale.as_deref().unwrap_or("en"),
                    )
                    .map_err(UiError::from_error)?;
                    *host.setup.lock().map_err(|_| UiError {
                        code: "library_unavailable".into(),
                    })? = Some(setup.clone());
                    Some(setup)
                }
            }
            None => None,
        };
        if let Some(setup) = setup {
            if choose_setup_candidate(
                &setup,
                path.clone(),
                new_library == Some(true),
                locale.as_deref(),
            )
            .map_err(|error| UiError::from_library_error(&error))?
            {
                host.opening.store(false, Ordering::Release);
                changed(&app, "state");
                return Ok(LibraryOpened { restarting: false });
            }
        }
        let active = host.active_services()?;
        let permit = if let Some(services) = active.as_ref() {
            Some(
                services
                    .catalogue
                    .begin_restore_work()
                    .map_err(UiError::from_error)?,
            )
        } else if let Some(setup) = host.active_setup()? {
            Some(
                setup
                    .catalogue
                    .begin_restore_work()
                    .map_err(UiError::from_error)?,
            )
        } else {
            None
        };
        let device_dir = host.device_dir.clone();
        let current = active.as_ref().map(|services| services.catalogue.clone());
        let catalogue = tauri::async_runtime::spawn_blocking(move || {
            open_selected_catalogue(path, device_dir, current, new_library == Some(true))
        })
        .await
        .map_err(|_| UiError {
            code: "worker_failed".into(),
        })?
        .map_err(|error| UiError::from_library_error(&error))?;
        let root = catalogue
            .library_root()
            .ok_or_else(|| UiError {
                code: "library_unavailable".into(),
            })?
            .to_path_buf();
        let previous = catalogue
            .get("preferences", "app")
            .map_err(UiError::from_error)?
            .unwrap_or(json!({}));
        let locale = locale
            .filter(|value| matches!(value.as_str(), "en" | "ja" | "ko" | "zh-Hans" | "zh-Hant"))
            .unwrap_or_else(|| previous["locale"].as_str().unwrap_or("en").into());
        if previous["setupComplete"] != true {
            catalogue
                .command("preferences", json!({"patch":{"locale":locale}}))
                .map_err(UiError::from_error)?;
        }
        profile::prepare_restart_handoff().map_err(UiError::from_error)?;
        profile::select_library(
            &host.device_dir,
            &profile::LibrarySelection { path: root, locale },
        )
        .map_err(|error| UiError::from_library_error(&error))?;
        *host.next_catalogue.lock().map_err(|_| UiError {
            code: "library_unavailable".into(),
        })? = Some(catalogue);
        *host.switching.lock().map_err(|_| UiError {
            code: "library_unavailable".into(),
        })? = permit;
        restart_for_library(app);
        Ok(LibraryOpened { restarting: true })
    }
    .await;
    if result.is_err() {
        host.opening.store(false, Ordering::Release);
    }
    result
}
#[tauri::command]
async fn updates_check(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    state: tauri::State<'_, updates::UpdateState>,
) -> Result<updates::UpdateCheck, UiError> {
    let _operation = trusted(&window)?;
    state.check(&app).await.map_err(UiError::from_error)
}

#[tauri::command]
async fn updates_install(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    host: tauri::State<'_, LibraryHost>,
    state: tauri::State<'_, updates::UpdateState>,
    gate: tauri::State<'_, NativeOperationGate>,
    version: String,
) -> Result<(), UiError> {
    trusted_access(&window)?;
    let _install = state.begin_install().map_err(UiError::from_error)?;
    update_ready(&host)?;
    let prepared = state
        .download(&app, &version)
        .await
        .map_err(UiError::from_error)?;
    // In-flight commands such as thumbnail fetches finish first; new ones are
    // refused until the installer launches or fails.
    let _commands = tokio::time::timeout(
        std::time::Duration::from_secs(45),
        gate.0.clone().write_owned(),
    )
    .await
    .map_err(|_| UiError {
        code: "updates_busy".into(),
    })?;
    let catalogue = update_ready(&host)?.catalogue;
    crate::window_state::flush_app(&app).await;
    // Exit between catalogue transactions; interrupted store syncs, downloads and
    // local imports resume when the updated App starts. The library's OS lock is
    // held until this process exits.
    tauri::async_runtime::spawn_blocking(move || {
        catalogue.between_transactions(|jobs| match jobs.iter().find_map(updates::blocking_job) {
            Some(code) => Err(anyhow::anyhow!(code)),
            None => prepared.launch(),
        })
    })
    .await
    .map_err(|_| UiError {
        code: "worker_failed".into(),
    })?
    .and_then(|launched| launched)
    .map_err(UiError::from_error)
}

/// An open library whose active jobs all resume after the restart; otherwise the blocking reason.
fn update_ready(host: &LibraryHost) -> Result<Services, UiError> {
    let busy = |code: &str| UiError { code: code.into() };
    if host.opening.load(Ordering::Acquire) || host.active_setup()?.is_some() {
        return Err(busy("updates_busy_library"));
    }
    let services = host.services()?;
    if services.setup_only {
        return Err(busy("updates_busy_library"));
    }
    let jobs = services
        .catalogue
        .list("job")
        .map_err(UiError::from_error)?;
    match jobs.iter().find_map(updates::blocking_job) {
        Some(code) => Err(busy(code)),
        None => Ok(services),
    }
}

pub fn run() {
    let mut context = tauri::generate_context!();
    profile::configure(&mut context).expect("NekoNekoAsset profile could not be selected");
    // Apply remembered bounds before revealing the main window. Authored hidden
    // verification windows keep their configured visibility and never gain focus.
    let main_visible = context
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "main")
        .is_some_and(|window| window.visible);
    if let Some(window) = context
        .config_mut()
        .app
        .windows
        .iter_mut()
        .find(|window| window.label == "main")
    {
        window.visible = false;
    }
    tauri::Builder::default()
        .manage(NativeOperationGate::default())
        .manage(updates::UpdateState::default())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            window_state::on_event(window, event);
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let app = window.app_handle().clone();
                    let services = app
                        .try_state::<LibraryHost>()
                        .and_then(|host| host.active_services().ok().flatten());
                    if let Some(services) = services {
                        if !services.storage.begin_local_import_shutdown() {
                            return;
                        }
                        tauri::async_runtime::spawn(async move {
                            if services
                                .storage
                                .discard_waiting_local_imports()
                                .await
                                .is_err()
                            {
                                eprintln!("Local import queue cleanup on close failed");
                            }
                            app.exit(0);
                        });
                    } else {
                        app.exit(0);
                    }
                }
            }
        })
        .setup(move |app| setup_library(app, main_visible))
        .invoke_handler(tauri::generate_handler![
            library_status,
            library_open,
            ai_action,
            ai_analysis_action,
            ai_suggestion_action,
            catalogue_action,
            booth_action,
            storage_action,
            licenses_action,
            unity_action,
            native_dialog,
            native_file_action,
            external_open,
            native_zoom,
            preview_cache,
            updates_check,
            updates_install
        ])
        .run(context)
        .expect("NekoNekoAsset could not start");
}

/// Opens the selected library or the setup state, then starts the long-lived workers.
fn setup_library(
    app: &mut tauri::App,
    main_visible: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = if cfg!(debug_assertions) {
        std::env::var_os("NNA_DATA_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or(app.path().app_local_data_dir()?)
    } else {
        app.path().app_local_data_dir()?
    };
    std::fs::create_dir_all(&data_dir)?;
    window_state::initialize(app, &data_dir, main_visible);
    setup_sessions::schedule_cleanup(&data_dir);
    if app_reset::pending(&data_dir) {
        manage_pending_reset(app, data_dir);
        return Ok(());
    }
    let (catalogue, mut status, setup) = select_library(&data_dir);
    let Some(catalogue) = catalogue else {
        app.manage(LibraryHost {
            device_dir: data_dir,
            status: Mutex::new(status),
            services: Mutex::new(None),
            setup: Mutex::new(setup),
            opening: AtomicBool::new(false),
            next_catalogue: Mutex::new(None),
            switching: Mutex::new(None),
        });
        return Ok(());
    };
    let prepared = (|| -> anyhow::Result<Services> {
        if let Some(preferences) = catalogue.get("preferences", "app")? {
            if let Some(locale) = preferences["locale"].as_str() {
                status.locale = locale.into();
            }
        }
        let services = Services::new(catalogue.clone(), &data_dir)?;
        app.asset_protocol_scope()
            .allow_directory(catalogue.data_dir().join("media"), true)?;
        app.asset_protocol_scope()
            .allow_directory(catalogue.data_dir().join("cache").join("previews"), true)?;
        if !services.setup_only {
            catalogue.start_sync_worker()?;
        }
        Ok(services)
    })();
    let services = match prepared {
        Ok(services) => services,
        Err(error) => {
            status.state = "blocked".into();
            status.error_code = Some(UiError::from_library_error(&error).code);
            app.manage(LibraryHost {
                device_dir: data_dir,
                status: Mutex::new(status),
                services: Mutex::new(None),
                setup: Mutex::new(None),
                opening: AtomicBool::new(false),
                next_catalogue: Mutex::new(None),
                switching: Mutex::new(None),
            });
            return Ok(());
        }
    };
    let setup_only = services.setup_only;
    let stores = services.stores.clone();
    let storage = services.storage.clone();
    let ai_jobs = services.ai_jobs.clone();
    let unity = services.unity.clone();
    let licenses = services.licenses.clone();
    app.manage(LibraryHost {
        device_dir: data_dir,
        status: Mutex::new(status),
        services: Mutex::new(Some(services)),
        setup: Mutex::new(None),
        opening: AtomicBool::new(false),
        next_catalogue: Mutex::new(None),
        switching: Mutex::new(None),
    });
    // Setup needs only explicit account/login commands. Starting the
    // long-lived workers here would retain an old library after a new
    // location choice and could begin downloads before Finish.
    if setup_only {
        return Ok(());
    }
    listen_for_source_changes(app, &storage);
    spawn_startup_work(app, catalogue, storage, stores, ai_jobs, unity, licenses);
    Ok(())
}

/// An interrupted reset finishes before anything opens; the App then returns to setup.
fn manage_pending_reset(app: &tauri::App, data_dir: PathBuf) {
    app.manage(LibraryHost {
        device_dir: data_dir.clone(),
        status: Mutex::new(LibraryStatus {
            state: "blocked".into(),
            path: app_reset::pending_library(&data_dir),
            default_path: None,
            error_code: Some("reset_incomplete".into()),
            locale: profile::selected_library(&data_dir)
                .ok()
                .flatten()
                .map(|selection| selection.locale)
                .unwrap_or_else(|| "en".into()),
            missing_credentials: vec![],
        }),
        services: Mutex::new(None),
        setup: Mutex::new(None),
        opening: AtomicBool::new(true),
        next_catalogue: Mutex::new(None),
        switching: Mutex::new(None),
    });
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        let result =
            tauri::async_runtime::spawn_blocking(move || app_reset::apply_pending(&data_dir)).await;
        let host = handle.state::<LibraryHost>();
        if matches!(result, Ok(Ok(()))) {
            if let Ok(mut status) = host.status.lock() {
                status.state = "unconfigured".into();
                status.path = None;
                status.error_code = None;
            }
        }
        host.opening.store(false, Ordering::Release);
        let _ = handle.emit("nna:changed", crate::ipc::Change::scope("library"));
    });
}

/// The library to open, its status, and the setup state when setup is not finished.
fn select_library(
    data_dir: &std::path::Path,
) -> (Option<Catalogue>, LibraryStatus, Option<SetupState>) {
    // Explicit reset retains its unconfigured fallback; no unrelated
    // completed default is opened or changed while returning to setup.
    let applying_reset = app_reset::pending(data_dir);
    let selection = app_reset::apply_pending(data_dir)
        .map_err(|_| anyhow::anyhow!("reset_incomplete"))
        .and_then(|_| profile::selected_library(data_dir));
    let mut setup = None;
    let (catalogue, status) = match selection {
        Ok(Some(selection)) => {
            let opened: anyhow::Result<Option<Catalogue>> = (|| {
                let preferences = Catalogue::probe_setup_preferences(&selection.path)?;
                if preferences.is_some_and(|preferences| preferences["setupComplete"] == true) {
                    Catalogue::open_bound(&selection.path, data_dir).map(Some)
                } else {
                    let candidate = validate_setup_candidate(selection.path.clone(), false)?;
                    setup = Some(SetupState::new(data_dir, candidate, &selection.locale)?);
                    Ok(None)
                }
            })();
            let error = opened
                .as_ref()
                .err()
                .map(|error| UiError::from_library_error(error).code);
            (
                opened.ok().flatten(),
                LibraryStatus {
                    state: if error.is_some() {
                        "blocked"
                    } else if setup.is_some() {
                        "setup"
                    } else {
                        "ready"
                    }
                    .into(),
                    path: Some(selection.path),
                    default_path: None,
                    error_code: error,
                    locale: selection.locale,
                    missing_credentials: vec![],
                },
            )
        }
        Ok(None) if !applying_reset => {
            let default_path = profile::default_library_path(data_dir).ok();
            let opened = default_path
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("library_unavailable"))
                .and_then(|path| validate_setup_candidate(path.clone(), false))
                .and_then(|path| {
                    if path.exists()
                        && Catalogue::probe_setup_preferences(&path)?
                            .is_some_and(|preferences| preferences["setupComplete"] == true)
                    {
                        Catalogue::open_bound(&path, data_dir).map(Some)
                    } else {
                        setup = Some(SetupState::new(data_dir, path, "en")?);
                        Ok(None)
                    }
                });
            let error = opened
                .as_ref()
                .err()
                .map(|error| UiError::from_library_error(error).code);
            let path = setup
                .as_ref()
                .and_then(|setup| setup.candidate().ok())
                .or_else(|| default_path.clone());
            let catalogue = opened.ok().flatten();
            (
                catalogue.clone(),
                LibraryStatus {
                    state: if error.is_some() {
                        if default_path
                            .as_ref()
                            .is_some_and(|path| path.join(".nna/data/catalogue.sqlite3").is_file())
                        {
                            "blocked"
                        } else {
                            "unconfigured"
                        }
                    } else if catalogue.is_some() {
                        "ready"
                    } else {
                        "setup"
                    }
                    .into(),
                    path,
                    default_path,
                    error_code: error,
                    locale: "en".into(),
                    missing_credentials: vec![],
                },
            )
        }
        Ok(None) => (
            None,
            LibraryStatus {
                state: "unconfigured".into(),
                path: None,
                default_path: None,
                error_code: None,
                locale: "en".into(),
                missing_credentials: vec![],
            },
        ),
        Err(error) => (
            None,
            LibraryStatus {
                state: "blocked".into(),
                path: profile::selected_library(data_dir)
                    .ok()
                    .flatten()
                    .map(|selection| selection.path)
                    .or_else(|| app_reset::pending_library(data_dir)),
                default_path: None,
                error_code: Some(UiError::from_error(error).code),
                locale: "en".into(),
                missing_credentials: vec![],
            },
        ),
    };
    (catalogue, status, setup)
}

/// Source changes and new sessions reevaluate automatic downloads.
fn listen_for_source_changes(app: &tauri::App, storage: &Arc<StorageService>) {
    let automatic_storage = storage.clone();
    app.listen("nna:changed", move |event| {
        let Ok(change) = serde_json::from_str::<ipc::Change>(event.payload()) else {
            return;
        };
        if change.session_changed == Some(true) {
            let storage = automatic_storage.clone();
            tauri::async_runtime::spawn(async move {
                let _ = storage.reevaluate_automatic().await;
            });
            return;
        }
        if change.scope != "catalogue" {
            return;
        }
        let ids = change.product_ids.unwrap_or_default();
        if ids.is_empty() {
            return;
        }
        let storage = automatic_storage.clone();
        tauri::async_runtime::spawn(async move {
            let _ = storage.source_products_changed(&ids).await;
        });
    });
}

/// Starts the workers once any installer has released the gate, syncs connected accounts
/// and then watches the library for changes from other devices.
fn spawn_startup_work(
    app: &tauri::App,
    catalogue: Catalogue,
    storage: Arc<StorageService>,
    stores: Arc<StoreService>,
    ai_jobs: Arc<ai::jobs::AiJobs>,
    unity: Arc<unity::UnityService>,
    licenses: licenses::LicenseService,
) {
    let handle = app.handle().clone();
    let startup_gate = handle.state::<NativeOperationGate>().0.clone();
    tauri::async_runtime::spawn(async move {
        // A failed installer releases this gate and startup work resumes normally.
        let startup_permit = startup_gate.clone().read_owned().await;
        let mut previous = catalogue.change_domains().unwrap_or((0, 0, 0));
        let setup_was_complete = catalogue
            .get("preferences", "app")
            .ok()
            .flatten()
            .is_some_and(|preferences| preferences["setupComplete"] == true);
        if storage.start(handle.clone()).await.is_err() {
            let _ = handle.emit("nna:changed", crate::ipc::Change::scope("recovery"));
        }
        // Explicit login/sync jobs need the workers even when isolated diagnostics
        // suppress automatic startup scans.
        let _ = stores.start(handle.clone()).await;
        let _ = ai_jobs.start(&handle).await;
        let _ = unity.start(&handle).await;
        let _ = licenses.start(&handle).await;
        drop(startup_permit);
        #[cfg(debug_assertions)]
        let skip_startup_sync = std::env::var_os("NNA_DATA_DIR").is_some()
            && std::env::var("NNA_SKIP_STARTUP_SYNC").as_deref() == Ok("1");
        #[cfg(not(debug_assertions))]
        let skip_startup_sync = false;
        if !skip_startup_sync && setup_was_complete {
            // Native local state renders before authenticated network work.
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let _sync_permit = startup_gate.read_owned().await;
            let _ = sync_connected_accounts(&handle, &catalogue, &stores).await;
        }
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            if catalogue.check_library_available().is_err() {
                // Never continue writing after a surfaced loss of the NAS lock handle.
                handle.exit(1);
                break;
            }
            if let Ok(revisions) = catalogue.change_domains() {
                if revisions.1 != previous.1 {
                    changed(&handle, "products");
                }
                if revisions.2 != previous.2 {
                    changed(&handle, "collections");
                }
                if revisions.0 != previous.0 {
                    changed(&handle, "state");
                }
                previous = revisions;
            }
        }
    });
}
