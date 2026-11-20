//! Public editor suggestions only. No account session or catalogue writes.
use super::{http, metadata, safety, BoothService};
use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use url::Url;

struct ItemUrl {
    id: String,
    source: Url,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FetchedProductMetadata {
    source_url: String,
    title: String,
    category: String,
    tags: Vec<String>,
    thumbnail_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    listing_price: Option<Value>,
}
fn item_url(raw: &str) -> Result<ItemUrl> {
    let raw = raw.trim();
    if raw.len() > 8192 || raw.contains('\\') || raw.chars().any(char::is_control) {
        bail!("INVALID_PRODUCT_URL");
    }
    let mut source = Url::parse(raw).map_err(|_| anyhow::anyhow!("INVALID_PRODUCT_URL"))?;
    let (_, rest) = raw
        .split_once("://")
        .ok_or_else(|| anyhow::anyhow!("INVALID_PRODUCT_URL"))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if !safety::secure(&source)
        || authority.contains([':', '@'])
        || source.query().is_some()
        || source.fragment().is_some()
    {
        bail!("INVALID_PRODUCT_URL");
    }
    if !source.host_str().is_some_and(safety::booth_host) {
        bail!("UNSUPPORTED_PROVIDER");
    }
    if matches!(
        source.host_str(),
        Some("accounts.booth.pm" | "manage.booth.pm")
    ) {
        bail!("INVALID_PRODUCT_URL");
    }
    // Inspect the original path too: URL normalization must not hide dot segments.
    let original_path = rest.strip_prefix(authority).unwrap_or("");
    let parts: Vec<_> = original_path.split('/').collect();
    let id = match parts.as_slice() {
        ["", "items", id] => *id,
        ["", locale, "items", id] if matches!(*locale, "ja" | "en" | "ko" | "zh-cn" | "zh-tw") => {
            *id
        }
        _ => bail!("INVALID_PRODUCT_URL"),
    };
    if !safety::numeric(id) || safety::item_id(source.as_str()).as_deref() != Some(id) {
        bail!("INVALID_PRODUCT_URL");
    }
    let normalized = safety::public_url(source.as_str(), "item")
        .ok_or_else(|| anyhow::anyhow!("INVALID_PRODUCT_URL"))?;
    source = Url::parse(&normalized).map_err(|_| anyhow::anyhow!("INVALID_PRODUCT_URL"))?;
    Ok(ItemUrl {
        id: id.into(),
        source,
    })
}
fn staged(item: &ItemUrl, payload: &Value) -> Result<FetchedProductMetadata> {
    let public = metadata::public_metadata(payload, &item.id)?;
    let host = item.source.host_str().unwrap();
    if !matches!(host, "booth.pm" | "www.booth.pm") {
        let shop = public["sourceShop"]["url"]
            .as_str()
            .and_then(|s| Url::parse(s).ok());
        if shop.as_ref().and_then(Url::host_str) != Some(host) {
            bail!("PUBLIC_SOURCE_HOST_MISMATCH");
        }
    }
    let category = crate::taxonomy::infer_product_category(&public)["category"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?
        .to_owned();
    let title = public["sourceTitle"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?
        .to_owned();
    let tags = public["sourceTags"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let thumbnail_url = payload["images"]
        .as_array()
        .into_iter()
        .flatten()
        .take(100)
        .filter_map(|image| image["original"].as_str())
        .find_map(|raw| {
            crate::preview::parse_public_url(raw)
                .ok()
                .map(|url| url.to_string())
        });
    // Which variation a local copy came from is unknown, so the cheapest digital one
    // stands in, as BOOTH's own headline price does.
    let listing_price = public["publicVariants"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|variant| variant["price"].as_f64())
        .min_by(f64::total_cmp)
        .or_else(|| metadata::listing_starting_price(&payload["price"]))
        .map(|price| json!({"price":price,"priceCurrency":"JPY"}));
    Ok(FetchedProductMetadata {
        source_url: item.source.to_string(),
        title,
        category,
        tags,
        thumbnail_url,
        listing_price,
    })
}
impl BoothService {
    pub(crate) async fn fetch_product_metadata(&self, raw: &str) -> Result<Value> {
        let item = item_url(raw)?;
        let client = http::build_client(Arc::new(reqwest::cookie::Jar::default()), false)?;
        let url = Url::parse(&format!("https://booth.pm/ja/items/{}.json", item.id)).unwrap();
        let (status, body) = self.inner.http.get(&client, url).await?;
        if matches!(status.as_u16(), 404 | 410) {
            bail!("PUBLIC_METADATA_UNAVAILABLE");
        }
        if !status.is_success() {
            bail!("PUBLIC_METADATA_FAILED");
        }
        let payload =
            serde_json::from_str(&body).map_err(|_| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?;
        serde_json::to_value(staged(&item, &payload)?)
            .map_err(|_| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))
    }
}

#[cfg(test)]
#[path = "fetch_tests.rs"]
mod tests;
