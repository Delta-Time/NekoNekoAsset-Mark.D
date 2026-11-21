//! What the UI sends through `catalogue_action` and what it reads back. `pnpm bindings`
//! writes these types to `app/src/bindings`, so renaming a field here fails `tsc` where
//! the UI still reads the old name.
//!
//! Stored records keep their exact JSON: the catalogue returns product bodies as stored,
//! and [`Product`] declares what they hold. Test builds check every body the projection
//! writes against it (`assert_product`).
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use ts_rs::TS;

/// One catalogue action and its arguments.
#[derive(Deserialize, TS)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum CatalogueRequest {
    /// Shell state and option registries; products only when `includeProducts` is not false.
    Bootstrap {
        #[serde(default)]
        #[ts(optional)]
        include_products: Option<bool>,
    },
    /// Accounts, roots, jobs, preferences, coverage and conflicts.
    State,
    AvatarOptions,
    AccountCounts,
    Collections,
    Conflicts,
    Manifest,
    /// Removes finished jobs from Activity.
    ClearActivity,
    /// The support report of one job.
    JobDiagnostics {
        job_id: String,
    },
    /// One product with its files, under an optional account scope.
    ProductDetails {
        id: String,
        #[serde(default)]
        #[ts(optional)]
        account_ids: Option<Vec<String>>,
    },
    Query(QueryRequest),
    SpendingSummary(SpendingRequest),
    Overlay(OverlayRequest),
    DismissAttention {
        product_ids: Vec<String>,
    },
    Undo {
        #[serde(alias = "undoToken")]
        token: String,
    },
    Collection(MembershipRequest),
    SaveSearch(MembershipRequest),
    /// The same as `save_search`.
    SavedSearch(MembershipRequest),
    CustomCategory(CustomCategoryRequest),
    /// Changes the listed preferences; the rest keep their values.
    Preferences {
        #[ts(type = "Record<string, unknown>")]
        patch: Map<String, Value>,
    },
    ResolveConflict(ConflictChoice),
    /// Exchanges catalogue changes with a shared folder.
    Exchange {
        folder: String,
    },
    ExportCsv(CsvExport),
    RestoreManifest(ManifestRestore),
}

impl CatalogueRequest {
    pub fn parse(action: &str, args: Value) -> anyhow::Result<Self> {
        crate::ipc::parse(
            action,
            args,
            "catalogue.unknown_action",
            "catalogue.invalid_request",
        )
    }
    /// Reads only; no catalogue work permit and no change event.
    pub fn reads_only(&self) -> bool {
        matches!(
            self,
            Self::Bootstrap { .. }
                | Self::AvatarOptions
                | Self::JobDiagnostics { .. }
                | Self::Query(_)
                | Self::Conflicts
                | Self::ProductDetails { .. }
                | Self::AccountCounts
                | Self::Manifest
                | Self::ExportCsv(_)
                | Self::Collections
                | Self::State
                | Self::SpendingSummary(_)
        )
    }
}

/// The library list: filters, sort and paging. Absent fields keep their defaults.
#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct QueryRequest {
    pub search: Option<String>,
    /// Older name of `search`.
    #[ts(skip)]
    pub query: Option<String>,
    pub category: Option<String>,
    pub avatar: Option<String>,
    pub status: Option<String>,
    /// `show`, `blur`, `hide` or `only`.
    pub sensitive: Option<String>,
    #[ts(skip)]
    pub sensitive_mode: Option<String>,
    #[ts(skip)]
    pub r18: Option<String>,
    /// Products of these accounts; `null` is every account.
    #[ts(optional = nullable)]
    pub account_ids: Option<Vec<String>>,
    /// Local imports beside the selected accounts; defaults to true without an account scope.
    pub include_offline: Option<bool>,
    /// `all`, `downloaded`, `attention` or `new`.
    pub quick: Option<String>,
    /// `paid`, `free` and `gift`; `null` is every kind.
    #[ts(optional = nullable)]
    pub acquisition: Option<Vec<String>>,
    /// License cells to match; read by the license filter, `null` turns it off.
    #[ts(optional = nullable)]
    pub license: Option<BTreeMap<String, String>>,
    /// Set from `license` by the license filter, never by the UI.
    #[serde(skip)]
    #[ts(skip)]
    pub license_ids: Option<Vec<String>>,
    pub hidden_only: Option<bool>,
    pub hide_unavailable: Option<bool>,
    pub hide_unclassified: Option<bool>,
    pub collection_id: Option<String>,
    /// Older name of `collectionId`.
    #[ts(skip)]
    pub collection: Option<String>,
    /// `acquired`, `title`, `price` or `status`.
    pub sort: Option<String>,
    /// `asc` or `desc`.
    pub direction: Option<String>,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    /// False returns list summaries without files.
    pub include_files: Option<bool>,
    /// `list` compacts summaries further.
    pub presentation: Option<String>,
    /// Every matched summary in one page, for list summaries.
    pub all_summaries: Option<bool>,
    /// Fingerprints of summaries the UI holds; only new or changed ones come back.
    pub known_summaries: Option<BTreeMap<String, String>>,
}

#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SpendingRequest {
    pub product_ids: Vec<String>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub account_ids: Option<Vec<String>>,
}

/// Overlay edits, either one change per product or one patch for several products.
#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct OverlayRequest {
    pub changes: Option<Vec<OverlayChange>>,
    pub ids: Option<Vec<String>>,
    #[ts(optional, type = "Record<string, unknown>")]
    pub patch: Option<Map<String, Value>>,
    #[serde(default, deserialize_with = "crate::ipc::present")]
    #[ts(optional, type = "Array<string>")]
    pub clear_fields: Option<Value>,
    #[serde(default, deserialize_with = "crate::ipc::present")]
    #[ts(as = "Option<AvatarDelta>")]
    pub owned_avatars_delta: Option<Value>,
}

#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct OverlayChange {
    pub id: String,
    #[ts(optional, type = "Record<string, unknown>")]
    pub patch: Option<Map<String, Value>>,
    #[serde(default, deserialize_with = "crate::ipc::present")]
    #[ts(optional, type = "Array<string>")]
    pub clear_fields: Option<Value>,
    #[serde(default, deserialize_with = "crate::ipc::present")]
    #[ts(as = "Option<AvatarDelta>")]
    pub owned_avatars_delta: Option<Value>,
}

/// Avatar choices a batch edit adds to and removes from each product. The overlay
/// editor validates the raw object, so this type only describes it.
#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AvatarDelta {
    pub add: Vec<String>,
    pub remove: Vec<String>,
}

impl OverlayRequest {
    /// The changes as the overlay editor reads them, one per product.
    pub(super) fn changes(self) -> Vec<Value> {
        if let Some(changes) = self.changes {
            return changes.into_iter().map(OverlayChange::into_value).collect();
        }
        let ids: std::collections::BTreeSet<_> = self.ids.unwrap_or_default().into_iter().collect();
        ids.into_iter()
            .map(|id| {
                OverlayChange {
                    id,
                    patch: Some(self.patch.clone().unwrap_or_default()),
                    clear_fields: Some(self.clear_fields.clone().unwrap_or(Value::Null)),
                    owned_avatars_delta: self.owned_avatars_delta.clone(),
                }
                .into_value()
            })
            .collect()
    }
}

impl OverlayChange {
    fn into_value(self) -> Value {
        let mut change = Map::new();
        change.insert("id".into(), Value::String(self.id));
        if let Some(patch) = self.patch {
            change.insert("patch".into(), Value::Object(patch));
        }
        if let Some(fields) = self.clear_fields {
            change.insert("clearFields".into(), fields);
        }
        if let Some(delta) = self.owned_avatars_delta {
            change.insert("ownedAvatarsDelta".into(), delta);
        }
        Value::Object(change)
    }
}

/// A collection or saved-search change. `op` defaults to `create`; `add_members` and
/// `remove_members` take `ids`. The record's fields come from `value`, `patch` or the
/// request itself, in that order, and are copied as sent.
#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct MembershipRequest {
    #[serde(alias = "operation")]
    pub op: Option<String>,
    pub id: Option<String>,
    pub ids: Option<Vec<String>>,
    pub collection_ids: Option<Vec<String>>,
    pub names: Option<Vec<String>>,
    #[serde(default, deserialize_with = "crate::ipc::present")]
    #[ts(optional, type = "Record<string, unknown>")]
    pub value: Option<Value>,
    #[serde(default, deserialize_with = "crate::ipc::present")]
    #[ts(optional, type = "Record<string, unknown>")]
    pub patch: Option<Value>,
    #[serde(default, deserialize_with = "crate::ipc::present")]
    #[ts(optional, type = "string")]
    pub name: Option<Value>,
    #[serde(default, deserialize_with = "crate::ipc::present")]
    #[ts(optional, type = "Array<string>")]
    pub product_ids: Option<Value>,
    /// A saved search's filters.
    #[serde(default, deserialize_with = "crate::ipc::present")]
    #[ts(optional, type = "unknown")]
    pub query: Option<Value>,
    #[serde(default, deserialize_with = "crate::ipc::present")]
    #[ts(optional, type = "string")]
    pub sort: Option<Value>,
    #[serde(default, deserialize_with = "crate::ipc::present")]
    #[ts(optional, type = "string")]
    pub direction: Option<Value>,
    /// A saved search's other fields.
    #[serde(flatten)]
    #[ts(skip)]
    pub fields: Map<String, Value>,
}

impl MembershipRequest {
    /// The fields a record copies, as one object: `value`, else `patch`, else the request.
    pub(super) fn record_fields(&self) -> Value {
        if let Some(value) = self.value.clone().or_else(|| self.patch.clone()) {
            return value;
        }
        let mut fields = self.fields.clone();
        for (key, value) in [
            ("name", &self.name),
            ("productIds", &self.product_ids),
            ("query", &self.query),
            ("sort", &self.sort),
            ("direction", &self.direction),
        ] {
            if let Some(value) = value {
                fields.insert(key.into(), value.clone());
            }
        }
        Value::Object(fields)
    }
}

#[derive(Deserialize, TS)]
#[ts(export, optional_fields)]
pub struct CustomCategoryRequest {
    /// `create`, `update` or `delete`.
    pub op: String,
    pub id: Option<String>,
    pub name: Option<String>,
}

/// Settles one exchange conflict: a retained `opId`, or an explicit `value`.
#[derive(Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, optional_fields)]
pub struct ConflictChoice {
    pub kind: Option<String>,
    pub id: Option<String>,
    pub field: Option<String>,
    pub op_id: Option<String>,
    #[serde(default, deserialize_with = "crate::ipc::present")]
    #[ts(optional)]
    pub value: Option<Value>,
}

#[derive(Deserialize, TS)]
#[ts(export, optional_fields)]
pub struct CsvExport {
    /// Only these products; every product when absent.
    #[serde(default, deserialize_with = "crate::ipc::nullable")]
    #[ts(optional, type = "Array<string>")]
    pub ids: Option<Option<Vec<String>>>,
    /// Writes the file here instead of returning its text.
    pub path: Option<String>,
}

/// A backup manifest, either under `manifest` or as the request itself.
#[derive(Deserialize, TS)]
#[ts(export)]
pub struct ManifestRestore {
    #[serde(default)]
    #[ts(optional)]
    pub manifest: Option<Value>,
    #[serde(flatten)]
    #[ts(skip)]
    pub fields: Map<String, Value>,
}

impl ManifestRestore {
    pub(super) fn into_manifest(self) -> Value {
        self.manifest.unwrap_or(Value::Object(self.fields))
    }
}

/// One page of the library list.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct QueryResult {
    /// Stored summaries, or full products when files were asked for.
    #[ts(as = "Vec<Product>")]
    pub products: Vec<Value>,
    /// Products in the library list before any filter.
    pub total: i64,
    /// Every match in order, beyond this page.
    pub matched_ids: Vec<String>,
    pub matched: usize,
    pub counts: ListCounts,
    /// Whether every match is downloaded.
    pub all_downloaded: bool,
    /// The fingerprint of each returned summary, when `knownSummaries` was sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fingerprints: Option<BTreeMap<String, String>>,
}

/// Sidebar counts under the saved list preferences.
#[derive(Serialize, Deserialize, TS, Clone, Default)]
#[ts(export)]
pub struct ListCounts {
    pub all: i64,
    pub downloaded: i64,
    pub attention: i64,
    pub hidden: i64,
    pub categories: BTreeMap<String, i64>,
    pub statuses: BTreeMap<String, i64>,
}

/// A product as the library list and the inspector read it. The projection stores the
/// whole product; `query` returns summaries (no files, `filesLoaded: false`), and the
/// list presentation drops a few more fields. A product also carries its source
/// record's own fields, which only the catalogue reads.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Product {
    pub id: String,
    pub title: String,
    pub source_title: String,
    pub title_override: bool,
    pub shop: String,
    pub category: String,
    pub category_override: bool,
    pub ai_category_applied: bool,
    /// Missing from list summaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub category_inference: Option<CategoryInference>,
    pub tags: Vec<String>,
    pub source_tags: Option<Vec<String>>,
    /// Tags the user removed; source sync never brings them back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rejected_tags: Option<Vec<String>>,
    pub tag_suggestions: Vec<TagSuggestion>,
    pub avatar: String,
    /// Avatars the listing names; not proof of owned compatibility.
    pub listing_avatars: Vec<String>,
    pub owned_avatars: Vec<String>,
    pub owned_avatar_evidence: Vec<AvatarEvidence>,
    /// `notDownloaded`, `partial`, `processing`, `ready` or `attention`.
    pub status: String,
    /// Files with complete bytes.
    pub downloaded: usize,
    pub total: usize,
    pub fully_downloaded: bool,
    pub manifest_complete: bool,
    pub known_file_count: usize,
    /// Files outside the store's current list that are still kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kept_files: Option<usize>,
    pub files: Vec<AssetFile>,
    /// False on summaries, whose `files` is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub files_loaded: Option<bool>,
    /// The current price: an owned quote, or a store's listing price.
    pub price: Option<f64>,
    pub price_currency: Option<String>,
    pub current_price_text: Option<String>,
    /// BOOTH's labelled starting price; never an owned quote.
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub listing_starting_price: Option<Option<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub owned_current_prices: Option<Vec<OwnedCurrentPrice>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub owned_paid_prices: Option<Vec<OwnedPaidPrice>>,
    /// A proven, unique item-level payment.
    pub paid_price: Option<f64>,
    pub paid_currency: Option<String>,
    /// The earliest acquisition date as the store showed it.
    pub acquired: String,
    pub acquired_sort_key: Option<String>,
    pub note: String,
    pub sensitive: bool,
    pub hidden: bool,
    /// Accounts holding this product.
    pub account_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub newly_discovered: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub preview_url: Option<Option<String>>,
    pub source_preview_url: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub thumbnail_path: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub thumbnail_hidden: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_thumbnail_url: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub source_url: Option<Option<String>>,
    pub source_url_available: bool,
    pub source_url_override: bool,
    /// The store's own link; missing from list summaries.
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub source_origin_url: Option<Option<String>>,
    /// `available`, `unavailable` or `unknown`; missing from list summaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub public_page_availability: Option<String>,
    /// List summaries only: the store no longer offers this product.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unavailable: Option<bool>,
    /// `booth`, `gumroad`, `jinxxy` or `local`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub store: Option<String>,
    /// Stores and accounts this product came from; missing from list summaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sources: Option<Vec<SourceRow>>,
    /// Acquisitions, each with its date and proven payment; missing from list summaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provenance: Option<Vec<AcquisitionProvenance>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub source_shop: Option<Option<SourceShopValue>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub source_category: Option<Option<SourceCategoryValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_kind: Option<Value>,
    /// A product imported from local files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub custom: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub import_source: Option<ImportSource>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub thumbnail_source_path: Option<Option<String>>,
    /// Whether the AI analysis still matches this product; full products only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ai_current: Option<bool>,
    /// Store pages the AI found for a local product; full products only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ai_source_url_candidates: Option<Vec<SourceUrlCandidate>>,
    /// The source record's own fields and the catalogue's internal ones.
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CategoryInference {
    pub category: String,
    /// `sourceCategory`, `tags`, `fallback`, `scopeGate` and the like.
    pub basis: String,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TagSuggestion {
    pub label: String,
    /// The rest are missing from list summaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub basis: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub inferred: Option<bool>,
}

/// Why a product counts toward an avatar.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AvatarEvidence {
    pub avatar: String,
    /// `accountVariant`, `accountFilename`, `user`, `sourceTag` or `userTag`.
    pub source: String,
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
    pub avatar_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tag: Option<String>,
    /// False for tag evidence, which proves no owned file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub entitlement_established: Option<bool>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AssetFile {
    pub id: String,
    pub name: String,
    /// The extension in capitals.
    #[serde(rename = "type")]
    pub kind: String,
    /// Bytes as text, empty when unknown.
    pub size: String,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub size_bytes: Option<Option<u64>>,
    /// `unknown`, `notDownloaded`, `queued`, `downloading`, `downloaded`, `extracting`,
    /// `ready`, `failed`, `extractionFailed`, `missing` and the like.
    pub status: String,
    pub revision: Option<String>,
    pub provenance: Vec<FileProvenance>,
    pub original_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub root_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub relative_path: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub verified_hash: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub error_code: Option<Option<String>>,
    /// A store product's file outside the store's current list: a download it stopped
    /// offering, or one the user added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kept: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub user_added: Option<bool>,
    /// Extraction output and other artifact facts only the catalogue reads.
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// Where a file came from: the account and variant that grant it.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileProvenance {
    pub account_id: Option<String>,
    pub source_file_id: Option<String>,
    pub variant_id: Option<String>,
    pub variant_name: Option<String>,
}

/// One acquisition as recorded, with its date and payment facts.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AcquisitionProvenance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub row_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub account_id: Option<String>,
    /// `purchase`, `gift`, `free`, `library` and the like.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub acquired_display: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub acquired_sort_key: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub acquired_date_text: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub acquisition_kind: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub acquisition_key: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub variant_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub variant_ids: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub variant_name: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub paid_price: Option<Option<f64>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub paid_currency: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub paid_price_basis: Option<Option<String>>,
    /// The store record's other fields.
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SourceRow {
    /// `BOOTH`, `Gumroad`, `Jinxxy` or `Local Files`.
    pub provider: String,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub label: String,
}

/// A current price of an owned variant.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OwnedCurrentPrice {
    pub variant_id: String,
    pub name: String,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub account_ids: Vec<String>,
}

/// A proven payment for an owned variant, or an unpriced owned variant.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OwnedPaidPrice {
    pub variant_id: String,
    pub name: String,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub account_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub acquisition_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub quantity: Option<u64>,
}

/// A store's own category: an object, or a bare name in some records.
#[derive(Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum SourceCategoryValue {
    Category(SourceCategory),
    Name(String),
}

/// BOOTH's category and its parent; other stores keep their own fields.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SourceCategory {
    #[serde(
        default,
        deserialize_with = "crate::ipc::present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional, type = "string | number | null")]
    pub id: Option<Value>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub name: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional, type = "string | number | null")]
    pub parent_id: Option<Value>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub parent_name: Option<Option<String>>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// A store's shop: an object, or a bare name in some records.
#[derive(Serialize, Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum SourceShopValue {
    Shop(SourceShop),
    Name(String),
}

/// The shop of a BOOTH product.
#[derive(Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SourceShop {
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub name: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::ipc::nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub url: Option<Option<String>>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// Where a local product's files were imported from.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportSource {
    /// `local` or `remote`.
    pub kind: String,
    pub location: String,
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub product_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub thumbnail_name: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub rest: Map<String, Value>,
}

/// A store page the AI found for a product.
#[derive(Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SourceUrlCandidate {
    pub url: String,
    pub basis: String,
    pub confidence: f64,
    pub evidence_refs: Vec<String>,
    /// `supported` when a source record names the same page, else `uncertain`.
    pub status: String,
}

/// Checks the forms of a product the projection hands out: the stored product, scoped
/// to an account selection, its summary and its list summary.
#[cfg(test)]
pub(super) fn assert_product(product: &Value) {
    use crate::ipc::assert_view;
    let open = |p: &Product| -> Vec<String> { p.rest.keys().cloned().collect() };
    assert_view::<Product>("product", product, false, open);
    let mut scoped = product.clone();
    super::acquisition::apply_scope(&mut scoped, Some(&vec![]));
    assert_view::<Product>("scoped product", &scoped, false, open);
    let summary = super::projection::summary(product);
    assert_view::<Product>("product summary", &summary, true, open);
    let mut list = summary;
    list["unavailable"] = Value::Bool(false);
    super::acquisition::apply_scope(&mut list, Some(&vec![]));
    super::query::compact_for_list(&mut list);
    assert_view::<Product>("list summary", &list, true, open);
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

/// The answer to each catalogue action, for the typed `invokeAction` in the UI.
#[cfg(test)]
#[derive(Serialize, TS)]
#[ts(export, rename = "CatalogueResponses")]
struct Responses {
    query: QueryResult,
    product_details: Product,
    conflicts: Vec<Conflict>,
}
