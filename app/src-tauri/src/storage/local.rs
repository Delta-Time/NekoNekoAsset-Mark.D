//! Device-only local import plans; input originals are never moved or removed.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    time::{Instant, UNIX_EPOCH},
};

const JOB_TYPE: &str = "storage:import_local";

fn emit_local_progress(
    app: &tauri::AppHandle,
    job: &str,
    phase: &str,
    bytes: Option<u64>,
    total_bytes: Option<u64>,
) {
    let _ = app.emit(
        "local-import:progress",
        json!({
            "jobId": job, "phase": phase, "bytes": bytes, "totalBytes": total_bytes
        }),
    );
}
const ENTRY_LIMIT: usize = 1_000_000;
/// A listing slower than this just leaves the price unknown.
const LISTING_PRICE_WAIT: Duration = Duration::from_secs(10);
const PRICE_FIELDS: [&str; 4] = [
    "price",
    "priceCurrency",
    "priceFormatted",
    "priceSourceCurrency",
];

#[derive(Clone, Serialize, Deserialize)]
struct LocalFile {
    source: PathBuf,
    relative: String,
    size: u64,
    modified: u128,
}
#[derive(Clone, Serialize, Deserialize)]
struct LocalGroup {
    product_id: String,
    title: String,
    prefix: String,
    files: Vec<LocalFile>,
    directories: Vec<String>,
    #[serde(default)]
    metadata: Option<Value>,
    #[serde(default)]
    remove_file_ids: Vec<String>,
    #[serde(default)]
    membership: bool,
    #[serde(default)]
    expected_mutation: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct LocalPlan {
    draft_id: String,
    groups: Vec<LocalGroup>,
    metadata: Value,
}
const SOURCE_KIND: &str = "local_source";
/// Where a local product's files were copied from, kept out of jobs so that Download
/// still repairs a copy after Clear history. Each published group adds an entry. It
/// leaves with its product and stays out of exports and backups: its paths belong to
/// the device that imported them.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalSource {
    id: String,
    product_id: String,
    entries: Vec<SourceEntry>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceEntry {
    draft_id: String,
    origin_device_id: String,
    root_id: String,
    group: LocalGroup,
}
impl LocalSource {
    fn new(product: &str) -> Self {
        Self {
            id: product.to_owned(),
            product_id: product.to_owned(),
            entries: vec![],
        }
    }
    /// Adds a published group in place of an entry from the same draft. Earlier entries
    /// lose the files it removes or copies again, and go once they hold none, so a
    /// later file of the same name never finds an old original.
    fn add(&mut self, job: &Value, plan: &LocalPlan, group: &LocalGroup, root: &str) {
        let product = &self.product_id;
        let replaced: HashSet<String> = group
            .remove_file_ids
            .iter()
            .cloned()
            .chain(
                group
                    .files
                    .iter()
                    .map(|file| file_id(product, &file.relative)),
            )
            .collect();
        let draft = &plan.draft_id;
        self.entries.retain(|entry| entry.draft_id != *draft);
        for entry in &mut self.entries {
            entry
                .group
                .files
                .retain(|file| !replaced.contains(&file_id(product, &file.relative)));
        }
        self.entries.retain(|entry| !entry.group.files.is_empty());
        let mut group = group.clone();
        // A repair runs the group again without the rest of its plan.
        group.metadata.get_or_insert_with(|| plan.metadata.clone());
        self.entries.push(SourceEntry {
            draft_id: draft.clone(),
            origin_device_id: job["originDeviceId"].as_str().unwrap_or("").to_owned(),
            root_id: root.to_owned(),
            group,
        });
    }
}

/// Selections often sit below redirected or junctioned folders (OneDrive moves Desktop and
/// Documents) and may hold cloud placeholders. Only a symbolic link or junction inside the
/// selection could lead out of it, so the selected items and their contents are checked
/// for those, never the folders above them.
fn reject_source_link(path: &Path) -> Result<()> {
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "unsafe_link"
    );
    Ok(())
}
/// `relative` places the file in its selection, so its parents up to the selected item are
/// checked again before the file is read.
fn signature(path: &Path, relative: &str) -> Result<(u64, u128)> {
    for item in path.ancestors().take(relative.split('/').count()) {
        reject_source_link(item)?;
    }
    let meta = fs::metadata(path)?;
    ensure!(meta.is_file(), "file_missing");
    Ok((
        meta.len(),
        meta.modified()?.duration_since(UNIX_EPOCH)?.as_nanos(),
    ))
}
/// Another program holding the user's file open is theirs to resolve; say so.
fn source_error(error: std::io::Error) -> anyhow::Error {
    if cfg!(windows) && matches!(error.raw_os_error(), Some(32 | 33)) {
        anyhow::anyhow!("storage_source_locked")
    } else {
        error.into()
    }
}
fn path_name(path: &Path) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("archive_unsafe_path"))?;
    archive::validate_relative_path(name)?;
    Ok(name.to_owned())
}
fn enumerate(
    path: &Path,
    relative: &str,
    group: &mut LocalGroup,
    cancel: Option<&AtomicBool>,
) -> Result<()> {
    if let Some(cancel) = cancel {
        io::check_cancel(cancel)?;
    }
    // Parents inside the selection were checked on the way down.
    reject_source_link(path)?;
    archive::validate_relative_path(relative)?;
    ensure!(
        group.files.len() + group.directories.len() < ENTRY_LIMIT,
        "local_import_too_many_files"
    );
    if path.is_dir() {
        group.directories.push(relative.to_owned());
        let mut children = fs::read_dir(path)?
            .map(|entry| Ok(entry?.path()))
            .collect::<Result<Vec<_>>>()?;
        children.sort();
        for child in children {
            enumerate(
                &child,
                &format!("{relative}/{}", path_name(&child)?),
                group,
                cancel,
            )?;
        }
    } else {
        let (size, modified) = signature(path, relative)?;
        group.files.push(LocalFile {
            source: path.to_owned(),
            relative: relative.to_owned(),
            size,
            modified,
        });
    }
    Ok(())
}
fn path_contains(parent: &Path, child: &Path) -> bool {
    #[cfg(windows)]
    {
        let parent: Vec<_> = parent
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect();
        let child: Vec<_> = child
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
            .collect();
        child.starts_with(&parent)
    }
    #[cfg(not(windows))]
    {
        child.starts_with(parent)
    }
}
fn checkpoint_id(draft: &str, group: &LocalGroup, file: &LocalFile) -> String {
    format!(
        "local-import-file:{draft}:{}",
        io::key(&format!("{}\0{}", group.product_id, file.relative))
    )
}
fn file_id(product: &str, relative: &str) -> String {
    format!("local-file:{}", io::key(&format!("{product}\0{relative}")))
}
fn category_manual(metadata: &Value) -> bool {
    metadata["categoryManual"]
        .as_bool()
        .unwrap_or_else(|| metadata["category"].as_str().unwrap_or("NonVRChat") != "NonVRChat")
}
fn apply_initial_category(overlay: &mut Value, metadata: &Value) {
    if category_manual(metadata) && overlay.get("category").is_none() {
        overlay["category"] = metadata["category"].clone();
    }
}
fn job_time(job: &Value) -> chrono::DateTime<chrono::Utc> {
    job["createdAt"]
        .as_str()
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.with_timezone(&chrono::Utc))
        .unwrap_or_else(chrono::Utc::now)
}
/// When the user added it, on this device's clock, written like a BOOTH receipt date.
fn added_date(job: &Value) -> String {
    crate::catalogue::acquisition::added_time(job_time(job), &chrono::Local).0
}
/// When the user added it, to the millisecond: the job's time plus a millisecond for each asset
/// the import listed before it, so assets added together keep their order within the minute
/// `added_date` shows.
fn added_at(job: &Value, index: usize) -> String {
    let at = job_time(job) + chrono::Duration::milliseconds(index as i64);
    crate::catalogue::acquisition::added_time(at, &chrono::Local).1
}
/// The link to read once for a product added with a date; earlier imports stay as they are.
fn listing_to_read(product: &Value) -> Option<&str> {
    let added = product["acquired"]
        .as_str()
        .is_some_and(|date| !date.is_empty());
    product["sourceUrl"]
        .as_str()
        .filter(|_| added && product["priceObservedAt"].is_null())
}
fn local_error(error: &anyhow::Error) -> &'static str {
    let raised = error.to_string();
    [
        "no_import_files",
        "local_source_changed",
        "local_import_too_many_files",
        "local_batch_requires_folder_or_zip",
        "local_membership_changed",
        "local_source_overlap",
        "local_source_contains_library",
        "unsafe_link",
    ]
    .into_iter()
    .find(|code| *code == raised)
    .unwrap_or_else(|| {
        let code = io::stable_error(error);
        if code == "storage_failed"
            && error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        {
            return "file_missing";
        }
        code
    })
}

impl StorageService {
    fn cleanup_removed_local(&self, group: &LocalGroup) -> Result<()> {
        let artifacts = self.catalogue.list("artifact")?;
        for marker in self.catalogue.related("acquisition", &group.product_id)? {
            if marker["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("local-import-fingerprint:"))
            {
                self.catalogue
                    .delete("acquisition", io::required_string(&marker, "id")?)?;
            }
        }
        for removed in &group.remove_file_ids {
            for artifact in artifacts
                .iter()
                .filter(|a| a["productId"] == group.product_id && a["fileId"] == *removed)
            {
                let root = self.root(artifact["rootId"].as_str())?;
                let root_path = self.root_path(&root)?;
                let relative = io::required_string(artifact, "relativePath")?;
                // Same-path references, user edits and unverifiable outputs retain their bytes.
                let shared = artifacts.iter().any(|a| {
                    a["id"] != artifact["id"]
                        && a["rootId"] == artifact["rootId"]
                        && (a["relativePath"] == relative
                            || a["extractedPath"]
                                .as_str()
                                .is_some_and(|p| path_contains(Path::new(p), Path::new(relative))))
                });
                let path = io::safe_join(&root_path, relative)?;
                if !shared
                    && io::verify(
                        &path,
                        artifact["hash"].as_str().unwrap_or(""),
                        artifact["sizeBytes"].as_u64().unwrap_or(u64::MAX),
                    )
                    .is_ok()
                {
                    fs::remove_file(&path)?;
                }
                if let Some(extracted) = artifact["extractedPath"].as_str() {
                    if !artifacts.iter().any(|a| {
                        a["id"] != artifact["id"]
                            && a["rootId"] == artifact["rootId"]
                            && (a["extractedPath"].as_str().is_some_and(|p| {
                                path_contains(Path::new(p), Path::new(extracted))
                                    || path_contains(Path::new(extracted), Path::new(p))
                            }) || a["relativePath"]
                                .as_str()
                                .is_some_and(|p| path_contains(Path::new(extracted), Path::new(p))))
                    }) {
                        let target = io::safe_join(&root_path, extracted)?;
                        let entries: Vec<archive::VerifiedEntry> =
                            serde_json::from_value(artifact["extractedInventory"].clone())
                                .unwrap_or_default();
                        // Every owned file must still match; never recurse into unrelated output.
                        if !entries.is_empty()
                            && entries
                                .iter()
                                .filter(|entry| !entry.is_directory)
                                .all(|entry| {
                                    io::safe_join(&target, &entry.relative_path)
                                        .ok()
                                        .is_some_and(|p| {
                                            io::verify(&p, &entry.hash, entry.size_bytes).is_ok()
                                        })
                                })
                        {
                            for entry in entries.iter().filter(|entry| !entry.is_directory) {
                                fs::remove_file(io::safe_join(&target, &entry.relative_path)?)?;
                            }
                            let mut dirs: Vec<_> =
                                entries.iter().filter(|e| e.is_directory).collect();
                            dirs.sort_by_key(|e| std::cmp::Reverse(e.relative_path.len()));
                            for entry in dirs {
                                let _ =
                                    fs::remove_dir(io::safe_join(&target, &entry.relative_path)?);
                            }
                            let _ = fs::remove_dir(&target);
                        }
                    }
                }
                self.catalogue
                    .delete("artifact", io::required_string(artifact, "id")?)?;
            }
            self.catalogue.delete("remote_file", removed)?;
        }
        Ok(())
    }
    pub(super) async fn mutate_local_files(&self, args: &Value) -> Result<Value> {
        let _operation = self
            .operations
            .try_read()
            .map_err(|_| anyhow::anyhow!("storage_busy"))?;
        let _gate = self.queue_gate.lock().await;
        let service = self.clone();
        let args = args.clone();
        tokio::task::spawn_blocking(move || service.prepare_local_membership(&args)).await?
    }

    fn prepare_local_membership(&self, args: &Value) -> Result<Value> {
        let product_id = io::required_string(args, "productId")?;
        let product = self
            .catalogue
            .get("product", product_id)?
            .ok_or_else(|| anyhow::anyhow!("product_missing"))?;
        // A store product accepts the user's own copies; its store files are
        // never removed here.
        ensure!(
            product["provider"] == "local" || strings(&args["removeFileIds"]).is_empty(),
            "local_product_required"
        );
        let draft = uuid::Uuid::parse_str(io::required_string(args, "draftId")?)?.to_string();
        let id = format!("local-import:{draft}");
        let fingerprint = io::key(&serde_json::to_string(args)?);
        if let Some(job) = self.catalogue.get("job", &id)? {
            ensure!(
                job["requestFingerprint"] == fingerprint,
                "local_draft_changed"
            );
            return super::api::answer(super::api::LocalJob {
                waiting: None,
                job_id: id,
                product_ids: vec![product_id.to_owned()],
                state: job["state"].as_str().map(str::to_owned),
            });
        }
        ensure!(
            !self.catalogue.list("job")?.iter().any(|job| matches!(
                job["state"].as_str(),
                Some("queued" | "running")
            ) && (strings(&job["productIds"])
                .contains(&product_id.to_owned())
                || job["productId"] == product_id)),
            "storage_busy"
        );
        let removes = strings(&args["removeFileIds"]);
        for key in ["removeFileIds", "addSources"] {
            ensure!(
                args.get(key).is_none()
                    || args[key]
                        .as_array()
                        .is_some_and(|values| values.iter().all(Value::is_string)),
                "invalid_arguments"
            );
        }
        let current = strings(&product["localManifest"]["fileIds"]);
        ensure!(
            removes.iter().all(|id| current.contains(id)),
            "file_missing"
        );
        let additions = strings(&args["addSources"]);
        ensure!(
            !removes.is_empty() || !additions.is_empty(),
            "invalid_arguments"
        );
        let artifacts = self.catalogue.related("artifact", product_id)?;
        let root_id = args["rootId"]
            .as_str()
            .or_else(|| artifacts.iter().find_map(|a| a["rootId"].as_str()));
        let root = self.root(root_id)?;
        let root_path = self.root_path(&root)?;
        let prefix = artifacts
            .iter()
            .find(|a| a["rootId"] == root["id"])
            .and_then(|a| a["productDirectory"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                // The same directory a store download would choose.
                format!(
                    "assets/{}",
                    io::readable_component(
                        product["sourceTitle"]
                            .as_str()
                            .or(product["title"].as_str())
                            .unwrap_or("Local"),
                        product_id
                    )
                )
            });
        io::safe_join(&root_path, &prefix)?;
        let mut group = LocalGroup {
            product_id: product_id.to_owned(),
            title: product["title"].as_str().unwrap_or("Local").to_owned(),
            prefix,
            files: vec![],
            directories: vec![],
            metadata: None,
            remove_file_ids: removes,
            membership: true,
            expected_mutation: product["localManifest"]["lastMutationId"]
                .as_str()
                .map(str::to_owned),
        };
        let mut names = HashSet::new();
        for source in additions {
            let path = PathBuf::from(source);
            ensure!(path.is_absolute(), "root_not_absolute");
            reject_source_link(&path)?;
            let path = path.canonicalize()?;
            ensure!(
                !(path.is_dir() && path_contains(&path, &root_path)),
                "local_source_contains_library"
            );
            let original = path_name(&path)?;
            let mut relative = original.clone();
            let mut n = 2;
            while names.contains(&relative.to_lowercase())
                || io::safe_join(&root_path, &format!("{}/{relative}", group.prefix))?.exists()
            {
                let stem = Path::new(&original).file_stem().unwrap().to_string_lossy();
                relative = match path
                    .extension()
                    .and_then(|e| e.to_str())
                    .filter(|_| path.is_file())
                {
                    Some(ext) => format!("{stem} ({n}).{ext}"),
                    None => format!("{original} ({n})"),
                };
                n += 1;
            }
            names.insert(relative.to_lowercase());
            enumerate(&path, &relative, &mut group, None)?;
        }
        let plan = LocalPlan {
            draft_id: draft,
            groups: vec![group],
            metadata: json!({}),
        };
        let job = json!({"id":id,"type":JOB_TYPE,"originDeviceId":self.catalogue.device_id(),"rootId":root["id"],"requestFingerprint":fingerprint,"productIds":[product_id],"completedProductIds":[],"localPlan":plan,"state":"queued","progress":0,"createdAt":now(),"updatedAt":now()});
        self.catalogue.put("job", &id, job)?;
        super::api::answer(super::api::LocalJob {
            waiting: None,
            job_id: id,
            product_ids: vec![product_id.to_owned()],
            state: Some("queued".into()),
        })
    }
    fn local_metadata(&self, args: &Value) -> Result<Value> {
        ensure!(
            args.get("categoryManual").is_none() || args["categoryManual"].is_boolean(),
            "invalid_arguments"
        );
        let category = args["category"].as_str().unwrap_or("NonVRChat");
        ensure!(
            [
                "Base",
                "Clothes",
                "Hair",
                "Accessory",
                "Texture",
                "Motion",
                "Tool",
                "Shader",
                "Gimmick",
                "World",
                "Udon",
                "NonVRChat"
            ]
            .contains(&category),
            "invalid_arguments"
        );
        let mut metadata = json!({"category":category,"categoryManual":category_manual(args),"tags":strings(&args["tags"])});
        if let Some(url) = args["sourceUrl"].as_str().filter(|s| !s.trim().is_empty()) {
            metadata["sourceUrl"] = json!(crate::native::normalize_public_url(url)?);
        }
        if let Some(thumbnail) = args["thumbnailPath"].as_str().filter(|s| !s.is_empty()) {
            archive::validate_relative_path(thumbnail)?;
            ensure!(thumbnail.starts_with("media/"), "invalid_arguments");
            io::ensure_regular(&io::safe_join(&self.catalogue.data_dir(), thumbnail)?)?;
            metadata["thumbnailPath"] = json!(thumbnail);
        }
        if let Some(url) = args["thumbnailUrl"].as_str().filter(|s| !s.is_empty()) {
            metadata["sourceThumbnailUrl"] = json!(crate::native::normalize_public_url(url)?);
        }
        Ok(metadata)
    }

    pub(super) async fn enqueue_local(&self, args: &Value) -> Result<Value> {
        let _work = self.catalogue.begin_catalogue_work()?;
        let _gate = self.queue_gate.lock().await;
        ensure!(
            !self.closing_local_imports.load(Ordering::Acquire),
            "storage_busy"
        );
        let service = self.clone();
        let args = args.clone();
        tokio::task::spawn_blocking(move || {
            let (job, is_new) = service.local_request_job(&args)?;
            let id = io::required_string(&job, "id")?;
            if is_new {
                service.catalogue.put("job", id, job.clone())?;
            }
            super::api::answer(super::api::LocalJob {
                waiting: service.catalogue.queued_device_job_count(JOB_TYPE).ok(),
                job_id: id.to_owned(),
                product_ids: super::api::ids(&job["productIds"]),
                state: job["state"].as_str().map(str::to_owned),
            })
        })
        .await?
    }

    #[cfg(test)]
    pub(super) fn prepare_local_import(&self, args: &Value) -> Result<Value> {
        let (mut job, is_new) = self.local_request_job(args)?;
        if !is_new {
            return super::api::answer(super::api::LocalJob {
                waiting: None,
                job_id: io::required_string(&job, "id")?.to_owned(),
                product_ids: super::api::ids(&job["productIds"]),
                state: job["state"].as_str().map(str::to_owned),
            });
        }
        self.plan_local_request(&mut job, None)?;
        self.catalogue
            .put("job", io::required_string(&job, "id")?, job.clone())?;
        super::api::answer(super::api::LocalJob {
            waiting: None,
            job_id: io::required_string(&job, "id")?.to_owned(),
            product_ids: super::api::ids(&job["productIds"]),
            state: None,
        })
    }

    fn local_request_job(&self, args: &Value) -> Result<(Value, bool)> {
        let draft = uuid::Uuid::parse_str(io::required_string(args, "draftId")?)?.to_string();
        let id = format!("local-import:{draft}");
        let fingerprint = io::key(&serde_json::to_string(args)?);
        if let Some(job) = self.catalogue.get("job", &id)? {
            ensure!(job["type"] == JOB_TYPE, "invalid_job");
            ensure!(
                job["requestFingerprint"] == fingerprint,
                "local_draft_changed"
            );
            return Ok((job, false));
        }
        let mode = args["mode"].as_str().unwrap_or("single");
        ensure!(matches!(mode, "single" | "batch"), "invalid_arguments");
        let sources = args["sources"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("invalid_arguments"))?;
        ensure!(
            !sources.is_empty() && sources.len() <= 10_000 && sources.iter().all(Value::is_string),
            "invalid_arguments"
        );
        ensure!(
            sources
                .iter()
                .all(|path| Path::new(path.as_str().unwrap()).is_absolute()),
            "root_not_absolute"
        );
        let root = self.root(args["rootId"].as_str())?;
        let title = args["name"]
            .as_str()
            .filter(|name| !name.trim().is_empty())
            .or_else(|| {
                Path::new(sources[0].as_str().unwrap())
                    .file_name()
                    .and_then(|name| name.to_str())
            });
        let planned_count = if mode == "single" { 1 } else { sources.len() };
        let planned_ids: Vec<_> = (0..planned_count)
            .map(|_| format!("local:{}", uuid::Uuid::new_v4()))
            .collect();
        let time = now();
        let job = json!({
            "id":id, "type":JOB_TYPE, "originDeviceId":self.catalogue.device_id(),
            "rootId":root["id"], "requestFingerprint":fingerprint, "localRequest":args,
            "localTitle":title, "productIds":planned_ids, "completedProductIds":[], "mode":mode,
            "state":"queued", "progress":0, "createdAt":time, "updatedAt":time
        });
        Ok((job, true))
    }

    fn plan_local_request(&self, job: &mut Value, cancel: Option<&AtomicBool>) -> Result<()> {
        let args = &job["localRequest"];
        let draft = uuid::Uuid::parse_str(io::required_string(args, "draftId")?)?.to_string();
        let mode = job["mode"].as_str().unwrap_or("single");
        let raw = args["sources"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("invalid_arguments"))?;
        let root = self.root(Some(io::required_string(job, "rootId")?))?;
        let root_path = self.root_path(&root)?.canonicalize()?;
        let metadata = self.local_metadata(args)?;
        let mut sources: Vec<PathBuf> = Vec::new();
        for value in raw {
            ensure!(
                !cancel.is_some_and(|stop| stop.load(Ordering::Acquire)),
                "cancelled"
            );
            let source = PathBuf::from(
                value
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("invalid_arguments"))?,
            );
            ensure!(source.is_absolute(), "root_not_absolute");
            reject_source_link(&source)?;
            let source = source.canonicalize()?;
            ensure!(
                !(source.is_dir() && path_contains(&source, &root_path)),
                "local_source_contains_library"
            );
            if mode == "batch" {
                ensure!(
                    source.is_dir()
                        || source
                            .extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| e.eq_ignore_ascii_case("zip")),
                    "local_batch_requires_folder_or_zip"
                );
            }
            if !sources.iter().any(|existing| {
                path_contains(existing, &source) && path_contains(&source, existing)
            }) {
                sources.push(source);
            }
        }
        sources.sort();
        // Do not copy an explicitly selected descendant twice.
        ensure!(
            !sources
                .iter()
                .any(|a| a.is_dir() && sources.iter().any(|b| a != b && path_contains(a, b))),
            "local_source_overlap"
        );
        let selections = if mode == "single" {
            vec![sources]
        } else {
            sources.into_iter().map(|s| vec![s]).collect()
        };
        let reserved_ids = strings(&job["productIds"]);
        let mut groups = Vec::new();
        for (index, selected) in selections.into_iter().enumerate() {
            let item_metadata = args["products"].as_array().and_then(|items| {
                items.iter().find(|item| {
                    item["source"].as_str().is_some_and(|path| {
                        PathBuf::from(path).canonicalize().ok().as_ref() == Some(&selected[0])
                    })
                })
            });
            let fields = item_metadata.unwrap_or(args);
            let product_id = reserved_ids
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("local:{}", uuid::Uuid::new_v4()));
            let title = if mode == "single" || item_metadata.is_some() {
                fields["name"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .map(str::to_owned)
            } else {
                None
            }
            .unwrap_or(path_name(&selected[0])?);
            let prefix = format!("assets/{}", io::readable_component(&title, &product_id));
            let mut group = LocalGroup {
                product_id,
                title,
                prefix,
                files: Vec::new(),
                directories: Vec::new(),
                metadata: item_metadata
                    .map(|value| self.local_metadata(value))
                    .transpose()?,
                remove_file_ids: Vec::new(),
                membership: false,
                expected_mutation: None,
            };
            let mut top_names = HashSet::new();
            for source in &selected {
                let original = path_name(source)?;
                let mut relative = original.clone();
                let mut suffix = 2;
                while !top_names.insert(relative.to_lowercase()) {
                    let path = Path::new(&original);
                    relative = match path
                        .extension()
                        .and_then(|v| v.to_str())
                        .filter(|_| source.is_file())
                    {
                        Some(ext) => format!(
                            "{} ({suffix}).{ext}",
                            path.file_stem().unwrap().to_string_lossy()
                        ),
                        None => format!("{original} ({suffix})"),
                    };
                    suffix += 1;
                }
                enumerate(source, &relative, &mut group, cancel)?;
            }
            ensure!(!group.files.is_empty(), "no_import_files");
            // Windows and portable archives must not alias distinct source entries.
            let mut names = HashSet::new();
            for relative in group
                .directories
                .iter()
                .chain(group.files.iter().map(|f| &f.relative))
            {
                ensure!(
                    names.insert(relative.to_lowercase()),
                    "archive_path_collision"
                );
            }
            group.directories.sort();
            groups.push(group);
        }
        let plan = LocalPlan {
            draft_id: draft,
            groups,
            metadata,
        };
        let product_ids: Vec<_> = plan.groups.iter().map(|g| g.product_id.clone()).collect();
        job["productIds"] = json!(product_ids);
        job["localPlan"] = serde_json::to_value(plan)?;
        job.as_object_mut().unwrap().remove("localRequest");
        Ok(())
    }

    pub(super) async fn retry_local_import(&self, id: &str) -> Result<Value> {
        let _gate = self.queue_gate.lock().await;
        ensure!(
            !self.closing_local_imports.load(Ordering::Acquire),
            "storage_busy"
        );
        let mut job = self
            .catalogue
            .get("job", id)?
            .ok_or_else(|| anyhow::anyhow!("job_missing"))?;
        ensure!(
            job["type"] == JOB_TYPE
                && (job["localPlan"].is_object() || job["localRequest"].is_object()),
            "invalid_job"
        );
        ensure!(
            self.catalogue.library_root().is_none()
                || job["originDeviceId"] == self.catalogue.device_id(),
            "local_other_device"
        );
        if !matches!(
            job["state"].as_str(),
            Some("queued" | "running" | "completed")
        ) {
            job["cancelRequested"] = json!(false);
            self.cancellation(id).store(false, Ordering::Release);
            self.put_job(&mut job, "queued", None)?;
        }
        super::api::answer(super::api::LocalJob {
            waiting: None,
            job_id: id.to_owned(),
            product_ids: super::api::ids(&job["productIds"]),
            state: None,
        })
    }

    pub(crate) fn begin_local_import_shutdown(&self) -> bool {
        !self.closing_local_imports.swap(true, Ordering::AcqRel)
    }

    pub(crate) async fn discard_waiting_local_imports(&self) -> Result<()> {
        let _gate = self.queue_gate.lock().await;
        let service = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut records = Vec::new();
            let mut expected = Vec::new();
            for mut job in service.catalogue.queued_jobs(Some(JOB_TYPE), None)? {
                if job["originDeviceId"] != service.catalogue.device_id()
                    || !matches!(job["mode"].as_str(), Some("single" | "batch"))
                {
                    continue;
                }
                let id = io::required_string(&job, "id")?.to_owned();
                expected.push(("job".into(), id.clone(), Some(job.clone())));
                let record = if job["localRequest"].is_object() && job.get("localPlan").is_none() {
                    None
                } else {
                    // An older job or a retry may own checkpoints or published files.
                    job["state"] = json!("needsAction");
                    job["updatedAt"] = json!(now());
                    job["errorCode"] = json!("interrupted");
                    Some(job)
                };
                records.push(("job".into(), id, record));
            }
            if !records.is_empty() {
                ensure!(
                    service
                        .catalogue
                        .write_many_if_unchanged(records, expected)?,
                    "storage_busy"
                );
            }
            Ok(())
        })
        .await?
    }

    /// Drops an import that stopped: what it copied for assets it did not add, its saved
    /// progress and the job. The originals and the assets it added stay.
    pub(super) async fn discard_local_import(&self, id: &str) -> Result<Value> {
        let _operation = self
            .operations
            .try_read()
            .map_err(|_| anyhow::anyhow!("storage_busy"))?;
        let _gate = self.queue_gate.lock().await;
        let job = self
            .catalogue
            .get("job", id)?
            .ok_or_else(|| anyhow::anyhow!("job_missing"))?;
        ensure!(
            job["type"] == JOB_TYPE
                && (job["localPlan"].is_object() || job["localRequest"].is_object())
                && matches!(
                    job["state"].as_str(),
                    Some("failed" | "needsAction" | "cancelled")
                ),
            "invalid_job"
        );
        if job["localPlan"].is_object() {
            let service = self.clone();
            tokio::task::spawn_blocking(move || service.discard_local_copies(&job)).await??;
        } else {
            self.catalogue.delete("job", id)?;
        }
        super::api::answer(super::api::ImportDiscarded {
            job_id: id.to_owned(),
        })
    }

    /// Files go before records, so a discard that stops part way can run again.
    fn discard_local_copies(&self, job: &Value) -> Result<()> {
        let plan: LocalPlan = serde_json::from_value(job["localPlan"].clone())?;
        let root = self.root(job["rootId"].as_str())?;
        let root_path = self.root_path(&root)?;
        let artifacts = self.catalogue.list("artifact")?;
        let mut checkpoints = Vec::new();
        for (index, group) in plan.groups.iter().enumerate() {
            checkpoints.extend(
                group
                    .files
                    .iter()
                    .map(|file| checkpoint_id(&plan.draft_id, group, file)),
            );
            if job["completedGroupIndexes"]
                .as_array()
                .is_some_and(|done| done.contains(&json!(index)))
            {
                continue;
            }
            let directory = io::safe_join(&root_path, &group.prefix)?;
            let product = self.catalogue.get("product", &group.product_id)?;
            if product.is_none() && !group.membership {
                // The folder of an asset it never added holds only what this import put there.
                if fs::symlink_metadata(&directory).is_ok() {
                    super::local_delete::product_directory(&group.prefix)?;
                    self.require_unshared_delete_path(&group.product_id, &directory)?;
                    super::local_delete::remove_checked_tree(&directory)?;
                }
                continue;
            }
            // Beside an asset's own files only its unchanged copies go that no record uses.
            for file in &group.files {
                let Some(copy) = self
                    .catalogue
                    .get("checkpoint", &checkpoint_id(&plan.draft_id, group, file))?
                else {
                    continue;
                };
                let relative = io::required_string(&copy, "relativePath")?;
                let used = artifacts.iter().any(|a| {
                    a["rootId"] == root["id"]
                        && (a["relativePath"] == relative
                            || a["extractedPath"]
                                .as_str()
                                .is_some_and(|p| path_contains(Path::new(p), Path::new(relative))))
                });
                let path = io::safe_join(&root_path, relative)?;
                let size = copy["sizeBytes"].as_u64().unwrap_or(u64::MAX);
                if !used && io::verify(&path, io::required_string(&copy, "hash")?, size).is_ok() {
                    fs::remove_file(&path)?;
                }
            }
            let kept = product
                .as_ref()
                .map(|p| strings(&p["localManifest"]["directories"]))
                .unwrap_or_default();
            let mut created: Vec<_> = group
                .directories
                .iter()
                .map(|d| format!("{}/{d}", group.prefix))
                .filter(|d| !kept.contains(d))
                .collect();
            created.sort_by_key(|d| std::cmp::Reverse(d.len()));
            for relative in created {
                let _ = fs::remove_dir(io::safe_join(&root_path, &relative)?);
            }
            if product.is_none() {
                let _ = fs::remove_dir(&directory);
            }
        }
        let partials = io::safe_join(
            &root_path,
            &format!(".nna-import-partials/{}", plan.draft_id),
        )?;
        if fs::symlink_metadata(&partials).is_ok() {
            super::local_delete::remove_checked_tree(&partials)?;
        }
        if let Some(parent) = partials.parent() {
            let _ = fs::remove_dir(parent);
        }
        for id in checkpoints {
            self.catalogue.delete("checkpoint", &id)?;
        }
        self.catalogue
            .delete("job", io::required_string(job, "id")?)?;
        Ok(())
    }

    /// Called under the existing queue gate; local products never request store tickets.
    pub(super) fn retry_local_product_locked(
        &self,
        product: &str,
        jobs: &[Value],
    ) -> Result<Option<String>> {
        let (files, complete) = self.remote_files(product)?;
        let mut ready = complete;
        for file in &files {
            let artifact = self.catalogue.get("artifact", &artifact_id(file))?;
            if let Some(artifact) = artifact.filter(|a| a["status"] == "ready") {
                let root = self.root_path(&self.root(artifact["rootId"].as_str())?)?;
                let path = io::safe_join(&root, io::required_string(&artifact, "relativePath")?)?;
                ready &= artifact["originalAvailable"] == true
                    && fs::metadata(path).is_ok_and(|m| {
                        m.is_file() && Some(m.len()) == artifact["sizeBytes"].as_u64()
                    });
            } else {
                ready = false;
            }
        }
        if ready {
            return Ok(None);
        }
        // An import already queued or running for it does the work.
        if let Some(job) = jobs.iter().find(|job| {
            job["type"] == JOB_TYPE
                && matches!(job["state"].as_str(), Some("queued" | "running"))
                && strings(&job["productIds"]).iter().any(|id| id == product)
        }) {
            return Ok(job["id"].as_str().map(str::to_owned));
        }
        let source = self
            .catalogue
            .get("product", product)?
            .ok_or_else(|| anyhow::anyhow!("product_missing"))?;
        let entries = self.source_entries(product)?;
        if source["localManifest"]["lastMutationId"].is_string() {
            // Build only from current membership and its recorded sources.
            // Never replay an old import or its old remove delta after a user edit.
            ensure!(!files.is_empty(), "local_source_required");
            let artifacts = self.catalogue.related("artifact", product)?;
            let first = files.first().unwrap();
            let artifact = artifacts
                .iter()
                .find(|a| a["fileId"] == first["id"])
                .ok_or_else(|| anyhow::anyhow!("local_source_required"))?;
            let prefix = io::required_string(artifact, "productDirectory")?.to_owned();
            let root = self.root(artifact["rootId"].as_str())?;
            let mut current_files = Vec::new();
            for file in &files {
                // The newest entry wins: a removed file's name can be added again.
                let selected = entries
                    .iter()
                    .rev()
                    .filter(|entry| entry.group.prefix == prefix)
                    .flat_map(|entry| &entry.group.files)
                    .find(|candidate| {
                        file_id(product, &candidate.relative) == file["id"].as_str().unwrap_or("")
                    });
                current_files.push(
                    selected
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("local_source_required"))?,
                );
            }
            let group = LocalGroup {
                product_id: product.to_owned(),
                title: source["title"].as_str().unwrap_or("Local").to_owned(),
                prefix: prefix.clone(),
                files: current_files,
                directories: strings(&source["localManifest"]["directories"])
                    .iter()
                    .filter_map(|dir| dir.strip_prefix(&format!("{prefix}/")).map(str::to_owned))
                    .collect(),
                metadata: None,
                remove_file_ids: vec![],
                membership: true,
                expected_mutation: source["localManifest"]["lastMutationId"]
                    .as_str()
                    .map(str::to_owned),
            };
            return self
                .queue_local_repair(group, io::required_string(&root, "id")?)
                .map(Some);
        }
        // The import that added it resumes while it is listed. Otherwise its recorded
        // group runs again in a job of its own.
        let added = |group: &Value| group["product_id"] == product && group["membership"] != true;
        let owner = jobs.iter().find(|job| {
            job["type"] == JOB_TYPE
                && (self.catalogue.library_root().is_none()
                    || job["originDeviceId"] == self.catalogue.device_id())
                && job["localPlan"]["groups"]
                    .as_array()
                    .is_some_and(|groups| groups.iter().any(added))
        });
        let Some(id) = owner.and_then(|j| j["id"].as_str()) else {
            let entry = entries
                .into_iter()
                .rev()
                .find(|entry| !entry.group.membership)
                .ok_or_else(|| anyhow::anyhow!("local_source_required"))?;
            return self
                .queue_local_repair(entry.group, &entry.root_id)
                .map(Some);
        };
        let mut job = self
            .catalogue
            .get("job", id)?
            .ok_or_else(|| anyhow::anyhow!("job_missing"))?;
        if matches!(job["state"].as_str(), Some("queued" | "running")) {
            return Ok(Some(id.to_owned()));
        }
        let index = job["localPlan"]["groups"]
            .as_array()
            .and_then(|groups| groups.iter().position(added))
            .ok_or_else(|| anyhow::anyhow!("local_source_required"))?;
        job["completedProductIds"] = json!(strings(&job["completedProductIds"])
            .into_iter()
            .filter(|id| id != product)
            .collect::<Vec<_>>());
        job["completedGroupIndexes"] = json!(job["completedGroupIndexes"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|v| v != &json!(index))
            .collect::<Vec<_>>());
        job["cancelRequested"] = json!(false);
        self.cancellation(id).store(false, Ordering::Release);
        self.put_job(&mut job, "queued", None)?;
        Ok(Some(id.to_owned()))
    }

    /// Copies one group again in a job of its own.
    fn queue_local_repair(&self, group: LocalGroup, root: &str) -> Result<String> {
        let draft = uuid::Uuid::new_v4().to_string();
        let id = format!("local-import:{draft}");
        let product = group.product_id.clone();
        let plan = LocalPlan {
            draft_id: draft,
            groups: vec![group],
            metadata: json!({}),
        };
        let job = json!({"id":id,"type":JOB_TYPE,"originDeviceId":self.catalogue.device_id(),"rootId":root,"requestFingerprint":io::key(&serde_json::to_string(&plan)?),"productIds":[product],"completedProductIds":[],"localPlan":plan,"state":"queued","progress":0,"createdAt":now(),"updatedAt":now()});
        self.catalogue.put("job", &id, job)?;
        Ok(id)
    }

    /// The product's source record with a published group added.
    fn with_source(
        &self,
        job: &Value,
        plan: &LocalPlan,
        group: &LocalGroup,
        root: &str,
    ) -> Result<Value> {
        let mut record = match self.catalogue.get(SOURCE_KIND, &group.product_id)? {
            Some(value) => serde_json::from_value(value)?,
            None => LocalSource::new(&group.product_id),
        };
        record.add(job, plan, group, root);
        Ok(serde_json::to_value(record)?)
    }

    /// This device's recorded sources of a product, oldest first.
    fn source_entries(&self, product: &str) -> Result<Vec<SourceEntry>> {
        let Some(value) = self.catalogue.get(SOURCE_KIND, product)? else {
            return Ok(vec![]);
        };
        let record: LocalSource = serde_json::from_value(value)?;
        Ok(record
            .entries
            .into_iter()
            .filter(|entry| {
                self.catalogue.library_root().is_none()
                    || entry.origin_device_id == self.catalogue.device_id()
            })
            .collect())
    }

    /// Products imported before they kept a source record have their sources only in
    /// import jobs. Recording what those jobs published lets Clear history remove them.
    pub(super) fn record_listed_local_sources(&self) -> Result<()> {
        let mut jobs: Vec<_> = self
            .catalogue
            .list("job")?
            .into_iter()
            .filter(|job| job["type"] == JOB_TYPE)
            .collect();
        // In the order they ran, so each edit replaces what came before it.
        jobs.sort_by(|a, b| a["createdAt"].as_str().cmp(&b["createdAt"].as_str()));
        // None: a product that already has its record, or is not local.
        let mut records: BTreeMap<String, Option<LocalSource>> = BTreeMap::new();
        for job in &jobs {
            let (Ok(plan), Some(root)) = (
                serde_json::from_value::<LocalPlan>(job["localPlan"].clone()),
                job["rootId"].as_str(),
            ) else {
                continue;
            };
            let completed = job["completedGroupIndexes"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for (index, group) in plan.groups.iter().enumerate() {
                // A product exists once its import published it; an edit, once complete.
                if group.membership && !completed.contains(&json!(index)) {
                    continue;
                }
                let id = &group.product_id;
                if !records.contains_key(id) {
                    let local = self
                        .catalogue
                        .get("product", id)?
                        .is_some_and(|product| product["provider"] == "local");
                    let missing = local && self.catalogue.get(SOURCE_KIND, id)?.is_none();
                    records.insert(id.clone(), missing.then(|| LocalSource::new(id)));
                }
                if let Some(Some(record)) = records.get_mut(id) {
                    record.add(job, &plan, group, root);
                }
            }
        }
        let mut writes = Vec::new();
        for (id, record) in records {
            if let Some(record) = record {
                writes.push((SOURCE_KIND.to_owned(), id, serde_json::to_value(record)?));
            }
        }
        if !writes.is_empty() {
            self.catalogue.put_many(writes)?;
        }
        Ok(())
    }

    /// The product directories local imports own: each published group's, from its
    /// product's source record, and each group of an import still listed.
    pub(super) fn local_import_directories(&self) -> Result<Vec<(String, String, String)>> {
        let mut directories = Vec::new();
        for record in self.catalogue.list(SOURCE_KIND)? {
            for entry in record["entries"].as_array().into_iter().flatten() {
                directories.push((
                    io::required_string(&record, "productId")?.to_owned(),
                    io::required_string(entry, "rootId")?.to_owned(),
                    io::required_string(&entry["group"], "prefix")?.to_owned(),
                ));
            }
        }
        for job in self.catalogue.list("job")? {
            if job["type"] != JOB_TYPE {
                continue;
            }
            for group in job["localPlan"]["groups"].as_array().into_iter().flatten() {
                directories.push((
                    io::required_string(group, "product_id")?.to_owned(),
                    io::required_string(&job, "rootId")?.to_owned(),
                    io::required_string(group, "prefix")?.to_owned(),
                ));
            }
        }
        Ok(directories)
    }

    fn copy_local_file(
        &self,
        root: &Path,
        draft: &str,
        group: &LocalGroup,
        file: &LocalFile,
        cancel: &AtomicBool,
        mut report: impl FnMut(Option<u64>),
    ) -> Result<Value> {
        io::check_cancel(cancel)?;
        let checkpoint = checkpoint_id(draft, group, file);
        let relative = format!("{}/{}", group.prefix, file.relative);
        let target = io::safe_join(root, &relative)?;
        let recorded = self.catalogue.get("checkpoint", &checkpoint)?;
        if let Some(record) = recorded.as_ref() {
            ensure!(record["relativePath"] == relative, "file_integrity_failed");
            if target.try_exists()? {
                report(None);
                io::verify_cancel(
                    &target,
                    io::required_string(record, "hash")?,
                    record["sizeBytes"]
                        .as_u64()
                        .ok_or_else(|| anyhow::anyhow!("file_integrity_failed"))?,
                    cancel,
                )?;
                return Ok(record.clone());
            }
            // A remapped root may lack a saved copy. Recopy unchanged input;
            // an existing damaged target above is retained and never overwritten.
        }
        if target.exists() {
            // A published copy remains valid even if its original source is now unavailable.
            if let Some(descriptor) = self
                .catalogue
                .get("remote_file", &file_id(&group.product_id, &file.relative))?
            {
                if let Some(artifact) = self.catalogue.get("artifact", &artifact_id(&descriptor))? {
                    if artifact["relativePath"] == relative {
                        let hash = io::required_string(&artifact, "hash")?;
                        let size = artifact["sizeBytes"]
                            .as_u64()
                            .ok_or_else(|| anyhow::anyhow!("file_integrity_failed"))?;
                        report(None);
                        io::verify_cancel(&target, hash, size, cancel)?;
                        let record = json!({"id":checkpoint,"kind":"localImportCopy","relativePath":relative,"hash":hash,"sizeBytes":size});
                        self.catalogue
                            .put("checkpoint", &checkpoint, record.clone())?;
                        return Ok(record);
                    }
                }
            }
        }
        ensure!(
            signature(&file.source, &file.relative)? == (file.size, file.modified),
            "local_source_changed"
        );
        io::space(root, file.size)?;
        fs::create_dir_all(target.parent().unwrap())?;
        io::safe_join(root, &relative)?;
        let partial_relative =
            format!(".nna-import-partials/{draft}/{}.part", io::key(&checkpoint));
        let partial = io::safe_join(root, &partial_relative)?;
        fs::create_dir_all(partial.parent().unwrap())?;
        let _empty_partials =
            io::EmptyDirectoryGuard(partial.parent().unwrap().parent().unwrap().to_path_buf());
        let _partial = io::PartialGuard::new(partial.clone());
        if partial.exists() {
            io::ensure_regular(&partial)?;
            fs::remove_file(&partial)?;
        }
        // A scanner or the program that just wrote the file may hold it briefly.
        let mut input = io::retry_busy(|| fs::File::open(&file.source)).map_err(source_error)?;
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&partial)?;
        let copied = (|| -> Result<(String, u64)> {
            let mut hash = Sha256::new();
            let mut size = 0u64;
            let mut buffer = [0u8; 128 * 1024];
            loop {
                io::check_cancel(cancel)?;
                let count = input.read(&mut buffer).map_err(source_error)?;
                if count == 0 {
                    break;
                }
                output.write_all(&buffer[..count])?;
                hash.update(&buffer[..count]);
                size += count as u64;
                report(Some(size));
            }
            report(None);
            output.sync_all()?;
            ensure!(
                size == file.size
                    && signature(&file.source, &file.relative)? == (file.size, file.modified),
                "local_source_changed"
            );
            let hash = hex::encode(hash.finalize());
            if let Some(record) = recorded.as_ref() {
                ensure!(
                    record["hash"] == hash && record["sizeBytes"] == size,
                    "local_source_changed"
                );
            }
            io::verify_cancel(&partial, &hash, size, cancel)?;
            Ok((hash, size))
        })();
        drop(output);
        let (hash, size) = match copied {
            Ok(value) => value,
            Err(error) => {
                let _ = fs::remove_file(&partial);
                return Err(error);
            }
        };
        io::check_cancel(cancel)?;
        if target.exists() {
            // Crash between publishing a copy and recording its checkpoint.
            io::verify_cancel(&target, &hash, size, cancel)?;
        } else {
            // Hard-link is exclusive and publishes the already flushed file without overwriting.
            if fs::hard_link(&partial, &target).is_err() {
                // Managed roots on filesystems without hard links retain the exclusive copy path.
                io::copy_new_verified(&partial, &target, &hash, size, cancel)?;
            }
        }
        // A scanner may still hold the copy it just watched being written.
        io::retry_busy(|| fs::remove_file(&partial))?;
        let record = json!({"id":checkpoint,"kind":"localImportCopy","relativePath":relative,"hash":hash,"sizeBytes":size});
        self.catalogue
            .put("checkpoint", &checkpoint, record.clone())?;
        Ok(record)
    }

    #[cfg(test)]
    pub(super) fn run_local_import(
        &self,
        job: &mut Value,
        seven_zip: &Path,
        cancel: &AtomicBool,
    ) -> Result<()> {
        self.run_local_import_inner(job, seven_zip, cancel, None)
    }

    fn run_local_import_inner(
        &self,
        job: &mut Value,
        seven_zip: &Path,
        cancel: &AtomicBool,
        app: Option<&tauri::AppHandle>,
    ) -> Result<()> {
        let plan: LocalPlan = serde_json::from_value(job["localPlan"].clone())?;
        let root = self.root(job["rootId"].as_str())?;
        let root_path = self.root_path(&root)?;
        let mut first_error = None;
        let mut failures = Vec::new();
        let job_id = io::required_string(job, "id")?.to_owned();
        let total = plan
            .groups
            .iter()
            .flat_map(|group| &group.files)
            .fold(0u64, |sum, file| sum.saturating_add(file.size));
        let mut processed = 0u64;
        let mut last_event = None::<(&'static str, Instant)>;
        let mut report = |phase: &'static str, bytes: Option<u64>, immediate: bool| {
            if let Some(app) = app {
                if immediate
                    || last_event.is_none_or(|(previous_phase, time)| {
                        (phase != "copying" && phase != previous_phase)
                            || time.elapsed() >= Duration::from_millis(350)
                    })
                {
                    last_event = Some((phase, Instant::now()));
                    emit_local_progress(app, &job_id, phase, bytes, bytes.map(|_| total));
                }
            }
        };
        for (index, group) in plan.groups.iter().enumerate() {
            let group_end = group
                .files
                .iter()
                .fold(processed, |sum, file| sum.saturating_add(file.size));
            let completed = job["completedGroupIndexes"]
                .as_array()
                .is_some_and(|a| a.contains(&json!(index)));
            if completed && !group.membership && plan.groups.len() > 1 {
                processed = group_end;
                continue;
            }
            let mut report_group = |phase: &'static str, bytes: Option<u64>, immediate: bool| {
                report(
                    phase,
                    bytes.map(|done| processed.saturating_add(done)),
                    immediate,
                );
            };
            let outcome = self.import_local_group(
                job,
                &plan,
                group,
                index,
                completed,
                &root,
                &root_path,
                seven_zip,
                cancel,
                app,
                &mut report_group,
            );
            processed = group_end;
            if let Err(error) = outcome {
                let code = local_error(&error);
                if code == "cancelled" {
                    return Err(error);
                }
                failures.push(
                    json!({"groupIndex":index,"productId":group.product_id,"errorCode":code}),
                );
                first_error.get_or_insert(error);
            }
        }
        job["result"] = json!({"productIds":job["productIds"],"completedProductIds":job["completedProductIds"],"failedGroups":failures});
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(())
    }

    /// Copies one group's files, records or reuses its product and commits the job's progress.
    fn import_local_group(
        &self,
        job: &mut Value,
        plan: &LocalPlan,
        group: &LocalGroup,
        index: usize,
        completed: bool,
        root: &Value,
        root_path: &Path,
        seven_zip: &Path,
        cancel: &AtomicBool,
        app: Option<&tauri::AppHandle>,
        report: &mut impl FnMut(&'static str, Option<u64>, bool),
    ) -> Result<()> {
        if let Some(product) = self.catalogue.get("product", &group.product_id)? {
            let current = product["localManifest"]["lastMutationId"].as_str();
            if group.membership {
                ensure!(
                    current == group.expected_mutation.as_deref()
                        || current == Some(plan.draft_id.as_str()),
                    "local_membership_changed"
                );
            } else {
                ensure!(current.is_none(), "local_membership_changed");
            }
        }
        if completed {
            // Membership cleanup must still be bound to its captured
            // mutation. A later membership edit invalidates this replay.
            if group.membership {
                self.cleanup_removed_local(group)?;
            }
            return Ok(());
        }
        io::check_cancel(cancel)?;
        for directory in &group.directories {
            fs::create_dir_all(io::safe_join(
                root_path,
                &format!("{}/{directory}", group.prefix),
            )?)?;
        }
        let mut copies = Vec::with_capacity(group.files.len());
        let mut copied = 0u64;
        report("copying", Some(copied), true);
        for file in &group.files {
            let checkpoint =
                self.copy_local_file(root_path, &plan.draft_id, group, file, cancel, |bytes| {
                    match bytes {
                        Some(bytes) => report(
                            "copying",
                            Some(copied.saturating_add(bytes.min(file.size))),
                            false,
                        ),
                        None => report("verifying", None, false),
                    }
                })?;
            copies.push(checkpoint);
            copied = copied.saturating_add(file.size);
        }
        report("finalizing", None, true);
        let facts: Vec<_> = group
            .files
            .iter()
            .zip(&copies)
            .map(|(file, copy)| json!([file.relative, copy["hash"], copy["sizeBytes"]]))
            .collect();
        let fingerprint = io::key(&serde_json::to_string(
            &json!({"files":facts,"directories":group.directories}),
        )?);
        let marker_id = format!("local-import-fingerprint:{fingerprint}");
        let duplicate = self
            .catalogue
            .get("acquisition", &marker_id)?
            .and_then(|v| v["productId"].as_str().map(str::to_owned));
        let duplicate =
            if let Some(id) = duplicate.filter(|id| id != &group.product_id && !group.membership) {
                self.catalogue
                    .get("product", &id)?
                    .filter(|p| p["provider"] == "local")
                    .map(|_| id)
            } else {
                None
            };
        let mut writes = Vec::new();
        let mut error = None;
        let product_id = if let Some(existing) = duplicate {
            existing
        } else {
            self.record_local_group(
                group,
                plan,
                job,
                index,
                root,
                root_path,
                &copies,
                marker_id,
                &mut writes,
                &mut error,
                seven_zip,
                cancel,
                &mut |phase| report(phase, None, false),
            )?
        };
        io::check_cancel(cancel)?;
        report("finalizing", None, false);
        let mut next_job = job.clone();
        next_job["productIds"][index] = json!(product_id);
        if error.is_none() {
            let mut completed = strings(&next_job["completedProductIds"]);
            if !completed.contains(&product_id) {
                completed.push(product_id.clone());
            }
            next_job["completedProductIds"] = json!(completed);
            let mut indexes = next_job["completedGroupIndexes"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            indexes.push(json!(index));
            next_job["completedGroupIndexes"] = json!(indexes);
            next_job["progress"] = json!(100.0 * (index + 1) as f64 / plan.groups.len() as f64);
        }
        next_job["updatedAt"] = json!(now());
        writes.push((
            "job".into(),
            io::required_string(job, "id")?.to_owned(),
            next_job.clone(),
        ));
        // The record was built before its files were copied. What an earlier run of a retry
        // or repair recorded, or a price read meanwhile, stays.
        for (kind, id, value) in &mut writes {
            if kind == "product" {
                if let Some(latest) = self.catalogue.get("product", id)? {
                    for key in [
                        "acquired",
                        "acquiredAt",
                        "priceObservedAt",
                        "priceSourceUrl",
                    ]
                    .iter()
                    .chain(&PRICE_FIELDS)
                    {
                        if let Some(recorded) = latest.get(*key) {
                            value[*key] = recorded.clone();
                        }
                    }
                    // A product added before acquiredAt existed keeps sorting by its minute.
                    if latest.get("acquiredAt").is_none() {
                        value.as_object_mut().unwrap().remove("acquiredAt");
                    }
                }
            }
        }
        self.catalogue.put_many(writes)?;
        // The terminal worker must only persist completion facts that committed.
        *job = next_job;
        if let Some(app) = app {
            self.changed(app, job["id"].as_str());
        }
        if group.membership {
            self.cleanup_removed_local(group)?;
        }
        if let Some(code) = error {
            anyhow::bail!(code);
        }
        // Publication is durable before removing no-longer-needed checkpoints.
        for file in &group.files {
            // Cleanup cannot undo or fail a durably published/reused group.
            let _ = self
                .catalogue
                .delete("checkpoint", &checkpoint_id(&plan.draft_id, group, file));
        }
        if product_id != group.product_id {
            // Duplicate staging is ours; originals and the reused product are untouched.
            for copy in &copies {
                let _ = fs::remove_file(io::safe_join(
                    root_path,
                    io::required_string(copy, "relativePath")?,
                )?);
            }
            let mut dirs = group.directories.clone();
            dirs.sort_by_key(|d| std::cmp::Reverse(d.len()));
            for dir in dirs {
                let _ = fs::remove_dir(io::safe_join(
                    root_path,
                    &format!("{}/{dir}", group.prefix),
                )?);
            }
            let _ = fs::remove_dir(io::safe_join(root_path, &group.prefix)?);
        }
        Ok(())
    }

    /// Builds the group's product, overlay and file records when no duplicate is reused.
    fn record_local_group(
        &self,
        group: &LocalGroup,
        plan: &LocalPlan,
        job: &Value,
        index: usize,
        root: &Value,
        root_path: &Path,
        copies: &[Value],
        marker_id: String,
        writes: &mut Vec<(String, String, Value)>,
        error: &mut Option<&'static str>,
        seven_zip: &Path,
        cancel: &AtomicBool,
        report_archive_phase: &mut impl FnMut(&'static str),
    ) -> Result<String> {
        let file_ids: Vec<_> = group
            .files
            .iter()
            .map(|f| file_id(&group.product_id, &f.relative))
            .collect();
        let metadata = group.metadata.as_ref().unwrap_or(&plan.metadata);
        let mut product = json!({"id":group.product_id,"provider":"local","store":"Local Files","custom":true,"title":group.title,"sourceTitle":group.title,"category":metadata["category"],"tags":metadata["tags"],"sourceUrl":metadata["sourceUrl"],"thumbnailPath":metadata["thumbnailPath"],"price":null,"acquired":added_date(job),"acquiredAt":added_at(job,index),"importedAt":now(),"localManifest":{"complete":true,"fileIds":file_ids,"directories":group.directories.iter().map(|d| format!("{}/{d}", group.prefix)).collect::<Vec<_>>(),"rootId":root["id"],"productDirectory":group.prefix}});
        product["previewUrl"] = metadata["sourceThumbnailUrl"].clone();
        let mut store_product = false;
        if group.membership {
            product = self
                .catalogue
                .get("product", &group.product_id)?
                .ok_or_else(|| anyhow::anyhow!("product_missing"))?;
            // Added files join a store product through their own
            // records; its store-observed record stays unchanged.
            store_product = product["provider"] != "local";
        }
        if group.membership && !store_product {
            let mut ids = strings(&product["localManifest"]["fileIds"]);
            ids.retain(|id| !group.remove_file_ids.contains(id));
            ids.extend(file_ids.iter().cloned());
            ids.sort();
            ids.dedup();
            product["localManifest"]["complete"] = json!(!ids.is_empty());
            product["localManifest"]["fileIds"] = json!(ids);
            product["localManifest"]["lastMutationId"] = json!(plan.draft_id);
            let mut dirs = strings(&product["localManifest"]["directories"]);
            dirs.extend(
                group
                    .directories
                    .iter()
                    .map(|d| format!("{}/{d}", group.prefix)),
            );
            dirs.sort();
            dirs.dedup();
            product["localManifest"]["directories"] = json!(dirs);
            product["localManifest"]["rootId"] = root["id"].clone();
            product["localManifest"]["productDirectory"] = json!(group.prefix);
        }
        if self.catalogue.get("product", &group.product_id)?.is_none() {
            let mut overlay = self
                .catalogue
                .get("overlay", &group.product_id)?
                .unwrap_or(json!({"id":group.product_id}));
            apply_initial_category(&mut overlay, metadata);
            if metadata["thumbnailPath"].is_string()
                && ["thumbnailPath", "sourceThumbnailUrl", "thumbnailHidden"]
                    .iter()
                    .all(|key| overlay.get(*key).is_none())
            {
                overlay["thumbnailPath"] = metadata["thumbnailPath"].clone();
            }
            writes.push(("overlay".into(), group.product_id.clone(), overlay));
        }
        if !store_product {
            writes.push(("product".into(), group.product_id.clone(), product));
            let root = io::required_string(root, "id")?;
            let source = self.with_source(job, plan, group, root)?;
            writes.push((SOURCE_KIND.into(), group.product_id.clone(), source));
        }
        let mut artifacts = Vec::new();
        for ((file, copy), id) in group.files.iter().zip(copies).zip(&file_ids) {
            let revision = format!("sha256:{}", io::required_string(copy, "hash")?);
            let descriptor = json!({"id":id,"productId":group.product_id,"provider":"local","originKind":"local","name":file.relative,"revision":revision,"sizeBytes":copy["sizeBytes"]});
            let artifact_id = artifact_id(&descriptor);
            let artifact = json!({"id":artifact_id,"fileId":id,"productId":group.product_id,"rootId":root["id"],"relativePath":copy["relativePath"],"productDirectory":group.prefix,"revision":revision,"hash":copy["hash"],"sizeBytes":copy["sizeBytes"],"status":"ready","originalAvailable":true});
            writes.push(("remote_file".into(), id.clone(), descriptor));
            artifacts.push(artifact);
        }
        self.extract_group_archives(
            root,
            root_path,
            group,
            &mut artifacts,
            seven_zip,
            cancel,
            error,
            report_archive_phase,
        )?;
        writes.extend(
            artifacts
                .into_iter()
                .map(|a| ("artifact".into(), a["id"].as_str().unwrap().to_owned(), a)),
        );
        if !group.membership {
            writes.push(("acquisition".into(), marker_id.clone(), json!({"id":marker_id,"productId":group.product_id,"observationKind":"localImport","provider":"local","source":"local","importedAt":now()})));
        }
        Ok(group.product_id.clone())
    }

    /// Extracts each archive among the group's files; a failure is marked on its artifact.
    fn extract_group_archives(
        &self,
        root: &Value,
        root_path: &Path,
        group: &LocalGroup,
        artifacts: &mut [Value],
        seven_zip: &Path,
        cancel: &AtomicBool,
        error: &mut Option<&'static str>,
        report_archive_phase: &mut impl FnMut(&'static str),
    ) -> Result<()> {
        // Expand only supplied managed archive files, never nested extraction output.
        let mut extracted = HashSet::new();
        for n in 0..artifacts.len() {
            if extracted.contains(&n) {
                continue;
            }
            let path = io::safe_join(
                root_path,
                io::required_string(&artifacts[n], "relativePath")?,
            )?;
            if !archive::is_supported_archive(&path) {
                continue;
            }
            // Explicitly selected split files can occupy separate selection containers.
            // Match only entries from the same original directory, never unrelated trees.
            let available = artifacts
                .iter()
                .zip(&group.files)
                .filter(|(_, file)| file.source.parent() == group.files[n].source.parent())
                .map(|(a, _)| io::safe_join(root_path, io::required_string(a, "relativePath")?))
                .collect::<Result<Vec<_>>>()?;
            let prior = self
                .catalogue
                .get("artifact", io::required_string(&artifacts[n], "id")?)?;
            let prior = prior.as_ref().filter(|a| {
                a["rootId"] == root["id"] && a["relativePath"] == artifacts[n]["relativePath"]
            });
            report_archive_phase("extracting");
            let result = self.extract_local_archive(
                root_path, group, &path, &available, prior, artifacts, seven_zip, cancel,
            );
            match result {
                Ok((relative, inventory, volumes)) => {
                    report_archive_phase("verifying");
                    let unchanged = artifacts
                        .iter()
                        .filter(|a| {
                            a["relativePath"]
                                .as_str()
                                .and_then(|r| io::safe_join(root_path, r).ok())
                                .is_some_and(|p| volumes.contains(&p))
                        })
                        .try_for_each(|a| {
                            io::verify_cancel(
                                &io::safe_join(root_path, io::required_string(a, "relativePath")?)?,
                                io::required_string(a, "hash")?,
                                a["sizeBytes"]
                                    .as_u64()
                                    .ok_or_else(|| anyhow::anyhow!("file_integrity_failed"))?,
                                cancel,
                            )
                        });
                    if let Err(e) = unchanged {
                        if local_error(&e) == "cancelled" {
                            return Err(e);
                        }
                        artifacts[n]["status"] = json!("extractionFailed");
                        artifacts[n]["errorCode"] = json!(local_error(&e));
                        error.get_or_insert(local_error(&e));
                        continue;
                    }
                    for (peer, artifact) in artifacts.iter_mut().enumerate() {
                        let peer_path = io::safe_join(
                            root_path,
                            io::required_string(artifact, "relativePath")?,
                        )?;
                        if volumes.contains(&peer_path) {
                            if inventory
                                .as_array()
                                .is_some_and(|entries| !entries.is_empty())
                            {
                                artifact["extractedPath"] = json!(relative);
                                artifact["extractedInventory"] = inventory.clone();
                            }
                            extracted.insert(peer);
                        }
                    }
                }
                Err(e) if local_error(&e) == "cancelled" => return Err(e),
                Err(e) => {
                    let code = local_error(&e);
                    artifacts[n]["status"] = json!("extractionFailed");
                    artifacts[n]["errorCode"] = json!(code);
                    error.get_or_insert(code);
                }
            }
        }
        Ok(())
    }

    fn extract_local_archive(
        &self,
        root: &Path,
        group: &LocalGroup,
        source: &Path,
        available: &[PathBuf],
        prior: Option<&Value>,
        artifacts: &[Value],
        tool: &Path,
        cancel: &AtomicBool,
    ) -> Result<(String, Value, Vec<PathBuf>)> {
        let mut relative = format!(
            "{}/extracted/{}",
            group.prefix,
            io::readable_component(
                source
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Archive"),
                source
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("archive_unsafe_path"))?
            )
        );
        if let Some(prior) = prior
            .and_then(|a| a["extractedPath"].as_str())
            .filter(|p| p.starts_with(&format!("{}/extracted/", group.prefix)))
        {
            relative = prior.to_owned();
        }
        let mut target = io::safe_join(root, &relative)?;
        if let Some(prior) = prior.filter(|a| {
            a["status"] == "ready" && a["extractedPath"].as_str() == Some(relative.as_str())
        }) {
            if let Ok(inventory) = serde_json::from_value::<Vec<archive::VerifiedEntry>>(
                prior["extractedInventory"].clone(),
            ) {
                let sources = archive::validate_volume_group(source, available)?;
                let records = self.catalogue.related("artifact", &group.product_id)?;
                let mut previous_sources = HashSet::new();
                let mut unchanged = !inventory.is_empty();
                // Every source in the recorded extraction must still be attached
                // with the same bytes. A shorter or changed split set is not reusable.
                for record in records.iter().filter(|a| {
                    a["rootId"] == prior["rootId"]
                        && a["extractedPath"] == prior["extractedPath"]
                        && a["extractedInventory"] == prior["extractedInventory"]
                }) {
                    let Some(current) = artifacts.iter().find(|a| a["id"] == record["id"]) else {
                        unchanged = false;
                        break;
                    };
                    if record["status"] != "ready"
                        || record["originalAvailable"] != true
                        || [
                            "fileId",
                            "productId",
                            "rootId",
                            "relativePath",
                            "revision",
                            "hash",
                            "sizeBytes",
                        ]
                        .iter()
                        .any(|key| record[*key] != current[*key])
                    {
                        unchanged = false;
                        break;
                    }
                    let path = io::safe_join(root, io::required_string(record, "relativePath")?)?;
                    if !previous_sources.insert(path) {
                        unchanged = false;
                        break;
                    }
                }
                unchanged &= previous_sources.len() == sources.len()
                    && sources.iter().all(|path| previous_sources.contains(path));
                if unchanged {
                    let expected = inventory
                        .iter()
                        .map(|entry| archive::ArchiveEntry {
                            relative_path: entry.relative_path.clone(),
                            size_bytes: entry.size_bytes,
                            is_directory: entry.is_directory,
                            crc32: None,
                        })
                        .collect::<Vec<_>>();
                    let matches = archive::verify_extracted_inventory(&target, &expected, cancel)
                        .is_ok_and(|actual| actual == inventory);
                    io::check_cancel(cancel)?;
                    if matches {
                        // The caller verifies every original source again before publication.
                        return Ok((relative, prior["extractedInventory"].clone(), sources));
                    }
                }
            }
        }
        let staging = io::safe_join(
            root,
            &format!(".nna-import-staging/{}", io::key(&group.product_id)),
        )?;
        fs::create_dir_all(&staging)?;
        let _empty_staging_parent =
            io::EmptyDirectoryGuard(staging.parent().unwrap().to_path_buf());
        let _empty_staging = io::EmptyDirectoryGuard(staging.clone());
        let extraction = archive::extract_archive(tool, source, available, &staging, cancel)?;
        io::check_cancel(cancel)?;
        let inventory = serde_json::to_value(&extraction.inventory)?;
        if target.exists() {
            // A prior extraction can survive a crash before group publication.
            let expected = extraction
                .inventory
                .iter()
                .map(|entry| archive::ArchiveEntry {
                    relative_path: entry.relative_path.clone(),
                    size_bytes: entry.size_bytes,
                    is_directory: entry.is_directory,
                    crc32: None,
                })
                .collect::<Vec<_>>();
            let matches = archive::verify_extracted_inventory(&target, &expected, cancel)
                .is_ok_and(|actual| actual == extraction.inventory);
            io::check_cancel(cancel)?;
            if matches {
                return Ok((relative, inventory, extraction.source_paths.clone()));
            }
            // Retain damaged or user-modified trees. Only a fresh managed path is published.
            relative = io::unused_relative(root, &relative)?;
            target = io::safe_join(root, &relative)?;
        }
        fs::create_dir_all(target.parent().unwrap())?;
        io::retry_busy(|| fs::rename(&extraction.staging_path, &target))?;
        Ok((relative, inventory, extraction.source_paths.clone()))
    }

    async fn claim_local_job(
        &self,
        id: &str,
        app: Option<&tauri::AppHandle>,
    ) -> Result<Option<(Value, Arc<AtomicBool>)>> {
        let gate = self.queue_gate.lock().await;
        if self.closing_local_imports.load(Ordering::Acquire) {
            return Ok(None);
        }
        let Some(before) = self
            .catalogue
            .get("job", id)?
            .filter(|job| job["state"] == "queued")
        else {
            return Ok(None);
        };
        let cancel = self.cancellation(id);
        if before["cancelRequested"] == true {
            cancel.store(true, Ordering::Release);
        }
        drop(gate);

        let mut job = before.clone();
        let result = if job["localRequest"].is_object() {
            if let Some(app) = app {
                emit_local_progress(app, id, "preparing", None, None);
            }
            let service = self.clone();
            let stop = cancel.clone();
            match tokio::task::spawn_blocking(move || {
                let result = service.plan_local_request(&mut job, Some(&stop));
                (job, result)
            })
            .await
            {
                Ok((planned, result)) => {
                    job = planned;
                    result
                }
                Err(_) => {
                    job = before.clone();
                    Err(anyhow::anyhow!("storage_failed"))
                }
            }
        } else if job["localPlan"].is_object() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("invalid_job"))
        };

        let _gate = self.queue_gate.lock().await;
        if self.closing_local_imports.load(Ordering::Acquire) {
            return Ok(None);
        }
        let (state, error) = match result.and_then(|()| io::check_cancel(&cancel)) {
            Ok(()) => ("running", None),
            Err(e) if local_error(&e) == "cancelled" => ("cancelled", None),
            Err(e) => ("needsAction", Some(local_error(&e))),
        };
        // A running job always has the inventory needed by older versions.
        // Publish it and claim the queue entry in one write, after scanning.
        job["state"] = json!(state);
        job["updatedAt"] = json!(now());
        job["errorCode"] = json!(error);
        let saved = self.catalogue.put_many_if_unchanged(
            vec![("job".into(), id.to_owned(), job.clone())],
            vec![("job".into(), id.to_owned(), Some(before))],
        )?;
        if saved {
            if let Some(app) = app {
                self.jobs_changed(app, id);
            }
        }
        if !saved || state != "running" {
            self.cancellations.lock().unwrap().remove(id);
            return Ok(None);
        }
        Ok(Some((job, cancel)))
    }

    pub(super) async fn process_local_job(&self, app: &tauri::AppHandle, job: Value) {
        let _operation = self.operations.write().await;
        let Some(id) = job["id"].as_str() else { return };
        let Ok(Some((mut job, cancel))) = self.claim_local_job(id, Some(app)).await else {
            return;
        };
        #[cfg(windows)]
        let tool = app
            .path()
            .resource_dir()
            .map(|p| p.join("tools/7zip/7z.exe"));
        #[cfg(not(windows))]
        let tool: std::result::Result<PathBuf, tauri::Error> = Ok(PathBuf::from("7z"));
        let service = self.clone();
        let stop = cancel.clone();
        let worker_job = job.clone();
        let progress_app = app.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            let mut job = worker_job;
            let result = tool.map_err(anyhow::Error::from).and_then(|tool| {
                service.run_local_import_inner(&mut job, &tool, &stop, Some(&progress_app))
            });
            (job, result)
        })
        .await;
        let result = match outcome {
            Ok((latest, result)) => {
                job = latest;
                result
            }
            Err(_) => Err(anyhow::anyhow!("storage_failed")),
        };
        let (state, error) = match &result {
            Ok(()) => ("completed", None),
            Err(e) if local_error(e) == "cancelled" => ("cancelled", None),
            Err(e) => ("needsAction", Some(local_error(e))),
        };
        {
            let _gate = self.queue_gate.lock().await;
            let _ = self.put_job(&mut job, state, error);
            self.cancellations.lock().unwrap().remove(id);
        }
        self.changed(app, Some(id));
        self.fill_listing_prices(app, &job);
    }

    /// Reads the current price from the link a new product was imported with. Nothing waits
    /// for it: the import has already finished, and a slow, missing or unreadable listing or
    /// any other failure just leaves the price unknown.
    fn fill_listing_prices(&self, app: &tauri::AppHandle, job: &Value) {
        let Ok(plan) = serde_json::from_value::<LocalPlan>(job["localPlan"].clone()) else {
            return;
        };
        let products: Vec<_> = plan
            .groups
            .into_iter()
            .filter(|group| !group.membership)
            .map(|group| group.product_id)
            .collect();
        let (service, app) = (self.clone(), app.clone());
        tauri::async_runtime::spawn(async move {
            for id in products {
                let Ok(Some(product)) = service.catalogue.get("product", &id) else {
                    continue;
                };
                if let Some(url) = listing_to_read(&product) {
                    service.read_link_price(&app, &id, url).await;
                }
            }
        });
    }

    /// The same read for a link the user just added or changed on a local product in Edit,
    /// whenever it was imported. The dialog has already closed by then.
    pub(crate) fn read_changed_link_prices(&self, app: &tauri::AppHandle, products: Vec<String>) {
        let (service, app) = (self.clone(), app.clone());
        tauri::async_runtime::spawn(async move {
            for id in products {
                if let Some(url) = service.current_link(&id) {
                    service.read_link_price(&app, &id, &url).await;
                }
            }
        });
    }

    async fn read_link_price(&self, app: &tauri::AppHandle, id: &str, url: &str) {
        let Ok(Ok(Some(price))) =
            tokio::time::timeout(LISTING_PRICE_WAIT, self.stores.listing_price(url)).await
        else {
            return;
        };
        if self.store_listing_price(id, url, &price).unwrap_or(false) {
            self.changed(app, None);
        }
    }

    /// The link a local product shows now: the user's edit, else what it was imported with.
    fn current_link(&self, id: &str) -> Option<String> {
        let product = self.catalogue.get("product", id).ok()??;
        if product["provider"] != "local" {
            return None;
        }
        let overlay = self.catalogue.get("overlay", id).ok()?.unwrap_or(json!({}));
        overlay
            .get("sourceUrl")
            .or_else(|| product.get("sourceUrl"))?
            .as_str()
            .filter(|url| !url.is_empty())
            .map(str::to_owned)
    }

    /// Keeps a price only for the link it came from, so a read overtaken by another edit is
    /// dropped, and writes over the record as it is now, so that edit stays as it is.
    fn store_listing_price(&self, id: &str, url: &str, price: &Value) -> Result<bool> {
        let _work = self.catalogue.begin_catalogue_work()?;
        if self.current_link(id).as_deref() != Some(url) {
            return Ok(false);
        }
        let Some(before) = self.catalogue.get("product", id)? else {
            return Ok(false);
        };
        let mut product = before.clone();
        for key in PRICE_FIELDS {
            product[key] = price.get(key).cloned().unwrap_or(Value::Null);
        }
        product["priceObservedAt"] = json!(now());
        product["priceSourceUrl"] = json!(crate::catalogue::listing_link(url));
        self.catalogue.put_many_if_unchanged(
            vec![("product".into(), id.to_owned(), product)],
            vec![("product".into(), id.to_owned(), Some(before))],
        )
    }
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
