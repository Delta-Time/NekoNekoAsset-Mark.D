//! Independently authored Rust parser, informed by this project's MIT collector.
//! Digital item fields and explicitly bound receipt metadata only.
//! Physical/contact/shipping/comment bodies are not parsed.
use super::safety;
use anyhow::{bail, Result};
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("fixed selector")
}
fn html_language(doc: &Html) -> &'static str {
    doc.select(&sel("html"))
        .next()
        .and_then(|node| node.value().attr("lang"))
        .filter(|value| !value.trim().is_empty())
        .map_or("unknown", |value| {
            let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
            match normalized.as_str() {
                "zh-cn" | "zh-hans" | "zh-hans-cn" => return "zh-cn",
                "zh-tw" | "zh-hant" | "zh-hant-tw" => return "zh-tw",
                _ => {}
            }
            match value.trim().split(['-', '_']).next().unwrap_or("") {
                value if value.eq_ignore_ascii_case("ja") => "ja",
                value if value.eq_ignore_ascii_case("en") => "en",
                value if value.eq_ignore_ascii_case("ko") => "ko",
                _ => "other",
            }
        })
}
fn text(e: ElementRef<'_>) -> String {
    safety::short(&e.text().collect::<String>())
}
fn first_text(e: ElementRef<'_>, selector: &str) -> Option<String> {
    e.select(&sel(selector))
        .next()
        .map(text)
        .filter(|s| !s.is_empty())
}
fn absolute(value: &str) -> Option<String> {
    url::Url::parse("https://accounts.booth.pm")
        .unwrap()
        .join(value)
        .ok()
        .map(|u| u.to_string())
}
fn links(e: ElementRef<'_>) -> Vec<ElementRef<'_>> {
    let mut v: Vec<_> = e.select(&sel("a[href]")).collect();
    if e.value().name() == "a" && e.value().attr("href").is_some() {
        v.insert(0, e);
    }
    v
}
fn identity(e: ElementRef<'_>) -> Option<String> {
    safety::item_id(&absolute(e.value().attr("href")?)?)
}
fn downloads(
    e: ElementRef<'_>,
    receipt: bool,
) -> Vec<(ElementRef<'_>, String, Option<String>, String)> {
    let mut found: Vec<_> = e
        .select(&sel(
            "[data-test='downloadable'],a[href*='/downloadables/'],[data-href*='/downloadables/']",
        ))
        .filter_map(|n| {
            let value = n
                .value()
                .attr("data-href")
                .or_else(|| n.value().attr("href"))?;
            let (id, variant, route) = safety::observed_resolver(&absolute(value)?)?;
            Some((n, id, variant, route))
        })
        .collect();
    for node in e.select(&sel(dropdown_controls(receipt))) {
        let Some(items) = dropdown_items(node) else {
            continue;
        };
        // A track menu offers one downloadable in several formats (`path?type=`).
        // Every format must resolve, or the menu stays an unresolved control.
        let key = if is_track(node) {
            "path"
        } else {
            "deeplinkDownloadableUrl"
        };
        let resolved: Vec<_> = items
            .iter()
            .map(|item| item[key].as_str().and_then(safety::observed_resolver))
            .collect();
        if is_track(node) && resolved.iter().any(Option::is_none) {
            continue;
        }
        let mut seen = BTreeSet::new();
        for (id, variant, route) in resolved.into_iter().flatten() {
            if seen.insert((id.clone(), variant.clone())) {
                found.push((node, id, variant, route));
            }
        }
    }
    let mut bindings = Vec::new();
    if receipt {
        // Primary receipt links omit variation_id; the same file row's alternate
        // methods supply it. Keep each control observation for closure checks,
        // while files() deduplicates their single confirmed file/variant identity.
        for (index, (node, file, variant, _)) in found.iter().enumerate() {
            if variant.is_some() {
                continue;
            }
            let Some(row) = node.ancestors().filter_map(ElementRef::wrap).find(|row| {
                row.value()
                    .classes()
                    .any(|class| class == "legacy-list-item")
            }) else {
                continue;
            };
            if row.id() != e.id() && !e.descendants().any(|child| child.id() == row.id()) {
                continue;
            }
            let controls: Vec<_> = found
                .iter()
                .filter(|(control, _, _, _)| {
                    row.descendants().any(|child| child.id() == control.id())
                })
                .collect();
            if controls.iter().any(|(_, id, _, _)| id != file) {
                continue;
            }
            let variants: BTreeSet<_> = controls
                .iter()
                .filter_map(|(control, _, variant, _)| {
                    control
                        .value()
                        .attr("data-dropdown-items")
                        .and(variant.as_ref())
                })
                .collect();
            if variants.len() == 1 {
                bindings.push((index, (*variants.iter().next().unwrap()).clone()));
            }
        }
    } else {
        // Alternate app controls cannot add Library files. They can clarify an
        // already accessible primary file when its own filename row binds one
        // exact downloadable and one variation.
        for (index, (node, file, variant, _)) in found.iter().enumerate() {
            if variant.is_some() {
                continue;
            }
            // A track row shows a title rather than a filename; its own format
            // menu already scopes it to one downloadable.
            let track = is_track(*node);
            for row in node.ancestors().filter_map(ElementRef::wrap) {
                if row.id() == e.id() || !e.descendants().any(|child| child.id() == row.id()) {
                    break;
                }
                if found.iter().any(|(control, id, _, _)| {
                    id != file && row.descendants().any(|child| child.id() == control.id())
                }) {
                    break;
                }
                let names = row.select(&sel("[data-test='filename'],.min-w-0.break-words.whitespace-pre-line .text-14,.legacy-list-item .min-w-0 > b"))
                    .filter(|name| looks_filename(&text(*name))).count();
                if names > 1 || links(row).into_iter().any(|link| identity(link).is_some()) {
                    break;
                }
                if names == 0 && !track {
                    continue;
                }
                let alternates: Vec<_> = row
                    .select(&sel(
                        "[data-test='other-downloads-button'][data-dropdown-items]",
                    ))
                    .collect();
                if alternates.is_empty() {
                    if track {
                        continue;
                    }
                    break;
                }
                let mut variants = BTreeSet::new();
                let valid = alternates.iter().all(|alternate| {
                    let Some(raw) = alternate
                        .value()
                        .attr("data-dropdown-items")
                        .filter(|raw| raw.len() <= 262_144)
                    else {
                        return false;
                    };
                    let Ok(items) = serde_json::from_str::<Vec<Value>>(raw) else {
                        return false;
                    };
                    items.iter().all(|item| {
                        let Some(url) = item["deeplinkDownloadableUrl"].as_str() else {
                            return false;
                        };
                        let Some((id, variant, _)) = safety::observed_resolver(url) else {
                            return false;
                        };
                        if &id != file {
                            return false;
                        }
                        if let Some(variant) = variant {
                            variants.insert(variant);
                        }
                        true
                    })
                });
                let primary_variants_agree = found
                    .iter()
                    .filter(|(control, _, _, _)| {
                        row.descendants().any(|child| child.id() == control.id())
                    })
                    .all(|(_, _, variant, _)| {
                        variant
                            .as_ref()
                            .is_none_or(|variant| variants.contains(variant))
                    });
                if valid && variants.len() == 1 && primary_variants_agree {
                    bindings.push((index, variants.into_iter().next().unwrap()));
                }
                break;
            }
        }
    }
    for (index, variant) in bindings {
        found[index].2 = Some(variant.clone());
        found[index].3 = format!(
            "https://booth.pm/downloadables/{}?variation_id={variant}",
            found[index].1
        );
    }
    found
}
fn raw_download_controls(e: ElementRef<'_>, receipt: bool) -> usize {
    let tracks: BTreeSet<_> = downloads(e, receipt)
        .into_iter()
        .filter(|(node, ..)| is_track(*node))
        .map(|(_, id, ..)| id)
        .collect();
    e.select(&sel(&format!(
        "[data-test='downloadable'],a[href*='/downloadables/'],[data-href*='/downloadables/'],{}",
        dropdown_controls(receipt)
    )))
    .filter(|node| !track_bundle(*node, &tracks))
    .count()
}
/// Library alternate-app menus can clarify a primary file but never add one.
fn dropdown_controls(receipt: bool) -> &'static str {
    if receipt {
        "[data-dropdown-items]"
    } else {
        "[data-dropdown-items]:not([data-test='other-downloads-button']):not([data-test='other-multitrack-downloads-button'])"
    }
}
fn dropdown_items(node: ElementRef<'_>) -> Option<Vec<Value>> {
    let raw = node
        .value()
        .attr("data-dropdown-items")
        .filter(|raw| raw.len() <= 262_144)?;
    serde_json::from_str(raw).ok()
}
fn is_track(node: ElementRef<'_>) -> bool {
    node.value().attr("data-test") == Some("track-download-button")
}
/// An album zip menu bundles listed tracks in the browser; it is not a file of
/// its own. An entry outside the card's resolved tracks keeps it unresolved.
fn track_bundle(node: ElementRef<'_>, tracks: &BTreeSet<String>) -> bool {
    node.value().attr("data-test") == Some("zipped-download-button")
        && dropdown_items(node).is_some_and(|items| {
            !items.is_empty()
                && items.iter().all(|item| {
                    item["zippedFiles"].as_array().is_some_and(|entries| {
                        !entries.is_empty()
                            && entries.iter().all(|entry| {
                                entry["prefetchUrl"]
                                    .as_str()
                                    .and_then(prefetch_file)
                                    .is_some_and(|id| tracks.contains(&id))
                            })
                    })
                })
        })
}
fn prefetch_file(value: &str) -> Option<String> {
    let url = url::Url::parse(value).ok()?;
    if url.scheme() != "https" || url.host_str() != Some("booth.pm") {
        return None;
    }
    let mut path = url.path_segments()?;
    let id = (path.next()? == "downloadables")
        .then(|| path.next())
        .flatten()?;
    safety::numeric(id).then(|| id.to_owned())
}
/// The track's one MP3 choice names its file; without it the name stays unknown.
fn track_download(node: ElementRef<'_>) -> Option<String> {
    let names: Vec<_> = dropdown_items(node)?
        .iter()
        .filter_map(|item| {
            let path = url::Url::parse(item["path"].as_str()?).ok()?;
            let name = item["data"]["filename"].as_str()?;
            let mp3 = path
                .query_pairs()
                .any(|(key, value)| key == "type" && value == safety::TRACK_FORMAT)
                && name
                    .rsplit_once('.')
                    .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case(safety::TRACK_FORMAT));
            mp3.then(|| name.chars().take(4096).collect::<String>())
        })
        .collect();
    match names.as_slice() {
        [name] => Some(name.clone()),
        _ => None,
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub id: String,
    pub variant_id: Option<String>,
    pub name: Option<String>,
    pub size_bytes: Option<u64>,
    pub source_url: String,
    pub revision: String,
    /// The one rendition requested from a multi-format downloadable (`type=`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub source_record_id: String,
    pub product_id: Option<String>,
    pub order_id: Option<String>,
    pub name: Option<String>,
    pub shop: Option<String>,
    pub shop_url: Option<String>,
    pub preview_url: Option<String>,
    pub acquired_date_text: Option<String>,
    /// When the App itself took the row, to the millisecond: a free item added by its
    /// listing. No store page has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acquired_at: Option<String>,
    pub paid_amount_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_line_total_jpy: Option<u64>,
    pub variant_name: Option<String>,
    pub files: Vec<File>,
    pub manifest_complete: bool,
    pub unavailable: bool,
    pub error_code: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub source: String,
    pub route: String,
    pub rows: Vec<Row>,
    pub next: Vec<String>,
    pub details: Vec<String>,
    pub terminal: bool,
    pub unresolved: usize,
    pub fingerprint: String,
    pub is_detail: bool,
    #[serde(default)]
    pub ordering: Ordering,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) free_item: Option<super::free_item::Evidence>,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Ordering {
    #[default]
    Unknown,
    AcquiredNewestFirst,
    /// No page reports this any more; saved checkpoints may still hold it.
    DownloadedNewestFirst,
    FileUpdatedNewestFirst,
}
/// The Library page states its query in `data-search-params`, whatever the page's language:
/// `sort` only when it is not BOOTH's default, newest acquired first (announcement 970), and a
/// search or filter as more keys. Only a whole list in a stated order counts.
fn ordering(doc: &Html, source: &str, is_detail: bool) -> Ordering {
    if is_detail || source != "library" {
        return Ordering::Unknown;
    }
    let Some(params) = doc
        .select(&sel("main [data-search-params]"))
        .next()
        .and_then(|node| node.value().attr("data-search-params"))
        .and_then(|raw| serde_json::from_str::<serde_json::Map<String, Value>>(raw).ok())
    else {
        return Ordering::Unknown;
    };
    if params
        .keys()
        .any(|key| !matches!(key.as_str(), "page" | "sort"))
    {
        return Ordering::Unknown;
    }
    match params.get("sort").map(|sort| sort.as_str()) {
        None | Some(Some("library_added_at_desc")) => Ordering::AcquiredNewestFirst,
        Some(Some(safety::LIBRARY_UPDATED_SORT)) => Ordering::FileUpdatedNewestFirst,
        Some(_) => Ordering::Unknown,
    }
}
fn files(card: ElementRef<'_>, receipt: bool) -> Vec<File> {
    let mut result = BTreeMap::new();
    let controls = downloads(card, receipt);
    for (button, id, variant, route) in controls.iter().cloned() {
        let key = format!("{id}:{}", variant.as_deref().unwrap_or(""));
        if result.contains_key(&key) {
            continue;
        }
        // A track row's text is a title; only its format menu can name the file.
        let track = controls
            .iter()
            .find(|(node, i, v, _)| *i == id && *v == variant && is_track(*node))
            .map(|(node, ..)| *node);
        let mut name = track.and_then(track_download);
        let format = name.is_some().then(|| safety::TRACK_FORMAT.to_owned());
        let mut size_bytes = None;
        for scope in button
            .ancestors()
            .filter_map(ElementRef::wrap)
            .filter(|_| track.is_none())
        {
            if !card.descendants().any(|n| n.id() == scope.id()) && scope.id() != card.id() {
                break;
            }
            let keys: BTreeSet<_> = controls
                .iter()
                .filter(|(node, _, _, _)| {
                    scope.id() == node.id()
                        || scope.descendants().any(|child| child.id() == node.id())
                })
                .map(|(_, i, v, _)| (i, v))
                .collect();
            if keys.len() != 1 {
                break;
            }
            let mut names: Vec<_> = scope.select(&sel("[data-test='filename']")).collect();
            if names.is_empty() {
                names = scope
                    .select(&sel(".min-w-0.break-words.whitespace-pre-line .text-14"))
                    .collect();
            }
            if names.is_empty() && scope.value().classes().any(|c| c == "legacy-list-item") {
                names = scope
                    .select(&sel(".min-w-0 > b"))
                    .filter(|n| {
                        n.select(&sel("a,button,input,textarea,select"))
                            .next()
                            .is_none()
                            && looks_filename(&text(*n))
                    })
                    .collect();
            }
            if names.len() == 1 {
                name = Some(
                    names[0]
                        .text()
                        .collect::<String>()
                        .trim()
                        .chars()
                        .take(4096)
                        .collect::<String>(),
                )
                .filter(|n| !n.is_empty());
                size_bytes = scope
                    .select(&sel("[data-size-bytes]"))
                    .next()
                    .and_then(|n| n.value().attr("data-size-bytes"))
                    .and_then(|v| v.parse().ok());
                break;
            }
            if names.len() > 1 || scope.id() == card.id() {
                break;
            }
        }
        if name.is_none() {
            name = button
                .value()
                .attr("data-label")
                .filter(|s| looks_filename(s))
                .map(|s| s.chars().take(4096).collect());
        }
        let revision = button
            .value()
            .attr("data-sha256")
            .filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
            .map(|v| format!("sha256:{}", v.to_ascii_lowercase()))
            .or_else(|| {
                button
                    .value()
                    .attr("data-revision")
                    .filter(|v| {
                        !v.is_empty()
                            && v.len() <= 100
                            && v.bytes().all(|c| {
                                c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.')
                            })
                    })
                    .map(|v| format!("source:{v}"))
            })
            .unwrap_or_else(|| "unknown".into());
        result.insert(
            key,
            File {
                id,
                variant_id: variant,
                name,
                size_bytes,
                source_url: route,
                revision,
                format,
            },
        );
    }
    result.into_values().collect()
}
fn looks_filename(value: &str) -> bool {
    value.trim().rsplit_once('.').is_some_and(|(_, ext)| {
        !ext.is_empty() && ext.len() <= 12 && ext.bytes().all(|c| c.is_ascii_alphanumeric())
    })
}
/// BOOTH fades a delisted row's thumbnail and stops linking its title. The badge beside it is
/// translated; the markup is not.
fn faded_row(card: ElementRef<'_>) -> bool {
    card.select(&sel("img.l-library-item-thumbnail"))
        .any(|image| {
            image
                .value()
                .classes()
                .any(|class| class.starts_with("opacity-"))
        })
}
fn unavailable(card: ElementRef<'_>) -> bool {
    card.select(&sel("[data-test='item-unavailable'],.item-unavailable"))
        .next()
        .is_some()
        || faded_row(card)
        || card
            .descendants()
            .filter(|n| {
                !n.ancestors()
                    .filter_map(ElementRef::wrap)
                    .any(|e| matches!(e.value().name(), "script" | "style"))
            })
            .filter_map(|n| n.value().as_text())
            .filter(|t| t.len() <= 540)
            .any(|t| {
                matches!(
                    t.trim(),
                    "この商品は非公開です"
                        | "この商品は非公開になりました"
                        | "商品が非公開になりました"
                        | "非公開の商品"
                        | "非公開"
                        | "非公開中"
                        | "公開停止中"
                        | "This item is private"
                        | "商品は削除されました"
                        | "この商品は削除されました"
                        | "商品が削除されました"
                        | "削除済みの商品"
                        | "Deleted item"
                        | "商品が見つかりません"
                        | "この商品は削除または非公開となっています"
                        | "この商品は削除されたか、非公開になっています"
                        | "非公開または削除された商品"
                        | "Item unavailable"
                        | "This item is unavailable"
                        | "この商品は販売を終了しました"
                        | "販売終了の商品"
                        | "商品の公開は終了しました"
                        | "This item is no longer available"
                        | "This item has been deleted"
                )
            })
}
fn numeric_identity_present(card: ElementRef<'_>) -> bool {
    std::iter::once(card)
        .chain(card.select(&sel(
            "[data-item-id],[data-product-id],[data-id],[data-item],[data-product]",
        )))
        .any(|e| {
            [
                "data-item-id",
                "data-product-id",
                "data-id",
                "data-item",
                "data-product",
            ]
            .iter()
            .any(|a| e.value().attr(a).is_some_and(safety::numeric))
        })
}
/// Digital controls bind financial fields independently of all-files readiness.
/// Missing filenames remain an incomplete manifest, not an unknown purchase.
pub(super) fn product_field_file_keys(row: &Row) -> Option<BTreeSet<(String, Option<String>)>> {
    if row.unavailable
        || row
            .error_code
            .as_deref()
            .is_some_and(|code| code != "PRODUCT_TITLE_UNRESOLVED")
        || !row.product_id.as_deref().is_some_and(safety::numeric)
        || row.files.is_empty()
    {
        return None;
    }
    let mut keys = BTreeSet::new();
    for file in &row.files {
        let (id, variant, route) = safety::resolver(&file.source_url)?;
        if id != file.id
            || variant != file.variant_id
            || route != file.source_url
            || !keys.insert((id, variant))
        {
            return None;
        }
    }
    Some(keys)
}
fn plain_leaf(node: ElementRef<'_>, limit: usize) -> Option<String> {
    if node
        .children()
        .filter_map(ElementRef::wrap)
        .next()
        .is_some()
        || matches!(
            node.value().name(),
            "script" | "style" | "a" | "button" | "input" | "textarea" | "select" | "form"
        )
    {
        return None;
    }
    let value = node.text().collect::<String>();
    (value.chars().count() <= limit && !value.chars().any(|c| c.is_control() && !c.is_whitespace()))
        .then(|| value.trim().to_owned())
}
fn bounded_leaf_value(node: ElementRef<'_>) -> Option<String> {
    if let Some(value) = plain_leaf(node, 128) {
        return Some(value);
    }
    if node
        .select(&sel("a,button,input,textarea,select,form,script,style"))
        .next()
        .is_some()
    {
        return None;
    }
    let leaves: Vec<_> = node
        .descendants()
        .filter_map(ElementRef::wrap)
        .filter_map(|n| plain_leaf(n, 128))
        .filter(|text| !text.is_empty())
        .collect();
    (leaves.len() == 1).then(|| leaves[0].clone())
}
pub(super) fn slash_date_shape(value: &str) -> bool {
    value.len() <= 32
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || matches!(c, b'/' | b':' | b' '))
        && (chrono::NaiveDate::parse_from_str(value, "%Y/%m/%d").is_ok()
            || ["%Y/%m/%d %H:%M", "%Y/%m/%d %H:%M:%S"]
                .iter()
                .any(|format| chrono::NaiveDateTime::parse_from_str(value, format).is_ok()))
}
fn noninteractive(node: ElementRef<'_>) -> bool {
    std::iter::once(node)
        .chain(node.ancestors().filter_map(ElementRef::wrap))
        .all(|n| {
            !matches!(
                n.value().name(),
                "a" | "time"
                    | "form"
                    | "button"
                    | "input"
                    | "textarea"
                    | "select"
                    | "script"
                    | "style"
            )
        })
}
fn currency_jpy(value: &str) -> Option<f64> {
    (value.contains(['¥', '￥', '円']) || value.trim_end().ends_with("JPY"))
        .then(|| super::metadata::parse_jpy_price(&serde_json::json!(value)))
        .flatten()
}
fn rebound_product_fields(card: ElementRef<'_>, proof_rows: &[Row]) -> bool {
    // A recognized message prefix excludes its opaque body from product
    // binding. This does not validate closure or gate unrelated receipt UI.
    if omitted_notice_scope(card) {
        return false;
    }
    let ids: BTreeSet<_> = links(card).into_iter().filter_map(identity).collect();
    let controls = downloads(card, true);
    let raw = raw_download_controls(card, true);
    if ids.len() != 1 || controls.is_empty() || raw != controls.len() {
        return false;
    }
    let id = ids.first().unwrap();
    let keys: BTreeSet<_> = controls
        .into_iter()
        .map(|(_, id, variant, _)| (id, variant))
        .collect();
    proof_rows.iter().any(|row| {
        row.product_id.as_ref() == Some(id) && product_field_file_keys(row).as_ref() == Some(&keys)
    }) && !unavailable(card)
        && !std::iter::once(card)
            .chain(card.select(&sel("[data-product-id],[data-item-id]")))
            .any(|node| {
                ["data-product-id", "data-item-id"]
                    .iter()
                    .any(|key| node.value().attr(key).is_some_and(|value| value != id))
            })
}
fn template_literal(value: &str) -> &str {
    value.trim().trim_end_matches([':', '：']).trim()
}
fn order_number_label(value: &str) -> bool {
    matches!(
        template_literal(value),
        "注文番号" | "ご注文番号" | "注文ID" | "Order number" | "Order Number" | "Order ID"
    )
}
fn ordered_date_label(value: &str) -> bool {
    matches!(
        template_literal(value),
        "注文日時"
            | "ご注文日時"
            | "注文日"
            | "ご注文日"
            | "Ordered at"
            | "Order date"
            | "Created At"
    )
}
fn template_label_value<'a>(
    mut label: ElementRef<'a>,
    scope: ElementRef<'a>,
    literal: &str,
) -> Option<ElementRef<'a>> {
    let mut paired = None;
    for _ in 0..=2 {
        if label.id() == scope.id()
            || !scope.descendants().any(|n| n.id() == label.id())
            || label
                .select(&sel("a,form,button,input,textarea,select,script,style"))
                .next()
                .is_some()
        {
            break;
        }
        let parts: Vec<_> = label
            .text()
            .filter(|part| !part.trim().is_empty())
            .collect();
        if parts.len() != 1 || template_literal(parts[0]) != literal {
            break;
        }
        if let Some(next) = label.next_siblings().filter_map(ElementRef::wrap).next() {
            if scope.descendants().any(|node|node.id()==next.id()) && std::iter::once(next).chain(next.ancestors().filter_map(ElementRef::wrap)).all(|node|!matches!(node.value().name(),"a"|"form"|"button"|"input"|"textarea"|"select"|"script"|"style"))
                && links(next).is_empty() && next.select(&sel("form,button,input,textarea,select,script,style,[data-product-id],[data-item-id]")).next().is_none(){paired=Some(next);}
        }
        let Some(parent) = label.parent().and_then(ElementRef::wrap) else {
            break;
        };
        label = parent;
    }
    paired
}
fn template_summary_candidate(candidate: ElementRef<'_>) -> bool {
    if omitted_notice_scope(candidate) {
        return false;
    }
    !matches!(candidate.value().name(),"main"|"body"|"html") && candidate.descendants().count()<=256
        && noninteractive(candidate)
        && std::iter::once(candidate).chain(candidate.select(&sel("[data-product-id],[data-item-id]"))).all(|node|node.value().attr("data-product-id").is_none() && node.value().attr("data-item-id").is_none())
        && candidate.select(&sel("form,a,button,input,textarea,select,script,style,[data-test='downloadable'],[data-href*='/downloadables/']")).next().is_none()
        && candidate.select(&sel(".sheet")).filter(|node|node.select(&sel(".sheet")).next().is_none()).count()<=1
}
// Private observations are never serialized or included in diagnostics.
#[derive(Default)]
struct ReceiptSummaryObservation {
    number_labels: usize,
    date_labels: usize,
    role: bool,
    number_matches: bool,
    date: Option<String>,
    slash: bool,
    iso: bool,
    ambiguous: bool,
}
fn receipt_summary_observation(
    candidate: ElementRef<'_>,
    receipt_id: &str,
) -> ReceiptSummaryObservation {
    let mut result = ReceiptSummaryObservation::default();
    if !safety::numeric(receipt_id) || !template_summary_candidate(candidate) {
        return result;
    }
    let mut numbers = Vec::new();
    let mut ordered = Vec::new();
    for node in candidate.descendants() {
        let Some(raw) = node
            .value()
            .as_text()
            .filter(|raw| raw.chars().count() <= 40)
        else {
            continue;
        };
        let Some(label) = node
            .parent()
            .and_then(ElementRef::wrap)
            .filter(|parent| noninteractive(*parent))
        else {
            continue;
        };
        if order_number_label(raw) {
            numbers.push((label, template_literal(raw)));
        }
        if ordered_date_label(raw) {
            ordered.push((label, template_literal(raw)));
        }
    }
    result.number_labels = numbers.len();
    result.date_labels = ordered.len();
    if numbers.len() != 1 || ordered.len() != 1 {
        result.ambiguous = !numbers.is_empty() || !ordered.is_empty();
        return result;
    }
    result.role = true;
    let number =
        template_label_value(numbers[0].0, candidate, numbers[0].1).and_then(bounded_leaf_value);
    result.number_matches = number
        .as_deref()
        .map(|number| {
            number
                .trim()
                .strip_prefix('#')
                .unwrap_or(number.trim())
                .trim()
        })
        .is_some_and(|number| safety::numeric(number) && number == receipt_id);
    // Do not read a date value until the bounded summary proves this receipt.
    if !result.number_matches {
        result.ambiguous = true;
        return result;
    }
    let Some(value) =
        template_label_value(ordered[0].0, candidate, ordered[0].1).and_then(bounded_leaf_value)
    else {
        result.ambiguous = true;
        return result;
    };
    result.slash = slash_date_shape(&value);
    result.iso = chrono::DateTime::parse_from_rfc3339(&value).is_ok()
        || chrono::NaiveDate::parse_from_str(&value, "%Y-%m-%d").is_ok();
    if result.slash || result.iso {
        result.date = Some(value);
    } else {
        result.ambiguous = true;
    }
    result
}
fn receipt_summary_candidates<'a>(
    cards: &[ElementRef<'a>],
    digital: &[ElementRef<'a>],
) -> Vec<ElementRef<'a>> {
    let mut summaries = cards
        .iter()
        .copied()
        .filter(|candidate| template_summary_candidate(*candidate))
        .collect::<Vec<_>>();
    for card in digital {
        let mut current = Some(*card);
        for _ in 0..4 {
            let Some(node) = current else {
                break;
            };
            if matches!(node.value().name(), "main" | "body" | "html") {
                break;
            }
            current = node.parent().and_then(ElementRef::wrap);
            if let Some(candidate) = node.prev_siblings().filter_map(ElementRef::wrap).next() {
                if template_summary_candidate(candidate)
                    && !summaries.iter().any(|scope| scope.id() == candidate.id())
                {
                    summaries.push(candidate);
                }
            }
        }
    }
    summaries
}
fn receipt_acquired_date<'a>(
    cards: &[ElementRef<'a>],
    digital: ElementRef<'a>,
    receipt_id: &str,
) -> Option<String> {
    let mut dates = BTreeSet::new();
    for candidate in receipt_summary_candidates(cards, &[digital]) {
        let observation = receipt_summary_observation(candidate, receipt_id);
        if observation.ambiguous {
            return None;
        }
        if let Some(date) = observation.date {
            dates.insert(date);
        }
    }
    (dates.len() == 1).then(|| dates.into_iter().next().unwrap())
}
fn normalized_product_lines<'a>(
    price_parent: ElementRef<'a>,
    card: ElementRef<'a>,
    proof_rows: &[Row],
) -> Vec<ElementRef<'a>> {
    let mut result = Vec::new();
    for scope in std::iter::once(price_parent)
        .chain(price_parent.ancestors().filter_map(ElementRef::wrap))
        .take(7)
    {
        if scope.id() == card.id() || !card.descendants().any(|node| node.id() == scope.id()) {
            break;
        }
        if scope.descendants().count()>128 || !noninteractive(scope)
            || scope.select(&sel("form,button,input,textarea,select,script,style,[data-test='downloadable'],a[href*='/downloadables/'],[data-href*='/downloadables/']")).next().is_some(){continue;}
        let anchors = links(scope);
        let ids: BTreeSet<_> = anchors.iter().copied().filter_map(identity).collect();
        for row in proof_rows
            .iter()
            .filter(|row| product_field_file_keys(row).is_some())
        {
            let Some(id) = row.product_id.as_deref() else {
                continue;
            };
            let title = row.name.as_deref().unwrap_or("");
            if ids.len() != 1 || ids.first().map(String::as_str) != Some(id) {
                continue;
            }
            let known_links = anchors.iter().copied().all(|anchor| {
                identity(anchor).as_deref() == Some(id)
                    || anchor
                        .value()
                        .attr("href")
                        .and_then(absolute)
                        .and_then(|url| safety::public_url(&url, "shop"))
                        .is_some()
            });
            if known_links
                && anchors.iter().copied().any(|anchor| {
                    identity(anchor).as_deref() == Some(id) && text(anchor) == safety::short(title)
                })
            {
                result.push(scope);
                break;
            }
        }
    }
    result
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PaidLabel {
    Boost,
    Paid,
    Subtotal,
    Unit,
    Generic,
}
fn paid_label(value: &str) -> Option<PaidLabel> {
    match template_literal(value) {
        "BOOST" | "BOOST↑" | "BOOST金額" | "ブースト" => Some(PaidLabel::Boost),
        "支払金額" | "支払額" | "Paid amount" => Some(PaidLabel::Paid),
        "小計" | "Subtotal" => Some(PaidLabel::Subtotal),
        "単価" | "Unit price" => Some(PaidLabel::Unit),
        "価格" | "金額" | "商品価格" | "Price" | "Amount" | "Item price" => {
            Some(PaidLabel::Generic)
        }
        _ => None,
    }
}
// Read all direct DOM pieces, including text next to wrappers. A wrapper with a
// recognized leaf plus unknown direct text must never silently lose that text.
fn paid_dom_pieces(node: ElementRef<'_>) -> Option<Vec<String>> {
    use unicode_normalization::UnicodeNormalization;
    fn collect(node: ElementRef<'_>, depth: usize, result: &mut Vec<String>) -> Option<()> {
        if depth > 3
            || !matches!(
                node.value().name(),
                "div" | "span" | "b" | "strong" | "small" | "p" | "td" | "dd" | "label"
            )
        {
            return None;
        }
        for child in node.children() {
            if let Some(raw) = child.value().as_text() {
                if raw.chars().count() > 64
                    || raw.chars().any(|c| c.is_control() && !c.is_whitespace())
                {
                    return None;
                }
                let value = raw.nfkc().collect::<String>().trim().to_owned();
                if !value.is_empty() {
                    result.push(value);
                }
            } else if let Some(element) = ElementRef::wrap(child) {
                collect(element, depth + 1, result)?;
            }
            if result.len() > 8 {
                return None;
            }
        }
        Some(())
    }
    if !noninteractive(node) || node.descendants().count() > 128 {
        return None;
    }
    let mut result = Vec::new();
    collect(node, 0, &mut result)?;
    (result.iter().map(String::len).sum::<usize>() <= 128).then_some(result)
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProductLineFieldDiagnostics {
    version: usize,
    digital_bound_items: usize,
    unique_metadata_items: usize,
    unit_price_items: usize,
    explicit_quantity_items: usize,
    omitted_quantity_items: usize,
    explicit_boost_items: usize,
    product_line_proof_items: usize,
    ambiguous_items: usize,
    digital_control_binding_unproven_items: usize,
    metadata_shape_unrecognized_items: usize,
    unit_price_unrecognized_items: usize,
    quantity_unrecognized_items: usize,
    boost_unrecognized_items: usize,
}
// Only the needed product fields participate; unrelated receipt UI has no role.
struct ProductLineProof {
    diagnostics: ProductLineFieldDiagnostics,
    charges: Vec<ReceiptCharge>,
}
// This typed proof stays local; production rows receive only the exact bound amount.
struct ReceiptCharge {
    source_record_id: String,
    product_id: String,
    amount: u64,
}
fn apply_product_line_charges(rows: &mut [Row], proof: ProductLineProof, receipt_id: &str) {
    for charge in proof.charges {
        let matches: Vec<_> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                row.source_record_id == charge.source_record_id
                    && row.product_id.as_deref() == Some(charge.product_id.as_str())
                    && row.order_id.as_deref() == Some(receipt_id)
                    && product_field_file_keys(row).is_some()
            })
            .map(|(index, _)| index)
            .collect();
        if matches.len() == 1 && charge.amount <= 9_007_199_254_740_991 {
            rows[matches[0]].product_line_total_jpy = Some(charge.amount);
        }
    }
}
fn strict_contact_route(action: ElementRef<'_>) -> bool {
    // Only the positively nominated outside action supplies a URL. Other
    // anchors are rejected by presence without ever reading their href.
    let Some(link) = action
        .value()
        .attr("href")
        .and_then(absolute)
        .and_then(|value| url::Url::parse(&value).ok())
    else {
        return false;
    };
    let Some(segment) = link.path().strip_prefix("/conversations/") else {
        return false;
    };
    safety::secure(&link)
        && link.host_str() == Some("accounts.booth.pm")
        && link.fragment().is_none()
        && !segment.is_empty()
        && segment.len() <= 128
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}
fn known_contact_prefix(card: ElementRef<'_>) -> Option<ElementRef<'_>> {
    if !sel(".sheet").matches(&card) || card.select(&sel(".sheet")).next().is_some() {
        return None;
    }
    let mut node = card;
    for _ in 0..256 {
        if sel("div.text-14").matches(&node) {
            return None;
        }
        let children: Vec<_> = node.children().filter_map(ElementRef::wrap).collect();
        if children
            .first()
            .is_some_and(|child| child.value().name() == "p")
        {
            let instruction = children[0];
            let action = *children
                .get(1)
                .filter(|child| child.value().name() == "a")?;
            let body = *children.get(2)?;
            if instruction
                .children()
                .filter_map(ElementRef::wrap)
                .next()
                .is_some()
                || action
                    .children()
                    .filter_map(ElementRef::wrap)
                    .next()
                    .is_some()
            {
                return None;
            }
            let label = action.text().collect::<String>();
            if label.chars().count() > 40
                || label.trim() != "ショップに連絡する"
                || !strict_contact_route(action)
            {
                return None;
            }
            // The first P and next known action establish the outside prefix;
            // the following body is retained only as a node boundary.
            return Some(body);
        }
        if children.len() != 1 {
            return None;
        }
        node = children[0];
    }
    None
}
struct NoticePrefix<'a> {
    header: ElementRef<'a>,
}
fn bounded_notice_heading(node: ElementRef<'_>) -> bool {
    if node.descendants().count() > 32 {
        return false;
    }
    std::iter::once(node)
        .chain(node.descendants().filter_map(ElementRef::wrap))
        .all(|part| {
            matches!(
                part.value().name(),
                "div" | "span" | "b" | "strong" | "h1" | "h2"
            ) && part.children().filter_map(ElementRef::wrap).count() <= 1
                && !financial_attribute(part)
                && !part.value().attrs().any(|(key, _)| {
                    key.starts_with("data-") || matches!(key, "id" | "href" | "onclick" | "role")
                })
        })
}
fn known_notice_prefix(card: ElementRef<'_>) -> Option<NoticePrefix<'_>> {
    if !card.value().classes().any(|class| class == "sheet")
        || card.select(&sel(".sheet")).next().is_some()
    {
        return None;
    }
    // First capture the header/following-DIV-or-P boundary, then compare only the
    // bounded inline heading. A mismatch never searches siblings or body.
    let mut node = card;
    for _ in 0..256 {
        // A known body-only wrapper cannot supply a heading from its contents.
        if node.value().name() == "div" && node.value().classes().any(|class| class == "text-14") {
            return None;
        }
        let children: Vec<_> = node.children().filter_map(ElementRef::wrap).collect();
        if children.len() >= 2 {
            let header = children[0];
            let body = children[1];
            if !matches!(body.value().name(), "div" | "p")
                || (body.value().name() == "p"
                    && (header.value().name() != "b"
                        || header
                            .children()
                            .filter_map(ElementRef::wrap)
                            .next()
                            .is_some()))
                || !bounded_notice_heading(header)
            {
                return None;
            }
            let literal = header.text().collect::<String>();
            return (literal.chars().count() <= 40 && literal.trim() == "発送通知コメント")
                .then_some(NoticePrefix { header });
        }
        if children.len() != 1
            || node
                .children()
                .any(|child| matches!(child.value(), scraper::Node::Text(_)))
            || !matches!(children[0].value().name(), "div" | "span")
        {
            return None;
        }
        node = children[0];
    }
    None
}
fn known_notice_header(card: ElementRef<'_>) -> Option<ElementRef<'_>> {
    known_notice_prefix(card).map(|prefix| prefix.header)
}
fn strict_whole_jpy(value: &str, currency_required: bool) -> Option<u64> {
    let amount = if currency_required {
        currency_jpy(value)?
    } else {
        super::metadata::parse_jpy_price(&serde_json::json!(value))?
    };
    Some(amount as u64)
}
fn receipt_boost_amount(node: ElementRef<'_>) -> Option<u64> {
    use unicode_normalization::UnicodeNormalization;
    fn collect(
        node: ElementRef<'_>,
        depth: usize,
        parts: &mut Vec<String>,
        icons: &mut usize,
    ) -> Option<()> {
        if depth > 3 || !noninteractive(node) {
            return None;
        }
        if node.value().classes().any(|class| class == "icon-boost") {
            if !matches!(node.value().name(), "i" | "span" | "pixiv-icon" | "svg") {
                return None;
            }
            let value = node.text().collect::<String>();
            if !value.trim().is_empty() && paid_label(&value) != Some(PaidLabel::Boost) {
                return None;
            }
            *icons += 1;
            return Some(());
        }
        if !matches!(
            node.value().name(),
            "div" | "span" | "b" | "strong" | "small" | "p" | "label"
        ) {
            return None;
        }
        for child in node.children() {
            if let Some(raw) = child.value().as_text() {
                if raw.chars().count() > 64
                    || raw.chars().any(|c| c.is_control() && !c.is_whitespace())
                {
                    return None;
                }
                let value = raw.nfkc().collect::<String>().trim().to_owned();
                if !value.is_empty() {
                    parts.push(value);
                }
            } else if let Some(element) = ElementRef::wrap(child) {
                collect(element, depth + 1, parts, icons)?;
            }
            if parts.len() > 8 || *icons > 1 {
                return None;
            }
        }
        Some(())
    }
    if node.descendants().count() > 128 {
        return None;
    }
    let mut parts = Vec::new();
    let mut icons = 0;
    collect(node, 0, &mut parts, &mut icons)?;
    let mut value = parts.join(" ");
    let mut labels = 0;
    for literal in ["BOOST↑", "BOOST金額", "BOOST", "ブースト"] {
        if let Some(rest) = value.strip_prefix(literal).filter(|rest| {
            rest.is_empty()
                || rest.starts_with(char::is_whitespace)
                || rest.starts_with([':', '：'])
        }) {
            value = rest
                .trim()
                .trim_start_matches([':', '：'])
                .trim()
                .to_owned();
            labels += 1;
            break;
        }
        if let Some(rest) = value
            .strip_suffix(literal)
            .filter(|rest| rest.ends_with(char::is_whitespace))
        {
            value = rest.trim().to_owned();
            labels += 1;
            break;
        }
    }
    if icons > 1 || labels > 1 || (icons == 0 && labels != 1) || value.len() > 128 {
        return None;
    }
    strict_whole_jpy(&value, false)
}
fn receipt_quantity(node: ElementRef<'_>) -> Option<u64> {
    let parts = paid_dom_pieces(node)?;
    let joined = parts.join(" ");
    let mut value = joined.trim();
    for label in ["数量", "個数", "Quantity", "×", "x"] {
        if let Some(rest) = value.strip_prefix(label) {
            value = rest.trim().trim_start_matches([':', '：']).trim();
            break;
        }
    }
    if value.is_empty() || !value.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    value
        .parse::<u64>()
        .ok()
        .filter(|quantity| *quantity > 0 && *quantity <= 9_007_199_254_740_991)
}
fn receipt_item_metadata<'a>(card: ElementRef<'a>, row: &Row) -> Option<Vec<ElementRef<'a>>> {
    let id = row.product_id.as_deref()?;
    let title = row.name.as_deref().unwrap_or("");
    let mut metadata = Vec::new();
    for anchor in card.select(&sel("b a[href]")) {
        if identity(anchor).as_deref() != Some(id) || text(anchor) != safety::short(title) {
            continue;
        }
        let Some(title_div) = anchor
            .ancestors()
            .filter_map(ElementRef::wrap)
            .find(|node| node.value().name() == "div")
        else {
            continue;
        };
        let Some(scope) = title_div.parent().and_then(ElementRef::wrap) else {
            continue;
        };
        if scope.id() == card.id()
            || scope.value().name() != "div"
            || !card.descendants().any(|node| node.id() == scope.id())
            || !normalized_product_lines(scope, card, std::slice::from_ref(row))
                .iter()
                .any(|line| line.id() == scope.id())
        {
            continue;
        }
        let children: Vec<_> = scope.children().filter_map(ElementRef::wrap).collect();
        if !(children.len() == 3 || children.len() == 4)
            || children.iter().any(|child| child.value().name() != "div")
            || children[0].id() != title_div.id()
            || !scope
                .children()
                .filter_map(|node| node.value().as_text())
                .all(|raw| raw.trim().is_empty())
        {
            continue;
        }
        if !metadata
            .iter()
            .any(|(seen, _): &(ElementRef<'_>, Vec<ElementRef<'_>>)| seen.id() == scope.id())
        {
            metadata.push((scope, children));
        }
    }
    (metadata.len() == 1).then(|| metadata.remove(0).1)
}
fn product_line_proof_document(
    doc: &Html,
    rows: &[Row],
    receipt_id: &str,
) -> Result<ProductLineProof> {
    if doc
        .select(&sel(
            "input[type='password'],form[action*='sign_in'],form[action*='login']",
        ))
        .next()
        .is_some()
    {
        bail!("AUTH_REQUIRED");
    }
    if doc
        .select(&sel("#challenge-form,.cf-challenge,iframe[src*='captcha']"))
        .next()
        .is_some()
    {
        bail!("AUTH_CHALLENGE");
    }
    let main = doc
        .select(&sel("main,[role='main']"))
        .next()
        .unwrap_or_else(|| doc.root_element());
    // Unknown regions without file controls never enter product-field binding.
    let cards: Vec<_> = main
        .select(&sel(".sheet"))
        .filter(|card| {
            card.select(&sel(".sheet")).next().is_none() && raw_download_controls(*card, true) != 0
        })
        .collect();
    let matching: Vec<_> = rows
        .iter()
        .filter(|row| {
            row.order_id.as_deref() == Some(receipt_id)
                && !row.source_record_id.is_empty()
                && product_field_file_keys(row).is_some()
        })
        .collect();
    let mut result = ProductLineProof {
        diagnostics: ProductLineFieldDiagnostics {
            version: 1,
            digital_control_binding_unproven_items: rows
                .iter()
                .filter(|row| row.order_id.as_deref() == Some(receipt_id))
                .count()
                .saturating_sub(matching.len()),
            ..Default::default()
        },
        charges: Vec::new(),
    };
    result.diagnostics.digital_bound_items = cards
        .iter()
        .filter(|card| {
            matching
                .iter()
                .any(|row| rebound_product_fields(**card, std::slice::from_ref(*row)))
        })
        .count();
    for row in &matching {
        let id = row.product_id.as_deref().unwrap();
        let keys = product_field_file_keys(row).unwrap();
        let siblings: Vec<_> = rows
            .iter()
            .filter(|other| {
                other.order_id.as_deref() == Some(receipt_id)
                    && other.product_id.as_deref() == Some(id)
            })
            .collect();
        // Separate rows of the same product are valid only when each row has
        // its own exact single variation and disjoint, rebound file controls.
        // No variant-name matching or aggregate amount allocation is involved.
        let mut variants = BTreeSet::new();
        let mut records = BTreeSet::new();
        let mut sibling_files = BTreeSet::new();
        let distinct_variants = siblings.iter().all(|other| {
            let Some(variant) = other.files.first().and_then(|f| f.variant_id.as_deref()) else {
                return false;
            };
            if !safety::numeric(variant)
                || !variants.insert(variant)
                || other.source_record_id.is_empty()
                || !records.insert(&other.source_record_id)
                || other
                    .files
                    .iter()
                    .any(|f| f.variant_id.as_deref() != Some(variant))
            {
                return false;
            }
            product_field_file_keys(other)
                .is_some_and(|files| files.into_iter().all(|file| sibling_files.insert(file)))
        });
        let duplicate = rows
            .iter()
            .filter(|other| {
                other.order_id.as_deref() == Some(receipt_id)
                    && other.source_record_id == row.source_record_id
            })
            .count()
            != 1
            || (siblings.len() != 1 && !distinct_variants);
        let conflicting_files = matching.iter().any(|other| {
            other.source_record_id != row.source_record_id
                && product_field_file_keys(other)
                    .is_some_and(|other_keys| !keys.is_disjoint(&other_keys))
        });
        let bound: Vec<_> = cards
            .iter()
            .copied()
            .filter(|card| rebound_product_fields(*card, std::slice::from_ref(*row)))
            .collect();
        if duplicate || conflicting_files || bound.len() != 1 {
            result.diagnostics.ambiguous_items += 1;
            result.diagnostics.digital_control_binding_unproven_items += 1;
            continue;
        }
        let Some(fields) = receipt_item_metadata(bound[0], row) else {
            result.diagnostics.ambiguous_items += 1;
            result.diagnostics.metadata_shape_unrecognized_items += 1;
            continue;
        };
        let scope = fields[0].parent().and_then(ElementRef::wrap).unwrap();
        if links(fields[0]).len() != 1
            || !closed_title(fields[0], row)
            || std::iter::once(scope)
                .chain(scope.descendants().filter_map(ElementRef::wrap))
                .any(financial_attribute)
        {
            result.diagnostics.ambiguous_items += 1;
            result.diagnostics.metadata_shape_unrecognized_items += 1;
            continue;
        }
        result.diagnostics.unique_metadata_items += 1;
        let unit =
            paid_dom_pieces(fields[1]).and_then(|parts| strict_whole_jpy(&parts.join(" "), true));
        let boost = receipt_boost_amount(*fields.last().unwrap());
        let quantity = if fields.len() == 4 {
            let quantity = receipt_quantity(fields[2]);
            result.diagnostics.explicit_quantity_items += usize::from(quantity.is_some());
            quantity
        } else {
            result.diagnostics.omitted_quantity_items += 1;
            None
        };
        result.diagnostics.unit_price_items += usize::from(unit.is_some());
        result.diagnostics.explicit_boost_items += usize::from(boost.is_some());
        result.diagnostics.unit_price_unrecognized_items += usize::from(unit.is_none());
        result.diagnostics.quantity_unrecognized_items +=
            usize::from(fields.len() == 4 && quantity.is_none());
        result.diagnostics.boost_unrecognized_items += usize::from(boost.is_none());
        let amount = match (unit, boost, quantity) {
            (Some(unit), Some(boost), Some(quantity)) => unit
                .checked_mul(quantity)
                .and_then(|price| price.checked_add(boost)),
            // Official current BOOTH cart omits its quantity spinner for digital
            // goods. Infer one only for this recognized three-field metadata
            // grammar and a freshly rebound digital control proof.
            (Some(unit), Some(boost), None) if fields.len() == 3 => unit.checked_add(boost),
            _ => None,
        }
        .filter(|amount| *amount <= 9_007_199_254_740_991);
        let Some(amount) = amount else {
            result.diagnostics.ambiguous_items += 1;
            continue;
        };
        result.charges.push(ReceiptCharge {
            source_record_id: row.source_record_id.clone(),
            product_id: id.to_owned(),
            amount,
        });
    }
    result.diagnostics.product_line_proof_items = result.charges.len();
    Ok(result)
}
fn covered_by(node: ElementRef<'_>, roles: &[ElementRef<'_>]) -> bool {
    roles.iter().any(|role| {
        role.id() == node.id() || role.descendants().any(|child| child.id() == node.id())
    })
}
fn omitted_notice_scope(candidate: ElementRef<'_>) -> bool {
    let known = |card| known_notice_header(card).is_some() || known_contact_prefix(card).is_some();
    known(candidate)
        || candidate
            .ancestors()
            .filter_map(ElementRef::wrap)
            .find(|node| node.value().classes().any(|class| class == "sheet"))
            .is_some_and(known)
        || candidate.select(&sel(".sheet")).any(known)
}
fn no_unconsumed_text(scope: ElementRef<'_>, roles: &[ElementRef<'_>]) -> bool {
    scope.descendants().all(|node| {
        node.parent()
            .and_then(ElementRef::wrap)
            .is_some_and(|parent| covered_by(parent, roles))
            || !node
                .value()
                .as_text()
                .is_some_and(|raw| !raw.trim().is_empty())
    })
}
fn financial_attribute(node: ElementRef<'_>) -> bool {
    [
        "data-price",
        "data-amount",
        "data-total",
        "data-quantity",
        "data-paid-amount",
        "data-unit-price",
        "data-line-total",
        "data-subtotal",
        "data-discount",
        "data-fee",
    ]
    .iter()
    .any(|key| node.value().attr(key).is_some())
}
fn closed_title<'a>(title: ElementRef<'a>, row: &Row) -> bool {
    let mut roles = Vec::new();
    for anchor in links(title) {
        if identity(anchor).as_deref() != row.product_id.as_deref()
            || text(anchor) != row.name.as_deref().map(safety::short).unwrap_or_default()
        {
            return false;
        }
        roles.push(anchor);
    }
    !roles.is_empty()
        && no_unconsumed_text(title, &roles)
        && title
            .select(&sel(
                "form,button,input,textarea,select,script,style,[data-href]",
            ))
            .next()
            .is_none()
        && std::iter::once(title)
            .chain(title.descendants().filter_map(ElementRef::wrap))
            .all(|node| !financial_attribute(node))
}
#[derive(Default)]
pub(super) struct FileStructureDiagnostics {
    valid_controls: usize,
    modern_name_controls: usize,
    test_name_controls: usize,
    legacy_name_controls: usize,
    track_name_controls: usize,
    unrecognized_entry_controls: usize,
    recognized_entries: usize,
    rows_all_entries_recognized: usize,
    rows_file_only_container: usize,
    rows_card_only: usize,
    orphan_modern_names: usize,
    rows_with_orphan_modern_name: usize,
    rows_conflicting_entry_scopes: usize,
    rows_other_download_methods: usize,
    rows_expand_or_load_more: usize,
    rows_variant_selector: usize,
    rows_unknown_dropdown: usize,
    rows_advertised_count: usize,
    rows_advertised_count_matches: usize,
    rows_advertised_count_mismatch: usize,
    rows_advertised_count_invalid: usize,
}
impl FileStructureDiagnostics {
    fn modern_list_witness(card: ElementRef<'_>, files: &[File]) -> bool {
        let mut evidence = Self::default();
        evidence.record(card, files);
        evidence.valid_controls > 0
            && evidence.modern_name_controls + evidence.track_name_controls
                == evidence.valid_controls
            && evidence.test_name_controls == 0
            && evidence.legacy_name_controls == 0
            && evidence.unrecognized_entry_controls == 0
            && evidence.rows_all_entries_recognized == 1
            && evidence.rows_file_only_container == 1
            && evidence.orphan_modern_names == 0
            && evidence.rows_conflicting_entry_scopes == 0
            && evidence.rows_expand_or_load_more == 0
            && evidence.rows_variant_selector == 0
            && evidence.rows_unknown_dropdown == 0
            && evidence.rows_advertised_count_invalid == 0
            && evidence.rows_advertised_count_mismatch == 0
            && (evidence.rows_advertised_count == 0 || evidence.rows_advertised_count_matches == 1)
    }
    fn record(&mut self, card: ElementRef<'_>, files: &[File]) {
        let controls = downloads(card, false);
        self.valid_controls += controls.len();
        let mut entries = BTreeMap::new();
        let mut recognized = 0;
        let mut accepted_modern_names = vec![];
        let mut conflicting_entries = false;
        let mut album = false;
        for (button, id, variant, _) in &controls {
            let mut found = None;
            for scope in button.ancestors().filter_map(ElementRef::wrap).take(4) {
                if scope.id() == card.id() || !card.descendants().any(|n| n.id() == scope.id()) {
                    break;
                }
                let keys: BTreeSet<_> = downloads(scope, false)
                    .into_iter()
                    .map(|(_, id, variant, _)| (id, variant))
                    .collect();
                if keys.len() != 1 {
                    break;
                }
                // A bound track menu names its own file; its row holds the
                // alternate-app menu that bound it.
                if is_track(*button) {
                    if variant.is_some()
                        && track_download(*button).is_some()
                        && scope
                            .select(&sel("[data-test='other-downloads-button']"))
                            .next()
                            .is_some()
                    {
                        found = Some((scope, 3, button.id()));
                        break;
                    }
                    continue;
                }
                let test: Vec<_> = scope.select(&sel("[data-test='filename']")).collect();
                let modern: Vec<_> = scope
                    .select(&sel(".min-w-0.break-words.whitespace-pre-line .text-14"))
                    .collect();
                let legacy: Vec<_> = if scope.value().classes().any(|c| c == "legacy-list-item") {
                    scope
                        .select(&sel(".min-w-0 > b"))
                        .filter(|n| {
                            n.select(&sel("a,button,input,textarea,select"))
                                .next()
                                .is_none()
                                && looks_filename(&text(*n))
                        })
                        .collect()
                } else {
                    vec![]
                };
                let (names, kind) = if !test.is_empty() {
                    (test, 0)
                } else if !modern.is_empty() {
                    (modern, 1)
                } else {
                    (legacy, 2)
                };
                if names.len() > 1 {
                    break;
                }
                if names.len() == 1 {
                    if !text(names[0]).is_empty() {
                        found = Some((scope, kind, names[0].id()));
                    }
                    break;
                }
            }
            if let Some((scope, kind, name_id)) = found {
                recognized += 1;
                match kind {
                    0 => self.test_name_controls += 1,
                    1 => {
                        self.modern_name_controls += 1;
                        accepted_modern_names.push(name_id);
                    }
                    3 => {
                        self.track_name_controls += 1;
                        album = true;
                    }
                    _ => self.legacy_name_controls += 1,
                }
                let key = (id.clone(), variant.clone());
                if entries
                    .get(&key)
                    .is_some_and(|old: &ElementRef<'_>| old.id() != scope.id())
                {
                    conflicting_entries = true;
                }
                entries.entry(key).or_insert(scope);
            } else {
                self.unrecognized_entry_controls += 1;
            }
        }
        self.recognized_entries += entries.len();
        let all = !files.is_empty() && recognized == controls.len() && entries.len() == files.len();
        self.rows_all_entries_recognized += usize::from(all);
        // Album track rows nest deeper than plain file rows; their file-only
        // container is searched up to the card.
        let container = if all {
            entries.values().next().and_then(|first| {
                first
                    .ancestors()
                    .filter_map(ElementRef::wrap)
                    .take(if album { usize::MAX } else { 4 })
                    .take_while(|n| n.id() != card.id())
                    .filter(|n| {
                        matches!(n.value().name(), "div" | "ul" | "ol" | "section")
                            && links(*n).into_iter().all(|a| identity(a).is_none())
                            && downloads(*n, false).len() == controls.len()
                            && entries
                                .values()
                                .all(|e| n.descendants().any(|d| d.id() == e.id()))
                    })
                    .last()
            })
        } else {
            None
        };
        // Album and plain file sections can be separate children of the card;
        // then each section holding entries must itself be file-only.
        let sections = all && album && container.is_none() && {
            let sections: Vec<_> = card
                .children()
                .filter_map(ElementRef::wrap)
                .filter(|child| {
                    entries
                        .values()
                        .any(|entry| child.descendants().any(|d| d.id() == entry.id()))
                })
                .collect();
            sections
                .iter()
                .all(|section| links(*section).into_iter().all(|a| identity(a).is_none()))
                && sections
                    .iter()
                    .map(|section| downloads(*section, false).len())
                    .sum::<usize>()
                    == controls.len()
        };
        self.rows_file_only_container += usize::from(container.is_some() || sections);
        self.rows_card_only += usize::from(all && container.is_none() && !sections);
        self.rows_conflicting_entry_scopes += usize::from(conflicting_entries);
        // A same-product anchor can disqualify an outer wrapper while leaving a
        // smaller eligible container. Do not let it hide a sibling filename row.
        let orphan_names = card
            .select(&sel(".min-w-0.break-words.whitespace-pre-line .text-14"))
            .filter(|name| !accepted_modern_names.contains(&name.id()))
            .count();
        self.orphan_modern_names += orphan_names;
        self.rows_with_orphan_modern_name += usize::from(orphan_names > 0);
        self.rows_other_download_methods += usize::from(
            card.select(&sel("[data-test='other-downloads-button']"))
                .next()
                .is_some(),
        );
        self.rows_expand_or_load_more+=usize::from(card.select(&sel("[data-test='load-more'],[data-test='show-more'],[data-test='more-downloadables'],button[aria-expanded='false']:not([data-test='other-downloads-button'])")).next().is_some());
        self.rows_variant_selector+=usize::from(card.select(&sel("[data-test='variation-select'],[data-test='variation-selector'],select[name='variation_id'],select[name='variation']")).next().is_some());
        let tracks: BTreeSet<_> = controls
            .iter()
            .filter(|(node, ..)| is_track(*node))
            .map(|(_, id, ..)| id.clone())
            .collect();
        self.rows_unknown_dropdown +=
            usize::from(card.select(&sel(dropdown_controls(false))).any(|node| {
                !track_bundle(node, &tracks)
                    && !controls
                        .iter()
                        .any(|(control, ..)| is_track(*control) && control.id() == node.id())
            }));
        let mut advertised = vec![];
        for node in std::iter::once(card).chain(card.select(&sel(
            "[data-file-count],[data-downloadable-count],[data-total-count]",
        ))) {
            for attr in [
                "data-file-count",
                "data-downloadable-count",
                "data-total-count",
            ] {
                if let Some(value) = node.value().attr(attr) {
                    advertised.push(value.parse::<usize>().ok());
                }
            }
        }
        self.rows_advertised_count += usize::from(!advertised.is_empty());
        self.rows_advertised_count_matches += usize::from(
            !advertised.is_empty() && advertised.iter().all(|v| *v == Some(files.len())),
        );
        self.rows_advertised_count_mismatch += usize::from(
            advertised
                .iter()
                .any(|v| v.is_some_and(|n| n != files.len())),
        );
        self.rows_advertised_count_invalid += usize::from(advertised.iter().any(Option::is_none));
    }
}
pub fn parse(html: &str, source: &str, route: &str) -> Result<Page> {
    let url = safety::route(route, source)?;
    let source = safety::source_name(source)?.to_owned();
    let path = safety::source_path(&source)?;
    let is_detail = path == "/orders" && url.path() != path;
    let doc = Html::parse_document(html);
    crate::support_trace::parser_language(&source, html_language(&doc));
    recognize_page(&doc, &source, path, is_detail)?;
    let main = doc
        .select(&sel("main,[role='main']"))
        .next()
        .unwrap_or_else(|| doc.root_element());
    let cards = product_cards(main, &source, is_detail);
    let mut page = Page {
        source: source.clone(),
        route: format!(
            "{}{}",
            url.path(),
            url.query().map(|q| format!("?{q}")).unwrap_or_default()
        ),
        rows: vec![],
        next: vec![],
        details: vec![],
        terminal: false,
        unresolved: 0,
        fingerprint: String::new(),
        is_detail,
        ordering: ordering(&doc, &source, is_detail),
        free_item: None,
    };
    let mut occurrences: BTreeMap<String, usize> = BTreeMap::new();
    for (ordinal, card) in cards.iter().copied().enumerate() {
        if source == "purchase" && !is_detail {
            order_detail(&mut page, card);
            continue;
        }
        read_card(
            &mut page,
            &mut occurrences,
            card,
            ordinal,
            &cards,
            &source,
            is_detail,
            &url,
            route,
        );
    }
    read_pager(
        &mut page, &doc, main, &url, path, &source, is_detail, &cards,
    )?;
    if source == "purchase" && is_detail {
        trace_receipt(&mut page, &doc, &url);
    }
    page.details.sort();
    page.details.dedup();
    // Include actual file facts, never signing parameters, in restart-boundary evidence.
    page.fingerprint = safety::hash(serde_json::to_vec(
        &serde_json::json!({"rows":page.rows.iter().map(|r|(&r.source_record_id,&r.product_id,r.files.iter().map(|f|(&f.id,&f.variant_id,&f.revision,&f.name,f.size_bytes)).collect::<Vec<_>>())).collect::<Vec<_>>(),"next":page.next,"details":page.details,"terminal":page.terminal,"unresolved":page.unresolved,"ordering":page.ordering}),
    )?);
    Ok(page)
}

/// Rejects a signed-out, challenged or unrecognized page before anything is read from it.
fn recognize_page(doc: &Html, source: &str, path: &str, is_detail: bool) -> Result<()> {
    // The site's anonymous OAuth page has no password/login form. Its explicit
    // signed-out marker must also reject an otherwise recognizable empty shelf.
    if doc
        .select(&sel("meta[name='js_const_user_signed_in']"))
        .any(|meta| meta.value().attr("content") == Some("false"))
        || doc
            .select(&sel(
                "input[type='password'],form[action*='sign_in'],form[action*='login']",
            ))
            .next()
            .is_some()
    {
        bail!("AUTH_REQUIRED");
    }
    if doc
        .select(&sel("#challenge-form,.cf-challenge,iframe[src*='captcha']"))
        .next()
        .is_some()
    {
        bail!("AUTH_CHALLENGE");
    }
    let heading = if is_detail {
        String::new()
    } else {
        doc.select(&sel("h1,h2,[data-test='page-title']"))
            .map(text)
            .collect::<Vec<_>>()
            .join(" ")
    };
    let marker = is_detail
        || match source {
            "library" => heading.contains("ライブラリ") || heading.contains("Library"),
            "purchase" => heading.contains("注文") || heading.contains("Order"),
            "gift" => heading.contains("ギフト") || heading.contains("Gift"),
            "free" => {
                heading.contains("無料")
                    || heading.contains("Free")
                    || heading.contains("ダウンロード")
            }
            _ => false,
        }
        || doc.select(&sel("a[href]")).any(|a| {
            a.value()
                .attr("href")
                .and_then(absolute)
                .and_then(|u| url::Url::parse(&u).ok())
                .is_some_and(|u| u.host_str() == Some("accounts.booth.pm") && u.path() == path)
        })
        // An order list with nothing in it has no link to itself and a translated heading.
        || (source == "purchase" && doc.select(&sel(".l-orders-index")).next().is_some());
    if !marker {
        bail!("SOURCE_MARKER_UNKNOWN");
    }
    Ok(())
}

/// The page's product cards, or each product anchor's own repeated component.
fn product_cards<'a>(main: ElementRef<'a>, source: &str, is_detail: bool) -> Vec<ElementRef<'a>> {
    let selector = if is_detail {
        ".sheet"
    } else if source == "purchase" {
        "[data-test='order'],.order-list-item,.order-item,.sheet"
    } else {
        "[data-test='library-item'],.l-library-item,.bg-white.p-16"
    };
    let mut cards: Vec<_> = main
        .select(&sel(selector))
        .filter(|e| e.select(&sel(selector)).next().is_none())
        .collect();
    // Digital-only fallback: scope each product anchor to one repeated component.
    if cards.is_empty() {
        for a in main.select(&sel("a[href]")) {
            let target = if source == "purchase" && !is_detail {
                order_id(a)
            } else {
                identity(a)
            };
            if target.is_none() {
                continue;
            }
            let mut picked = None;
            for n in a.ancestors().filter_map(ElementRef::wrap) {
                if n.id() == main.id()
                    || matches!(
                        n.value().name(),
                        "body" | "html" | "nav" | "header" | "footer"
                    )
                {
                    break;
                }
                let ids: BTreeSet<_> = links(n)
                    .into_iter()
                    .filter_map(|a| {
                        if source == "purchase" && !is_detail {
                            order_id(a)
                        } else {
                            identity(a)
                        }
                    })
                    .collect();
                if ids.len() > 1 {
                    break;
                }
                picked = Some(n);
                if matches!(n.value().name(), "article" | "li" | "tr")
                    || n.value().classes().any(|c| matches!(c, "sheet" | "p-16"))
                {
                    break;
                }
            }
            if let Some(n) = picked {
                if !cards.iter().any(|e| e.id() == n.id()) {
                    cards.push(n);
                }
            }
        }
    }
    cards
}

/// An order-list card names one order whose receipt is read next.
fn order_detail(page: &mut Page, card: ElementRef<'_>) {
    let mut ids: BTreeSet<_> = links(card).into_iter().filter_map(order_id).collect();
    if let Some(id) = enclosing_order(card) {
        ids.insert(id);
    }
    if ids.len() == 1 {
        let id = ids.into_iter().next().unwrap();
        page.details.push(format!("/orders/{id}"));
    } else {
        page.unresolved += 1;
    }
}

/// One card's row: its product, files, record identity and acquisition date.
fn read_card<'a>(
    page: &mut Page,
    occurrences: &mut BTreeMap<String, usize>,
    card: ElementRef<'a>,
    ordinal: usize,
    cards: &[ElementRef<'a>],
    source: &str,
    is_detail: bool,
    url: &url::Url,
    route: &str,
) {
    let receipt = source == "purchase" && is_detail;
    let raw_controls = raw_download_controls(card, receipt);
    if is_detail && (raw_controls == 0 || omitted_notice_scope(card)) {
        return;
    }
    let anchors = links(card);
    let ids: BTreeSet<_> = anchors.iter().copied().filter_map(identity).collect();
    let f = files(card, receipt);
    let unavailable = unavailable(card);
    // Do not inspect text of order summaries, physical goods, comments or contacts.
    if is_detail && f.is_empty() && !unavailable {
        unresolved_controls(page, raw_controls, url, ordinal);
        return;
    }
    let product_id = if ids.len() == 1 {
        ids.into_iter().next()
    } else {
        None
    };
    let mut error_code =
        if product_id.is_none() && unavailable && f.is_empty() && !numeric_identity_present(card) {
            Some("BOOTH_WITHHELD_PRODUCT_ID".to_owned())
        } else if product_id.is_none() {
            Some("PRODUCT_ID_UNRESOLVED".to_owned())
        } else {
            None
        };
    if error_code.as_deref() == Some("PRODUCT_ID_UNRESOLVED") {
        page.unresolved += 1;
    }
    let title = first_text(card,"[data-test='item-name'],.font-bold.text-16.break-all,.text-text-default.font-bold,b a.nav[href*='/items/']").or_else(||anchors.iter().copied().find(|a|product_id.is_some() && identity(*a)==product_id && !text(*a).is_empty()).map(text));
    if product_id.is_some() && title.is_none() && !unavailable {
        error_code = Some("PRODUCT_TITLE_UNRESOLVED".into());
        page.unresolved += 1;
    }
    if raw_controls != downloads(card, receipt).len() {
        error_code = Some("DIGITAL_CONTROLS_UNRESOLVED".into());
        page.unresolved += 1;
    }
    let shop = shop_anchor(&anchors);
    let img = card
        .select(&sel(
            "img.l-library-item-thumbnail,img[src*='pximg'],[data-test='item-thumbnail']",
        ))
        .next();
    let explicit_list = explicit_file_list(card);
    let manifest_complete = !f.is_empty()
        && error_code.is_none()
        && !unavailable
        && raw_controls == downloads(card, receipt).len()
        && f.iter().all(|f| f.name.is_some())
        && (explicit_list
            || (matches!(source, "library" | "gift" | "free")
                && FileStructureDiagnostics::modern_list_witness(card, &f)));
    let positive_purchase = source == "purchase"
        && is_detail
        && error_code
            .as_deref()
            .is_none_or(|code| code == "PRODUCT_TITLE_UNRESOLVED")
        && rebound_product_fields(
            card,
            &[Row {
                product_id: product_id.clone(),
                files: f.clone(),
                manifest_complete,
                ..Row::default()
            }],
        );
    // Retain the previous direct-card component for existing native IDs only.
    // A receipt-summary date never enters this legacy identity component.
    // Preserve previously established native keys; a newly recoverable
    // financial field must not change an existing undated acquisition ID.
    let legacy_date = if positive_purchase {
        card.select(&sel("time[datetime],[data-test='downloaded-at'],[data-test='acquired-at'],[data-test='ordered-at']")).next().map(|n|n.value().attr("datetime").map(safety::short).unwrap_or_else(||text(n)))
    } else {
        None
    };
    let mut date = if source == "purchase" && is_detail {
        None
    } else {
        card.select(&sel("time[datetime],[data-test='downloaded-at'],[data-test='acquired-at'],[data-test='ordered-at']")).next().map(|n|n.value().attr("datetime").map(safety::short).unwrap_or_else(||text(n)))
    };
    let line = card
        .value()
        .attr("data-order-item-id")
        .or_else(|| card.value().attr("data-line-item-id"))
        .filter(|v| safety::numeric(v));
    let order_id = if is_detail {
        url.path().strip_prefix("/orders/").map(str::to_owned)
    } else {
        None
    };
    let variant_name = first_text(card, "[data-test='variation-name'],.variation-name");
    let variant_identity = card
        .value()
        .attr("data-variation-id")
        .filter(|v| safety::numeric(v))
        .unwrap_or("");
    let record_key = line.map(|v| format!("line:{v}")).unwrap_or_else(|| {
        format!(
            "{}:{}:{}:{}:{}",
            order_id.as_deref().unwrap_or(""),
            product_id.as_deref().unwrap_or("unknown"),
            variant_identity,
            // Preserve both previously dated and undated native purchase IDs.
            if source == "purchase" && is_detail {
                legacy_date.as_deref().unwrap_or("")
            } else {
                date.as_deref().unwrap_or("")
            },
            if product_id.is_none() {
                format!("{route}:{ordinal}")
            } else {
                String::new()
            }
        )
    });
    let base_record_id = format!("{source}:{}", safety::hash(record_key));
    let occurrence = occurrences.entry(base_record_id.clone()).or_default();
    let source_record_id = if *occurrence == 0 {
        base_record_id
    } else {
        format!("{base_record_id}:occurrence:{occurrence}")
    };
    *occurrence += 1;
    if positive_purchase {
        date = order_id
            .as_deref()
            .and_then(|id| receipt_acquired_date(cards, card, id));
        if legacy_date
            .as_ref()
            .is_some_and(|legacy| date.as_ref().is_some_and(|date| date != legacy))
        {
            date = None;
        }
    }
    page.rows.push(Row {
        source_record_id,
        product_id,
        order_id,
        name: title,
        shop: shop.map(text),
        shop_url: shop
            .and_then(|a| a.value().attr("href"))
            .and_then(absolute)
            .and_then(|v| safety::public_url(&v, "shop")),
        preview_url: img
            .and_then(|e| e.value().attr("src").or_else(|| e.value().attr("data-src")))
            .and_then(absolute)
            .and_then(|v| safety::public_url(&v, "preview")),
        acquired_date_text: date,
        acquired_at: None,
        paid_amount_text: first_text(card, "[data-test='paid-amount']"),
        product_line_total_jpy: None,
        variant_name,
        files: f,
        manifest_complete,
        unavailable,
        error_code,
    });
}

/// Download controls without readable files leave the receipt unresolved.
fn unresolved_controls(page: &mut Page, raw_controls: usize, url: &url::Url, ordinal: usize) {
    // Ignore unrelated receipt regions. Invalid digital candidates still
    // prevent a complete traversal, without inspecting their body.
    if raw_controls != 0 {
        page.unresolved += 1;
        page.rows.push(Row {
            source_record_id: format!("purchase:{}:{ordinal}", url.path()),
            order_id: url.path().strip_prefix("/orders/").map(str::to_owned),
            error_code: Some("DIGITAL_CONTROLS_UNRESOLVED".into()),
            ..Row::default()
        });
    }
}

/// The card's link to its shop.
fn shop_anchor<'a>(anchors: &[ElementRef<'a>]) -> Option<ElementRef<'a>> {
    anchors.iter().copied().find(|a| {
        a.value()
            .attr("href")
            .and_then(absolute)
            .and_then(|v| safety::public_url(&v, "shop"))
            .is_some()
    })
}

/// Whether the card lists its downloads explicitly.
fn explicit_file_list(card: ElementRef<'_>) -> bool {
    card.select(&sel("[data-test='downloadable-list'],.list.list--collapse"))
        .next()
        .is_some()
        || card
            .select(&sel(".legacy-list-item [data-dropdown-items]"))
            .next()
            .is_some()
}

/// Where the list goes next, and whether this page ends it.
fn read_pager(
    page: &mut Page,
    doc: &Html,
    main: ElementRef<'_>,
    url: &url::Url,
    path: &str,
    source: &str,
    is_detail: bool,
    cards: &[ElementRef<'_>],
) -> Result<()> {
    if is_detail {
        if cards.is_empty() {
            page.unresolved += 1;
        }
    } else {
        let current = url
            .query_pairs()
            .find(|(k, _)| k == "page")
            .and_then(|(_, v)| v.parse::<u32>().ok())
            .unwrap_or(1);
        if cards.is_empty() {
            if !empty_list(main, source) {
                bail!("NO_RECOGNIZED_ROWS");
            }
            page.terminal = true;
            // Past the first page, the list moved while it was being read.
            page.unresolved += usize::from(current > 1);
        }
        // A sorted page goes on in its own order; the Library tabs on it link the default one.
        let this_order = |u: &url::Url| {
            safety::updated_order(&format!("{path}?{}", u.query().unwrap_or("")))
                == safety::updated_order(&page.route)
        };
        let mut next = vec![];
        for a in doc.select(&sel("a[href]")) {
            if let Some(u) = a
                .value()
                .attr("href")
                .and_then(|v| safety::route(v, source).ok())
            {
                if u.path() == path && this_order(&u) {
                    let p = u
                        .query_pairs()
                        .find(|(k, _)| k == "page")
                        .and_then(|(_, v)| v.parse::<u32>().ok())
                        .unwrap_or(0);
                    // Past the end of a list BOOTH still offers a next page, also empty.
                    if p > current && !cards.is_empty() {
                        next.push(format!("{path}?{}", u.query().unwrap_or("")));
                    }
                }
            }
        }
        page.next.append(&mut next);
        page.next.sort();
        page.next.dedup();
        // A list of one page keeps BOOTH's pager element, with nothing in it.
        if page.next.is_empty() && current == 1 && !cards.is_empty() && only_page(doc) {
            page.terminal = true;
        }
        if page.next.is_empty() {
            let explicit_last = doc
                .select(&sel("a.last-page[href],a[aria-label='Last'][href]"))
                .any(|a| {
                    a.value()
                        .attr("href")
                        .and_then(|v| safety::route(v, source).ok())
                        .is_some_and(|u| u == *url)
                });
            if explicit_last {
                page.terminal = true;
            }
        }
        for pager in doc.select(&sel(".pagination,.pager")) {
            let own = pager.select(&sel("a[href]")).any(|a| {
                a.value()
                    .attr("href")
                    .and_then(|v| safety::route(v, source).ok())
                    .is_some_and(|u| u.path() == path)
            });
            if !own {
                continue;
            }
            let current_matches = pager
                .select(&sel(
                    "li.current > a[href],a.current[href],a[aria-current='page']",
                ))
                .any(|a| {
                    a.value()
                        .attr("href")
                        .and_then(|v| safety::route(v, source).ok())
                        .is_some_and(|u| u == *url)
                });
            let enabled_next = pager.select(&sel("a[rel='next'][href]")).any(|a| {
                a.value().attr("aria-disabled") != Some("true")
                    && !a.value().classes().any(|c| c == "disabled")
            });
            let disabled_next=pager.select(&sel(".next.disabled,.next-page.disabled,[rel='next'][aria-disabled='true'],[aria-label='Next'][aria-disabled='true']")).next().is_some();
            if page.next.is_empty() && (disabled_next || (current_matches && !enabled_next)) {
                page.terminal = true;
            }
        }
    }
    Ok(())
}

/// Fixed counters about a receipt's paid and dated product lines, for support reports.
fn trace_receipt(page: &mut Page, doc: &Html, url: &url::Url) {
    if let Some(receipt_id) = url.path().strip_prefix("/orders/") {
        let (fields, rejected) = match product_line_proof_document(doc, &page.rows, receipt_id) {
            Ok(proof) => {
                let fields = serde_json::to_value(&proof.diagnostics).unwrap_or(Value::Null);
                apply_product_line_charges(&mut page.rows, proof, receipt_id);
                (fields, false)
            }
            Err(_) => (Value::Null, true),
        };
        let (rows, paid_rows, dated_rows) = page
            .rows
            .iter()
            .filter(|row| row.order_id.as_deref() == Some(receipt_id))
            .fold((0u64, 0u64, 0u64), |(rows, paid, dated), row| {
                (
                    rows + 1,
                    paid + u64::from(row.product_line_total_jpy.is_some()),
                    dated
                        + u64::from(
                            row.acquired_date_text
                                .as_ref()
                                .is_some_and(|value| !value.trim().is_empty()),
                        ),
                )
            });
        let language = html_language(doc);
        // This counts parser calls in the current job attempt, not unique
        // receipts. Only fixed counters/booleans and a closed language code
        // leave the parser; receipt identities and source values stay here.
        crate::support_trace::receipt(&fields, rows, paid_rows, dated_rows, rejected, language);
    }
}
/// A list with nothing in it. BOOTH translates the sentence, so its layout says what it is:
/// a lone text panel, inside the order list for orders.
fn empty_list(main: ElementRef<'_>, source: &str) -> bool {
    let panel = if source == "purchase" {
        ".l-orders-index > .text-center"
    } else {
        ".bg-white.text-center"
    };
    let selector = sel(panel);
    let mut panels = main.select(&selector);
    panels
        .next()
        .and_then(|node| plain_leaf(node, 200))
        .is_some_and(|text| !text.is_empty())
        && panels.next().is_none()
}
fn only_page(doc: &Html) -> bool {
    doc.select(&sel(".pager")).any(|pager| {
        pager
            .children()
            .filter_map(ElementRef::wrap)
            .next()
            .is_none()
    })
}
fn order_id(e: ElementRef<'_>) -> Option<String> {
    let u = url::Url::parse(&absolute(e.value().attr("href")?)?).ok()?;
    let id = u.path().strip_prefix("/orders/")?;
    (safety::secure(&u)
        && u.host_str() == Some("accounts.booth.pm")
        && safety::numeric(id)
        && u.query().is_none())
    .then(|| id.to_owned())
}
fn enclosing_order(card: ElementRef<'_>) -> Option<String> {
    let repeated = sel("[data-test='order'],.order-list-item,.order-item,.sheet");
    for wrapper in card.ancestors().filter_map(ElementRef::wrap).take(4) {
        if matches!(
            wrapper.value().name(),
            "main" | "body" | "html" | "nav" | "header" | "footer" | "article" | "li" | "tr"
        ) {
            break;
        }
        // A shared parent containing another order card cannot supply identity.
        if wrapper.select(&repeated).count() != 1 {
            break;
        }
        if wrapper.value().name() == "a" {
            return order_id(wrapper);
        }
        if wrapper.value().name() != "div" {
            break;
        }
    }
    None
}

#[cfg(test)]
#[path = "parser_tests.rs"]
mod tests;
