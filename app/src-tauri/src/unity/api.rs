//! What the UI sends through `unity_action` and what it reads back. `pnpm bindings`
//! writes these types to `app/src/bindings`.
use super::wire::{FileResult, LiveEditor};
use crate::storage::UnityLeaf;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// One Unity action and its arguments.
#[derive(Deserialize, TS)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[ts(export)]
pub enum UnityRequest {
    /// Live verified Editors.
    ListEditors,
    /// The managed files of these products that Unity can import.
    Inventory {
        product_ids: Vec<String>,
    },
    /// Imports the selected files into one Editor. One package opens in Unity's own
    /// importer and leaves no job; anything else becomes a Bridge request.
    Submit {
        #[serde(default)]
        #[ts(optional)]
        locale: Option<String>,
        product_ids: Vec<String>,
        leaf_refs: Vec<String>,
        instance_id: String,
        session_id: String,
    },
    Details {
        job_id: String,
    },
    /// Asks the Editor where a request stands.
    Status {
        job_id: String,
    },
    Cancel {
        job_id: String,
    },
    /// What a finished request did not import, to select again.
    RetryInventory {
        job_id: String,
    },
}

impl UnityRequest {
    pub fn parse(action: &str, args: Value) -> anyhow::Result<Self> {
        crate::ipc::parse(
            action,
            args,
            "UNITY_INVALID_REQUEST",
            "UNITY_INVALID_REQUEST",
        )
    }
}

#[derive(Serialize, TS)]
#[ts(export, rename = "UnityEditors")]
pub struct Editors {
    pub editors: Vec<LiveEditor>,
}

#[derive(Serialize, TS)]
#[ts(export, rename = "UnityInventory")]
pub struct Inventory {
    pub files: Vec<UnityLeaf>,
}

/// A Bridge request's job, or `native` when Unity's own importer took the package.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "UnitySubmitted")]
pub struct Submitted {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub native: Option<bool>,
}

/// A Bridge request as Activity shows it.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UnityDetails {
    pub job_id: String,
    pub project_name: String,
    pub project_root: String,
    pub unity_version: String,
    pub stage: String,
    pub error_code: Option<String>,
    #[ts(as = "Option<UnityReceipt>")]
    pub receipt: Option<Value>,
}

/// The Editor's receipt as the App presents it: compiler output is never shown as
/// importer work.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UnityReceipt {
    pub request_id: String,
    pub fingerprint: String,
    pub instance_id: String,
    pub session_id: String,
    pub stage: String,
    pub staged: bool,
    pub writes_started: bool,
    pub updated_utc: String,
    pub preparation_phase: Option<String>,
    pub preparation_waiting_for_editor: bool,
    pub preparation_bytes: Option<u64>,
    pub preparation_total_bytes: Option<u64>,
    pub preparation_entries: Option<u64>,
    pub preparation_total_entries: Option<u64>,
    pub error: Option<String>,
    pub results: Vec<FileResult>,
}

/// The products and files to select again.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "UnityRetrySelection")]
pub struct RetrySelection {
    pub product_ids: Vec<String>,
    pub leaf_refs: Vec<String>,
}

/// The answer to each Unity action, for the typed `invokeAction` in the UI.
#[cfg(test)]
#[derive(Serialize, TS)]
#[ts(export, rename = "UnityResponses")]
struct Responses {
    list_editors: Editors,
    inventory: Inventory,
    submit: Submitted,
    details: UnityDetails,
    status: UnityDetails,
    cancel: UnityDetails,
    retry_inventory: RetrySelection,
}
