//! Explicit removal of downloaded store files. Product, source file and
//! entitlement records stay, so the files can be downloaded again.
use super::local_delete::{checked_tree, contains, product_directory, remove_checked_tree};
use super::*;
use std::{collections::BTreeSet, fs};

/// One product's removal, or one file's. The storage worker runs it after the download in
/// progress and before any queued one; while it waits or runs, no download of its product
/// can be queued.
pub(super) const DELETE_JOB: &str = "storage:delete_downloads";

fn active(job: &Value) -> bool {
    matches!(job["state"].as_str(), Some("queued" | "running"))
}

/// Whether other work holds the product's files: a download, a Unity transfer or a local
/// import.
fn files_in_use(jobs: &[Value], product: &str) -> bool {
    jobs.iter().any(|job| {
        active(job)
            && matches!(
                job["type"].as_str(),
                Some("download" | "unity:import" | "storage:import_local")
            )
            && std::iter::once(&job["productId"])
                .chain(job["productIds"].as_array().into_iter().flatten())
                .any(|id| id == product)
    })
}

/// The products whose removal waits or runs.
pub(super) fn deleting(jobs: &[Value]) -> HashSet<&str> {
    jobs.iter()
        .filter(|job| job["type"] == DELETE_JOB && active(job))
        .filter_map(|job| job["productId"].as_str())
        .collect()
}

/// Bytes an artifact owns: its retained original and its extraction.
fn owned_paths(artifact: &Value) -> Vec<&str> {
    let original = artifact["relativePath"]
        .as_str()
        .filter(|_| artifact["originalAvailable"] == true);
    original
        .into_iter()
        .chain(artifact["extractedPath"].as_str())
        .collect()
}

/// `assets/<product>` in lower case; managed paths can only overlap below it.
fn product_top(path: &str) -> Option<String> {
    let mut parts = path.split(['/', '\\']);
    Some(format!("{}/{}", parts.next()?, parts.next()?).to_lowercase())
}

impl StorageService {
    /// Queues the removal of these products' downloads, or of one file, and answers its jobs.
    pub(super) async fn delete_downloads(&self, args: &Value) -> Result<Value> {
        let (products, file) = if let Some(id) = args["fileId"].as_str() {
            ensure!(args.get("productIds").is_none(), "invalid_arguments");
            let file = self
                .catalogue
                .get("remote_file", id)?
                .ok_or_else(|| anyhow::anyhow!("file_missing"))?;
            (
                vec![io::required_string(&file, "productId")?.to_owned()],
                Some(id.to_owned()),
            )
        } else {
            let ids = strings(&args["productIds"]);
            ensure!(
                !ids.is_empty()
                    && args["productIds"].as_array().map(Vec::len) == Some(ids.len())
                    && ids.iter().all(|id| !id.is_empty())
                    && ids.iter().collect::<HashSet<_>>().len() == ids.len(),
                "invalid_arguments"
            );
            (ids, None)
        };
        for id in &products {
            let product = self
                .catalogue
                .get("product", id)?
                .ok_or_else(|| anyhow::anyhow!("product_missing"))?;
            // Offline products hold their only copy; they use Remove or Delete instead.
            ensure!(product["provider"] != "local", "invalid_arguments");
        }
        // Only the jobs are written here, which no download or busy disk holds up for long.
        let _work = self.catalogue.begin_catalogue_work()?;
        let _gate = self.queue_gate.lock().await;
        let jobs = self.catalogue.list("job")?;
        ensure!(
            !products.iter().any(|id| files_in_use(&jobs, id)),
            "storage_busy"
        );
        let mut writes = Vec::new();
        let mut ids = Vec::new();
        for product in &products {
            // A removal already waiting answers for the same request, and one of the whole
            // product for any of its files.
            if let Some(job) = jobs.iter().find(|job| {
                job["type"] == DELETE_JOB
                    && active(job)
                    && job["productId"] == product.as_str()
                    && (job["fileId"].is_null() || job["fileId"].as_str() == file.as_deref())
            }) {
                ids.push(io::required_string(job, "id")?.to_owned());
                continue;
            }
            let id = uuid::Uuid::new_v4().to_string();
            // An empty bar while it waits, as a queued download shows.
            let mut job = json!({"id":id,"type":DELETE_JOB,"productId":product,"state":"queued","progress":0,"createdAt":now(),"updatedAt":now()});
            if let Some(file) = &file {
                job["fileId"] = json!(file);
            }
            writes.push(("job".to_owned(), id.clone(), job));
            ids.push(id);
        }
        self.catalogue.put_many(writes)?;
        super::api::answer(super::api::DownloadJobs { job_ids: ids })
    }

    /// Queues a removal that failed or was cancelled again, as it was asked for.
    pub(super) async fn retry_delete(&self, id: &str) -> Result<Value> {
        let _work = self.catalogue.begin_catalogue_work()?;
        let _gate = self.queue_gate.lock().await;
        let mut job = self
            .catalogue
            .get("job", id)?
            .ok_or_else(|| anyhow::anyhow!("job_missing"))?;
        ensure!(job["type"] == DELETE_JOB, "invalid_job");
        if !active(&job) {
            let product = io::required_string(&job, "productId")?;
            ensure!(
                self.catalogue.get("product", product)?.is_some(),
                "product_missing"
            );
            ensure!(
                !files_in_use(&self.catalogue.list("job")?, product),
                "storage_busy"
            );
            job.as_object_mut().unwrap().remove("cancelRequested");
            job["progress"] = json!(0);
            self.put_job(&mut job, "queued", None)?;
        }
        super::api::answer(super::api::DownloadJobs {
            job_ids: vec![id.to_owned()],
        })
    }

    /// The oldest queued removal whose files no other work holds. One waiting for a Unity
    /// transfer of its product runs once that ends.
    pub(super) fn queued_delete(&self) -> Result<Option<Value>> {
        let mut queued = self.catalogue.queued_jobs(Some(DELETE_JOB), None)?;
        if queued.is_empty() {
            return Ok(None);
        }
        queued.sort_by(|a, b| a["createdAt"].as_str().cmp(&b["createdAt"].as_str()));
        let jobs = self.catalogue.list("job")?;
        Ok(queued
            .into_iter()
            .find(|job| !files_in_use(&jobs, job["productId"].as_str().unwrap_or(""))))
    }

    /// Runs a queued removal; the storage worker waits for it.
    pub(super) async fn process_delete_job(&self, app: &tauri::AppHandle, job: Value) {
        let _operation = self.operations.read().await;
        let id = job["id"].as_str().unwrap_or("").to_owned();
        let (service, events, worker) = (self.clone(), app.clone(), id.clone());
        let ran = tokio::task::spawn_blocking(move || {
            service.run_delete_job(&worker, || service.jobs_changed(&events, &worker))
        })
        .await;
        if ran.is_err() {
            if let Ok(Some(mut job)) = self.catalogue.get("job", &id) {
                let _ = self.put_job(&mut job, "failed", Some("storage_failed"));
            }
        }
        self.changed(app, Some(&id));
    }

    /// Runs the removal if it still waits, which a cancel may have ended, and records how it
    /// ended; `started` follows its start. Answers the files whose bytes are gone.
    pub(super) fn run_delete_job(&self, id: &str, started: impl FnOnce()) -> Result<Vec<String>> {
        let Some(before) = self
            .catalogue
            .get("job", id)?
            .filter(|job| job["state"] == "queued")
        else {
            return Ok(Vec::new());
        };
        // Running, its bar moves without a percent.
        let mut job = before.clone();
        job.as_object_mut().unwrap().remove("progress");
        job["state"] = json!("running");
        job["updatedAt"] = json!(now());
        if !self.catalogue.put_many_if_unchanged(
            vec![("job".into(), id.to_owned(), job.clone())],
            vec![("job".into(), id.to_owned(), Some(before))],
        )? {
            return Ok(Vec::new());
        }
        started();
        let result = self.remove_asked(&job);
        let (state, error) = match &result {
            Ok(_) => ("completed", None),
            Err(error) => ("needsAction", Some(io::stable_error(error))),
        };
        self.put_job(&mut job, state, error)?;
        result
    }

    /// A product removed from the library meanwhile left nothing to remove.
    fn remove_asked(&self, job: &Value) -> Result<Vec<String>> {
        let product = io::required_string(job, "productId")?;
        if self.catalogue.get("product", product)?.is_none() {
            return Ok(Vec::new());
        }
        self.remove_downloads(&[product.to_owned()], job["fileId"].as_str())
    }

    fn remove_downloads(&self, products: &[String], file: Option<&str>) -> Result<Vec<String>> {
        // Removing a product's downloads leaves every file outside the store's
        // current download list, shown or not, and every file the store refused:
        // the store cannot provide it again. Each kept file's own delete still removes it.
        let mut irreplaceable = HashSet::new();
        if file.is_none() {
            for product in products {
                let attached: HashSet<String> = self
                    .without_last_failure(
                        self.remote_files(product)?.0,
                        &["store_file_unavailable"],
                    )?
                    .iter()
                    .filter_map(|f| f["id"].as_str().map(str::to_owned))
                    .collect();
                irreplaceable.extend(
                    self.catalogue
                        .related("remote_file", product)?
                        .iter()
                        .filter_map(|f| f["id"].as_str().map(str::to_owned))
                        .filter(|id| !attached.contains(id)),
                );
            }
        }
        let (targets, kept): (Vec<_>, Vec<_>) = self
            .catalogue
            .list("artifact")?
            .into_iter()
            .partition(|artifact| {
                artifact["productId"]
                        .as_str()
                        .is_some_and(|id| products.iter().any(|product| product == id))
                        && file.is_none_or(|file| artifact["fileId"] == file)
                        && !artifact["fileId"]
                            .as_str()
                            .is_some_and(|id| irreplaceable.contains(id))
                        // The store's refusal stays, so a refused file's own delete takes
                        // it out of the list instead of offering it again.
                        && artifact["errorCode"] != "store_file_unavailable"
            });
        let mut tops = HashSet::new();
        for artifact in &targets {
            let directory = io::required_string(artifact, "productDirectory")?;
            product_directory(directory)?;
            tops.extend(product_top(directory));
        }
        // Bytes still recorded by another file or product stay in place.
        let mut references = Vec::new();
        for artifact in &kept {
            let same_product = artifact["productId"]
                .as_str()
                .is_some_and(|id| products.iter().any(|product| product == id));
            for field in ["relativePath", "extractedPath", "productDirectory"] {
                let Some(path) = artifact[field].as_str() else {
                    continue;
                };
                if (field != "productDirectory" || !same_product)
                    && product_top(path).is_none_or(|top| tops.contains(&top))
                {
                    references.push((artifact["rootId"].as_str().unwrap_or(""), path));
                }
            }
        }
        // Check every path before removing any of them.
        let mut plan: Vec<(String, String, PathBuf, PathBuf)> = Vec::new();
        for artifact in &targets {
            let directory = Path::new(io::required_string(artifact, "productDirectory")?);
            let root_id = io::required_string(artifact, "rootId")?;
            for relative in owned_paths(artifact) {
                let path = archive::validate_relative_path(relative)
                    .map_err(|_| anyhow::anyhow!("local_delete_unsafe_path"))?;
                ensure!(
                    contains(directory, &path)
                        && path.components().count() > directory.components().count(),
                    "local_delete_unsafe_path"
                );
                if plan
                    .iter()
                    .any(|(root, planned, ..)| root == root_id && planned == relative)
                    || references.iter().any(|(root, other)| {
                        *root == root_id
                            && archive::validate_relative_path(other).map_or(true, |other| {
                                contains(&other, &path) || contains(&path, &other)
                            })
                    })
                {
                    continue;
                }
                let root = self.delete_root(Some(root_id))?;
                let absolute = io::safe_join(&root, relative)
                    .map_err(|_| anyhow::anyhow!("local_delete_unsafe_path"))?;
                match fs::symlink_metadata(&absolute) {
                    Ok(_) => {
                        checked_tree(&absolute)?;
                        ensure!(
                            contains(&root.join("assets"), &absolute.canonicalize()?),
                            "local_delete_unsafe_path"
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
                plan.push((
                    root_id.to_owned(),
                    relative.to_owned(),
                    absolute,
                    root.join("assets"),
                ));
            }
        }
        let mut failed = HashSet::new();
        for (root_id, relative, absolute, boundary) in &plan {
            let removed = match fs::symlink_metadata(absolute) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.into()),
                Ok(metadata) if metadata.is_dir() => remove_checked_tree(absolute),
                Ok(_) => io::reject_link(absolute)
                    .and_then(|_| fs::remove_file(absolute).map_err(Into::into)),
            };
            if removed.is_err() {
                failed.insert((root_id.as_str(), relative.as_str()));
            } else if let Some(parent) = absolute.parent() {
                io::remove_empty_parents(parent, boundary);
            }
        }
        // A file left partly on disk is shown as missing until a retry removes the rest.
        let mut records = Vec::new();
        let mut expected = Vec::new();
        let mut incomplete = BTreeSet::new();
        for artifact in &targets {
            let id = io::required_string(artifact, "id")?.to_owned();
            let root = artifact["rootId"].as_str().unwrap_or("");
            let value = if owned_paths(artifact)
                .into_iter()
                .any(|path| failed.contains(&(root, path)))
            {
                incomplete.extend(artifact["fileId"].as_str());
                let mut missing = artifact.clone();
                missing
                    .as_object_mut()
                    .unwrap()
                    .remove("attentionDismissed");
                missing["status"] = json!("missing");
                missing["errorCode"] = json!("download_delete_incomplete");
                missing["updatedAt"] = json!(now());
                Some(missing)
            } else {
                None
            };
            expected.push(("artifact".into(), id.clone(), Some(artifact.clone())));
            records.push(("artifact".into(), id, value));
        }
        if let Some(id) = file.filter(|id| !incomplete.contains(*id)) {
            // A file the user added to a store product leaves with its bytes.
            if let Some(remote) = self
                .catalogue
                .get("remote_file", id)?
                .filter(|remote| remote["originKind"] == "local")
            {
                expected.push(("remote_file".into(), id.to_owned(), Some(remote)));
                records.push(("remote_file".into(), id.to_owned(), None));
            }
        }
        if file.is_none() {
            // Explicit removal also stops automatic downloads until the next Download.
            for id in products {
                records.push((
                    "download_intent".into(),
                    id.clone(),
                    Some(json!({"id":id,"wantDownload":false})),
                ));
            }
        }
        ensure!(
            self.catalogue.write_many_if_unchanged(records, expected)?,
            "storage_busy"
        );
        ensure!(failed.is_empty(), "download_delete_incomplete");
        let deleted: BTreeSet<_> = targets
            .iter()
            .filter_map(|artifact| artifact["fileId"].as_str())
            .filter(|id| !incomplete.contains(id))
            .collect();
        Ok(deleted.into_iter().map(str::to_owned).collect())
    }
}

#[cfg(test)]
#[path = "download_delete_tests.rs"]
mod tests;
