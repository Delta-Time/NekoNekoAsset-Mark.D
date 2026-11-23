//! Whole local-product deletion. Only checked managed copies are removed.
use super::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs};

const JOB_TYPE: &str = "storage:delete_local_products";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteDirectory {
    product_id: String,
    root_id: String,
    directory: String,
    staged_directory: String,
}

pub(super) fn contains(parent: &Path, child: &Path) -> bool {
    #[cfg(windows)]
    {
        let components = |path: &Path| {
            path.components()
                .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
                .collect::<Vec<_>>()
        };
        components(child).starts_with(&components(parent))
    }
    #[cfg(not(windows))]
    {
        child.starts_with(parent)
    }
}

pub(super) fn product_directory(value: &str) -> Result<()> {
    let path = archive::validate_relative_path(value)
        .map_err(|_| anyhow::anyhow!("local_delete_unsafe_path"))?;
    let parts: Vec<_> = path.components().collect();
    ensure!(
        parts.len() == 2 && parts[0].as_os_str() == "assets",
        "local_delete_unsafe_path"
    );
    ensure!(
        !parts[1]
            .as_os_str()
            .to_string_lossy()
            .starts_with(".nna-delete-"),
        "local_delete_unsafe_path"
    );
    Ok(())
}

/// Inspect the complete tree before staging/removal; never traverse a reparse point.
pub(super) fn checked_tree(path: &Path) -> Result<()> {
    let mut pending = vec![path.to_owned()];
    while let Some(path) = pending.pop() {
        io::reject_link(&path).map_err(|_| anyhow::anyhow!("local_delete_unsafe_path"))?;
        let metadata = fs::metadata(&path)?;
        if metadata.is_dir() {
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
        } else {
            ensure!(metadata.is_file(), "local_delete_unsafe_path");
        }
    }
    Ok(())
}

/// Explicit entry-by-entry removal preserves the no-link boundary at every step.
pub(super) fn remove_checked_tree(path: &Path) -> Result<()> {
    checked_tree(path)?;
    let mut pending = vec![(path.to_owned(), false)];
    while let Some((path, visited)) = pending.pop() {
        io::reject_link(&path).map_err(|_| anyhow::anyhow!("local_delete_unsafe_path"))?;
        if fs::metadata(&path)?.is_dir() {
            if visited {
                fs::remove_dir(&path)?;
            } else {
                pending.push((path.clone(), true));
                for entry in fs::read_dir(&path)? {
                    pending.push((entry?.path(), false));
                }
            }
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

impl StorageService {
    pub(super) fn delete_root(&self, id: Option<&str>) -> Result<PathBuf> {
        let path = self.root_path(&self.root(id)?).map_err(|error| {
            if error.to_string() == "unsafe_link" {
                anyhow::anyhow!("local_delete_unsafe_path")
            } else {
                error
            }
        })?;
        archive::validate_no_links(&path)
            .map_err(|_| anyhow::anyhow!("local_delete_unsafe_path"))?;
        Ok(path.canonicalize()?)
    }
    pub(super) async fn delete_local_products(
        &self,
        args: &Value,
        retry: Option<&str>,
    ) -> Result<Value> {
        // Keep both guards inside the blocking task: dropping an IPC future must
        // not release them while filesystem changes are still in progress.
        let work = self.catalogue.begin_restore_work()?;
        let operation = self
            .operations
            .clone()
            .try_write_owned()
            .map_err(|_| anyhow::anyhow!("storage_busy"))?;
        let service = self.clone();
        let args = args.clone();
        let retry = retry.map(str::to_owned);
        tokio::task::spawn_blocking(move || {
            let (_work, _operation) = (work, operation);
            service.run_local_delete(&args, retry.as_deref())
        })
        .await
        .map_err(|_| anyhow::anyhow!("storage_failed"))?
    }

    fn delete_plan(&self, ids: &[String], job_id: &str) -> Result<Vec<DeleteDirectory>> {
        let artifacts = self.catalogue.list("artifact")?;
        let imports = self.local_import_directories()?;
        let mut directories = BTreeSet::new();
        for id in ids {
            let product = self
                .catalogue
                .get("product", id)?
                .ok_or_else(|| anyhow::anyhow!("product_missing"))?;
            ensure!(product["provider"] == "local", "local_product_required");
            if let Some(directory) = product["localManifest"]["productDirectory"].as_str() {
                product_directory(directory)?;
                directories.insert((
                    id.clone(),
                    io::required_string(&product["localManifest"], "rootId")?.to_owned(),
                    directory.to_owned(),
                ));
            }
            for artifact in artifacts.iter().filter(|a| a["productId"] == *id) {
                let directory = io::required_string(artifact, "productDirectory")?;
                product_directory(directory)?;
                for field in ["relativePath", "extractedPath"] {
                    if let Some(relative) = artifact[field].as_str() {
                        ensure!(
                            contains(
                                Path::new(directory),
                                &archive::validate_relative_path(relative)?
                            ),
                            "local_delete_unsafe_path"
                        );
                    }
                }
                directories.insert((
                    id.clone(),
                    io::required_string(artifact, "rootId")?.to_owned(),
                    directory.to_owned(),
                ));
            }
            // Membership may be empty, with user-edited bytes retained by file
            // removal. Its imports still own their stable directories.
            for (product, root, prefix) in &imports {
                if product == id {
                    product_directory(prefix)?;
                    directories.insert((id.clone(), root.clone(), prefix.clone()));
                }
            }
        }
        let mut plan = Vec::new();
        for (product_id, root_id, directory) in directories {
            let root = self.delete_root(Some(&root_id))?;
            let target = io::safe_join(&root, &directory)
                .map_err(|_| anyhow::anyhow!("local_delete_unsafe_path"))?;
            self.require_unshared_delete_path(&product_id, &target)?;
            match fs::symlink_metadata(&target) {
                Ok(metadata) => {
                    ensure!(metadata.is_dir(), "local_delete_unsafe_path");
                    checked_tree(&target)?;
                    ensure!(
                        contains(&root.join("assets"), &target.canonicalize()?),
                        "local_delete_unsafe_path"
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            ensure!(
                !plan.iter().any(|entry: &DeleteDirectory| {
                    self.delete_root(Some(&entry.root_id))
                        .and_then(|p| io::safe_join(&p, &entry.directory))
                        .is_ok_and(|p| contains(&p, &target) || contains(&target, &p))
                }),
                "local_delete_shared_path"
            );
            let staged_directory = format!("assets/.nna-delete-{job_id}-{}", plan.len());
            let staged = io::safe_join(&root, &staged_directory)?;
            ensure!(
                fs::symlink_metadata(staged)
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
                "local_delete_unsafe_path"
            );
            plan.push(DeleteDirectory {
                product_id,
                root_id,
                directory,
                staged_directory,
            });
        }
        Ok(plan)
    }

    pub(super) fn require_unshared_delete_path(&self, product: &str, target: &Path) -> Result<()> {
        for artifact in self
            .catalogue
            .list("artifact")?
            .into_iter()
            .filter(|a| a["productId"] != product)
        {
            let root = self.delete_root(artifact["rootId"].as_str())?;
            for field in ["productDirectory", "relativePath", "extractedPath"] {
                if let Some(relative) = artifact[field].as_str() {
                    let path = io::safe_join(&root, relative)?;
                    ensure!(
                        !contains(target, &path) && !contains(&path, target),
                        "local_delete_shared_path"
                    );
                }
            }
        }
        for (other, root, prefix) in self.local_import_directories()? {
            if other != product {
                let path = io::safe_join(&self.delete_root(Some(&root))?, &prefix)?;
                ensure!(
                    !contains(target, &path) && !contains(&path, target),
                    "local_delete_shared_path"
                );
            }
        }
        for source in self
            .catalogue
            .list("product")?
            .into_iter()
            .filter(|p| p["id"] != product && p["provider"] == "local")
        {
            if let Some(directory) = source["localManifest"]["productDirectory"].as_str() {
                let root = self.delete_root(source["localManifest"]["rootId"].as_str())?;
                let path = io::safe_join(&root, directory)?;
                ensure!(
                    !contains(target, &path) && !contains(&path, target),
                    "local_delete_shared_path"
                );
            }
            for relative in strings(&source["localManifest"]["directories"]) {
                let root = self.delete_root(source["localManifest"]["rootId"].as_str())?;
                let path = io::safe_join(&root, &relative)?;
                ensure!(
                    !contains(target, &path) && !contains(&path, target),
                    "local_delete_shared_path"
                );
            }
        }
        Ok(())
    }

    fn delete_paths(
        &self,
        job_id: &str,
        index: usize,
        entry: &DeleteDirectory,
    ) -> Result<(PathBuf, PathBuf)> {
        uuid::Uuid::parse_str(job_id).map_err(|_| anyhow::anyhow!("local_delete_unsafe_path"))?;
        product_directory(&entry.directory)?;
        ensure!(
            entry.staged_directory == format!("assets/.nna-delete-{job_id}-{index}"),
            "local_delete_unsafe_path"
        );
        let root = self.delete_root(Some(&entry.root_id))?;
        let original = io::safe_join(&root, &entry.directory)
            .map_err(|_| anyhow::anyhow!("local_delete_unsafe_path"))?;
        let staged = io::safe_join(&root, &entry.staged_directory)
            .map_err(|_| anyhow::anyhow!("local_delete_unsafe_path"))?;
        ensure!(
            contains(&root.join("assets"), &original) && contains(&root.join("assets"), &staged),
            "local_delete_unsafe_path"
        );
        if staged.exists() {
            ensure!(
                contains(&root.join("assets"), &staged.canonicalize()?),
                "local_delete_unsafe_path"
            );
        }
        Ok((original, staged))
    }

    fn rollback_delete(&self, id: &str, directories: &[DeleteDirectory]) -> Result<()> {
        for (index, entry) in directories.iter().enumerate().rev() {
            let (original, staged) = self.delete_paths(id, index, entry)?;
            if fs::symlink_metadata(&staged).is_ok() {
                ensure!(!original.exists(), "local_delete_rollback_failed");
                fs::rename(staged, original)?;
            }
        }
        Ok(())
    }

    fn run_local_delete(&self, args: &Value, retry: Option<&str>) -> Result<Value> {
        let mut job = if let Some(id) = retry {
            let job = self
                .catalogue
                .get("job", id)?
                .ok_or_else(|| anyhow::anyhow!("job_missing"))?;
            ensure!(
                job["type"] == JOB_TYPE && job["localDelete"].is_object(),
                "invalid_job"
            );
            ensure!(
                self.catalogue.library_root().is_none()
                    || job["originDeviceId"] == self.catalogue.device_id(),
                "local_other_device"
            );
            job
        } else {
            let values = args["productIds"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("invalid_arguments"))?;
            let ids = strings(&args["productIds"]);
            ensure!(
                !ids.is_empty()
                    && ids.len() == values.len()
                    && ids.iter().all(|id| !id.is_empty())
                    && ids.iter().collect::<HashSet<_>>().len() == ids.len(),
                "invalid_arguments"
            );
            ensure!(
                !self
                    .catalogue
                    .list("job")?
                    .iter()
                    .any(|job| job["type"] == JOB_TYPE
                        && job["state"] != "completed"
                        && strings(&job["productIds"])
                            .iter()
                            .any(|id| ids.contains(id))),
                "storage_busy"
            );
            json!({"id":uuid::Uuid::new_v4().to_string(),"type":JOB_TYPE,"originDeviceId":self.catalogue.device_id(),"productIds":ids,"state":"running","createdAt":now(),"updatedAt":now(),"localDelete":{"recordsDeleted":false}})
        };
        let id = io::required_string(&job, "id")?.to_owned();
        let ids = strings(&job["productIds"]);
        if job["localDelete"]["recordsDeleted"] != true {
            if let Some(value) = job["localDelete"].get("directories") {
                let old: Vec<DeleteDirectory> = serde_json::from_value(value.clone())?;
                if self.rollback_delete(&id, &old).is_err() {
                    self.put_job(
                        &mut job,
                        "needsAction",
                        Some("local_delete_rollback_failed"),
                    )?;
                    bail!("local_delete_rollback_failed");
                }
            }
            let directories = self.delete_plan(&ids, &id)?;
            job["localDelete"]["directories"] = json!(directories);
            self.put_job(&mut job, "running", None)?;
            let staged = (|| -> Result<()> {
                for (index, entry) in directories.iter().enumerate() {
                    let (original, staged) = self.delete_paths(&id, index, entry)?;
                    if fs::symlink_metadata(&original).is_ok() {
                        fs::rename(original, staged)?;
                    }
                }
                let mut committed = job.clone();
                committed["localDelete"]["recordsDeleted"] = json!(true);
                self.catalogue
                    .delete_local_products(&ids, committed.clone())?;
                job = committed;
                Ok(())
            })();
            if let Err(error) = staged {
                let rollback_failed = self.rollback_delete(&id, &directories).is_err();
                let code = if rollback_failed {
                    "local_delete_rollback_failed"
                } else {
                    io::stable_error(&error)
                };
                self.put_job(&mut job, "needsAction", Some(code))?;
                if rollback_failed {
                    bail!("local_delete_rollback_failed");
                }
                // The durable job still owns these IDs after a successful
                // rollback. Surface its retry route instead of inviting a new
                // deletion that would correctly be blocked by that job.
                bail!("local_delete_retry_required");
            }
        }
        let directories: Vec<DeleteDirectory> =
            serde_json::from_value(job["localDelete"]["directories"].clone())?;
        self.put_job(&mut job, "running", None)?;
        let cleanup = (|| -> Result<()> {
            for (index, entry) in directories.iter().enumerate() {
                let (_, staged) = self.delete_paths(&id, index, entry)?;
                self.require_unshared_delete_path(&entry.product_id, &staged)?;
                match fs::symlink_metadata(&staged) {
                    Ok(_) => remove_checked_tree(&staged)?,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(())
        })();
        if cleanup.is_err() {
            self.put_job(&mut job, "needsAction", Some("local_delete_cleanup_failed"))?;
            bail!("local_delete_cleanup_failed");
        }
        job["result"] = json!({"deletedProductIds":ids});
        self.put_job(&mut job, "completed", None)?;
        super::api::answer(super::api::DeletedProducts {
            deleted_product_ids: ids,
        })
    }
}

#[cfg(test)]
#[path = "local_delete_tests.rs"]
mod tests;
