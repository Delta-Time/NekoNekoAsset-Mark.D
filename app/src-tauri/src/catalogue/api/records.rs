//! Stored records other than products, as the UI reads them. Each keeps its stored JSON:
//! the catalogue returns records as stored, and test builds check every record it writes
//! against its type here (`assert_record`). Fields the App always writes are required in
//! TypeScript even where an old or synthetic record may lack them.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use ts_rs::TS;

/// A store account. `localSessionAvailable` is added when the catalogue returns it.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Account {
    pub id: String,
    /// `BOOTH`, `Gumroad` or `Jinxxy`.
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional = false, type = "string | null")]
    pub store: Option<Option<String>>,
    /// `null` after the account was removed elsewhere or restored without one.
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional = false, type = "string | null")]
    pub name: Option<Option<String>>,
    /// `connected`, `disconnected`, `expired` and the like.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = false, type = "string")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub identity_verified: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub created_at: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub authenticated_at: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub last_sync: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub last_sync_at: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub disconnected: Option<bool>,
    /// Whether this device still holds a sign-in for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub local_session_available: Option<bool>,
    /// Sync bookkeeping and identity proof only the stores read.
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// A durable job: store sync, download, local import, storage operation, Unity import,
/// AI analysis or license read. Each kind adds its own fields.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Job {
    pub id: String,
    /// `boothSync`, `download`, `storage:import_local`, `unity:import`, `ai:classify` and
    /// the like.
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = false, type = "string")]
    pub kind: Option<String>,
    /// `queued`, `running`, `completed`, `failed`, `needsAction` or `cancelled`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = false, type = "string")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub store: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub account_id: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub account_name: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub product_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub product_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub completed_product_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub file_ids: Option<Vec<String>>,
    /// The files a download was asked for; `null` is every file.
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub selected_file_ids: Option<Option<Vec<String>>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub progress: Option<Option<JobProgress>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub error_code: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cancel_requested: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub created_at: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub updated_at: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub completed_at: Option<Option<String>>,
    /// A Unity import's or a download's current stage.
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub stage: Option<Option<String>>,
    /// What went wrong per product (AI analysis) or per file (download).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "Record<string, unknown>")]
    pub failures: Option<BTreeMap<String, Value>>,
    /// The request a store job was started with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "Record<string, unknown>")]
    pub args: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unity_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub writes_started: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub result_counts: Option<ResultCounts>,
    /// How a Unity import's transfer is going while the Editor prepares.
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub preparation_phase: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub preparation_waiting_for_editor: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub preparation_bytes: Option<Option<u64>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub preparation_total_bytes: Option<Option<u64>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub preparation_entries: Option<Option<u64>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub preparation_total_entries: Option<Option<u64>>,
    /// Checkpoints, plans and other facts only the job's own service reads.
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// A percentage, or a store sync's stages and counts.
#[derive(Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum JobProgress {
    Percent(serde_json::Number),
    Stages(SourceProgress),
}

/// Where a store sync stands.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct SourceProgress {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details_completed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requests_completed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub products: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_completed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub availability_warnings: Option<u64>,
    /// Products whose file list is still incomplete when the sync settles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incomplete_products: Option<u64>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// What a Unity import applied, skipped and failed.
#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ResultCounts {
    pub applied: u64,
    pub skipped: u64,
    pub failed: u64,
}

/// A storage root: where managed files live.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Root {
    pub id: String,
    /// Shown as an absolute path; a library-relative root is resolved first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = false, type = "string")]
    pub path: Option<String>,
    /// `online`, `offline` and the like.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = false, type = "string")]
    pub status: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub label: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub library_relative: Option<bool>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// How completely one account's store source has been read.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct SourceCoverage {
    #[ts(optional = false)]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// `purchase`, `library`, `gift`, `free` and the like.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// `running`, `complete`, `incomplete`, `failed`, `auth-required` and the like.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional = nullable)]
    pub error_code: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional = nullable)]
    pub last_successful_at: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional = nullable)]
    pub last_sync_at: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional = nullable)]
    pub completed_at: Option<Option<String>>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// A collection of products the user made.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub product_ids: Vec<String>,
}

/// A saved search. Its fields are kept as the UI sent them.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SavedSearch {
    pub id: String,
    pub name: String,
    /// The library filters at the time it was saved.
    #[serde(
        default,
        deserialize_with = "crate::ipc::present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional, type = "unknown")]
    pub query: Option<Value>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional, type = "string")]
    pub sort: Option<Value>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional, type = "string")]
    pub direction: Option<Value>,
    /// Older saved-search fields.
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// A category the user made; only explicit edits assign its `custom:` ID.
#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CustomCategory {
    pub id: String,
    pub name: String,
}

/// A tag of the original vocabulary, offered in tag pickers.
#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TagOption {
    pub id: String,
    pub label: String,
    /// `style`, `use`, `rating` and the like.
    pub kind: String,
    pub aliases: Vec<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// An avatar the compatible-avatar filter and pickers offer: a Base in this library, a
/// dictionary avatar or one named by evidence.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AvatarOption {
    pub id: String,
    pub official_name: String,
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub product_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_identity_value: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub english_name: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label_status: Option<String>,
    /// `single`, `bundle` or `fallback`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub label_qualifiers: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub shop_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_shop_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub shop_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub matching_product_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_product_ids: Option<Vec<String>>,
    /// Whether the filter lists it: the user's choice, else the automatic default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub enabled: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub explicit_enabled: Option<Option<bool>>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// Checks a record the catalogue writes against its type.
#[cfg(test)]
pub(crate) fn assert_record(kind: &str, value: &Value) {
    use crate::ipc::assert_view;
    fn none<T>(_: &T) -> Vec<String> {
        vec![]
    }
    match kind {
        "account" => assert_view::<Account>(kind, value, false, none),
        "job" => assert_view::<Job>(kind, value, false, none),
        "root" => assert_view::<Root>(kind, value, false, none),
        "source_coverage" => assert_view::<SourceCoverage>(kind, value, false, none),
        "collection" => assert_view::<Collection>(kind, value, true, none),
        "saved_search" => assert_view::<SavedSearch>(kind, value, false, none),
        "custom_category" => assert_view::<CustomCategory>(kind, value, true, none),
        _ => {}
    }
}

/// Two devices changed one field differently; the user keeps one of the values.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Conflict {
    pub kind: String,
    pub id: String,
    pub field: String,
    pub values: Vec<ConflictValue>,
    /// The value in effect now.
    pub effective_value: Value,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConflictValue {
    pub op_id: String,
    pub value: Value,
}

/// Checks the tag and avatar options the shell reads.
#[cfg(test)]
pub(crate) fn assert_options(tags: &Value, options: &[&Vec<Value>]) {
    use crate::ipc::assert_view;
    for tag in tags.as_array().into_iter().flatten() {
        assert_view::<TagOption>("tag option", tag, false, |_| vec![]);
    }
    for option in options.iter().flat_map(|options| options.iter()) {
        assert_view::<AvatarOption>("avatar option", option, false, |_| vec![]);
    }
}
