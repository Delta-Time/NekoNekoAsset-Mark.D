//! Public editor suggestions and explicit rechecks of an owned listing's availability.
use super::{http, metadata, safety, BoothService};
use crate::catalogue::Catalogue;
use anyhow::{bail, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tauri::AppHandle;
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
struct EditedListing {
    product: Value,
    refusals: Vec<Value>,
}

impl EditedListing {
    fn snapshot(catalogue: &Catalogue, item: &ItemUrl, edited_id: &str) -> Result<Option<Self>> {
        if item.id != edited_id {
            return Ok(None);
        }
        let Some(product) = catalogue.get("product", edited_id)? else {
            return Ok(None);
        };
        let provider = product["provider"].as_str().or(product["store"].as_str());
        if !provider.is_some_and(|name| name.eq_ignore_ascii_case("booth")) {
            return Ok(None);
        }
        let refusals = catalogue
            .related("artifact", edited_id)?
            .into_iter()
            .filter(|artifact| {
                artifact["errorCode"] == "store_file_unavailable"
                    && artifact["id"].as_str().is_some_and(|id| id.starts_with("pending-"))
                    && artifact["originalAvailable"] == false
                    && artifact["extractedPath"].is_null()
            })
            .collect();
        Ok(Some(Self { product, refusals }))
    }

    fn record(self, catalogue: &Catalogue, available: bool) -> Result<bool> {
        let id = self.product["id"].as_str().unwrap().to_owned();
        let mut updated = self.product.clone();
        if available {
            updated["publicPageAvailability"] = json!("available");
            updated["metadataUnavailable"] = json!(false);
            if updated["publicMetadataErrorCode"] == "PUBLIC_METADATA_UNAVAILABLE" {
                updated.as_object_mut().unwrap().remove("publicMetadataErrorCode");
            }
        } else {
            metadata::apply_public_response(&mut updated, &id, 404, None)?;
        }
        let mut writes = vec![];
        if updated != self.product {
            writes.push(("product".into(), id.clone(), Some(updated)));
        }
        let mut expected = vec![("product".into(), id, Some(self.product))];
        if available {
            for refusal in self.refusals {
                let id = refusal["id"].as_str().unwrap().to_owned();
                writes.push(("artifact".into(), id.clone(), None));
                expected.push(("artifact".into(), id, Some(refusal)));
            }
        }
        // A sync or fresh download may have replaced a record while HTTP was in flight.
        Ok(!writes.is_empty() && catalogue.write_many_if_unchanged(writes, expected)?)
    }
}

impl BoothService {
    pub(crate) async fn fetch_product_metadata(
        &self,
        raw: &str,
        edited: Option<(&AppHandle, &str)>,
    ) -> Result<Value> {
        let item = item_url(raw)?;
        let catalogue = &self.inner.catalogue;
        let snapshot = if let Some((app, id)) = edited {
            EditedListing::snapshot(catalogue, &item, id)?.map(|listing| (app, listing))
        } else {
            None
        };
        let client = http::build_client(Arc::new(reqwest::cookie::Jar::default()), false)?;
        let url = Url::parse(&format!("https://booth.pm/ja/items/{}.json", item.id)).unwrap();
        // Always probe the live listing, even if an earlier fetch marked it unavailable.
        let (status, body) = self.inner.http.get(&client, url).await?;
        if matches!(status.as_u16(), 404 | 410) {
            if let Some((app, snapshot)) = snapshot {
                if snapshot.record(catalogue, false)? {
                    self.changed(app, None);
                }
            }
            bail!("PUBLIC_METADATA_UNAVAILABLE");
        }
        if !status.is_success() {
            bail!("PUBLIC_METADATA_FAILED");
        }
        let payload =
            serde_json::from_str(&body).map_err(|_| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?;
        let values = serde_json::to_value(staged(&item, &payload)?)
            .map_err(|_| anyhow::anyhow!("PUBLIC_METADATA_INVALID"))?;
        if let Some((app, snapshot)) = snapshot {
            if snapshot.record(catalogue, true)? {
                self.changed(app, None);
            }
        }
        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn editor_url_fetch_accepts_only_canonical_public_booth_items() {
        for raw in [
            "https://booth.pm/items/900001",
            "https://booth.pm/ja/items/900001",
            "https://booth.pm/ko/items/900001",
            "https://booth.pm/zh-cn/items/900001",
            "https://booth.pm/zh-tw/items/900001",
            "https://synthetic.booth.pm/en/items/900001",
        ] {
            assert_eq!(item_url(raw).unwrap().id, "900001");
        }
        assert_eq!(
            item_url("https://booth.pm/items/900001")
                .unwrap()
                .source
                .as_str(),
            "https://booth.pm/ja/items/900001"
        );
        assert_eq!(
            item_url("https://synthetic.booth.pm/en/items/900001")
                .unwrap()
                .source
                .as_str(),
            "https://synthetic.booth.pm/en/items/900001"
        );
        for raw in [
            "http://booth.pm/items/900001",
            "https://user:secret@booth.pm/items/900001",
            "https://booth.pm:443/items/900001",
            "https://booth.pm/items/900001?token=PRIVATE_TOKEN",
            "https://booth.pm/items/900001#private",
            "https://booth.pm/ignored/../items/900001",
            "https://booth.pm/items/%39%30%30%30%30%31",
            "https://booth.pm/items/900001/",
            "https://booth.pm/downloadables/900001",
            "https://accounts.booth.pm/items/900001",
            "https://manage.booth.pm/items/900001",
            "https://booth.pm\\items\\900001",
            "https://boo\nth.pm/items/900001",
        ] {
            assert_eq!(
                item_url(raw).err().unwrap().to_string(),
                "INVALID_PRODUCT_URL"
            );
        }
        assert_eq!(
            item_url("https://gumroad.com/items/900001")
                .err()
                .unwrap()
                .to_string(),
            "UNSUPPORTED_PROVIDER"
        );
        assert_eq!(
            item_url("https://booth.pm.evil.test/items/900001")
                .err()
                .unwrap()
                .to_string(),
            "UNSUPPORTED_PROVIDER"
        );
    }
    #[test]
    fn fetched_editor_values_have_closed_schema_and_never_include_entitlements() {
        let payload = json!({"id":900001,"name":"<b>Authored VRChat Shader</b>","description":"Unity shader for VRChat","category":{"name":"3D"},"tags":[{"name":"VRChat"},{"name":"Shader"}],"shop":{"url":"https://synthetic.booth.pm/"},"images":[{"original":"https://booth.pximg.net/unsafe.png?token=PRIVATE_TOKEN"},{"original":"https://booth.pximg.net/authored.png"}],"cookies":"PRIVATE_SESSION","variations":[{"downloadable":{"url":"https://booth.pm/downloadables/700001?signature=PRIVATE_SIGNATURE"}}]});
        let value = serde_json::to_value(
            staged(
                &item_url("https://synthetic.booth.pm/items/900001").unwrap(),
                &payload,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(value["title"], "Authored VRChat Shader");
        assert_eq!(value["category"], "Shader");
        assert_eq!(value["tags"], json!(["VRChat", "Shader"]));
        assert_eq!(
            value["thumbnailUrl"],
            "https://booth.pximg.net/authored.png"
        );
        assert_eq!(
            value["sourceUrl"],
            "https://synthetic.booth.pm/items/900001"
        );
        let keys = ["sourceUrl", "title", "category", "tags", "thumbnailUrl"];
        assert_eq!(value.as_object().unwrap().len(), keys.len());
        assert!(value
            .as_object()
            .unwrap()
            .keys()
            .all(|key| keys.contains(&key.as_str())));
        assert!(!value.to_string().contains("PRIVATE"));
        let mut no_image = payload.clone();
        no_image["images"] = json!([]);
        assert!(serde_json::to_value(
            staged(
                &item_url("https://booth.pm/items/900001").unwrap(),
                &no_image
            )
            .unwrap()
        )
        .unwrap()["thumbnailUrl"]
            .is_null());
        assert_eq!(
            staged(
                &item_url("https://other.booth.pm/items/900001").unwrap(),
                &payload
            )
            .err()
            .unwrap()
            .to_string(),
            "PUBLIC_SOURCE_HOST_MISMATCH"
        );
        assert_eq!(
            staged(
                &item_url("https://booth.pm/items/900002").unwrap(),
                &payload
            )
            .err()
            .unwrap()
            .to_string(),
            "PUBLIC_ID_MISMATCH"
        );
    }
    #[test]
    fn manual_editor_fetch_rechecks_a_booth_listing_and_preserves_saved_files() {
        let root = std::env::temp_dir().join(format!("booth-editor-recheck-{}", uuid::Uuid::new_v4()));
        let catalogue = Catalogue::open(&root).unwrap();
        let id = "900001";
        let item = item_url("https://booth.pm/ja/items/900001").unwrap();
        catalogue.put("product", id, json!({
            "id": id, "store": "BOOTH", "sourceTitle": "Retained source title",
            "sourceUrl": item.source.as_str(), "publicPageAvailability": "available"
        })).unwrap();
        catalogue.put("remote_file", "file-1", json!({
            "id": "file-1", "productId": id, "name": "Authored.zip", "revision": "r1"
        })).unwrap();
        catalogue.put("entitlement", "grant", json!({
            "id": "grant", "productId": id, "fileIds": ["file-1"], "manifestComplete": true
        })).unwrap();
        let refusal = json!({
            "id": "pending-test", "productId": id, "fileId": "file-1",
            "status": "notDownloaded", "errorCode": "store_file_unavailable", "originalAvailable": false
        });
        catalogue.put("artifact", "pending-test", refusal.clone()).unwrap();
        let saved = json!({
            "id": "saved", "productId": id, "fileId": "other-file",
            "revision": "r0", "status": "ready", "originalAvailable": true
        });
        catalogue.put("artifact", "saved", saved.clone()).unwrap();
        let details = || catalogue.command("product_details", json!({"id": id})).unwrap();
        let snapshot = || EditedListing::snapshot(&catalogue, &item, id).unwrap().unwrap();

        assert!(EditedListing::snapshot(&catalogue, &item, "900002").unwrap().is_none());
        assert_eq!(details()["total"], 0);
        assert!(snapshot().record(&catalogue, false).unwrap());
        assert_eq!(details()["publicPageAvailability"], "unavailable");
        assert_eq!(catalogue.get("artifact", "pending-test").unwrap(), Some(refusal.clone()));

        // A refusal written during the request belongs to that later download.
        let old_snapshot = snapshot();
        let mut newer = refusal;
        newer["updatedAt"] = json!("2026-11-21T10:00:00Z");
        catalogue.put("artifact", "pending-test", newer.clone()).unwrap();
        assert!(!old_snapshot.record(&catalogue, true).unwrap());
        assert_eq!(catalogue.get("artifact", "pending-test").unwrap(), Some(newer.clone()));

        assert!(snapshot().record(&catalogue, true).unwrap());
        let recovered = details();
        assert_eq!(recovered["publicPageAvailability"], "available");
        assert_eq!(recovered["sourceTitle"], "Retained source title");
        assert_eq!(recovered["total"], 1);
        assert!(catalogue.get("artifact", "pending-test").unwrap().is_none());
        assert_eq!(catalogue.get("artifact", "saved").unwrap(), Some(saved));

        // Even when availability is already known, clearing the last refusal relists the file.
        catalogue.put("artifact", "pending-test", newer).unwrap();
        assert_eq!(details()["total"], 0);
        assert!(snapshot().record(&catalogue, true).unwrap());
        assert_eq!(details()["total"], 1);

        assert!(snapshot().record(&catalogue, false).unwrap());
        assert_eq!(details()["publicPageAvailability"], "unavailable");
        assert_eq!(details()["total"], 1); // Page errors do not revoke file grants.
        drop(catalogue);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn local_price_is_the_cheapest_digital_variation_or_the_listing_headline() {
        let item = item_url("https://booth.pm/items/900001").unwrap();
        let price = |listing: Value, variations: Value| {
            let payload =
                json!({"id":900001,"name":"Authored","price":listing,"variations":variations});
            serde_json::to_value(staged(&item, &payload).unwrap()).unwrap()["listingPrice"].clone()
        };
        assert_eq!(
            price(
                json!("¥ 300 ~"),
                json!([
                    {"id":800001,"name":"Full","type":"digital","price":4200},
                    {"id":800002,"name":"One avatar","type":"digital","price":1800},
                    {"id":800003,"name":"Printed","type":"direct","price":300}
                ])
            ),
            json!({"price":1800.0,"priceCurrency":"JPY"})
        );
        assert_eq!(
            price(
                json!("¥ 1,500 ~"),
                json!([{"id":800001,"type":"direct","price":1500}])
            ),
            json!({"price":1500.0,"priceCurrency":"JPY"})
        );
        assert_eq!(
            price(json!(0), json!([{"id":800001,"type":"digital","price":0}])),
            json!({"price":0.0,"priceCurrency":"JPY"})
        );
        assert!(price(Value::Null, json!([])).is_null());
    }
}
