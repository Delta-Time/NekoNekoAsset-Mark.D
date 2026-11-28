//! The product record: what the projection stores and the library and inspector read.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

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
    /// Listed files whose free offer had ended when a download asked for them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ended_files: Option<usize>,
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
    /// Whether the BOOTH listing sells a variation for a price, so its free files can be only
    /// its terms of use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub listing_sells_paid: Option<bool>,
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
    pub archive_password_required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub archive_password: Option<String>,
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
    /// Its last download found the store's free offer ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub offer_ended: Option<bool>,
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
pub(crate) fn assert_product(product: &Value) {
    use crate::ipc::assert_view;
    let open = |p: &Product| -> Vec<String> { p.rest.keys().cloned().collect() };
    assert_view::<Product>("product", product, false, open);
    let mut scoped = product.clone();
    super::super::acquisition::apply_scope(&mut scoped, Some(&vec![]));
    assert_view::<Product>("scoped product", &scoped, false, open);
    let summary = super::super::projection::summary(product);
    assert_view::<Product>("product summary", &summary, true, open);
    let mut list = summary;
    list["unavailable"] = Value::Bool(false);
    super::super::acquisition::apply_scope(&mut list, Some(&vec![]));
    super::super::query::compact_for_list(&mut list);
    assert_view::<Product>("list summary", &list, true, open);
}
