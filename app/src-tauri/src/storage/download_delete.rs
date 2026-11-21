//! Explicit removal of downloaded store files. Product, source file and
//! entitlement records stay, so the files can be downloaded again.
use super::local_delete::{checked_tree, contains, product_directory, remove_checked_tree};
use super::*;
use std::{collections::BTreeSet, fs};

/// While a product's bytes are removed, no download for it can be queued.
pub(super) struct DeleteClaim {
    claimed: Arc<Mutex<HashSet<String>>>,
    ids: Vec<String>,
}
impl Drop for DeleteClaim {
    fn drop(&mut self) {
        if let Ok(mut claimed) = self.claimed.lock() {
            for id in &self.ids {
                claimed.remove(id);
            }
        }
    }
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
        let work = self.catalogue.begin_catalogue_work()?;
        let operation = self
            .operations
            .clone()
            .try_read_owned()
            .map_err(|_| anyhow::anyhow!("storage_busy"))?;
        let claim = {
            let _gate = self.queue_gate.lock().await;
            let busy = self.catalogue.list("job")?.iter().any(|job| {
                matches!(job["state"].as_str(), Some("queued" | "running"))
                    && (job["type"] == "download"
                        || job["type"] == "unity:import"
                        || job["type"] == "storage:import_local")
                    && std::iter::once(&job["productId"])
                        .chain(job["productIds"].as_array().into_iter().flatten())
                        .filter_map(Value::as_str)
                        .any(|id| products.iter().any(|product| product == id))
            });
            let mut claimed = self.deleting.lock().unwrap();
            ensure!(
                !busy && products.iter().all(|id| !claimed.contains(id)),
                "storage_busy"
            );
            claimed.extend(products.iter().cloned());
            DeleteClaim {
                claimed: self.deleting.clone(),
                ids: products.clone(),
            }
        };
        let service = self.clone();
        tokio::task::spawn_blocking(move || {
            // Dropping the IPC future must not release these while files are removed.
            let _guards = (work, operation, claim);
            service.remove_downloads(&products, file.as_deref())
        })
        .await
        .map_err(|_| anyhow::anyhow!("storage_failed"))?
    }

    fn remove_downloads(&self, products: &[String], file: Option<&str>) -> Result<Value> {
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
        super::api::answer(super::api::DeletedFiles {
            deleted_file_ids: deleted.into_iter().map(str::to_owned).collect(),
        })
    }
}

#[cfg(test)]
#[path = "download_delete_tests.rs"]
mod tests;
