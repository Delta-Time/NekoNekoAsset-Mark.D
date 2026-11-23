//! Device-local request evidence. Activity rows are disposable projections, never dedupe authority.
use crate::catalogue::Catalogue;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::Arc;

const KIND: &str = "unity_request";

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestRecord {
    pub request_id: String,
    pub origin_device_id: String,
    pub fingerprint: String,
    pub instance_id: String,
    pub session_id: String,
    pub project_name: String,
    pub project_root: String,
    pub unity_version: String,
    pub product_ids: Vec<String>,
    pub manifest_json: String,
    pub phase: String,
    pub bytes_started: bool,
    pub payload_rejected: bool,
    pub snapshot_ready: bool,
    pub snapshot_removed: bool,
    pub receipt: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
    pub error_code: Option<String>,
}

fn terminal(phase: &str) -> bool {
    matches!(phase, "completed" | "cancelled" | "failed" | "interrupted")
}

/// The Editor took the connection but sent no answer: its main thread is busy, not gone.
/// Any reply, even an invalid or unauthorized one, is not this.
pub(super) fn no_answer(error: &str) -> bool {
    matches!(
        error.split(':').next(),
        Some("UNITY_TIMEOUT" | "UNITY_NO_RESPONSE")
    )
}

/// Applied, skipped and failed files as the Bridge's review counts them: a result whose path
/// holds other results is a folder, and a loose file's `.meta` goes with its file.
fn file_counts(results: &[Value]) -> Value {
    let paths: Vec<String> = results
        .iter()
        .map(|result| result["path"].as_str().unwrap_or_default().to_lowercase())
        .collect();
    let folders: HashSet<&str> = paths
        .iter()
        .flat_map(|path| path.match_indices('/').map(|(end, _)| &path[..end]))
        .collect();
    let count = |state: &str| {
        results
            .iter()
            .zip(&paths)
            .filter(|(result, path)| {
                result["state"] == state
                    && !path.is_empty()
                    && !path.ends_with(".meta")
                    && !folders.contains(path.as_str())
            })
            .count()
    };
    json!({"applied":count("applied"),"skipped":count("skipped"),"failed":count("failed")})
}

impl RequestRecord {
    pub(crate) fn snapshot_cleanup_proved(&self) -> bool {
        !self.bytes_started
            || self.payload_rejected
            || self.receipt.as_ref().is_some_and(|receipt| {
                receipt["staged"] == true || terminal(receipt["stage"].as_str().unwrap_or(""))
            })
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            uuid::Uuid::parse_str(&self.request_id).is_ok()
                && uuid::Uuid::parse_str(&self.session_id).is_ok()
                && [&self.fingerprint, &self.instance_id, &self.origin_device_id]
                    .iter()
                    .all(|v| {
                        v.len() == 64
                            && v.bytes()
                                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                    }),
            "unity_invalid_request"
        );
        ensure!(
            matches!(
                self.phase.as_str(),
                "preparing"
                    | "staging"
                    | "waitingForUser"
                    | "importing"
                    | "refreshing"
                    | "compiling"
                    | "completed"
                    | "cancelled"
                    | "failed"
                    | "interrupted"
                    | "unresolved"
            ),
            "unity_invalid_phase"
        );
        ensure!(
            self.phase != "unresolved" || self.error_code.is_some(),
            "unity_unresolved_reason_required"
        );
        ensure!(
            !self.payload_rejected
                || (self.phase == "failed"
                    && self.receipt.is_none()
                    && matches!(
                        self.error_code.as_deref(),
                        Some("UNITY_BUSY" | "UNITY_DISABLED" | "UNITY_INVALID_RESPONSE")
                    )),
            "unity_invalid_payload_rejection"
        );
        ensure!(
            self.phase != "completed"
                || self
                    .receipt
                    .as_ref()
                    .is_some_and(|receipt| receipt["stage"] == "completed"),
            "unity_completion_unproven"
        );
        ensure!(
            hex::encode(Sha256::digest(self.manifest_json.as_bytes())) == self.fingerprint,
            "unity_manifest_mismatch"
        );
        if !self.manifest_json.is_empty() {
            let manifest: Value = serde_json::from_str(&self.manifest_json)
                .map_err(|_| anyhow::anyhow!("unity_manifest_mismatch"))?;
            ensure!(
                manifest["instanceId"] == self.instance_id
                    && manifest["sessionId"] == self.session_id,
                "unity_manifest_mismatch"
            );
        }
        ensure!(
            self.error_code.as_ref().is_none_or(|code| !code.is_empty()
                && code.len() <= 128
                && code
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')),
            "unity_invalid_error_code"
        );
        if let Some(receipt) = &self.receipt {
            ensure!(
                receipt.is_object()
                    && receipt["requestId"] == self.request_id
                    && receipt["fingerprint"] == self.fingerprint
                    && receipt["instanceId"] == self.instance_id
                    && receipt["sessionId"] == self.session_id,
                "unity_receipt_mismatch"
            );
            // The transport validates full receipt content. Never retain connection credentials.
            ensure!(
                receipt.get("capability").is_none() && receipt.get("port").is_none(),
                "unity_invalid_receipt"
            );
        }
        Ok(())
    }

    fn stored(&self) -> Result<Value> {
        self.validate()?;
        let mut value = serde_json::to_value(self)?;
        value["id"] = json!(self.request_id);
        Ok(value)
    }

    fn from_stored(mut value: Value) -> Result<Self> {
        let id = value
            .as_object_mut()
            .context("unity_invalid_request")?
            .remove("id")
            .context("unity_invalid_request")?;
        let record: Self = serde_json::from_value(value)?;
        ensure!(id == record.request_id, "unity_invalid_request");
        record.validate()?;
        Ok(record)
    }

    fn same_request(&self, other: &Self) -> bool {
        let freeze = self.phase == "preparing"
            && self.manifest_json.is_empty()
            && !self.bytes_started
            && !self.snapshot_ready
            && self.receipt.is_none()
            && other.phase == "staging"
            && !other.manifest_json.is_empty();
        self.request_id == other.request_id
            && self.origin_device_id == other.origin_device_id
            && (freeze
                || (self.fingerprint == other.fingerprint
                    && self.manifest_json == other.manifest_json))
            && self.instance_id == other.instance_id
            && self.session_id == other.session_id
            && self.product_ids == other.product_ids
            && self.project_name == other.project_name
            && self.project_root == other.project_root
            && self.unity_version == other.unity_version
            && self.created_at == other.created_at
    }

    /// Writing or refreshing holds Unity's main thread, and Bridges before 0.4.0 then answer
    /// nothing: the request keeps its last stage instead of turning unresolved.
    pub(super) fn keeps_stage_without_answer(&self, error: &str) -> bool {
        matches!(
            self.phase.as_str(),
            "importing" | "refreshing" | "compiling"
        ) && no_answer(error)
    }

    pub(super) fn presented_phase(&self) -> &str {
        // Older v1 Bridges may still wait for compilation after applying the files.
        // Keep that request active without presenting compiler state as importer work.
        if self.phase == "compiling" {
            "refreshing"
        } else {
            &self.phase
        }
    }

    pub(super) fn presented_receipt(&self) -> Option<Value> {
        self.receipt.as_ref().map(|receipt| {
            let mut projected = receipt.clone();
            let failed = receipt["results"]
                .as_array()
                .is_some_and(|results| results.iter().any(|result| result["state"] == "failed"));
            let compiler_summary = match receipt["stage"].as_str() {
                Some("completed") if !failed => {
                    receipt["error"] == "Files imported; Unity compilation reported errors."
                }
                Some("interrupted") => receipt["error"]
                    == "Files imported; Unity compilation/reload could not be confirmed. Applied files and backups remain.",
                _ => false,
            };
            if let Some(fields) = projected.as_object_mut() {
                fields.remove("compileErrors");
                if compiler_summary {
                    fields.insert("error".into(), Value::Null);
                }
                if receipt["stage"] == "compiling" {
                    fields.insert("stage".into(), json!("refreshing"));
                }
            }
            projected
        })
    }

    pub(super) fn presented_error_code(&self) -> Option<&str> {
        self.error_code
            .as_deref()
            .filter(|code| !matches!(*code, "unity_compile_errors" | "UNITY_COMPILE_ERRORS"))
    }

    fn job(&self) -> Value {
        let receipt = self.receipt.as_ref().unwrap_or(&Value::Null);
        let results = receipt["results"].as_array();
        let count = |state: &str| {
            results.map_or(0, |items| {
                items.iter().filter(|r| r["state"] == state).count()
            })
        };
        let failed = count("failed");
        let state = match self.phase.as_str() {
            "completed" if failed > 0 => "needsAction",
            "completed" => "completed",
            "cancelled" => "cancelled",
            "failed" | "interrupted" | "unresolved" => "needsAction",
            "preparing" if self.receipt.is_none() => "queued",
            _ => "running",
        };
        let error = self.presented_error_code().or(match self.phase.as_str() {
            "completed" if failed > 0 => Some("unity_partial_failure"),
            "failed" => Some("unity_import_failed"),
            "interrupted" => Some("unity_interrupted"),
            _ => None,
        });
        let mut job = json!({
            "id": self.request_id, "type":"unity:import", "requestId":self.request_id,
            "originDeviceId":self.origin_device_id,
            "productIds":self.product_ids, "projectName":self.project_name,
            "unityVersion":self.unity_version, "targetInstanceId":self.instance_id,
            "state":state, "stage":self.presented_phase(),
            "writesStarted":receipt["writesStarted"].as_bool().unwrap_or(false),
            "resultCounts":file_counts(results.map_or(&[][..], Vec::as_slice)),
            "errorCode":error,
            "createdAt":self.created_at,"updatedAt":self.updated_at
        });
        // Only safe counters are projected into Activity; complete results remain in details.
        if self.phase == "preparing" && !receipt.is_null() {
            for field in [
                "preparationPhase",
                "preparationWaitingForEditor",
                "preparationBytes",
                "preparationTotalBytes",
                "preparationEntries",
                "preparationTotalEntries",
            ] {
                if let Some(value) = receipt.get(field) {
                    job[field] = value.clone();
                }
            }
        }
        if terminal(&self.phase) {
            job["completedAt"] = json!(self.updated_at);
        }
        job
    }
}

#[derive(Clone)]
pub struct RequestStore {
    catalogue: Arc<Catalogue>,
}

impl RequestStore {
    pub fn new(catalogue: Arc<Catalogue>) -> Self {
        Self { catalogue }
    }

    pub fn device_id(&self) -> &str {
        self.catalogue.device_id()
    }

    pub fn is_local(&self, record: &RequestRecord) -> bool {
        self.catalogue.library_root().is_none() || record.origin_device_id == self.device_id()
    }

    /// Leave the origin's durable request/receipt intact for its later restart.
    pub fn mark_other_device(&self, record: &RequestRecord) -> Result<()> {
        if let Some(mut job) = self.catalogue.get("job", &record.request_id)? {
            job["state"] = json!("needsAction");
            job["stage"] = json!("interrupted");
            job["errorCode"] = json!("UNITY_OTHER_DEVICE");
            self.catalogue.put("job", &record.request_id, job)?;
        }
        Ok(())
    }

    pub(crate) fn begin_catalogue_work(&self) -> Result<crate::catalogue::CatalogueWorkPermit> {
        self.catalogue.begin_catalogue_work()
    }

    /// Features › Use native import for a single package, on unless turned off.
    pub fn native_single_package(&self) -> Result<bool> {
        Ok(self
            .catalogue
            .get("preferences", "app")?
            .is_none_or(|preferences| preferences["nativeSinglePackage"] != false))
    }

    pub fn create(&self, record: &RequestRecord) -> Result<()> {
        let stored = record.stored()?;
        ensure!(
            self.catalogue.put_many_if_unchanged(
                vec![
                    (KIND.into(), record.request_id.clone(), stored),
                    ("job".into(), record.request_id.clone(), record.job())
                ],
                vec![
                    (KIND.into(), record.request_id.clone(), None),
                    ("job".into(), record.request_id.clone(), None)
                ],
            )?,
            "unity_request_exists"
        );
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<RequestRecord>> {
        self.catalogue
            .get(KIND, id)?
            .map(RequestRecord::from_stored)
            .transpose()
            .context("UNITY_REQUEST_INVALID")
    }

    fn readable_records(&self) -> Result<Vec<RequestRecord>> {
        let mut records = vec![];
        for raw in self.catalogue.list(KIND)? {
            match RequestRecord::from_stored(raw.clone()) {
                Ok(record) => records.push(record),
                Err(_) => {
                    // Keep unknown/corrupt dedupe evidence intact. It must neither
                    // authorize cleanup/replay nor disable unrelated requests.
                    let Some(id) = raw["id"]
                        .as_str()
                        .filter(|id| uuid::Uuid::parse_str(id).is_ok())
                    else {
                        continue;
                    };
                    for _ in 0..3 {
                        if self.catalogue.get(KIND, id)?.as_ref() != Some(&raw) {
                            break;
                        }
                        let Some(before) = self.catalogue.get("job", id)? else {
                            break;
                        };
                        if before["type"] != "unity:import"
                            || !matches!(before["state"].as_str(), Some("queued" | "running"))
                        {
                            break;
                        }
                        let mut job = before.clone();
                        job["state"] = json!("needsAction");
                        job["errorCode"] = json!("UNITY_REQUEST_INVALID");
                        if self.catalogue.put_many_if_unchanged(
                            vec![("job".into(), id.into(), job)],
                            vec![
                                (KIND.into(), id.into(), Some(raw.clone())),
                                ("job".into(), id.into(), Some(before)),
                            ],
                        )? {
                            break;
                        }
                    }
                }
            }
        }
        Ok(records)
    }

    pub fn pending(&self) -> Result<Vec<RequestRecord>> {
        Ok(self
            .readable_records()?
            .into_iter()
            .filter(|record| !terminal(&record.phase))
            .collect())
    }

    /// Startup-only cleanup candidates; terminal receipts remain eligible.
    /// The caller owns filesystem cleanup and persists snapshot_removed after success.
    pub fn cleanup_pending(&self) -> Result<Vec<RequestRecord>> {
        Ok(self
            .readable_records()?
            .into_iter()
            .filter(|record| {
                self.is_local(record)
                    && !record.snapshot_removed
                    && record.snapshot_cleanup_proved()
            })
            .collect())
    }

    pub fn update(&self, record: &RequestRecord) -> Result<()> {
        self.project(record)
    }

    /// Receipt and any surviving Activity row commit together. An absent job stays absent.
    pub fn project(&self, record: &RequestRecord) -> Result<()> {
        let stored = record.stored()?;
        let before = self
            .catalogue
            .get(KIND, &record.request_id)?
            .context("unity_request_missing")?;
        let previous = RequestRecord::from_stored(before.clone())?;
        ensure!(previous.same_request(record), "unity_request_mismatch");
        ensure!(
            !terminal(&previous.phase) || previous.phase == record.phase,
            "unity_request_changed"
        );
        ensure!(
            (!previous.bytes_started || record.bytes_started)
                && (!previous.payload_rejected || record.payload_rejected)
                && (!previous.snapshot_ready || record.snapshot_ready)
                && (!previous.snapshot_removed || record.snapshot_removed),
            "unity_request_changed"
        );
        let mut old_job = self.catalogue.get("job", &record.request_id)?;
        for attempt in 0..2 {
            let mut records = vec![(KIND.into(), record.request_id.clone(), stored.clone())];
            if old_job.is_some() {
                records.push(("job".into(), record.request_id.clone(), record.job()));
            }
            if self.catalogue.put_many_if_unchanged(
                records,
                vec![
                    (KIND.into(), record.request_id.clone(), Some(before.clone())),
                    ("job".into(), record.request_id.clone(), old_job.clone()),
                ],
            )? {
                return Ok(());
            }
            // Retry only a concurrent Activity clear, never a different receipt update.
            ensure!(
                attempt == 0
                    && old_job.is_some()
                    && self.catalogue.get(KIND, &record.request_id)?.as_ref() == Some(&before)
                    && self.catalogue.get("job", &record.request_id)?.is_none(),
                "unity_request_changed"
            );
            old_job = None;
        }
        unreachable!()
    }
}

#[cfg(test)]
#[path = "jobs_tests.rs"]
mod tests;
