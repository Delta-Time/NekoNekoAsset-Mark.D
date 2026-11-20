//! Unauthenticated editor suggestions. Public URLs never establish ownership.
use super::{http, StoreProvider};
use anyhow::{bail, ensure, Result};
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use url::Url;

pub(super) fn product_url(raw: &str) -> Result<Option<(StoreProvider, Url)>> {
    let raw = raw.trim();
    ensure!(
        raw.len() <= 8192 && !raw.contains('\\') && !raw.chars().any(char::is_control),
        "INVALID_PRODUCT_URL"
    );
    let url = Url::parse(raw).map_err(|_| anyhow::anyhow!("INVALID_PRODUCT_URL"))?;
    let rest = raw
        .split_once("://")
        .ok_or_else(|| anyhow::anyhow!("INVALID_PRODUCT_URL"))?
        .1;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    ensure!(
        url.scheme() == "https"
            && !authority.contains([':', '@'])
            && url.query().is_none()
            && url.fragment().is_none(),
        "INVALID_PRODUCT_URL"
    );
    let host = url.host_str().unwrap_or("");
    if host == "booth.pm" || host.ends_with(".booth.pm") {
        return Ok(None);
    }
    let provider = if host == "gumroad.com" || host.ends_with(".gumroad.com") {
        StoreProvider::Gumroad
    } else if super::session::jinxxy_host(host) {
        StoreProvider::Jinxxy
    } else {
        bail!("UNSUPPORTED_PROVIDER")
    };
    let original = rest.strip_prefix(authority).unwrap_or("");
    let parts: Vec<_> = original.split('/').collect();
    let segment = |part: &str| {
        !part.is_empty()
            && part.len() <= 256
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    };
    let valid = match (provider, parts.as_slice()) {
        (StoreProvider::Gumroad, ["", "l", slug]) => {
            segment(slug) && host != "files.gumroad.com" && host != "app.gumroad.com"
        }
        (StoreProvider::Jinxxy, ["", seller, slug]) => {
            segment(seller)
                && segment(slug)
                && !matches!(
                    *seller,
                    "my" | "login" | "sign_in" | "graphql" | "api" | "marketplace" | "search"
                )
        }
        _ => false,
    };
    ensure!(valid && url.path() == original, "INVALID_PRODUCT_URL");
    Ok(Some((provider, url)))
}

/// A link copied from a browser may carry tracking parameters or a trailing slash.
pub(super) fn bare_url(raw: &str) -> Result<String> {
    let mut url = Url::parse(raw.trim()).map_err(|_| anyhow::anyhow!("INVALID_PRODUCT_URL"))?;
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn text(raw: &str, max: usize) -> String {
    Html::parse_fragment(raw)
        .root_element()
        .text()
        .collect::<String>()
        .chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Jinxxy descriptions are serialized rich-text trees, not their JSON syntax.
pub(super) fn jinxxy_description(raw: &str) -> String {
    fn append(node: &Value, output: &mut String) {
        match node["type"].as_str() {
            Some("text") => {
                if let Some(value) = node["text"].as_str() {
                    output.push_str(value);
                }
            }
            Some("linebreak") => output.push('\n'),
            _ => {}
        }
        for child in node["children"].as_array().into_iter().flatten() {
            append(child, output);
        }
        if matches!(
            node["type"].as_str(),
            Some("paragraph" | "heading" | "listitem")
        ) {
            output.push('\n');
        }
    }
    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        if value["root"]["children"].is_array() {
            let mut result = String::new();
            append(&value["root"], &mut result);
            return result
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .filter(|c| !c.is_control())
                .take(40000)
                .collect();
        }
        if value.is_object() || value.is_array() {
            return String::new();
        }
    }
    Html::parse_fragment(raw)
        .root_element()
        .text()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .take(40000)
        .collect()
}

// Decode data literals only. Text records use UTF-8 byte lengths and can contain
// newlines; splitting the whole stream into lines would lose their boundaries.
fn jinxxy_flight(document: &Html) -> Result<BTreeMap<String, Value>> {
    const LIMIT: usize = 16 * 1024 * 1024;
    const PUSH: &str = "self.__next_f.push(";
    let mut stream = String::new();
    for script in document.select(&Selector::parse("script").unwrap()) {
        // HTML serialization escapes raw script text (including NBSP), changing
        // Flight's declared UTF-8 byte lengths. Read the text nodes verbatim.
        let content = script.text().collect::<String>();
        let mut remaining = content.trim();
        if let Some(value) = remaining.strip_prefix("(self.__next_f=self.__next_f||[]).push([0]);")
        {
            remaining = value.trim_start();
        }
        while let Some(literal) = remaining.strip_prefix(PUSH) {
            let mut decoder = serde_json::Deserializer::from_str(literal).into_iter::<Value>();
            let value = decoder
                .next()
                .transpose()
                .map_err(|_| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?
                .ok_or_else(|| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?;
            remaining = literal[decoder.byte_offset()..]
                .trim_start()
                .strip_prefix(')')
                .ok_or_else(|| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?
                .trim_start();
            remaining = remaining
                .strip_prefix(';')
                .unwrap_or(remaining)
                .trim_start();
            if value[0] == 1 {
                let chunk = value[1]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?;
                ensure!(
                    stream.len() + chunk.len() <= LIMIT,
                    "PUBLIC_METADATA_INVALID"
                );
                stream.push_str(chunk);
            }
        }
    }
    let mut records = BTreeMap::new();
    let mut remaining = stream.as_str();
    while !remaining.is_empty() {
        let Some((id, body)) = remaining.split_once(':') else {
            break;
        };
        // Next's public stream includes ID-less CSS/font preload hints. They
        // carry no model data and must not prevent reading the product records.
        if id.is_empty() && body.starts_with("HL[") {
            remaining = body.split_once('\n').map_or("", |(_, rest)| rest);
            continue;
        }
        if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("PUBLIC_METADATA_INVALID");
        }
        if let Some(length) = body.strip_prefix('T') {
            let (length, payload) = length
                .split_once(',')
                .ok_or_else(|| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?;
            let length = usize::from_str_radix(length, 16)
                .map_err(|_| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?;
            ensure!(
                length <= payload.len() && payload.is_char_boundary(length),
                "PUBLIC_METADATA_INVALID"
            );
            records.insert(id.to_owned(), Value::String(payload[..length].to_owned()));
            remaining = payload[length..].trim_start_matches('\n');
        } else {
            let (row, rest) = body.split_once('\n').unwrap_or((body, ""));
            if let Ok(value) = serde_json::from_str(row) {
                records.insert(id.to_owned(), value);
            }
            remaining = rest;
        }
    }
    Ok(records)
}

fn jinxxy_presentation(
    source: &Url,
    document: &Html,
    expected_id: Option<&str>,
) -> Result<Option<Value>> {
    fn products<'a>(value: &'a Value, result: &mut Vec<&'a Value>) {
        match value {
            Value::Object(object) => {
                if value["__typename"] == "PublicProduct" {
                    result.push(value);
                }
                for child in object.values() {
                    products(child, result);
                }
            }
            Value::Array(values) => {
                for child in values {
                    products(child, result);
                }
            }
            _ => {}
        }
    }
    let (_, validated) = product_url(source.as_str())?
        .filter(|(provider, _)| *provider == StoreProvider::Jinxxy)
        .ok_or_else(|| anyhow::anyhow!("INVALID_PRODUCT_URL"))?;
    let parts: Vec<_> = validated.path().split('/').collect();
    let records = jinxxy_flight(document)?;
    let mut candidates = Vec::new();
    for record in records.values() {
        products(record, &mut candidates);
    }
    let Some(product) = candidates.iter().copied().find(|product| {
        product["user"]["username"].as_str() == Some(parts[1])
            && product["url"].as_str() == Some(parts[2])
    }) else {
        ensure!(candidates.is_empty(), "PUBLIC_SOURCE_HOST_MISMATCH");
        return Ok(None);
    };
    let id = product["id"]
        .as_str()
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 20
                && id.bytes().all(|byte| byte.is_ascii_digit())
                && id.parse::<u64>().is_ok_and(|id| id > 0)
        })
        .ok_or_else(|| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?;
    ensure!(
        expected_id.is_none_or(|expected| expected == id),
        "PUBLIC_SOURCE_HOST_MISMATCH"
    );
    let resolve = |value: &Value| -> Option<String> {
        let raw = value.as_str()?;
        if let Some(id) = raw
            .strip_prefix('$')
            .filter(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            records.get(id)?.as_str().map(str::to_owned)
        } else if raw == "$undefined" {
            None
        } else {
            Some(raw.to_owned())
        }
    };
    let labels = |value: &Value| -> Option<Vec<String>> {
        Some(
            value
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .take(100)
                .map(|value| text(value, 256))
                .filter(|value| !value.is_empty())
                .collect(),
        )
    };
    let mut result = json!({"publicProductId":id,"sourceUrl":source.as_str()});
    if let Some(title) = resolve(&product["name"])
        .map(|title| text(&title, 4096))
        .filter(|title| !title.is_empty())
    {
        result["sourceTitle"] = json!(title);
    }
    if let Some(description) = resolve(&product["description"])
        .map(|description| jinxxy_description(&description))
        .filter(|description| !description.is_empty())
    {
        result["sourceDescription"] = json!(description);
    }
    let category = resolve(&product["category"]).map(|category| text(&category, 256));
    let types = labels(&product["type"]);
    if category.is_some() || types.is_some() {
        result["sourceCategory"] =
            json!({"name":category.unwrap_or_default(),"types":types.unwrap_or_default()});
    }
    if let Some(tags) = labels(&product["tags"]) {
        result["sourceTags"] = json!(tags);
    }
    if let Some(url) = product["cover"]["url"]
        .as_str()
        .and_then(|raw| crate::preview::parse_public_url(raw).ok())
    {
        result["previewUrl"] = json!(url.as_str());
    }
    if let Some(price) =
        super::jinxxy::usd_minor_amount(&product["base_price"], &product["currency_code"])
    {
        result["listingPrice"] = json!({"price":price,"priceCurrency":"USD"});
    }
    Ok(Some(result))
}

fn suggestions(
    url: &Url,
    title: &str,
    description: &str,
    tags: Vec<String>,
    thumbnail: Option<&str>,
    provider: Option<StoreProvider>,
    source_category: Option<&Value>,
) -> Result<Value> {
    let title = text(title, 4096);
    ensure!(!title.is_empty(), "PUBLIC_METADATA_INVALID");
    let tags: BTreeSet<_> = tags
        .iter()
        .take(100)
        .map(|tag| text(tag, 256))
        .filter(|tag| !tag.is_empty())
        .collect();
    // Gumroad's bound JSON body is already decoded plain text; do not parse its
    // literal angle brackets or HTML entities a second time.
    let description = if provider == Some(StoreProvider::Gumroad) {
        description.to_owned()
    } else {
        text(description, 40000)
    };
    let mut product =
        json!({"sourceTitle":title,"sourceDescription":description,"sourceTags":tags});
    if let Some(provider) = provider {
        product["provider"] = json!(provider.key());
    }
    if let Some(category) = source_category {
        product["sourceCategory"] = category.clone();
    }
    let category = crate::taxonomy::infer_product_category(&product)["category"].clone();
    let thumbnail = thumbnail
        .and_then(|raw| crate::preview::parse_public_url(raw).ok())
        .map(|url| url.to_string());
    Ok(
        json!({"sourceUrl":url.as_str(),"title":title,"category":category,"tags":tags,"thumbnailUrl":thumbnail}),
    )
}

fn gumroad_values(url: &Url, value: &Value) -> Result<Value> {
    let observation = super::gumroad::parse_public_product(value, None)?;
    if let Some(permalink) = value["short_permalink"]
        .as_str()
        .or_else(|| value["permalink"].as_str())
    {
        ensure!(
            url.path().strip_prefix("/l/") == Some(permalink),
            "PUBLIC_SOURCE_HOST_MISMATCH"
        );
    }
    let tags = value["tags"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tag| tag.as_str().or_else(|| tag["name"].as_str()))
        .map(str::to_owned)
        .collect();
    let mut result = suggestions(
        url,
        value["name"].as_str().unwrap_or(""),
        observation.description.as_deref().unwrap_or(""),
        tags,
        value["preview_url"]
            .as_str()
            .or_else(|| value["cover_image_url"].as_str())
            .or_else(|| value["thumbnail_url"].as_str()),
        Some(StoreProvider::Gumroad),
        None,
    )?;
    let amount = observation
        .current_price
        .as_ref()
        .and_then(super::gumroad::PublicPrice::usd_amount);
    if amount.is_some() || observation.price_formatted.is_some() {
        result["listingPrice"] = json!({
            "price":amount,"priceCurrency":amount.map(|_| "USD"),
            "priceFormatted":observation.price_formatted,
            "priceSourceCurrency":observation.price_source_currency,
        });
    }
    Ok(result)
}

fn html_values(url: &Url, html: &str) -> Result<Value> {
    let document = Html::parse_document(html);
    let meta = |key: &str| {
        document
            .select(&Selector::parse("meta").unwrap())
            .find_map(|node| {
                let name = node
                    .value()
                    .attr("property")
                    .or_else(|| node.value().attr("name"));
                (name == Some(key))
                    .then(|| node.value().attr("content"))
                    .flatten()
                    .map(str::to_owned)
            })
    };
    // Public presentation only; never parse buyer/inventory/receipt records.
    if let Some(canonical) = meta("og:url") {
        let canonical = product_url(&canonical)?
            .ok_or_else(|| anyhow::anyhow!("PUBLIC_SOURCE_HOST_MISMATCH"))?
            .1;
        ensure!(
            canonical.path() == url.path() && canonical.host_str() == url.host_str(),
            "PUBLIC_SOURCE_HOST_MISMATCH"
        );
    }
    if let Some(product) = jinxxy_presentation(url, &document, None)? {
        let title = product["sourceTitle"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| meta("og:title"))
            .unwrap_or_default();
        let thumbnail = product["previewUrl"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| meta("og:image"));
        let mut result = suggestions(
            url,
            &title,
            product["sourceDescription"].as_str().unwrap_or(""),
            product["sourceTags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            thumbnail.as_deref(),
            Some(StoreProvider::Jinxxy),
            product.get("sourceCategory"),
        )?;
        if let Some(price) = product.get("listingPrice") {
            result["listingPrice"] = price.clone();
        }
        return Ok(result);
    }
    let title = meta("og:title").ok_or_else(|| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?;
    let description = meta("og:description")
        .or_else(|| meta("description"))
        .unwrap_or_default();
    let tags = meta("keywords")
        .unwrap_or_default()
        .split(',')
        .map(str::to_owned)
        .collect();
    suggestions(
        url,
        &title,
        &description,
        tags,
        meta("og:image").as_deref(),
        Some(StoreProvider::Jinxxy),
        None,
    )
}

async fn fetch_body(
    transport: &http::MetadataHttp,
    provider: StoreProvider,
    source: Url,
) -> Result<String> {
    let client = http::build_client(provider, Arc::new(reqwest::cookie::Jar::default()))?;
    let mut request = source.clone();
    if provider == StoreProvider::Gumroad {
        request.set_path(&format!("{}.json", source.path()));
    }
    let (status, body, final_url) = transport.get_with_url(&client, request.clone()).await?;
    ensure!(final_url == request, "PUBLIC_SOURCE_HOST_MISMATCH");
    if matches!(status.as_u16(), 404 | 410) {
        bail!("PUBLIC_METADATA_UNAVAILABLE");
    }
    ensure!(status.is_success(), "PUBLIC_METADATA_FAILED");
    Ok(body)
}

/// Public presentation is bound to a buyer-known product ID but grants no access.
pub(super) async fn fetch_jinxxy_presentation(
    transport: &http::MetadataHttp,
    source: Url,
    expected_product_id: &str,
) -> Result<Value> {
    product_url(source.as_str())?
        .filter(|(provider, _)| *provider == StoreProvider::Jinxxy)
        .ok_or_else(|| anyhow::anyhow!("INVALID_PRODUCT_URL"))?;
    let body = fetch_body(transport, StoreProvider::Jinxxy, source.clone()).await?;
    let mut presentation = jinxxy_presentation(
        &source,
        &Html::parse_document(&body),
        Some(expected_product_id),
    )?
    .ok_or_else(|| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?;
    // Buyer sync merges this into the product and has its own quote.
    if let Some(fields) = presentation.as_object_mut() {
        fields.remove("listingPrice");
    }
    Ok(presentation)
}

pub(super) async fn fetch(
    transport: &http::MetadataHttp,
    provider: StoreProvider,
    source: Url,
) -> Result<Value> {
    let body = fetch_body(transport, provider, source.clone()).await?;
    match provider {
        StoreProvider::Gumroad => gumroad_values(
            &source,
            &serde_json::from_str(&body).map_err(|_| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?,
        ),
        StoreProvider::Jinxxy => html_values(&source, &body),
        StoreProvider::Booth => unreachable!(),
    }
}

#[cfg(test)]
#[path = "public_metadata_tests.rs"]
mod tests;
