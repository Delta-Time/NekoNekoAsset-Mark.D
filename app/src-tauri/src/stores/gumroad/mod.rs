//! Original parsers for the source-derived Gumroad buyer/public payloads.
//! Deployed-shape verification and collector wiring are separate from parsing.
//! Bearer content/resolver routes are transient capabilities, never catalogue fields.
pub(super) mod service;
use anyhow::{bail, ensure, Result};
use chrono::{DateTime, Utc};
use scraper::{Html, Selector};
use serde_json::Value;
use std::collections::BTreeSet;
use url::Url;

const LIMIT: usize = 8 * 1024 * 1024;

/// No Debug or Serialize: the path/query can grant access to private purchased content.
#[derive(Clone)]
pub struct TransientRoute(Url);
impl TransientRoute {
    pub fn url(&self) -> &Url {
        &self.0
    }
}

fn trusted_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && url
            .host_str()
            .is_some_and(|host| host == "gumroad.com" || host.ends_with(".gumroad.com"))
}

fn transient_route(value: &Value, base: &Url) -> Result<Option<TransientRoute>> {
    match value {
        Value::Null => Ok(None),
        Value::String(value) => {
            if value.is_empty() {
                return Ok(None);
            }
            let mut url = base
                .join(value)
                .map_err(|_| anyhow::anyhow!("GUMROAD_ROUTE_UNSUPPORTED"))?;
            ensure!(trusted_url(&url), "GUMROAD_ROUTE_UNSUPPORTED");
            // A fragment never reaches the server.
            url.set_fragment(None);
            Ok(Some(TransientRoute(url)))
        }
        _ => bail!("GUMROAD_ROUTE_UNSUPPORTED"),
    }
}

fn file_route(value: &Value, file_id: &str) -> Result<Option<TransientRoute>> {
    let root = Url::parse("https://gumroad.com/").unwrap();
    let route = transient_route(value, &root)?;
    if let Some(route) = &route {
        let url = route.url();
        let path: Vec<_> = url.path().split('/').collect();
        ensure!(
            path.len() == 4 && path[1] == "r" && !path[2].is_empty() && path[3] == "product_files",
            "GUMROAD_FILE_BINDING_UNPROVEN"
        );
        // The one requested file ID binds the route; other parameters do not change it.
        let files: Vec<_> = url
            .query_pairs()
            .filter(|(key, _)| key == "product_file_ids[]")
            .collect();
        ensure!(
            files.len() == 1 && files[0].1 == file_id,
            "GUMROAD_FILE_BINDING_UNPROVEN"
        );
    }
    Ok(route)
}

fn identifier(value: &Value) -> Result<String> {
    let value = value
        .as_str()
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 1024
                && value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'=' | b'+' | b'/')
                })
        })
        .ok_or_else(|| anyhow::anyhow!("GUMROAD_IDENTITY_UNPROVEN"))?;
    Ok(value.to_owned())
}

fn optional_text(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|value| !value.trim().is_empty() && value.len() <= 4096)
        .map(str::to_owned)
}

fn pick(value: &Value, keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .filter_map(|key| {
                value
                    .get(*key)
                    .map(|value| ((*key).to_owned(), value.clone()))
            })
            .collect(),
    )
}

fn needed_items(value: &Value) -> Value {
    let Some(items) = value.as_array() else {
        return Value::Null;
    };
    Value::Array(
        items
            .iter()
            .map(|item| {
                let mut needed = pick(
                    item,
                    &[
                        "type",
                        "id",
                        "name",
                        "file_name",
                        "file_size",
                        "pdf_stamp_enabled",
                        "download_url",
                        "stream_url",
                        "read_url",
                        "external_link_url",
                    ],
                );
                if item.get("children").is_some() {
                    needed["children"] = needed_items(&item["children"]);
                }
                if let Some(subtitles) = item["subtitle_files"].as_array() {
                    needed["subtitle_files"] = Value::Array(
                        subtitles
                            .iter()
                            .map(|item| pick(item, &["download_url"]))
                            .collect(),
                    );
                }
                needed
            })
            .collect(),
    )
}

/// Reduces either SSR or current plain history.state.page to needed observations.
/// The result still contains transient bearer routes; it is not a portable record or diagnostic.
pub fn necessary_page(page: &Value) -> Result<Value> {
    let component = page["component"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("GUMROAD_SOURCE_UNSUPPORTED"))?;
    // The echoed page URL is not needed; the component and its props are.
    ensure!(page["props"].is_object(), "GUMROAD_SOURCE_UNSUPPORTED");
    // A protected buyer request redirected to the observed login page needs login,
    // while unfamiliar components remain unsupported source shapes.
    ensure!(component != "Logins/New", "AUTH_REQUIRED");
    let props = &page["props"];
    let needed = if component == "Library/Index" {
        let mut needed = pick(props, &["archived_count", "unarchived_count"]);
        // Sole authentication subject: neither current_seller nor display/profile fields bind a buyer.
        needed["buyerIdentity"] = props["logged_in_user"]["id"].clone();
        needed["pagination"] = pick(
            &props["pagination"],
            &["page", "pages", "from", "to", "count"],
        );
        needed["search"] = pick(
            &props["search"],
            &["sort", "query", "creators", "bundles", "show_archived_only"],
        );
        needed["bundles"] = Value::Array(
            props["bundles"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|bundle| pick(bundle, &["id", "label"]))
                .collect(),
        );
        needed["results"] = match props["results"].as_array() {
            Some(rows) => Value::Array(rows.iter().map(|row| {
                let mut product = pick(&row["product"], &["name", "thumbnail_url", "native_type"]);
                product["creator"] = pick(&row["product"]["creator"], &["name"]);
                serde_json::json!({"product":product,"purchase":pick(&row["purchase"], &["id", "is_archived", "download_url", "variants"])})
            }).collect()),
            None => Value::Null,
        };
        needed
    } else if component == "UrlRedirects/DownloadPage" {
        let mut purchase = pick(
            &props["purchase"],
            &[
                "id",
                "receipt_purchase_id",
                "product_id",
                "created_at",
                "product_name",
                "variant_id",
                "product_available",
                "product_long_url",
            ],
        );
        if props["purchase"]["membership"].is_object() {
            purchase["membership"] = serde_json::json!({});
        }
        let bundles = props["content"]["bundle_products"].as_array().map(|items| {
            Value::Array(
                items
                    .iter()
                    .map(|item| pick(item, &["id", "name", "url"]))
                    .collect(),
            )
        });
        serde_json::json!({"purchase":purchase,"content_unavailability_reason_code":props["content_unavailability_reason_code"],"content":{"content_items":needed_items(&props["content"]["content_items"]),"bundle_products":bundles}})
    } else {
        bail!("GUMROAD_SOURCE_UNSUPPORTED");
    };
    Ok(serde_json::json!({"component":component,"url":page["url"],"props":needed}))
}

/// Reads only an explicit Inertia data-page payload; never evaluates scripts or private HTML text.
/// Shared login/user/token/license props are discarded; bearer routes remain transient.
pub fn inertia_page(html: &str) -> Result<Value> {
    ensure!(html.len() <= LIMIT, "GUMROAD_METADATA_TOO_LARGE");
    let document = Html::parse_document(html);
    let selector = Selector::parse("[data-page]").unwrap();
    // The app root precedes any element it renders, so the first page payload in
    // document order is the page; other data-page attributes are not.
    let page = document
        .select(&selector)
        .filter_map(|node| node.value().attr("data-page"))
        .filter_map(|payload| serde_json::from_str::<Value>(payload).ok())
        .find(|page| page["props"].is_object() && page["component"].is_string())
        .ok_or_else(|| anyhow::anyhow!("GUMROAD_SOURCE_UNSUPPORTED"))?;
    necessary_page(&page)
}

#[derive(Clone)]
pub struct LibraryPurchase {
    pub purchase_id: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub thumbnail_url: Option<String>,
    pub native_type: Option<String>,
    pub variant_name: Option<String>,
    pub archived: bool,
    pub content_route: Option<TransientRoute>,
}

impl LibraryPurchase {
    pub fn excluded_non_asset(&self) -> bool {
        matches!(
            self.native_type.as_deref(),
            Some("physical" | "call" | "coffee" | "commission")
        )
    }
}

pub struct LibraryPage {
    pub buyer_identity: String,
    pub pages: u64,
    pub count: u64,
    pub purchases: Vec<LibraryPurchase>,
    pub bundles: Vec<LibraryBundle>,
}
#[derive(Clone)]
pub struct LibraryBundle {
    pub product_id: String,
    pub label: Option<String>,
}

/// Expected page/archive scope must match the returned props; stale Inertia state cannot certify a page.
#[cfg(test)]
pub fn parse_library_props(
    props: &Value,
    base: &Url,
    expected_page: u64,
    archived: bool,
) -> Result<LibraryPage> {
    parse_library_scope(props, base, expected_page, archived, None)
}
pub fn parse_library_scope(
    props: &Value,
    base: &Url,
    expected_page: u64,
    archived: bool,
    bundle: Option<&str>,
) -> Result<LibraryPage> {
    ensure!(
        trusted_url(base) && base.path() == "/library",
        "GUMROAD_ROUTE_UNSUPPORTED"
    );
    let number = |value: &Value| {
        value
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("GUMROAD_PAGINATION_UNPROVEN"))
    };
    let pagination = &props["pagination"];
    let page = number(&pagination["page"])?;
    let pages = number(&pagination["pages"])?;
    let count = number(&pagination["count"])?;
    ensure!(
        page == expected_page && page >= 1 && pages >= page && pages <= 10000,
        "GUMROAD_PAGINATION_UNPROVEN"
    );
    // An echoed parameter fails the page only when it shows another tab or a filter.
    // A missing or renamed echo and the sort order are not needed to read the page.
    let search = &props["search"];
    ensure!(
        search["show_archived_only"]
            .as_bool()
            .is_none_or(|value| value == archived)
            && search["query"].as_str().is_none_or(str::is_empty)
            && search["creators"].as_array().is_none_or(Vec::is_empty)
            && match bundle {
                None => search["bundles"].as_array().is_none_or(Vec::is_empty),
                // Only the echo proves that member cards belong to this bundle.
                Some(bundle) => search["bundles"]
                    .as_array()
                    .is_some_and(|ids| ids.len() == 1 && ids[0].as_str() == Some(bundle)),
            },
        "GUMROAD_SCOPE_CHANGED"
    );
    let rows = props["results"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("GUMROAD_SOURCE_UNSUPPORTED"))?;
    ensure!(
        rows.len() <= 1000 && (count != 0 || rows.is_empty()) && (count == 0 || !rows.is_empty()),
        "GUMROAD_PAGINATION_UNPROVEN"
    );
    let from = number(&pagination["from"])?;
    let to = number(&pagination["to"])?;
    ensure!(
        if count == 0 {
            from == 0 && to == 0 && pages == 1
        } else {
            from >= 1 && to >= from && to <= count && to - from + 1 == rows.len() as u64
        },
        "GUMROAD_PAGINATION_UNPROVEN"
    );
    let mut seen = BTreeSet::new();
    let mut purchases = Vec::with_capacity(rows.len());
    for row in rows {
        let purchase = &row["purchase"];
        let product = &row["product"];
        let purchase_id = identifier(&purchase["id"])?;
        ensure!(
            seen.insert(purchase_id.clone()),
            "GUMROAD_PURCHASE_AMBIGUOUS"
        );
        ensure!(
            purchase["is_archived"]
                .as_bool()
                .is_none_or(|value| value == archived),
            "GUMROAD_SCOPE_CHANGED"
        );
        purchases.push(LibraryPurchase {
            purchase_id,
            archived,
            title: optional_text(&product["name"]),
            author: optional_text(&product["creator"]["name"]),
            thumbnail_url: optional_text(&product["thumbnail_url"]),
            native_type: optional_text(&product["native_type"]),
            variant_name: optional_text(&purchase["variants"]),
            // An unusable route is that purchase's own failure, never the page's.
            content_route: transient_route(&purchase["download_url"], base)
                .ok()
                .flatten(),
        });
    }
    let buyer_identity = identifier(
        props
            .get("buyerIdentity")
            .unwrap_or(&props["logged_in_user"]["id"]),
    )?;
    let mut bundle_ids = BTreeSet::new();
    let mut bundles = Vec::new();
    for row in props["bundles"].as_array().into_iter().flatten() {
        let product_id = identifier(&row["id"])?;
        ensure!(
            bundle_ids.insert(product_id.clone()),
            "GUMROAD_PURCHASE_AMBIGUOUS"
        );
        bundles.push(LibraryBundle {
            product_id,
            label: optional_text(&row["label"]),
        });
    }
    ensure!(
        bundle.is_none_or(|id| bundle_ids.contains(id)),
        "GUMROAD_BUNDLE_OWNERSHIP_UNPROVEN"
    );
    Ok(LibraryPage {
        buyer_identity,
        pages,
        count,
        purchases,
        bundles,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileAccess {
    Downloadable,
    ViewOnly,
    ExternalReference,
    Unavailable,
}

#[derive(Clone)]
pub struct FileObservation {
    pub file_id: String,
    pub source_product_id: String,
    pub source_purchase_id: String,
    pub name: Option<String>,
    pub folders: Vec<String>,
    pub size_bytes: Option<u64>,
    pub access: FileAccess,
    pub resolver: Option<TransientRoute>,
}

#[derive(Clone)]
pub struct ContentObservation {
    pub purchase_id: String,
    pub product_id: String,
    pub variant_id: Option<String>,
    pub title: Option<String>,
    pub acquired_at: Option<DateTime<Utc>>,
    pub has_membership: bool,
    pub files: Vec<FileObservation>,
    pub inventory_complete: bool,
    pub unsupported_items: usize,
    pub public_product_url: Option<Url>,
    pub bundles: Vec<BundleObservation>,
    pub source_bundle_id: Option<String>,
    pub member_purchase_ids: BTreeSet<String>,
}

#[derive(Clone)]
pub struct BundleObservation {
    pub product_id: String,
    pub content_route: Option<TransientRoute>,
}

impl ContentObservation {
    pub fn catalogue_product_id(&self) -> String {
        format!("gumroad:{}", self.product_id)
    }
}

fn file_items(
    items: &[Value],
    folders: &[String],
    files: &mut Vec<FileObservation>,
    seen: &mut BTreeSet<String>,
    unresolved: &mut usize,
    depth: usize,
) -> Result<()> {
    ensure!(
        depth <= 32 && files.len() + items.len() <= 10000,
        "GUMROAD_INVENTORY_TOO_LARGE"
    );
    for item in items {
        match item["type"].as_str() {
            Some("folder") => {
                let Some(name) = optional_text(&item["name"]) else {
                    *unresolved += 1;
                    continue;
                };
                let Some(children) = item["children"].as_array() else {
                    *unresolved += 1;
                    continue;
                };
                let mut path = folders.to_vec();
                path.push(name);
                file_items(children, &path, files, seen, unresolved, depth + 1)?;
            }
            Some("file") => {
                let Ok(file_id) = identifier(&item["id"]) else {
                    *unresolved += 1;
                    continue;
                };
                if !seen.insert(file_id.clone()) {
                    *unresolved += 1;
                    if let Some(previous) = files.iter_mut().find(|file| file.file_id == file_id) {
                        previous.access = FileAccess::Unavailable;
                        previous.resolver = None;
                    }
                    continue;
                }
                let resolver = match file_route(&item["download_url"], &file_id) {
                    Ok(route) => route,
                    Err(_) => {
                        *unresolved += 1;
                        None
                    }
                };
                let access = if resolver.is_some() {
                    FileAccess::Downloadable
                } else if item["external_link_url"]
                    .as_str()
                    .is_some_and(|value| !value.is_empty())
                {
                    FileAccess::ExternalReference
                } else if ["stream_url", "read_url"]
                    .iter()
                    .any(|key| item[*key].as_str().is_some_and(|value| !value.is_empty()))
                {
                    FileAccess::ViewOnly
                } else {
                    FileAccess::Unavailable
                };
                let name = optional_text(&item["file_name"]);
                let has_additional_downloads = item["subtitle_files"]
                    .as_array()
                    .is_some_and(|items| items.iter().any(|item| item["download_url"].is_string()));
                if access == FileAccess::Unavailable
                    || (access == FileAccess::Downloadable && name.is_none())
                    || has_additional_downloads
                {
                    *unresolved += 1;
                }
                files.push(FileObservation {
                    file_id,
                    source_product_id: String::new(),
                    source_purchase_id: String::new(),
                    name,
                    folders: folders.to_vec(),
                    // content_length is playback duration/page count, not byte length.
                    // Stamping serves a different per-purchase PDF object from the original upload.
                    size_bytes: if item["pdf_stamp_enabled"] == true {
                        None
                    } else {
                        item["file_size"].as_u64()
                    },
                    access,
                    resolver,
                });
            }
            _ => *unresolved += 1,
        }
    }
    Ok(())
}

/// Purchase identity is supplied by the enumerated Library card, never inferred from title or URL.
pub fn parse_content_props(
    props: &Value,
    base: &Url,
    expected_purchase_id: &str,
    expected_product_id: Option<&str>,
) -> Result<ContentObservation> {
    ensure!(trusted_url(base), "GUMROAD_ROUTE_UNSUPPORTED");
    let purchase = &props["purchase"];
    let purchase_id = identifier(&purchase["id"])?;
    let product_id = identifier(&purchase["product_id"])?;
    ensure!(
        purchase_id == expected_purchase_id
            && expected_product_id.is_none_or(|id| id == product_id),
        "GUMROAD_PRODUCT_BINDING_UNPROVEN"
    );
    let optional_id = |value: &Value| {
        if value.is_null() {
            Ok(None)
        } else {
            identifier(value).map(Some)
        }
    };
    let acquired_at = purchase["created_at"]
        .as_str()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc));
    let mut files = Vec::new();
    let mut unsupported_items = 0;
    match props["content"]["content_items"].as_array() {
        Some(items) => file_items(
            items,
            &[],
            &mut files,
            &mut BTreeSet::new(),
            &mut unsupported_items,
            0,
        )?,
        None => unsupported_items += 1,
    }
    let mut bundles = Vec::new();
    if let Some(members) = props["content"]["bundle_products"].as_array() {
        let mut seen = BTreeSet::new();
        for member in members {
            let Ok(product_id) = identifier(&member["id"]) else {
                unsupported_items += 1;
                continue;
            };
            if !seen.insert(product_id.clone()) {
                unsupported_items += 1;
                continue;
            }
            let content_route = transient_route(&member["url"], base).ok().flatten();
            bundles.push(BundleObservation {
                product_id,
                content_route,
            });
            // Parent content alone does not prove each bundle member's inventory.
            unsupported_items += 1;
        }
    }
    if props["content_unavailability_reason_code"]
        .as_str()
        .is_some_and(|value| !value.is_empty())
    {
        unsupported_items += 1;
    }
    let public_product_url = purchase["product_long_url"]
        .as_str()
        .and_then(|value| Url::parse(value).ok())
        .filter(|url| {
            trusted_url(url)
                && url.path().starts_with("/l/")
                && url.query().is_none()
                && url.fragment().is_none()
        });
    for file in &mut files {
        file.source_product_id = product_id.clone();
        file.source_purchase_id = purchase_id.clone();
    }
    Ok(ContentObservation {
        purchase_id,
        product_id,
        // Unused beyond this observation, so an unreadable value is unknown, not a failure.
        variant_id: optional_id(&purchase["variant_id"])?,
        title: optional_text(&purchase["product_name"]),
        acquired_at,
        has_membership: purchase["membership"].is_object(),
        inventory_complete: unsupported_items == 0,
        unsupported_items,
        files,
        public_product_url,
        bundles,
        source_bundle_id: None,
        member_purchase_ids: BTreeSet::new(),
    })
}

/// Source units are preserved as price_cents; presentation/conversion belongs to the money contract.
#[derive(Clone, Debug)]
pub struct PublicPrice {
    pub price_cents: u64,
    pub currency_code: String,
    pub pay_what_you_want: bool,
    pub recurring: bool,
}

impl PublicPrice {
    pub fn usd_amount(&self) -> Option<f64> {
        (self.currency_code == "USD" && self.price_cents <= 9_007_199_254_740_991)
            .then(|| self.price_cents as f64 / 100.0)
    }
}

#[derive(Clone, Debug)]
pub struct PublicProductObservation {
    pub product_id: String,
    // Missing/unknown preserves the last observed body; an observed empty string clears it.
    pub description: Option<String>,
    pub current_price: Option<PublicPrice>,
    pub price_formatted: Option<String>,
    pub price_source_currency: Option<String>,
}

fn public_description(raw: &str) -> String {
    const TEXT_LIMIT: usize = 40_000;
    let bounded: String = raw.chars().take(TEXT_LIMIT * 4).collect();
    let fragment = Html::parse_fragment(&bounded);
    let mut text = String::new();
    let mut pending: Vec<_> = fragment
        .root_element()
        .children()
        .rev()
        .map(|node| (node, false))
        .collect();
    while let Some((node, closing)) = pending.pop() {
        if closing {
            text.push(' ');
            continue;
        }
        if let Some(part) = node.value().as_text() {
            text.extend(
                part.chars()
                    .filter(|character| !character.is_control() || character.is_whitespace()),
            );
            continue;
        }
        if let Some(element) = node.value().as_element() {
            if matches!(
                element.name(),
                "script" | "style" | "template" | "noscript" | "nav"
            ) {
                continue;
            }
            if matches!(
                element.name(),
                "address"
                    | "article"
                    | "aside"
                    | "blockquote"
                    | "br"
                    | "dd"
                    | "details"
                    | "div"
                    | "dl"
                    | "dt"
                    | "fieldset"
                    | "figcaption"
                    | "figure"
                    | "footer"
                    | "form"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "header"
                    | "hgroup"
                    | "hr"
                    | "li"
                    | "main"
                    | "ol"
                    | "p"
                    | "pre"
                    | "section"
                    | "summary"
                    | "table"
                    | "tbody"
                    | "td"
                    | "tfoot"
                    | "th"
                    | "thead"
                    | "tr"
                    | "ul"
            ) {
                text.push(' ');
                pending.push((node, true));
            }
        }
        pending.extend(node.children().rev().map(|child| (child, false)));
    }
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(TEXT_LIMIT)
        .collect()
}

pub fn parse_public_product(
    value: &Value,
    expected_product_id: Option<&str>,
) -> Result<PublicProductObservation> {
    ensure!(
        value["api_version"].as_u64() == Some(1),
        "GUMROAD_PUBLIC_SCHEMA_UNSUPPORTED"
    );
    let product_id = identifier(&value["id"])?;
    ensure!(
        expected_product_id.is_none_or(|id| id == product_id),
        "GUMROAD_PRODUCT_BINDING_UNPROVEN"
    );
    let currency = value["currency_code"]
        .as_str()
        .filter(|value| value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_alphabetic()));
    let current_price = match (
        value["price_cents"].as_u64(),
        currency,
        value["is_pay_what_you_want"].as_bool(),
        value["is_recurring_billing"].as_bool(),
    ) {
        (Some(price_cents), Some(currency), Some(pay_what_you_want), Some(recurring)) => {
            Some(PublicPrice {
                price_cents,
                currency_code: currency.to_ascii_uppercase(),
                pay_what_you_want,
                recurring,
            })
        }
        _ => None,
    };
    Ok(PublicProductObservation {
        product_id,
        description: value["description"].as_str().map(public_description),
        current_price,
        price_formatted: value["price_formatted"]
            .as_str()
            .map(str::trim)
            .filter(|text| {
                !text.is_empty()
                    && text.chars().count() <= 256
                    && !text.chars().any(char::is_control)
            })
            .map(str::to_owned),
        price_source_currency: currency.map(str::to_ascii_uppercase),
    })
}

#[cfg(test)]
mod tests;
