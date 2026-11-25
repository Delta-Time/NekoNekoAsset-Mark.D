use super::*;

/// A dropped transfer restarts from the first byte after 2, 4, 8 and 16 s, so a Wi-Fi drop or
/// waking from sleep does not fail the download; final size and hash are still checked.
const DOWNLOAD_ATTEMPTS: u32 = 5;

fn retryable_download_resolution(error: &anyhow::Error) -> bool {
    matches!(
        io::stable_error(error),
        "network_failed" | "network_timeout" | "download_resolver_failed" | "link_expired"
    )
}

fn download_retry_delay(value: Option<&str>, attempt: u32, now: i64) -> Option<Duration> {
    let seconds = value
        .and_then(|value| {
            value.parse::<u64>().ok().or_else(|| {
                chrono::DateTime::parse_from_rfc2822(value)
                    .ok()
                    .map(|at| at.timestamp().saturating_sub(now).max(0) as u64)
            })
        })
        .unwrap_or(1 << attempt)
        .max(1);
    (seconds <= 120).then(|| Duration::from_secs(seconds))
}

fn expected_download_size(
    provider: crate::stores::StoreProvider,
    response_length: Option<u64>,
    ticket_length: Option<u64>,
    saved_length: Option<u64>,
) -> Result<Option<u64>> {
    if provider == crate::stores::StoreProvider::Booth {
        // Historical listing size is not a current content validator. A chunked
        // transfer still binds actual streamed hash/size before publication.
        return Ok(response_length.or(ticket_length));
    }
    if let (Some(response), Some(verified)) = (response_length, ticket_length) {
        ensure!(response == verified, "file_integrity_failed");
    }
    Ok(ticket_length.or(response_length).or(saved_length))
}

fn bind_revision(
    file: &Value,
    current: &Value,
    response_revision: Option<String>,
    hash: &str,
) -> Result<(String, String)> {
    for observed in [file, current] {
        if observed["revisionBasis"] != "contentHash"
            && observed["requiresContentVerification"] != true
        {
            if let Some(expected) = observed["revision"]
                .as_str()
                .and_then(|r| r.strip_prefix("sha256:"))
            {
                ensure!(expected == hash, "file_integrity_failed");
            }
        }
    }
    let source = file["revision"].as_str().filter(|r| {
        !r.is_empty()
            && *r != "unknown"
            && file["revisionBasis"] == "source"
            && file["requiresContentVerification"] != true
    });
    Ok(if let Some(revision) = source {
        (revision.to_owned(), "source".into())
    } else if let Some(revision) = response_revision {
        (revision, "etag".into())
    } else {
        (format!("sha256:{hash}"), "contentHash".into())
    })
}
pub(super) fn bind_downloaded_file(
    mut file: Value,
    revision: String,
    basis: String,
    size: u64,
) -> Value {
    file["revision"] = json!(revision);
    file["revisionBasis"] = json!(basis);
    file["requiresContentVerification"] = json!(false);
    file["sizeBytes"] = json!(size);
    file
}
fn same_remote_identity(a: &Value, b: &Value) -> bool {
    [
        "id",
        "accountId",
        "sourceFileId",
        "variantId",
        "productId",
        "name",
        "inventoryId",
        "sourceRelativePath",
    ]
    .iter()
    .all(|key| a[*key] == b[*key])
}

/// A source path as stored in the managed tree. Each name maps like any other store file
/// name; empty, `.` and `..` segments are still refused.
pub(super) fn managed_source_path(file: &Value) -> Result<String> {
    let mut parts = Vec::new();
    for part in io::required_string(file, "sourceRelativePath")?.split(['/', '\\']) {
        ensure!(
            !part.is_empty() && part != "." && part != "..",
            "archive_unsafe_path"
        );
        parts.push(io::managed_name(part));
    }
    let path = parts.join("/");
    archive::validate_relative_path(&path)?;
    Ok(path)
}

// Jinxxy exposes ordinary leaves as relative filenames in one inventory tree.
// Jinxxy leaves share a product source tree; BOOTH/Gumroad use file directories.
fn source_tree_relative(
    file: &Value,
    job_id: &str,
    product_directory: &str,
) -> Result<Option<String>> {
    if file["provider"] != "jinxxy" || file["sourceRelativePath"].is_null() {
        return Ok(None);
    }
    let path = managed_source_path(file)?;
    let name = io::managed_name(io::required_string(file, "name")?);
    ensure!(
        path.rsplit('/').next() == Some(name.as_str()),
        "archive_unsafe_path"
    );
    let account = io::required_string(file, "accountId")?;
    let inventory = io::required_string(file, "inventoryId")?;
    let group = io::key(&serde_json::to_string(&json!([account, inventory]))?);
    Ok(Some(format!(
        "{}/source-{}-{}/{}",
        product_directory,
        &io::key(job_id)[..12],
        &group[..12],
        path
    )))
}

fn validate_source_tree_paths(files: &[Value]) -> Result<()> {
    let mut paths = HashSet::new();
    for file in files {
        if let Some(path) = source_tree_relative(file, "validation", "NonVRChat")? {
            ensure!(paths.insert(path.to_lowercase()), "archive_unsafe_path");
        }
    }
    for path in &paths {
        let mut parent = path.as_str();
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            ensure!(!paths.contains(prefix), "archive_unsafe_path");
            parent = prefix;
        }
    }
    Ok(())
}

fn publish_source_tree_part(
    root: &Path,
    relative: &str,
    part: &Path,
    hash: &str,
    size: u64,
    cancel: &AtomicBool,
) -> Result<()> {
    io::check_cancel(cancel)?;
    let target = io::safe_join(root, relative)?;
    if target.try_exists()? {
        io::verify(&target, hash, size)?;
        std::fs::remove_file(part)?;
        return Ok(());
    }
    std::fs::create_dir_all(target.parent().unwrap())?;
    let target = io::safe_join(root, relative)?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
        }
        let existing: Vec<u16> = part.as_os_str().encode_wide().chain(Some(0)).collect();
        let new: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        // Same-root staging permits a same-volume rename. Flags 0 never replace
        // an existing target and work without hard links or another payload copy.
        io::retry_busy(|| {
            if unsafe { MoveFileExW(existing.as_ptr(), new.as_ptr(), 0) } == 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        })?;
    }
    #[cfg(not(windows))]
    {
        // Exclusive publication protects an existing sibling/retry target.
        if std::fs::hard_link(part, &target).is_err() {
            io::copy_new_verified(part, &target, hash, size, cancel)?;
        }
        std::fs::remove_file(part)?;
    }
    Ok(())
}

fn retain_source_tree_copy(
    root: &Path,
    old_relative: &str,
    relative: &str,
    hash: &str,
    size: u64,
    cancel: &AtomicBool,
) -> Result<()> {
    retain_source_tree_copy_between(root, old_relative, root, relative, hash, size, cancel)
}
fn retain_source_tree_copy_between(
    source_root: &Path,
    old_relative: &str,
    root: &Path,
    relative: &str,
    hash: &str,
    size: u64,
    cancel: &AtomicBool,
) -> Result<()> {
    let part = io::safe_join(
        root,
        &format!(".nna-partials/source-copy-{}.part", io::key(relative)),
    )?;
    let _partial = io::PartialGuard::new(part.clone());
    let target = io::safe_join(root, relative)?;
    if target.try_exists()? {
        return io::verify(&target, hash, size);
    }
    io::check_cancel(cancel)?;
    let source = io::safe_join(source_root, old_relative)?;
    io::space(root, size)?;
    std::fs::create_dir_all(part.parent().unwrap())?;
    if part.try_exists()? {
        io::ensure_regular(&part)?;
        std::fs::remove_file(&part)?;
    }
    io::copy_new_verified(&source, &part, hash, size, cancel)?;
    publish_source_tree_part(root, relative, &part, hash, size, cancel)
}

/// One file's download: its job, its catalogue records and where its bytes go.
struct DownloadTarget<'a> {
    app: &'a tauri::AppHandle,
    job_id: &'a str,
    root: &'a Value,
    file: &'a Value,
    cancel: &'a Arc<AtomicBool>,
    selected: bool,
    id: &'a str,
    product: &'a str,
    name: &'a str,
    product_directory: &'a str,
    source_tree: &'a Option<String>,
    relative: &'a String,
    root_path: &'a PathBuf,
    part: &'a PathBuf,
    destination: &'a PathBuf,
}

impl StorageService {
    pub(super) fn commit_download_records(
        &self,
        updates: Vec<(String, String, Value)>,
        expected: Vec<(String, String, Option<Value>)>,
    ) -> Result<()> {
        ensure!(
            self.catalogue.put_many_if_unchanged(updates, expected)?,
            "source_changed"
        );
        Ok(())
    }

    pub(super) fn delete_pending_download_record(&self, file_id: &str) -> Result<()> {
        let id = format!("pending-{}", io::key(file_id));
        self.catalogue.delete("artifact", &id)
    }

    async fn prepare_selected_source_siblings(
        &self,
        root: &Value,
        root_path: &Path,
        current: &Value,
        files: &[Value],
        job_id: &str,
        directory: &str,
        cancel: &Arc<AtomicBool>,
    ) -> Result<(
        Vec<(String, String, Value)>,
        Vec<(String, String, Option<Value>)>,
    )> {
        let mut updates = Vec::new();
        let mut expected = Vec::new();
        for sibling in files.iter().filter(|file| {
            file["id"] != current["id"]
                && file["accountId"] == current["accountId"]
                && file["inventoryId"] == current["inventoryId"]
        }) {
            let Some(relative) = source_tree_relative(sibling, job_id, directory)? else {
                continue;
            };
            let id = artifact_id(sibling);
            let Some(prior) = self
                .catalogue
                .get("artifact", &id)?
                .filter(|a| a["status"] == "ready")
            else {
                continue;
            };
            let source_root = self.root_path(&self.root(prior["rootId"].as_str())?)?;
            if prior["originalAvailable"] != true {
                // A durably extracted archive is usable without its eligible
                // deleted original. Keep its verified inventory at its stable path.
                ensure!(
                    prior["extractedPath"].is_string()
                        && self.artifact_verified(&prior, &source_root).await?,
                    "source_tree_incomplete"
                );
                expected.push((
                    "remote_file".into(),
                    io::required_string(sibling, "id")?.to_owned(),
                    Some(sibling.clone()),
                ));
                expected.push(("artifact".into(), id, Some(prior)));
                continue;
            }
            let old_relative = io::required_string(&prior, "relativePath")?.to_owned();
            let hash = io::required_string(&prior, "hash")?.to_owned();
            let size = prior["sizeBytes"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("source_tree_incomplete"))?;
            let target_root = root_path.to_owned();
            let copy_relative = relative.clone();
            let copy_cancel = cancel.clone();
            let copy = tokio::task::spawn_blocking(move || {
                retain_source_tree_copy_between(
                    &source_root,
                    &old_relative,
                    &target_root,
                    &copy_relative,
                    &hash,
                    size,
                    &copy_cancel,
                )
            })
            .await?;
            if copy.is_err() {
                io::check_cancel(cancel)?;
                bail!("source_tree_incomplete");
            }
            io::check_cancel(cancel)?;
            expected.push((
                "remote_file".into(),
                io::required_string(sibling, "id")?.to_owned(),
                Some(sibling.clone()),
            ));
            expected.push(("artifact".into(), id.clone(), Some(prior.clone())));
            let mut historical = prior.clone();
            let historical_id = format!("historical-{}", uuid::Uuid::new_v4());
            historical["id"] = json!(historical_id);
            historical["sourceRevision"] = historical["revision"].clone();
            historical["revision"] = json!(historical_id);
            updates.push(("artifact".into(), historical_id, historical));
            let mut next = prior;
            next["relativePath"] = json!(relative);
            next["rootId"] = root["id"].clone();
            next["productDirectory"] = json!(directory);
            updates.push(("artifact".into(), id, next));
        }
        Ok((updates, expected))
    }
    async fn verified_single_source_content(
        &self,
        root: &Value,
        root_path: &Path,
        current: &Value,
        files: &[Value],
        complete: bool,
        hash: &str,
        size: u64,
    ) -> Result<Option<Value>> {
        if !complete
            || files.len() != 1
            || current["provider"] != "jinxxy"
            || !same_remote_identity(current, &files[0])
        {
            return Ok(None);
        }
        if !current["sourceRelativePath"].is_string() {
            return Ok(None);
        }
        let Some(prior) = self.catalogue.get("artifact", &artifact_id(current))? else {
            return Ok(None);
        };
        let suffix = format!("/{}", managed_source_path(current)?);
        if prior["status"] != "ready"
            || prior["rootId"] != root["id"]
            || prior["revision"] != current["revision"]
            || prior["hash"] != hash
            || prior["sizeBytes"] != size
            || !prior["relativePath"]
                .as_str()
                .is_some_and(|path| path.replace('\\', "/").ends_with(&suffix))
            || !self.artifact_verified(&prior, root_path).await?
        {
            return Ok(None);
        }
        Ok(Some(prior))
    }

    async fn verified_source_manifest(
        &self,
        root: &Value,
        root_path: &Path,
        files: &[Value],
        complete: bool,
        cancel: &AtomicBool,
    ) -> Result<bool> {
        if !complete
            || files.is_empty()
            || files.iter().any(|file| file["provider"] != "jinxxy")
            || !files
                .iter()
                .any(|file| file["sourceRelativePath"].is_string())
        {
            return Ok(false);
        }
        let mut artifacts = Vec::new();
        let mut prefixes = HashMap::new();
        // Check all metadata before reading bytes: a changed/new leaf needs the
        // existing common-tree rebuild, not an extra hash pass over its siblings.
        for file in files {
            if file["requiresContentVerification"] == true
                || !file["revision"]
                    .as_str()
                    .is_some_and(|revision| !revision.is_empty() && revision != "unknown")
            {
                return Ok(false);
            }
            let Some(artifact) = self.catalogue.get("artifact", &artifact_id(file))? else {
                return Ok(false);
            };
            if artifact["status"] != "ready"
                || artifact["rootId"] != root["id"]
                || artifact["revision"] != file["revision"]
            {
                return Ok(false);
            }
            if artifact["originalAvailable"] != true && !artifact["extractedPath"].is_string() {
                return Ok(false);
            }
            if file["sourceRelativePath"].is_string() && artifact["originalAvailable"] == true {
                let suffix = format!("/{}", managed_source_path(file)?);
                let stored = io::required_string(&artifact, "relativePath")?.replace('\\', "/");
                let Some(prefix) = stored.strip_suffix(&suffix) else {
                    return Ok(false);
                };
                let group = (
                    io::required_string(file, "accountId")?.to_owned(),
                    io::required_string(file, "inventoryId")?.to_owned(),
                );
                if prefixes
                    .get(&group)
                    .is_some_and(|existing| existing != prefix)
                {
                    return Ok(false);
                }
                prefixes.insert(group, prefix.to_owned());
            }
            artifacts.push(artifact);
        }
        for artifact in artifacts {
            io::check_cancel(cancel)?;
            if !self.artifact_verified(&artifact, root_path).await? {
                return Ok(false);
            }
        }
        io::check_cancel(cancel)?;
        Ok(true)
    }

    pub(super) async fn run_download(
        &self,
        app: &tauri::AppHandle,
        job: &mut Value,
        cancel: &Arc<AtomicBool>,
    ) -> Result<()> {
        io::check_cancel(cancel)?;
        let root = self.root(job["rootId"].as_str())?;
        let root_path = self.root_path(&root)?;
        let _empty_partials = io::EmptyDirectoryGuard(io::safe_join(&root_path, ".nna-partials")?);
        let product = io::required_string(job, "productId")?.to_owned();
        let (files, complete) = self.remote_files(&product)?;
        let files = self.job_files(job, files)?;
        let files = selected_download_files(files, &job["selectedFileIds"])?;
        let attempted_files = files.clone();
        ensure_files(job, &files)?;
        // Queued before its files were all Ready; an interrupted extraction still runs.
        if job["automatic"] == true && self.all_at_revision(&files, &["ready"])? {
            job["fileIds"] = json!(files.iter().map(|f| f["id"].clone()).collect::<Vec<_>>());
            job["manifestFingerprint"] = json!(fingerprint(&files));
            job["progress"] = json!(100.0);
            return Ok(());
        }
        validate_source_tree_paths(&files)?;
        job["fileIds"] = json!(files.iter().map(|f| f["id"].clone()).collect::<Vec<_>>());
        self.catalogue
            .put("job", io::required_string(job, "id")?, job.clone())?;
        if job["redownload"] != true
            && job["selectedFileIds"].is_null()
            && self
                .verified_source_manifest(&root, &root_path, &files, complete, cancel)
                .await?
        {
            let (current, complete_after) = self.remote_files(&product)?;
            let current = self.job_files(job, current)?;
            if complete_after && fingerprint(&current) == fingerprint(&files) {
                job["manifestFingerprint"] = json!(fingerprint(&current));
                job["progress"] = json!(100.0);
                return Ok(());
            }
        }
        let provider = self
            .catalogue
            .get("product", &product)?
            .and_then(|product| {
                product["store"]
                    .as_str()
                    .or_else(|| product["provider"].as_str())
                    .and_then(|store| crate::stores::StoreProvider::parse(store).ok())
            });
        let job_id = io::required_string(job, "id")?.to_owned();
        let redownload = job["redownload"] == true;
        let selected = job["selectedFileIds"].is_array();
        let mut waiting = files.iter().cloned();
        let mut next = waiting.next();
        let mut in_flight = FuturesUnordered::new();
        let mut results = Vec::new();
        while next.is_some() || !in_flight.is_empty() {
            let concurrency = adaptive_download_count(
                &self.preferences()?["downloadConcurrency"],
                provider,
                &self.catalogue.list("job")?,
            );
            // Sync reduces new launches for the same provider. Existing transfers
            // finish normally; each completion rechecks sync and saved preferences.
            while next.is_some() && in_flight.len() < concurrency && !cancel.load(Ordering::Acquire)
            {
                let file = next.take().unwrap();
                let cancel = cancel.clone();
                let root = root.clone();
                let job_id = job_id.clone();
                in_flight.push(async move {
                    let mut result = self
                        .download_one(app, &job_id, &root, &file, &cancel, redownload, selected)
                        .await;
                    if result.is_ok() {
                        result = self
                            .file_progress(app, &job_id, file["id"].as_str().unwrap_or(""), 1.0)
                            .await;
                    }
                    (file, result)
                });
                next = waiting.next();
            }
            if let Some(result) = in_flight.next().await {
                results.push(result);
            } else {
                break;
            }
        }
        io::check_cancel(cancel)?;
        // A failed sibling never discards completed bytes. Extract only after all volumes have arrived.
        let mut error = None;
        let mut verified = HashSet::new();
        job["failures"] = json!({});
        for (file, result) in results {
            let id = file["id"].as_str().unwrap_or("");
            match result {
                Ok(()) => {
                    verified.insert(id.to_owned());
                }
                Err(failure) => {
                    job["failures"][id] = json!({"fileId":id,"fileName":file["name"],"errorCode":io::stable_error(&failure)});
                    if error.is_none() {
                        error = Some(failure);
                    }
                }
            }
        }
        if let Some(latest) = self.catalogue.get("job", &job_id)? {
            job["progress"] = latest["progress"].clone();
            job["fileProgress"] = latest["fileProgress"].clone();
        }
        let (all_files, _) = self.remote_files(&product)?;
        let files = self.job_files(job, all_files.clone())?;
        let files = selected_download_files(files, &job["selectedFileIds"])?;
        let processed = self.settled_files(&attempted_files, &files, &verified)?;
        job["manifestFingerprint"] = json!(fingerprint(&processed));
        if processed.len() != files.len() && error.is_none() {
            error = Some(anyhow::anyhow!("manifest_changed"));
        }
        self.extract_downloaded_files(
            app, job, &job_id, &root, &root_path, &files, &all_files, cancel, &mut error,
        )
        .await?;
        // An archive kept beside its Ready extraction goes only once every transfer of the
        // job has settled: a sibling that reused those bytes is recorded by then, and the
        // removal keeps a path any record still needs.
        for file in files.iter().filter(|file| {
            verified.contains(file["id"].as_str().unwrap_or(""))
                && !(file["provider"] == "jinxxy" && !file["sourceRelativePath"].is_null())
        }) {
            if let Some(artifact) = self.catalogue.get("artifact", &artifact_id(file))? {
                self.remove_kept_original(&root, &root_path, &artifact)
                    .await;
            }
        }
        if let Some(error) = error {
            return Err(error);
        }
        // Completion describes the known selected work, not source coverage.
        // Catalogue keeps a partial product partial until every required file is Ready.
        job["progress"] = json!(100.0);
        Ok(())
    }

    /// The list a download settled: each file it attempted, as its current record when an
    /// artifact holds that revision's bytes, else as attempted. A store read since the job
    /// verified a file asks for that again, which the job did not do, so the settled record
    /// stays the verified one and automatic downloads still ask for the new request.
    pub(super) fn settled_files(
        &self,
        attempted: &[Value],
        files: &[Value],
        verified: &HashSet<String>,
    ) -> Result<Vec<Value>> {
        let mut settled = Vec::new();
        for before in attempted {
            let current = files.iter().find(|f| f["id"] == before["id"]);
            let bound = if let Some(current) = current {
                self.catalogue
                    .get("artifact", &artifact_id(current))?
                    .is_some_and(|a| {
                        matches!(
                            a["status"].as_str(),
                            Some("ready" | "downloaded" | "extractionFailed")
                        )
                    })
            } else {
                false
            };
            settled.push(if bound {
                let mut current = current.unwrap().clone();
                if verified.contains(current["id"].as_str().unwrap_or("")) {
                    current["requiresContentVerification"] = json!(false);
                }
                current
            } else {
                before.clone()
            });
        }
        Ok(settled)
    }

    /// Makes each downloaded file Ready, extracting archives and recording failures on the job.
    async fn extract_downloaded_files(
        &self,
        app: &tauri::AppHandle,
        job: &mut Value,
        job_id: &str,
        root: &Value,
        root_path: &Path,
        files: &[Value],
        all_files: &[Value],
        cancel: &Arc<AtomicBool>,
        error: &mut Option<anyhow::Error>,
    ) -> Result<()> {
        let seven_zip = self.seven_zip(app)?;
        for file in files {
            io::check_cancel(cancel)?;
            let id = artifact_id(file);
            let Some(mut artifact) = self.catalogue.get("artifact", &id)? else {
                if error.is_none() {
                    *error = Some(anyhow::anyhow!("manifest_changed"));
                }
                continue;
            };
            if artifact["status"] == "ready" {
                continue;
            }
            if artifact["errorCode"] == "source_tree_incomplete" {
                let repair = async {
                    let directory = io::required_string(&artifact, "productDirectory")?.to_owned();
                    ensure!(
                        source_tree_relative(file, job_id, &directory)?.as_deref()
                            == artifact["relativePath"].as_str(),
                        "source_tree_incomplete"
                    );
                    ensure!(
                        artifact["originalAvailable"] == true
                            && self.artifact_verified(&artifact, root_path).await?,
                        "source_tree_incomplete"
                    );
                    let (mut updates, mut expected) = self
                        .prepare_selected_source_siblings(
                            root, root_path, file, all_files, job_id, &directory, cancel,
                        )
                        .await?;
                    expected.push((
                        "remote_file".into(),
                        io::required_string(file, "id")?.to_owned(),
                        Some(file.clone()),
                    ));
                    expected.push(("artifact".into(), id.clone(), Some(artifact.clone())));
                    let mut recovered = artifact.clone();
                    recovered["errorCode"] = Value::Null;
                    updates.push(("artifact".into(), id.clone(), recovered.clone()));
                    self.commit_download_records(updates, expected)?;
                    Ok::<Value, anyhow::Error>(recovered)
                }
                .await;
                match repair {
                    Ok(recovered) => artifact = recovered,
                    Err(failure) => {
                        io::check_cancel(cancel)?;
                        let file_id = file["id"].as_str().unwrap_or("");
                        job["failures"][file_id] = json!({"fileId":file_id,"fileName":file["name"],"errorCode":io::stable_error(&failure)});
                        if error.is_none() {
                            *error = Some(failure);
                        }
                        continue;
                    }
                }
            }
            if !matches!(
                artifact["status"].as_str(),
                Some("downloaded" | "extractionFailed")
            ) {
                continue;
            }
            let path = io::safe_join(root_path, io::required_string(&artifact, "relativePath")?)?;
            if !archive::is_supported_archive(&path) {
                artifact["status"] = json!("ready");
                self.catalogue.put("artifact", &id, artifact)?;
                continue;
            }
            let result = self
                .extract_for_download(&seven_zip, root, &mut artifact, all_files, cancel)
                .await;
            if let Err(e) = result {
                let file_id = file["id"].as_str().unwrap_or("");
                job["failures"][file_id] = json!({"fileId":file_id,"fileName":file["name"],"errorCode":io::stable_error(&e)});
                if error.is_none() {
                    *error = Some(e)
                }
            }
            self.changed(app, Some(job_id));
        }
        Ok(())
    }
    async fn file_progress(
        &self,
        app: &tauri::AppHandle,
        job_id: &str,
        file_id: &str,
        fraction: f64,
    ) -> Result<()> {
        let _gate = self.queue_gate.lock().await;
        if let Some(mut job) = self.catalogue.get("job", job_id)? {
            job["fileProgress"][file_id] = json!(fraction.clamp(0.0, 1.0));
            let ids = strings(&job["fileIds"]);
            let sum: f64 = ids
                .iter()
                .map(|id| job["fileProgress"][id].as_f64().unwrap_or(0.0))
                .sum();
            job["progress"] = json!(90.0 * sum / ids.len().max(1) as f64);
            job["updatedAt"] = json!(now());
            self.catalogue.put("job", job_id, job)?;
            self.jobs_changed(app, job_id);
        }
        Ok(())
    }
    fn seven_zip(&self, app: &tauri::AppHandle) -> Result<PathBuf> {
        // Test overrides require a debug build and explicitly isolated synthetic data.
        if cfg!(debug_assertions) && std::env::var_os("NNA_DATA_DIR").is_some() {
            if let Some(path) = std::env::var_os("NNA_7ZIP") {
                return Ok(path.into());
            }
        }
        #[cfg(windows)]
        {
            Ok(app.path().resource_dir()?.join("tools/7zip/7z.exe"))
        }
        #[cfg(not(windows))]
        {
            let _ = app;
            Ok(PathBuf::from("7z"))
        }
    }
    pub(super) async fn artifact_verified(&self, artifact: &Value, root: &Path) -> Result<bool> {
        let artifact = artifact.clone();
        let root = root.to_owned();
        tokio::task::spawn_blocking(move || -> Result<bool> {
            if artifact["status"] == "ready" && artifact["extractedPath"].as_str().is_some() {
                let base = io::safe_join(&root, io::required_string(&artifact, "extractedPath")?)?;
                let entries = artifact["extractedInventory"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("file_integrity_failed"))?;
                ensure!(!entries.is_empty(), "file_integrity_failed");
                for entry in entries {
                    let path = io::safe_join(&base, io::required_string(entry, "relativePath")?)?;
                    if entry["isDirectory"] == true {
                        ensure!(path.is_dir(), "file_missing");
                    } else {
                        io::verify(
                            &path,
                            io::required_string(entry, "hash")?,
                            entry["sizeBytes"]
                                .as_u64()
                                .ok_or_else(|| anyhow::anyhow!("file_integrity_failed"))?,
                        )?;
                    }
                }
                return Ok(true);
            }
            let path = io::safe_join(&root, io::required_string(&artifact, "relativePath")?)?;
            io::verify(
                &path,
                io::required_string(&artifact, "hash")?,
                artifact["sizeBytes"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("file_integrity_failed"))?,
            )?;
            Ok(true)
        })
        .await?
        .or(Ok(false))
    }
    async fn download_one(
        &self,
        app: &tauri::AppHandle,
        job_id: &str,
        root: &Value,
        file: &Value,
        cancel: &Arc<AtomicBool>,
        redownload: bool,
        selected: bool,
    ) -> Result<()> {
        io::check_cancel(cancel)?;
        let root_path = self.root_path(root)?;
        let existing_id = artifact_id(file);
        let existing = self.catalogue.get("artifact", &existing_id)?;
        let has_source_tree = file["provider"] == "jinxxy" && !file["sourceRelativePath"].is_null();
        let known_revision = file["revision"]
            .as_str()
            .is_some_and(|r| !r.is_empty() && r != "unknown");
        if !has_source_tree {
            if let Some(artifact) = existing.as_ref() {
                if !redownload
                    && known_revision
                    && file["requiresContentVerification"] != true
                    && matches!(
                        artifact["status"].as_str(),
                        Some("ready" | "downloaded" | "extractionFailed" | "extracting")
                    )
                    && self.artifact_verified(artifact, &root_path).await?
                {
                    if artifact["status"] == "extracting" {
                        let mut recovered = artifact.clone();
                        recovered["status"] = json!("downloaded");
                        self.catalogue.put("artifact", &existing_id, recovered)?;
                    }
                    return Ok(());
                }
            }
        }
        let id = format!("pending-{}", io::key(io::required_string(file, "id")?));
        // The catalogue keeps the store's name; the managed copy gets one Windows can store.
        let name = io::managed_name(io::required_string(file, "name")?);
        let name = name.as_str();
        ensure!(
            archive::validate_relative_path(name)?.components().count() == 1,
            "archive_unsafe_path"
        );
        let product = io::required_string(file, "productId")?;
        let product_source = self
            .catalogue
            .get("product", product)?
            .ok_or_else(|| anyhow::anyhow!("product_missing"))?;
        // Freeze initial storage category independently of future effective classification edits.
        let prior = self
            .catalogue
            .related("artifact", product)?
            .into_iter()
            .next();
        let inference = crate::taxonomy::infer_category(
            &product_source["sourceCategory"],
            &strings(&product_source["sourceTags"]),
        );
        let category = Self::storage_category(&prior, &inference);
        let product_directory = self.managed_product_directory(product)?;
        let source_tree = source_tree_relative(file, job_id, &product_directory)?;
        let relative = if let Some(path) = &source_tree {
            path.clone()
        } else {
            let directory = format!(
                "{}/{}",
                product_directory,
                io::readable_component(
                    Path::new(name)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("File"),
                    io::required_string(file, "id")?
                )
            );
            format!("{}/{}", io::unused_relative(&root_path, &directory)?, name)
        };
        if let Some(artifact) = existing.filter(|_| has_source_tree) {
            if !redownload
                && known_revision
                && file["requiresContentVerification"] != true
                && matches!(
                    artifact["status"].as_str(),
                    Some("ready" | "downloaded" | "extractionFailed" | "extracting")
                )
                && (artifact["originalAvailable"] == true
                    || (artifact["status"] == "ready" && artifact["extractedPath"].is_string()))
                && artifact["rootId"] == root["id"]
                && self.artifact_verified(&artifact, &root_path).await?
            {
                return self
                    .reuse_source_tree_artifact(
                        app,
                        job_id,
                        artifact,
                        existing_id,
                        root_path,
                        relative,
                        product_directory,
                        source_tree,
                        cancel,
                    )
                    .await;
            }
        }
        self.retire_pending_record(&id)?;
        let destination = io::safe_join(&root_path, &relative)?;
        let part = io::safe_join(&root_path, &format!(".nna-partials/{}.part", id))?;
        std::fs::create_dir_all(part.parent().unwrap())?;
        // Only our stable per-file partial is removed; prior completed revisions are never overwritten.
        if part.exists() {
            io::ensure_regular(&part)?;
            std::fs::remove_file(&part)?;
        }
        let _partial = io::PartialGuard::new(part.clone());
        let mut artifact = json!({"id":id,"productId":product,"fileId":file["id"],"rootId":root["id"],"relativePath":relative,"productDirectory":product_directory,"revision":null,"initialCategory":category,"status":"downloading","originalAvailable":false,"updatedAt":now()});
        self.catalogue.put("artifact", &id, artifact.clone())?;
        let result = self
            .stream_download(app, job_id, file, &part, &root_path, cancel)
            .await;
        let outcome = match result {
            Ok((hash, size, revision, basis, observed)) => {
                self.finish_download(
                    DownloadTarget {
                        app,
                        job_id,
                        root,
                        file,
                        cancel,
                        selected,
                        id: &id,
                        product,
                        name,
                        product_directory: &product_directory,
                        source_tree: &source_tree,
                        relative: &relative,
                        root_path: &root_path,
                        part: &part,
                        destination: &destination,
                    },
                    artifact,
                    hash,
                    size,
                    revision,
                    basis,
                    observed,
                )
                .await
            }
            Err(error) => {
                if part.exists() {
                    let _ = std::fs::remove_file(&part);
                }
                artifact["status"] = json!("notDownloaded");
                artifact["errorCode"] = json!(io::stable_error(&error));
                artifact["updatedAt"] = json!(now());
                self.catalogue.put("artifact", &id, artifact)?;
                Err(error)
            }
        };
        if let Err(error) = &outcome {
            if let Some(mut pending) = self.catalogue.get("artifact", &id)? {
                if pending["originalAvailable"] != true {
                    pending["status"] = json!("notDownloaded");
                    pending["errorCode"] = json!(io::stable_error(error));
                    pending["updatedAt"] = json!(now());
                    self.catalogue.put("artifact", &id, pending)?;
                }
            }
        }
        outcome
    }

    /// The category a new file is filed under: the product's frozen or inferred one, if known.
    fn storage_category<'a>(prior: &'a Option<Value>, inference: &'a Value) -> &'a str {
        let initial = prior
            .as_ref()
            .and_then(|a| a["initialCategory"].as_str())
            .unwrap_or_else(|| inference["category"].as_str().unwrap_or("NonVRChat"));
        if [
            "Base",
            "Clothes",
            "Hair",
            "Accessory",
            "Texture",
            "Texture-Material",
            "Motion",
            "Tool",
            "Shader",
            "Gimmick",
            "World",
            "Udon",
            "NonVRChat",
        ]
        .contains(&initial)
        {
            initial
        } else {
            "NonVRChat"
        }
    }

    /// Keeps a verified source-tree artifact, copying it to the declared path when that moved.
    async fn reuse_source_tree_artifact(
        &self,
        app: &tauri::AppHandle,
        job_id: &str,
        artifact: Value,
        existing_id: String,
        root_path: PathBuf,
        relative: String,
        product_directory: String,
        source_tree: Option<String>,
        cancel: &Arc<AtomicBool>,
    ) -> Result<()> {
        let mut recovered = artifact.clone();
        let mut updates = Vec::new();
        let mut recovered_changed = false;
        if source_tree.is_some() && artifact["originalAvailable"] == true {
            let old_relative = io::required_string(&artifact, "relativePath")?.to_owned();
            let copy_root = root_path.clone();
            let copy_relative = relative.clone();
            let old = old_relative.clone();
            let hash = io::required_string(&artifact, "hash")?.to_owned();
            let size = artifact["sizeBytes"]
                .as_u64()
                .ok_or_else(|| anyhow::anyhow!("file_integrity_failed"))?;
            let copy_cancel = cancel.clone();
            tokio::task::spawn_blocking(move || {
                retain_source_tree_copy(&copy_root, &old, &copy_relative, &hash, size, &copy_cancel)
            })
            .await??;
            if old_relative != relative {
                let mut historical = artifact;
                let historical_id = format!("historical-{}", uuid::Uuid::new_v4());
                historical["id"] = json!(historical_id);
                historical["sourceRevision"] = historical["revision"].clone();
                historical["revision"] = json!(historical_id);
                updates.push(("artifact".into(), historical_id, historical));
                recovered["relativePath"] = json!(relative);
                recovered["productDirectory"] = json!(product_directory);
                recovered["updatedAt"] = json!(now());
                recovered_changed = true;
            }
        }
        if recovered["status"] == "extracting" {
            recovered["status"] = json!("downloaded");
            recovered_changed = true;
        }
        if recovered_changed {
            updates.push(("artifact".into(), existing_id, recovered));
            self.catalogue.put_many(updates)?;
            self.changed(app, Some(job_id));
        }
        Ok(())
    }

    /// A pending record that still holds an available original becomes a historical record.
    fn retire_pending_record(&self, id: &str) -> Result<()> {
        if let Some(mut retained) = self.catalogue.get("artifact", id)? {
            if retained["originalAvailable"] == true {
                let historical = format!("historical-{}", uuid::Uuid::new_v4());
                retained["id"] = json!(historical);
                retained["sourceRevision"] = retained["revision"].clone();
                retained["revision"] = json!(historical);
                self.catalogue.put("artifact", &historical, retained)?;
            }
        }
        Ok(())
    }

    /// Binds the arrived bytes to the current record; unchanged source-tree content stays Ready.
    async fn finish_download(
        &self,
        target: DownloadTarget<'_>,
        mut artifact: Value,
        hash: String,
        size: u64,
        revision: String,
        basis: String,
        observed: Value,
    ) -> Result<()> {
        let DownloadTarget {
            app,
            job_id,
            root,
            file,
            cancel,
            selected,
            id,
            product,
            source_tree,
            root_path,
            part,
            ..
        } = target;
        if cancel.load(Ordering::Acquire) {
            let _ = std::fs::remove_file(part);
            artifact["status"] = json!("notDownloaded");
            artifact["errorCode"] = json!("cancelled");
            self.catalogue.put("artifact", id, artifact)?;
            bail!("cancelled")
        }
        let current = self
            .catalogue
            .get("remote_file", io::required_string(file, "id")?)?
            .ok_or_else(|| anyhow::anyhow!("file_missing"))?;
        ensure!(same_remote_identity(&current, &observed), "source_changed");
        ensure!(
            current["revision"] == observed["revision"]
                && !(current["requiresContentVerification"] == true
                    && observed["requiresContentVerification"] != true),
            "source_changed"
        );
        let observed_current = current.clone();
        let current = bind_downloaded_file(current, revision, basis, size);
        let final_id = artifact_id(&current);
        let mut source_expected = vec![(
            "remote_file".into(),
            io::required_string(file, "id")?.to_owned(),
            Some(observed_current),
        )];
        if source_tree.is_some() {
            let mut expected = source_expected.clone();
            expected.extend([
                (
                    "root".into(),
                    io::required_string(root, "id")?.to_owned(),
                    Some(root.clone()),
                ),
                (
                    "product".into(),
                    product.to_owned(),
                    self.catalogue.get("product", product)?,
                ),
            ]);
            for entitlement in self.catalogue.related("entitlement", product)? {
                expected.push((
                    "entitlement".into(),
                    io::required_string(&entitlement, "id")?.to_owned(),
                    Some(entitlement),
                ));
            }
            let (files, complete) = self.remote_files(product)?;
            let prior = self
                .verified_single_source_content(
                    root,
                    root_path,
                    &current,
                    if selected {
                        std::slice::from_ref(&current)
                    } else {
                        &files
                    },
                    complete || selected,
                    &hash,
                    size,
                )
                .await?;
            if cancel.load(Ordering::Acquire) {
                let _ = std::fs::remove_file(part);
                artifact["status"] = json!("notDownloaded");
                artifact["errorCode"] = json!("cancelled");
                self.catalogue.put("artifact", id, artifact)?;
                bail!("cancelled")
            }
            if let Some(prior) = prior {
                // The actual GET has established equal content. One leaf has
                // no sibling layout to rebuild; retain its durable Ready path.
                expected.push(("artifact".into(), final_id.clone(), Some(prior.clone())));
                self.commit_download_records(
                    vec![
                        (
                            "remote_file".into(),
                            io::required_string(&current, "id")?.to_owned(),
                            current,
                        ),
                        ("artifact".into(), final_id, prior),
                    ],
                    expected,
                )?;
                std::fs::remove_file(part)?;
                self.delete_pending_download_record(io::required_string(file, "id")?)?;
                self.changed(app, Some(job_id));
                return Ok(());
            }
            source_expected = expected;
        }
        self.publish_download(
            target,
            artifact,
            hash,
            size,
            current,
            final_id,
            source_expected,
        )
        .await
    }

    /// Moves the bytes into place, or onto an identical copy, and commits the file's records.
    async fn publish_download(
        &self,
        target: DownloadTarget<'_>,
        mut artifact: Value,
        hash: String,
        size: u64,
        current: Value,
        final_id: String,
        mut source_expected: Vec<(String, String, Option<Value>)>,
    ) -> Result<()> {
        let DownloadTarget {
            app,
            job_id,
            root,
            file,
            cancel,
            selected,
            id,
            product,
            name,
            product_directory,
            source_tree,
            relative,
            root_path,
            part,
            destination,
        } = target;
        // A source tree always publishes bytes at the declared relative path.
        // BOOTH and Gumroad retain verified-content sharing within the product.
        let mut duplicate = None;
        for existing in if source_tree.is_some() {
            Vec::new()
        } else {
            self.catalogue.related("artifact", product)?
        } {
            if existing["hash"] == hash
                && existing["sizeBytes"] == size
                && existing["rootId"] == root["id"]
                && existing["originalAvailable"] == true
                && existing["relativePath"]
                    .as_str()
                    .and_then(|p| Path::new(p).file_name())
                    .is_some_and(|n| n == std::ffi::OsStr::new(name))
            {
                let p = io::safe_join(root_path, io::required_string(&existing, "relativePath")?)?;
                let p2 = p.clone();
                let h = hash.clone();
                if tokio::task::spawn_blocking(move || io::verify(&p2, &h, size))
                    .await?
                    .is_ok()
                {
                    duplicate = Some(existing["relativePath"].clone());
                    break;
                }
            }
        }
        if source_tree.is_some() {
            let copy_root = root_path.clone();
            let copy_relative = relative.clone();
            let copy_part = part.clone();
            let copy_hash = hash.clone();
            let copy_cancel = cancel.clone();
            tokio::task::spawn_blocking(move || {
                publish_source_tree_part(
                    &copy_root,
                    &copy_relative,
                    &copy_part,
                    &copy_hash,
                    size,
                    &copy_cancel,
                )
            })
            .await??;
        } else if let Some(relative) = duplicate {
            artifact["relativePath"] = relative;
            let part = part.clone();
            tokio::task::spawn_blocking(move || io::retry_busy(|| std::fs::remove_file(&part)))
                .await??;
        } else {
            std::fs::create_dir_all(destination.parent().unwrap())?;
            let (part, destination) = (part.clone(), destination.clone());
            tokio::task::spawn_blocking(move || {
                io::retry_busy(|| std::fs::rename(&part, &destination))
            })
            .await??;
        }
        artifact["hash"] = json!(hash);
        artifact["sizeBytes"] = json!(size);
        artifact["status"] = json!("downloaded");
        artifact["originalAvailable"] = json!(true);
        self.catalogue.put("artifact", id, artifact.clone())?;
        let mut sibling_updates = Vec::new();
        let mut sibling_error = None;
        if source_tree.is_some() && selected {
            let (files, _) = self.remote_files(product)?;
            match self
                .prepare_selected_source_siblings(
                    root,
                    root_path,
                    &current,
                    &files,
                    job_id,
                    product_directory,
                    cancel,
                )
                .await
            {
                Ok((updates, expected)) => {
                    sibling_updates = updates;
                    source_expected.extend(expected);
                }
                Err(error) => {
                    io::check_cancel(cancel)?;
                    sibling_error = Some(error);
                }
            }
        }
        artifact["id"] = json!(final_id);
        artifact["revision"] = current["revision"].clone();
        artifact["hash"] = json!(hash);
        artifact["sizeBytes"] = json!(size);
        artifact["status"] = json!("downloaded");
        artifact["originalAvailable"] = json!(true);
        artifact["updatedAt"] = json!(now());
        let mut updates = sibling_updates;
        let prior = self.catalogue.get("artifact", &final_id)?;
        source_expected.push(("artifact".into(), final_id.clone(), prior.clone()));
        if let Some(mut prior) = prior {
            if sibling_error.is_none()
                && prior["status"] == "ready"
                && prior["hash"] == hash
                && prior["sizeBytes"] == size
                && self.artifact_verified(&prior, root_path).await?
            {
                artifact["status"] = json!("ready");
                if let Some(path) = prior.get("extractedPath") {
                    artifact["extractedPath"] = path.clone();
                    artifact["extractedInventory"] = prior["extractedInventory"].clone();
                }
                if source_tree.is_some() && prior["relativePath"] != artifact["relativePath"] {
                    let historical = format!("historical-{}", uuid::Uuid::new_v4());
                    prior["id"] = json!(historical);
                    prior["sourceRevision"] = prior["revision"].clone();
                    prior["revision"] = json!(historical);
                    updates.push(("artifact".into(), historical, prior));
                }
            } else {
                let historical = format!("historical-{}", uuid::Uuid::new_v4());
                prior["id"] = json!(historical);
                prior["sourceRevision"] = prior["revision"].clone();
                prior["revision"] = json!(historical);
                updates.push(("artifact".into(), historical, prior));
            }
        }
        updates.push((
            "remote_file".into(),
            io::required_string(&current, "id")?.to_owned(),
            current,
        ));
        if sibling_error.is_some() {
            artifact["errorCode"] = json!("source_tree_incomplete");
        }
        updates.push(("artifact".into(), final_id, artifact.clone()));
        self.commit_download_records(updates, source_expected)?;
        self.delete_pending_download_record(io::required_string(file, "id")?)?;
        self.changed(app, Some(job_id));
        if let Some(error) = sibling_error {
            return Err(error);
        }
        Ok(())
    }
    async fn stream_download(
        &self,
        app: &tauri::AppHandle,
        job_id: &str,
        file: &Value,
        part: &Path,
        root: &Path,
        cancel: &AtomicBool,
    ) -> Result<(String, u64, String, String, Value)> {
        let mut last = anyhow::anyhow!("download_failed");
        for attempt in 0..DOWNLOAD_ATTEMPTS {
            io::check_cancel(cancel)?;
            let ticket = io::cancellable(
                cancel,
                self.stores
                    .download_ticket(app, io::required_string(file, "id")?),
            )
            .await?;
            let ticket = match ticket {
                Ok(ticket) => ticket,
                Err(error)
                    if retryable_download_resolution(&error) && attempt + 1 < DOWNLOAD_ATTEMPTS =>
                {
                    io::delay(cancel, Duration::from_secs(2 << attempt)).await?;
                    continue;
                }
                Err(error) => return Err(error),
            };
            let current = self
                .catalogue
                .get("remote_file", io::required_string(file, "id")?)?
                .ok_or_else(|| anyhow::anyhow!("file_missing"))?;
            ensure!(same_remote_identity(file, &current), "source_changed");
            if file["revisionBasis"] == "source" && current["revisionBasis"] == "source" {
                ensure!(file["revision"] == current["revision"], "source_changed");
            }
            if let Some(size) = expected_download_size(
                ticket.provider,
                None,
                ticket.expected_size,
                file["sizeBytes"].as_u64(),
            )? {
                io::space(root, size)?;
            }
            let response = io::cancellable(
                cancel,
                tokio::time::timeout(
                    Duration::from_secs(60),
                    ticket.client.get(ticket.url).send(),
                ),
            )
            .await?
            .map_err(|_| anyhow::anyhow!("network_timeout"))
            .and_then(|result| {
                result.map_err(|error| {
                    anyhow::anyhow!(if error.is_timeout() {
                        "network_timeout"
                    } else {
                        "network_failed"
                    })
                })
            });
            let response = match response {
                Ok(r) => r,
                Err(e) => {
                    last = e;
                    if attempt + 1 < DOWNLOAD_ATTEMPTS {
                        io::delay(cancel, Duration::from_secs(2 << attempt)).await?;
                    }
                    continue;
                }
            };
            let status = response.status();
            if status.as_u16() == 401 || status.as_u16() == 403 {
                let error = crate::stores::validate_download_response(
                    ticket.provider,
                    &response,
                    ticket.expected_origin.as_ref(),
                )
                .err()
                .unwrap_or_else(|| anyhow::anyhow!("download_http_failed"));
                if io::stable_error(&error) != "link_expired" {
                    return Err(error);
                }
                last = error;
                if attempt + 1 < DOWNLOAD_ATTEMPTS {
                    io::delay(cancel, Duration::from_secs(2 << attempt)).await?;
                }
                continue;
            }
            if status.as_u16() == 429 || status.is_server_error() {
                last = anyhow::anyhow!(if status.as_u16() == 429 {
                    "rate_limited"
                } else {
                    "download_http_failed"
                });
                let wait = download_retry_delay(
                    response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok()),
                    attempt,
                    chrono::Utc::now().timestamp(),
                );
                let Some(wait) = wait else {
                    return Err(last);
                };
                if attempt + 1 < DOWNLOAD_ATTEMPTS {
                    io::delay(cancel, wait).await?;
                }
                continue;
            }
            crate::stores::validate_download_response(
                ticket.provider,
                &response,
                ticket.expected_origin.as_ref(),
            )?;
            ensure!(status.is_success(), "download_http_failed");
            let response_revision = response
                .headers()
                .get(reqwest::header::ETAG)
                .and_then(|v| v.to_str().ok())
                .filter(|v| v.starts_with('"') && v.ends_with('"') && v.len() <= 1024)
                .map(|v| format!("etag:{}", io::key(v)));
            let expected = expected_download_size(
                ticket.provider,
                response.content_length(),
                ticket.expected_size,
                file["sizeBytes"].as_u64(),
            )?;
            let size_error = if ticket.provider == crate::stores::StoreProvider::Booth {
                "download_size_mismatch"
            } else {
                "file_integrity_failed"
            };
            if let Some(download) = self
                .write_response(
                    app,
                    job_id,
                    file,
                    part,
                    root,
                    cancel,
                    response,
                    current,
                    response_revision,
                    expected,
                    size_error,
                    attempt,
                    &mut last,
                )
                .await?
            {
                return Ok(download);
            }
        }
        Err(last)
    }

    /// Writes one response into the partial file; a failure worth retrying is kept in `last`.
    async fn write_response(
        &self,
        app: &tauri::AppHandle,
        job_id: &str,
        file: &Value,
        part: &Path,
        root: &Path,
        cancel: &AtomicBool,
        response: reqwest::Response,
        current: Value,
        response_revision: Option<String>,
        expected: Option<u64>,
        size_error: &str,
        attempt: u32,
        last: &mut anyhow::Error,
    ) -> Result<Option<(String, u64, String, String, Value)>> {
        archive::validate_no_links(part)?;
        tokio::fs::create_dir_all(
            part.parent()
                .ok_or_else(|| anyhow::anyhow!("archive_unsafe_path"))?,
        )
        .await?;
        archive::validate_no_links(part)?;
        let mut output = tokio::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(part)
            .await?;
        let mut hasher = Sha256::new();
        let mut size = 0u64;
        let mut chunks = response.bytes_stream();
        let mut last_progress = std::time::Instant::now();
        let mut next_space_check = 0u64;
        let mut rate = self.preferences()?["downloadRateBytesPerSecond"]
            .as_u64()
            .unwrap_or(0);
        let slots = download_count(&self.preferences()?["downloadConcurrency"]) as f64;
        let result = async {
            while let Some(chunk) = io::cancellable(
                cancel,
                tokio::time::timeout(Duration::from_secs(60), chunks.next()),
            )
            .await?
            .map_err(|_| anyhow::anyhow!("network_timeout"))?
            {
                io::check_cancel(cancel)?;
                let bytes = chunk.map_err(|_| anyhow::anyhow!("network_failed"))?;
                if size >= next_space_check {
                    io::space(root, 8 * 1024 * 1024)?;
                    next_space_check = size.saturating_add(8 * 1024 * 1024);
                }
                output.write_all(&bytes).await?;
                hasher.update(&bytes);
                size += bytes.len() as u64;
                if let Some(expected) = expected {
                    ensure!(size <= expected, "{size_error}");
                }
                if last_progress.elapsed() >= Duration::from_millis(750) {
                    self.file_progress(
                        app,
                        job_id,
                        io::required_string(file, "id")?,
                        expected
                            .filter(|n| *n > 0)
                            .map(|n| size as f64 / n as f64)
                            .unwrap_or(0.0),
                    )
                    .await?;
                    rate = self.preferences()?["downloadRateBytesPerSecond"]
                        .as_u64()
                        .unwrap_or(0);
                    last_progress = std::time::Instant::now();
                }
                if rate > 0 {
                    io::delay(
                        cancel,
                        Duration::from_secs_f64(bytes.len() as f64 * slots / rate as f64),
                    )
                    .await?;
                }
            }
            if let Some(expected) = expected {
                ensure!(size == expected, "{size_error}");
            }
            output.flush().await?;
            output.sync_all().await?;
            Ok::<(), anyhow::Error>(())
        }
        .await;
        drop(output);
        match result {
            Ok(()) => {
                let hash = hex::encode(hasher.finalize());
                let (revision, basis) = bind_revision(file, &current, response_revision, &hash)?;
                return Ok(Some((hash, size, revision, basis, current)));
            }
            Err(e) => {
                let code = io::stable_error(&e);
                if matches!(
                    code,
                    "cancelled" | "low_space" | "permission_denied" | "store_access_denied"
                ) {
                    return Err(e);
                }
                *last = e;
            }
        }
        if attempt + 1 < DOWNLOAD_ATTEMPTS {
            io::delay(cancel, Duration::from_secs(2 << attempt)).await?;
        }
        Ok(None)
    }
    pub(super) async fn extract_for_download(
        &self,
        seven_zip: &Path,
        root: &Value,
        artifact: &mut Value,
        files: &[Value],
        cancel: &Arc<AtomicBool>,
    ) -> Result<()> {
        let result = self
            .extract_one(seven_zip, root, artifact, files, cancel)
            .await;
        if let Err(error) = &result {
            artifact
                .as_object_mut()
                .unwrap()
                .remove("attentionDismissed");
            artifact["status"] = json!("extractionFailed");
            artifact["errorCode"] = json!(io::stable_error(error));
            self.catalogue.put(
                "artifact",
                io::required_string(artifact, "id")?,
                artifact.clone(),
            )?;
        }
        result
    }

    pub(super) async fn extract_one(
        &self,
        seven_zip: &Path,
        root: &Value,
        artifact: &mut Value,
        files: &[Value],
        cancel: &Arc<AtomicBool>,
    ) -> Result<()> {
        let _permit = self.extraction.acquire().await?;
        io::check_cancel(cancel)?;
        let root_path = self.root_path(root)?;
        let source = io::safe_join(&root_path, io::required_string(artifact, "relativePath")?)?;
        let current_file = self
            .catalogue
            .get("remote_file", io::required_string(artifact, "fileId")?)?
            .ok_or_else(|| anyhow::anyhow!("file_missing"))?;
        let mut candidates = HashMap::new();
        for file in files {
            if file["accountId"] != current_file["accountId"]
                || file["variantId"] != current_file["variantId"]
            {
                continue;
            }
            if let Some(a) = self.catalogue.get("artifact", &artifact_id(file))? {
                if a["originalAvailable"] == true {
                    let path = io::safe_join(&root_path, io::required_string(&a, "relativePath")?)?;
                    if path.file_name() == source.file_name() && path != source {
                        continue;
                    }
                    let h = io::required_string(&a, "hash")?.to_owned();
                    let size = a["sizeBytes"].as_u64().unwrap_or(0);
                    candidates.insert(path, (h, size));
                }
            }
        }
        // Identify the requested archive's group before reading any bytes.
        // Other complete archives in the same account/variant are unrelated.
        let volumes = archive::validate_volume_group(
            &source,
            &candidates.keys().cloned().collect::<Vec<_>>(),
        )?;
        for path in &volumes {
            let (hash, size) = candidates
                .get(path)
                .ok_or_else(|| anyhow::anyhow!("archive_missing_volume"))?;
            let (path, hash, size) = (path.clone(), hash.clone(), *size);
            tokio::task::spawn_blocking(move || io::verify(&path, &hash, size)).await??;
        }
        let id = io::required_string(artifact, "id")?.to_owned();
        artifact["status"] = json!("extracting");
        self.catalogue.put("artifact", &id, artifact.clone())?;
        let staging = io::safe_join(&root_path, ".nna-staging")?;
        std::fs::create_dir_all(&staging)?;
        let _empty_staging = io::EmptyDirectoryGuard(staging.clone());
        let archive_directory = io::readable_component(
            source
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Archive"),
            io::required_string(artifact, "fileId")?,
        );
        let tool = seven_zip.to_owned();
        let cancel2 = cancel.clone();
        let extracted = tokio::task::spawn_blocking(move || {
            archive::extract_archive(&tool, &source, &volumes, &staging, &cancel2)
        })
        .await??;
        io::check_cancel(cancel)?;
        // The listed and CRC-checked extraction must still correspond to the
        // recorded source revisions before it can become durably Ready.
        for path in &extracted.source_paths {
            let (hash, size) = candidates
                .get(path)
                .ok_or_else(|| anyhow::anyhow!("archive_missing_volume"))?;
            let (path, hash, size) = (path.clone(), hash.clone(), *size);
            tokio::task::spawn_blocking(move || io::verify(&path, &hash, size)).await??;
        }
        let product_directory =
            self.managed_product_directory(io::required_string(artifact, "productId")?)?;
        let relative = io::unused_relative(
            &root_path,
            &format!("{}/extracted/{}", product_directory, archive_directory),
        )?;
        let destination = io::safe_join(&root_path, &relative)?;
        std::fs::create_dir_all(destination.parent().unwrap())?;
        let (staging, target) = (extracted.staging_path.clone(), destination.clone());
        tokio::task::spawn_blocking(move || io::retry_busy(|| std::fs::rename(&staging, &target)))
            .await??;
        artifact["status"] = json!("ready");
        artifact["extractedPath"] = json!(relative);
        artifact["productDirectory"] = json!(product_directory);
        artifact["extractedInventory"] = serde_json::to_value(&extracted.inventory)?;
        artifact["errorCode"] = Value::Null;
        // Critical ordering: usable inventory is durable before any archive is considered removable.
        self.catalogue.put("artifact", &id, artifact.clone())?;
        // Each volume retains its own original hash while sharing one complete extracted inventory.
        let mut group_updates = Vec::new();
        for mut peer in self
            .catalogue
            .related("artifact", io::required_string(artifact, "productId")?)?
        {
            if peer["rootId"] != root["id"] || peer["productId"] != artifact["productId"] {
                continue;
            }
            let Some(path) = peer["relativePath"]
                .as_str()
                .and_then(|r| io::safe_join(&root_path, r).ok())
            else {
                continue;
            };
            if extracted.source_paths.contains(&path) {
                peer["status"] = json!("ready");
                peer["extractedPath"] = artifact["extractedPath"].clone();
                peer["productDirectory"] = artifact["productDirectory"].clone();
                peer["extractedInventory"] = artifact["extractedInventory"].clone();
                peer["errorCode"] = Value::Null;
                group_updates.push((
                    "artifact".into(),
                    io::required_string(&peer, "id")?.to_owned(),
                    peer,
                ));
            }
        }
        self.catalogue.put_many(group_updates)?;
        let product = io::required_string(artifact, "productId")?.to_owned();
        for record in self
            .remove_extracted_originals(
                root,
                &root_path,
                &product,
                &product_directory,
                extracted.source_paths,
            )
            .await?
        {
            if record["id"] == id.as_str() {
                *artifact = record;
            }
        }
        Ok(())
    }

    /// Deletes the archives an extraction made usable, returning the records it changed.
    /// A path goes only once every record naming it is Ready and its bytes still match;
    /// a failed delete leaves usable extracted state and the original available.
    pub(super) async fn remove_extracted_originals(
        &self,
        root: &Value,
        root_path: &Path,
        product: &str,
        product_directory: &str,
        sources: Vec<PathBuf>,
    ) -> Result<Vec<Value>> {
        let group_records = self.catalogue.related("artifact", product)?;
        let mut changed = Vec::new();
        for source in sources {
            let records: Vec<_> = group_records
                .iter()
                .filter(|&a| {
                    a["rootId"] == root["id"]
                        && a["relativePath"]
                            .as_str()
                            .and_then(|r| io::safe_join(root_path, r).ok())
                            .as_ref()
                            == Some(&source)
                })
                .cloned()
                .collect();
            if records.is_empty() || records.iter().any(|a| a["status"] != "ready") {
                continue;
            }
            let hash = io::required_string(&records[0], "hash")?.to_owned();
            let size = records[0]["sizeBytes"].as_u64().unwrap_or(0);
            let check = source.clone();
            tokio::task::spawn_blocking(move || io::verify(&check, &hash, size)).await??;
            let pending: Vec<_> = records
                .iter()
                .map(|record| {
                    let mut record = record.clone();
                    record["originalDeletionPending"] = json!(true);
                    (
                        "artifact".into(),
                        record["id"].as_str().unwrap().to_owned(),
                        record,
                    )
                })
                .collect();
            self.catalogue.put_many(pending)?;
            let removed = std::fs::remove_file(&source).is_ok();
            if removed {
                io::remove_empty_parents(
                    source.parent().unwrap(),
                    &io::safe_join(root_path, product_directory)?,
                );
            }
            let mut updates = Vec::new();
            for mut record in records {
                record["originalAvailable"] = json!(!removed);
                record
                    .as_object_mut()
                    .unwrap()
                    .remove("originalDeletionPending");
                let peer_id = io::required_string(&record, "id")?.to_owned();
                updates.push(("artifact".into(), peer_id, record.clone()));
                changed.push(record);
            }
            self.catalogue.put_many(updates)?;
        }
        Ok(changed)
    }

    /// An archive still beside its Ready extraction, as a download that reused that
    /// extraction kept it, goes the way extraction removes one. Failing leaves it.
    pub(super) async fn remove_kept_original(
        &self,
        root: &Value,
        root_path: &Path,
        artifact: &Value,
    ) {
        if artifact["status"] != "ready"
            || !artifact["extractedPath"].is_string()
            || artifact["originalAvailable"] != true
        {
            return;
        }
        let (Some(product), Some(relative)) = (
            artifact["productId"].as_str(),
            artifact["relativePath"].as_str(),
        ) else {
            return;
        };
        let Some(directory) = artifact["productDirectory"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| self.managed_product_directory(product).ok())
        else {
            return;
        };
        if let Ok(source) = io::safe_join(root_path, relative) {
            let _ = self
                .remove_extracted_originals(root, root_path, product, &directory, vec![source])
                .await;
        }
    }
}

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;
