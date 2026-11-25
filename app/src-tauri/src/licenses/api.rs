//! What the UI sends through `licenses_action` and what it reads back. `pnpm bindings`
//! writes these types to `app/src/bindings`.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use ts_rs::TS;

/// One license-filter action and its arguments.
#[derive(Deserialize, TS)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum LicensesRequest {
    /// Whether the license database exists, its counts and the running read.
    Status,
    /// One product's read and edited license tables.
    Get {
        #[serde(default)]
        product_id: String,
    },
    /// Saves the user's table for a product; `null` removes it.
    Set {
        #[serde(default)]
        product_id: String,
        #[serde(default)]
        #[ts(optional, type = "Record<string, string> | null")]
        cells: Option<Value>,
    },
    /// Reads the licenses of these products, or of every product not read yet.
    Sync {
        #[serde(default)]
        #[ts(optional)]
        product_ids: Option<Vec<String>>,
    },
    /// Reads again what a finished job missed.
    Retry {
        #[serde(default)]
        job_id: String,
    },
    /// Stops that job, or any running read.
    Cancel {
        #[serde(default)]
        #[ts(optional)]
        job_id: Option<String>,
    },
    /// Deletes the license database.
    Delete,
}

impl LicensesRequest {
    pub fn parse(action: &str, args: Value) -> anyhow::Result<Self> {
        crate::ipc::parse(action, args, "UNKNOWN_ACTION", "licenses.invalid_request")
    }
}

/// A license table: letter to choice.
pub type Cells = BTreeMap<String, String>;

#[derive(Serialize, TS)]
#[ts(export)]
pub struct LicenseStatus {
    pub exists: bool,
    pub counts: Option<LicenseCounts>,
    /// The running read's job.
    pub running: Option<String>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct LicenseCounts {
    /// Products with a license read.
    pub found: i64,
    /// Products with a table the user edited.
    pub edited: i64,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct LicenseDetail {
    pub synced: Option<SyncedLicense>,
    pub edited: Option<Cells>,
}

/// A license read from a product's files.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncedLicense {
    /// `vn3`, `uv` or `cc0`.
    pub kind: String,
    pub version: Option<String>,
    pub source: Option<String>,
    pub checked_at: String,
    pub cells: Cells,
}

/// A started read; `null` when there was nothing to read.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LicenseJob {
    pub job_id: Option<String>,
}

/// An answer without content.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct Done {}

/// The answer to each license action, for the typed `invokeAction` in the UI.
#[cfg(test)]
#[derive(Serialize, TS)]
#[ts(export, rename = "LicensesResponses")]
struct Responses {
    status: LicenseStatus,
    get: LicenseDetail,
    set: Done,
    sync: LicenseJob,
    retry: LicenseJob,
    cancel: Done,
    delete: Done,
}
