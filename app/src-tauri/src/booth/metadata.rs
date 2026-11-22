use super::safety;
use anyhow::{bail, Result};
use scraper::{ElementRef, Html, Selector};
use serde_json::{json, Value};
use unicode_normalization::UnicodeNormalization;

const MAX_SAFE_JPY: u64 = 9_007_199_254_740_991;
fn normalized_price_text(value: &Value) -> Option<String> {
    let raw = value.as_str()?;
    if raw.chars().count() > 128 || raw.chars().any(|c| c.is_control() && !c.is_whitespace()) {
        return None;
    }
    let normalized: String = raw.nfkc().collect();
    Some(normalized.split_whitespace().collect::<Vec<_>>().join(" "))
}
fn jpy_amount(text: &str) -> Option<u64> {
    let text = text.trim();
    let digits = text
        .strip_prefix('¥')
        .or_else(|| text.strip_suffix('円'))
        .or_else(|| text.strip_suffix("JPY"))
        .unwrap_or(text)
        .trim();
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit() || c == ',') {
        return None;
    }
    if digits.contains(',') {
        let groups: Vec<_> = digits.split(',').collect();
        if !(1..=3).contains(&groups[0].len())
            || groups[0].starts_with('0')
            || groups.iter().skip(1).any(|group| group.len() != 3)
        {
            return None;
        }
    }
    digits
        .replace(',', "")
        .parse::<u64>()
        .ok()
        .filter(|v| *v <= MAX_SAFE_JPY)
}
/// Exact integer JPY only. A displayed range is never substituted with its lower bound.
pub(crate) fn parse_jpy_price(value: &Value) -> Option<f64> {
    if let Some(number) = value.as_number() {
        if let Some(integer) = number.as_u64() {
            return (integer <= MAX_SAFE_JPY).then_some(integer as f64);
        }
        let amount = number.as_f64()?;
        return (amount.is_finite()
            && !amount.is_sign_negative()
            && amount.fract() == 0.0
            && amount <= MAX_SAFE_JPY as f64)
            .then_some(amount);
    }
    jpy_amount(&normalized_price_text(value)?).map(|v| v as f64)
}
/// Bounded recognized price text; no HTML stripping or currency/amount guessing.
pub(crate) fn listing_price_display(value: &Value) -> Option<String> {
    if value.is_number() {
        return parse_jpy_price(value).map(|v| format!("¥ {}", v as u64));
    }
    let text = normalized_price_text(value)?;
    if jpy_amount(&text).is_some() {
        return Some(text);
    }
    let mut parts = text.split(['~', '〜']);
    let lower = jpy_amount(parts.next()?)?;
    let upper = parts.next()?.trim();
    if parts.next().is_some() || (!upper.is_empty() && jpy_amount(upper)? < lower) {
        return None;
    }
    Some(text)
}
/// The lowest advertised BOOTH price is a display fallback, not an owned quote.
pub(crate) fn listing_starting_price(value: &Value) -> Option<f64> {
    let display = listing_price_display(value)?;
    let lower = display.split(['~', '〜']).next()?.trim();
    jpy_amount(lower).map(|amount| amount as f64)
}

fn string(v: &Value, key: &str) -> Value {
    v.get(key)
        .and_then(Value::as_str)
        .map(|s| Value::String(s.chars().take(40000).collect()))
        .unwrap_or(Value::Null)
}
fn scalar(v: &Value, key: &str) -> Value {
    v.get(key)
        .filter(|v| v.is_string() || v.is_number())
        .cloned()
        .unwrap_or(Value::Null)
}
fn plain_text(value: &str, limit: usize) -> String {
    let bounded: String = value.chars().take(limit.saturating_mul(4)).collect();
    let fragment = Html::parse_fragment(&bounded);
    let mut text = String::new();
    for node in fragment.root_element().descendants() {
        if node
            .ancestors()
            .filter_map(ElementRef::wrap)
            .any(|e| matches!(e.value().name(), "script" | "style"))
        {
            continue;
        }
        if let Some(part) = node.value().as_text() {
            text.push(' ');
            text.extend(
                part.chars()
                    .filter(|c| !c.is_control() || c.is_whitespace()),
            );
        }
    }
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}
fn plain_string(v: &Value, key: &str, limit: usize) -> Value {
    v.get(key)
        .and_then(Value::as_str)
        .map(|s| Value::String(plain_text(s, limit)))
        .unwrap_or(Value::Null)
}
/// Explicit current-source availability; absent evidence remains unknown.
pub(crate) fn effective_public_page_availability(source: &Value) -> &'static str {
    match source["publicPageAvailability"].as_str() {
        Some("available") => "available",
        Some("unavailable") => "unavailable",
        _ => "unknown",
    }
}
/// The product fields public metadata owns: publication copies exactly these from an
/// enriched draft. A test checks that the writers here write no other field.
pub(super) const PUBLIC_FIELDS: [&str; 29] = [
    "store",
    "title",
    "sourceTitle",
    "name",
    "description",
    "sourceDescription",
    "publicPageAvailability",
    "publishedAt",
    "listingPriceText",
    "price",
    "isAdult",
    "sensitive",
    "sourceShop",
    "shop",
    "author",
    "sourceCategory",
    "sourceTags",
    "tags",
    "publicVariants",
    "previews",
    "previewUrl",
    "sourceUrl",
    "metadataUnavailable",
    "observedAt",
    "publicMetadataObservedAt",
    "publicMetadataState",
    "publicMetadataErrorCode",
    "publicSupportAvatars",
    "publicSupportObservedAt",
];
pub(super) fn ensure_source_fields(product: &mut Value) -> Result<()> {
    if product["sourceDescription"]
        .as_str()
        .is_none_or(|s| s.trim().is_empty())
    {
        let retained = plain_string(product, "description", 40000);
        if retained.as_str().is_some_and(|s| !s.is_empty()) {
            product["sourceDescription"] = retained;
        }
    }
    let availability = effective_public_page_availability(product);
    if product["publicPageAvailability"] != availability {
        product
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("PRODUCT_INVALID"))?
            .insert("publicPageAvailability".into(), json!(availability));
    }
    Ok(())
}
/// Only canonical public HTTP 404/410 is deletion evidence. Other failures do not mutate metadata.
pub(super) fn apply_public_response(
    product: &mut Value,
    id: &str,
    status: u16,
    payload: Option<&Value>,
) -> Result<()> {
    if matches!(status, 404 | 410) {
        ensure_source_fields(product)?;
        product["publicPageAvailability"] = json!("unavailable");
        product["metadataUnavailable"] = json!(true);
        product["publicMetadataErrorCode"] = json!("PUBLIC_METADATA_UNAVAILABLE");
        return Ok(());
    }
    if !(200..300).contains(&status) {
        bail!("PUBLIC_METADATA_FAILED");
    }
    let payload = payload.ok_or_else(|| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?;
    let fresh = public_metadata(payload, id)?;
    ensure_source_fields(product)?;
    let object = product
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("PRODUCT_INVALID"))?;
    for (key, value) in fresh.as_object().unwrap() {
        // A recognized live range supersedes a prior exact quote; sparse/invalid data does not.
        if key == "price" && value.is_null() && listing_price_display(&payload["price"]).is_some() {
            object.insert(key.clone(), Value::Null);
            continue;
        }
        if value.is_null()
            || (matches!(
                key.as_str(),
                "sourceDescription" | "description" | "sourceTitle" | "title" | "name"
            ) && value.as_str().is_some_and(|s| s.trim().is_empty()))
        {
            continue;
        }
        if (matches!(key.as_str(), "sourceTags" | "tags") && !payload["tags"].is_array())
            || (key == "publicVariants" && !payload["variations"].is_array())
            || (key == "sourceCategory"
                && (!payload["category"].is_object()
                    || value
                        .as_object()
                        .is_some_and(|o| o.values().all(Value::is_null))))
        {
            continue;
        }
        if matches!(key.as_str(), "sourceCategory" | "sourceShop") {
            let incoming = value.as_object().unwrap();
            let target = object.entry(key.clone()).or_insert_with(|| json!({}));
            if !target.is_object() {
                *target = json!({});
            }
            for (field, field_value) in incoming {
                if !field_value.is_null()
                    && !field_value.as_str().is_some_and(|s| s.trim().is_empty())
                {
                    target
                        .as_object_mut()
                        .unwrap()
                        .insert(field.clone(), field_value.clone());
                }
            }
            continue;
        }
        object.insert(key.clone(), value.clone());
    }
    object.insert("publicMetadataObservedAt".into(), json!(super::now()));
    object.insert("publicMetadataState".into(), json!("complete"));
    object.remove("publicMetadataErrorCode");
    Ok(())
}
/// Public digital variation identity, label and exact current JPY quote.
fn public_variants(value: &Value) -> Vec<Value> {
    value["variations"]
        .as_array()
        .into_iter()
        .flatten()
        .take(500)
        .filter(|v| v["type"] == "digital")
        .filter_map(|v| {
            let id = v["id"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| v["id"].as_u64().map(|id| id.to_string()))?;
            if !safety::numeric(&id) {
                return None;
            }
            // BOOTH's ordinary single digital variation can have a null name.
            // Its exact ID still binds the owned variation's current quote.
            let mut variant = json!({"id":id,"type":"digital"});
            if let Some(name) = v["name"]
                .as_str()
                .map(|name| plain_text(name, 1000))
                .filter(|name| !name.is_empty())
            {
                variant["name"] = json!(name);
            }
            if let Some(price) = parse_jpy_price(&v["price"]) {
                variant["price"] = json!(price);
                variant["currency"] = json!("JPY");
            }
            Some(variant)
        })
        .collect()
}
/// A second public HTML request is useful only where JSON has no positive support,
/// or an avatar-related digital offer cannot yet be interpreted. No global body is stored.
pub(super) fn needs_public_support_html(product: &Value) -> bool {
    if !matches!(
        crate::taxonomy::infer_product_category(product)["category"].as_str(),
        Some("Clothes" | "Hair" | "Accessory" | "Texture" | "Gimmick")
    ) {
        return false;
    }
    // HTML-only support is a last observation, not a reason to skip refreshing
    // the field that supplied it. Fresh JSON may omit that entire section.
    if product["publicSupportAvatars"]
        .as_array()
        .is_some_and(|v| !v.is_empty())
    {
        return true;
    }
    let mut fresh = product.clone();
    if let Some(object) = fresh.as_object_mut() {
        object.remove("publicSupportAvatars");
        object.remove("publicSupportObservedAt");
        object.remove("listingAvatars");
    }
    crate::taxonomy::listing_avatar_evidence(&fresh).is_empty()
        || product["publicVariants"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|v| {
                v["type"] == "digital"
                    && (crate::taxonomy::is_all_inclusive_variant(v["name"].as_str().unwrap_or(""))
                        || crate::taxonomy::variant_avatars(v["name"].as_str().unwrap_or(""))
                            .is_empty())
            })
}
fn support_heading(text: &str) -> bool {
    let key: String = crate::taxonomy::normalize(text)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    [
        "対応アバター",
        "対応モデル",
        "対応素体",
        "対応素體",
        "對應素體",
        "相容素體",
        "支持的模型",
        "supportedavatars",
        "compatibleavatars",
        "supportedmodels",
        "지원아바타",
    ]
    .contains(&key.as_str())
}
fn negative_support(text: &str) -> bool {
    let text = crate::taxonomy::normalize(text);
    [
        "非対応",
        "未対応",
        "対応予定",
        "not supported",
        "unsupported",
        "incompatible",
        "不支援",
        "不支持",
        "미지원",
    ]
    .iter()
    .any(|word| text.contains(word))
}
/// Only explicit public support sections and their base item links are read.
/// Shipping/comments/recommendations/dependency sections have no role here.
pub(super) fn apply_public_support_html(product: &mut Value, body: &str) -> Result<()> {
    let page = Html::parse_document(body);
    let selector = Selector::parse("section > h2, section > h3").unwrap();
    let dictionary = crate::taxonomy::dictionary();
    let known = dictionary["avatars"].as_array().unwrap();
    let mut avatars = std::collections::BTreeMap::new();
    let mut section_seen = false;
    for heading in page.select(&selector).take(200) {
        if !support_heading(&heading.text().collect::<String>()) {
            continue;
        }
        section_seen = true;
        for block in heading
            .next_siblings()
            .filter_map(ElementRef::wrap)
            .take(20)
        {
            if matches!(block.value().name(), "h1" | "h2" | "h3" | "h4" | "section") {
                break;
            }
            let text: String = block
                .text()
                .collect::<String>()
                .chars()
                .take(20000)
                .collect();
            let mut previous = "";
            for line in text.lines().take(500).map(str::trim) {
                if line.is_empty() {
                    continue;
                }
                // A preceding item label is enough to bind a plain autolink URL.
                // Known IDs resolve directly; unknown IDs retain the seller's label.
                for token in line
                    .split_whitespace()
                    .filter(|s| s.starts_with("https://"))
                {
                    let token = token.trim_end_matches([')', '）', ']', '】', '。', ',', '、']);
                    let Some(url) = safety::public_url(token, "item") else {
                        continue;
                    };
                    let Some(id) = safety::item_id(&url) else {
                        continue;
                    };
                    if negative_support(previous) || negative_support(line) {
                        continue;
                    }
                    let candidate = known
                        .iter()
                        .find(|v| v["productId"].as_str() == Some(id.as_str()))
                        .cloned();
                    let mut candidate = if let Some(candidate) = candidate {
                        candidate
                    } else {
                        let label = previous
                            .trim_start_matches(['・', '-', '*', '●', '•'])
                            .trim();
                        let original = crate::taxonomy::clean_avatar_name(label);
                        if original.is_empty()
                            || original.chars().count() > 96
                            || original.contains("://")
                            || !original.chars().any(char::is_alphabetic)
                        {
                            continue;
                        }
                        let english = crate::taxonomy::romanized_name(&original);
                        let mut candidate = json!({"id":format!("booth:{id}"),"productId":id,"officialName":original,"englishName":english});
                        candidate["displayName"] =
                            json!(crate::taxonomy::avatar_display_name(&candidate));
                        candidate
                    };
                    candidate["sourceUrl"] = json!(url);
                    candidate["basis"] = json!("publicSupportSection");
                    candidate["entitlementEstablished"] = json!(false);
                    avatars.insert(candidate["id"].as_str().unwrap().to_owned(), candidate);
                }
                if !line.starts_with("https://") && !negative_support(line) {
                    for mut candidate in crate::taxonomy::match_avatars(line) {
                        candidate["basis"] = json!("publicSupportSection");
                        avatars
                            .entry(candidate["id"].as_str().unwrap().to_owned())
                            .or_insert(candidate);
                    }
                }
                previous = line;
            }
            for link in block.select(&Selector::parse("a[href]").unwrap()).take(100) {
                let Some(url) = link
                    .value()
                    .attr("href")
                    .and_then(|raw| safety::public_url(raw, "item"))
                else {
                    continue;
                };
                let Some(id) = safety::item_id(&url) else {
                    continue;
                };
                let context = link
                    .ancestors()
                    .filter_map(ElementRef::wrap)
                    .find(|e| matches!(e.value().name(), "p" | "li"));
                if negative_support(&link.text().collect::<String>())
                    || context.is_some_and(|e| negative_support(&e.text().collect::<String>()))
                {
                    continue;
                }
                if let Some(mut candidate) = known
                    .iter()
                    .find(|v| v["productId"].as_str() == Some(id.as_str()))
                    .cloned()
                {
                    candidate["sourceUrl"] = json!(url);
                    candidate["basis"] = json!("publicSupportSection");
                    candidate["entitlementEstablished"] = json!(false);
                    avatars.insert(candidate["id"].as_str().unwrap().to_owned(), candidate);
                }
            }
        }
    }
    // A successful matching public product page with its known description
    // scope can confirm that the former support section was removed. An
    // unrelated or login/error body cannot withdraw the last-known field.
    let description_seen = page
        .select(&Selector::parse("section.shop__text").unwrap())
        .next()
        .is_some();
    if section_seen || description_seen {
        product["publicSupportAvatars"] = json!(avatars.into_values().collect::<Vec<_>>());
        product["publicSupportObservedAt"] = json!(super::now());
    }
    Ok(())
}
/// Whitelist public metadata; public offered files never become account entitlements.
pub fn public_metadata(value: &Value, id: &str) -> Result<Value> {
    if !safety::numeric(id)
        || !value.is_object()
        || value.get("id").is_none()
        || value["name"]
            .as_str()
            .is_none_or(|s| plain_text(s, 4096).is_empty())
    {
        bail!("PUBLIC_METADATA_INVALID");
    }
    if let Some(actual) = value.get("id").map(|v| {
        v.as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| v.to_string())
    }) {
        if actual != id {
            bail!("PUBLIC_ID_MISMATCH");
        }
    }
    let shop = &value["shop"];
    let category = &value["category"];
    let tags = value["tags"]
        .as_array()
        .map(|a| {
            a.iter()
                .take(500)
                .filter_map(|t| t["name"].as_str().map(|s| plain_text(s, 1000)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let previews = value["images"]
        .as_array()
        .map(|a| {
            a.iter()
                .take(100)
                .filter_map(|i| {
                    i["original"]
                        .as_str()
                        .and_then(|s| safety::public_url(s, "preview"))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(
        json!({"id":id,"store":"BOOTH","title":string(value,"name"),"sourceTitle":plain_string(value,"name",4096),"name":string(value,"name"),"description":string(value,"description"),"sourceDescription":plain_string(value,"description",40000),"publicPageAvailability":"available","publishedAt":string(value,"published_at"),"listingPriceText":scalar(value,"price"),"price":parse_jpy_price(&value["price"]),"isAdult":value.get("is_adult").and_then(Value::as_bool),"sensitive":value.get("is_adult").and_then(Value::as_bool),"sourceShop":{"name":string(shop,"name"),"url":shop["url"].as_str().and_then(|s|safety::public_url(s,"shop")),"uuid":string(shop,"uuid")},"shop":string(shop,"name"),"author":string(shop,"name"),"sourceCategory":{"id":scalar(category,"id"),"name":plain_string(category,"name",4096),"parentId":scalar(&category["parent"],"id"),"parentName":plain_string(&category["parent"],"name",4096)},"sourceTags":tags,"tags":tags,"publicVariants":public_variants(value),"previews":previews,"previewUrl":previews.first(),"sourceUrl":format!("https://booth.pm/ja/items/{id}"),"metadataUnavailable":false,"observedAt":chrono::Utc::now().to_rfc3339()}),
    )
}
/// When this account accepted a gift of the item, from the JSON its own session reads.
/// `gift.url` is a private claim link, so only the date leaves here. BOOTH writes the date
/// as 年月日 under `/ja/` and as `YYYY-MM-DD` under its other locales.
pub(super) fn gift_accepted_date(value: &Value, id: &str) -> Option<String> {
    let same_item = match value.get("id")? {
        Value::String(actual) => actual == id,
        // A numeric id matches by its JSON text; `Value == &str` only matches strings.
        #[allow(clippy::cmp_owned)]
        actual => actual.to_string() == id,
    };
    if !same_item {
        return None;
    }
    let accepted = value["gift"]["accepted_at"].as_str()?.trim();
    ["%Y年%m月%d日", "%Y-%m-%d"]
        .into_iter()
        .find_map(|format| chrono::NaiveDate::parse_from_str(accepted, format).ok())
        .map(|date| date.format("%Y/%m/%d").to_string())
}
#[cfg(test)]
#[path = "metadata_tests.rs"]
mod tests;
