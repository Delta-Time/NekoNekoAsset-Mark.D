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

mod preferences;
mod product;
mod records;
pub use preferences::*;
pub use product::*;
pub use records::*;

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
    /// Accounts, roots, jobs, preferences and coverage.
    State,
    AvatarOptions,
    AccountCounts,
    Collections,
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
    CustomCategory(CustomCategoryRequest),
    /// Changes the listed preferences; the rest keep their values.
    Preferences {
        #[ts(as = "Preferences")]
        patch: Map<String, Value>,
    },
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
                | Self::ProductDetails { .. }
                | Self::AccountCounts
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
    /// Leaves out free downloads that hold only a listing's terms of use.
    pub hide_terms_only: Option<bool>,
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

/// What removing an account removed: the account, the products only it held, and the
/// shared products it left to their other accounts.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AccountRemoved {
    pub account_removed: usize,
    pub product_removed: usize,
    pub retained_shared: usize,
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

/// Sidebar counts under the saved list preferences. A library not set up yet has only the
/// first three.
#[derive(Serialize, Deserialize, TS, Clone, Default)]
#[ts(export)]
pub struct ListCounts {
    pub all: i64,
    pub downloaded: i64,
    pub attention: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hidden: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub categories: Option<BTreeMap<String, i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub statuses: Option<BTreeMap<String, i64>>,
}

/// The shell's state between list queries: what `state` returns, and part of `bootstrap`.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StateSnapshot {
    #[ts(as = "Vec<Account>")]
    pub accounts: Vec<Value>,
    #[ts(as = "Vec<Root>")]
    pub roots: Vec<Value>,
    #[ts(as = "Vec<Job>")]
    pub jobs: Vec<Value>,
    /// The stored preferences; `null` before any were saved.
    #[ts(as = "Option<Preferences>")]
    pub preferences: Option<Value>,
    #[ts(as = "Vec<SourceCoverage>")]
    pub source_coverage: Vec<Value>,
}

/// Everything the shell loads when a library opens.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Bootstrap {
    pub version: u32,
    /// Products in the library list before any filter.
    pub total: i64,
    pub counts: ListCounts,
    /// Every visible product, unless `includeProducts` was false.
    #[ts(as = "Vec<Product>")]
    pub products: Vec<Value>,
    #[serde(flatten)]
    pub state: StateSnapshot,
    #[ts(as = "Vec<Collection>")]
    pub collections: Vec<Value>,
    #[ts(as = "Vec<SavedSearch>")]
    pub saved_searches: Vec<Value>,
    #[ts(as = "Vec<CustomCategory>")]
    pub custom_categories: Vec<Value>,
    #[ts(as = "Vec<TagOption>")]
    pub tag_options: Value,
    #[ts(as = "Vec<AvatarOption>")]
    pub avatar_options: Vec<Value>,
    /// Bases this library owns, offered first in avatar pickers.
    #[ts(as = "Vec<AvatarOption>")]
    pub owned_base_options: Vec<Value>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AvatarOptions {
    #[ts(as = "Vec<AvatarOption>")]
    pub avatar_options: Vec<Value>,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct Collections {
    #[ts(as = "Vec<Collection>")]
    pub collections: Vec<Value>,
}

/// Products per account, ignoring filters; `unassigned` counts store products of no
/// remaining account.
#[derive(Serialize, TS)]
#[ts(export)]
pub struct AccountCounts {
    pub accounts: BTreeMap<String, i64>,
    pub unassigned: i64,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ActivityCleared {
    pub cleared: usize,
    pub remaining_active: usize,
}

/// Proven payments of the selected products, per currency; never converted.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SpendingSummary {
    pub totals: Vec<SpendingTotal>,
    pub products: Vec<ProductSpending>,
    /// Products without any proven payment.
    pub skipped_count: usize,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProductSpending {
    pub product_id: String,
    pub title: String,
    pub totals: Vec<SpendingTotal>,
    pub unknown_payment_count: usize,
}

/// An exact amount: decimal text and integer minor units.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SpendingTotal {
    pub currency: String,
    pub amount: String,
    pub minor_units: String,
    pub fraction_digits: u32,
}

/// An overlay edit or its undo.
#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EditResult {
    /// False when undo met a later conflicting edit.
    pub ok: bool,
    pub changed: usize,
    /// Undoes this edit; missing when nothing changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub token: Option<String>,
    /// The same as `token`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub undo_token: Option<String>,
    /// Products undo left as they were.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub conflicts: Option<Vec<String>>,
    /// Products whose store link changed; `catalogue_action` reads their prices and drops
    /// this.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(skip)]
    pub changed_links: Option<Vec<String>>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CustomCategories {
    pub ok: bool,
    #[ts(as = "Vec<CustomCategory>")]
    pub custom_categories: Vec<Value>,
}

/// What a collection change returns besides the changed record: every collection.
#[cfg(test)]
#[derive(Serialize, TS)]
#[ts(export)]
struct CollectionChange {
    collections: Vec<Collection>,
}

/// The answer to each catalogue action, for the typed `invokeAction` in the UI.
#[cfg(test)]
#[derive(Serialize, TS)]
#[ts(export, rename = "CatalogueResponses")]
struct Responses {
    bootstrap: Bootstrap,
    state: StateSnapshot,
    avatar_options: AvatarOptions,
    collections: Collections,
    collection: CollectionChange,
    account_counts: AccountCounts,
    clear_activity: ActivityCleared,
    /// The support report, as text to copy.
    job_diagnostics: String,
    query: QueryResult,
    product_details: Product,
    spending_summary: SpendingSummary,
    overlay: EditResult,
    undo: EditResult,
    custom_category: CustomCategories,
    preferences: preferences::PreferencesSaved,
}
