use anyhow::{bail, Result};
use sha2::{Digest, Sha256};
use url::Url;

pub fn hash(value: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(value.as_ref()))
}
pub fn numeric(value: &str) -> bool {
    !value.is_empty() && value.len() <= 24 && value.bytes().all(|c| c.is_ascii_digit())
}
pub fn account_id(value: &str) -> Result<String> {
    Ok(uuid::Uuid::parse_str(value)
        .map_err(|_| anyhow::anyhow!("INVALID_ACCOUNT_ID"))?
        .to_string())
}
pub fn secure(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
}
pub fn booth_host(host: &str) -> bool {
    host == "booth.pm" || host.ends_with(".booth.pm")
}
/// Sign-in passes through pixiv and whichever provider the human picks, and providers add or
/// rename their own hops (Google's accounts.youtube.com and country-domain cookie hops, X's
/// api.x.com), so no provider host list is kept. The login window has no IPC, refuses downloads
/// and only BOOTH cookies are read from it; BOOTH's own item, download, gift and checkout pages
/// stay closed there.
pub fn auth_navigation(url: &Url) -> bool {
    let Some(url::Host::Domain(host)) = url.host().filter(|_| secure(url)) else {
        return false;
    };
    !booth_host(host)
        || ![
            "/downloadables/",
            "/gifts/",
            "/items/",
            "/cart",
            "/checkout",
        ]
        .iter()
        .any(|p| url.path().contains(p))
}
/// Any public HTTPS domain: no IP literal, credentials, other port or local name.
pub fn secure_domain(url: &Url) -> bool {
    secure(url)
        && matches!(url.host(), Some(url::Host::Domain(_)))
        && crate::stores::http::public_https(url)
}
/// BOOTH's own download start may hand the file to any HTTPS host, as its CDN and signing
/// hosts change. The request jar sends BOOTH cookies only to BOOTH.
pub fn download_destination(url: &Url) -> bool {
    secure_domain(url)
}
/// BOOTH's own sign-in page. Another host's path that merely mentions a login is not one.
pub fn sign_in_page(url: &Url) -> bool {
    url.host_str().is_some_and(booth_host)
        && (url.path().contains("sign_in") || url.path().contains("login"))
}
pub fn item_id(value: &str) -> Option<String> {
    let url = Url::parse(value).ok()?;
    if !secure(&url)
        || !url
            .host_str()
            .is_some_and(|h| booth_host(h) && !matches!(h, "accounts.booth.pm" | "manage.booth.pm"))
    {
        return None;
    }
    let pieces: Vec<_> = url.path_segments()?.collect();
    let id = match pieces.as_slice() {
        ["items", id] => *id,
        [locale, "items", id] if locale.bytes().all(|c| c.is_ascii_alphabetic() || c == b'-') => {
            *id
        }
        _ => return None,
    };
    numeric(id).then(|| id.to_owned())
}
pub fn public_url(value: &str, kind: &str) -> Option<String> {
    let mut url = Url::parse(value).ok()?;
    if !secure(&url) {
        return None;
    }
    let host = url.host_str()?;
    let valid = match kind {
        "item" => item_id(value).is_some(),
        "shop" => {
            booth_host(host)
                && !matches!(
                    host,
                    "booth.pm" | "accounts.booth.pm" | "manage.booth.pm" | "www.booth.pm"
                )
                && url.path() == "/"
        }
        "preview" => host == "booth.pximg.net" || host.ends_with(".pximg.net"),
        _ => false,
    };
    if !valid {
        return None;
    }
    if kind == "item" && host == "booth.pm" && url.path().starts_with("/items/") {
        url.set_path(&format!("/ja{}", url.path()));
    }
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}
/// Normalize only observed first-party canonical identity for the live item parser.
pub(super) fn observed_product_url(value: &str) -> Option<String> {
    let path = value.strip_prefix("https://booth.pm")?;
    let pieces: Vec<_> = path.split('/').collect();
    match pieces.as_slice() {
        ["", "items", id] if numeric(id) => Some(format!("https://booth.pm/ja/items/{id}")),
        ["", locale, "items", id] if matches!(*locale, "ja" | "en") && numeric(id) => {
            Some(value.to_owned())
        }
        _ => None,
    }
}
pub(super) fn free_item_id(route: &str) -> Option<&str> {
    route
        .strip_prefix("https://booth.pm/ja/items/")
        .filter(|id| numeric(id))
}
pub(super) fn free_item_route(id: &str) -> Result<String> {
    if !numeric(id) {
        bail!("INVALID_FREE_ITEM_ID");
    }
    Ok(format!("https://booth.pm/ja/items/{id}"))
}
/// Audio tracks are kept as MP3, which Unity imports directly; it is the only
/// rendition a download route may request.
pub const TRACK_FORMAT: &str = "mp3";
pub fn resolver(value: &str) -> Option<(String, Option<String>, String)> {
    let url = Url::parse(value).ok()?;
    if !secure(&url) || url.host_str()? != "booth.pm" {
        return None;
    }
    let id = url.path().strip_prefix("/downloadables/")?;
    if !numeric(id) {
        return None;
    }
    let (mut variant, mut format) = (None, false);
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "variation_id" if numeric(&value) && variant.is_none() => {
                variant = Some(value.into_owned())
            }
            "type" if value == TRACK_FORMAT && !format => format = true,
            _ => return None,
        }
    }
    let route = format!(
        "https://booth.pm/downloadables/{id}{}",
        variant
            .as_ref()
            .map(|v| format!("?variation_id={v}"))
            .unwrap_or_default()
    );
    Some((id.to_owned(), variant, route))
}
/// Read a DOM control's stable IDs, discarding all signing parameters immediately.
/// Unlike resolver(), this never supplies an untrusted URL to the HTTP client.
pub fn observed_resolver(value: &str) -> Option<(String, Option<String>, String)> {
    let url = Url::parse(value).ok()?;
    if !secure(&url) || url.host_str() != Some("booth.pm") {
        return None;
    }
    let observed = url.path().strip_prefix("/downloadables/")?;
    let id = observed.strip_suffix("/deeplink").unwrap_or(observed);
    if !numeric(id) {
        return None;
    }
    let mut variant = None;
    for (key, value) in url.query_pairs() {
        if key == "variation_id" {
            if variant.is_some() || !numeric(&value) {
                return None;
            }
            variant = Some(value.into_owned());
        }
    }
    let route = format!(
        "https://booth.pm/downloadables/{id}{}",
        variant
            .as_ref()
            .map(|v| format!("?variation_id={v}"))
            .unwrap_or_default()
    );
    Some((id.into(), variant, route))
}
pub fn source_path(source: &str) -> Result<&'static str> {
    match source {
        "library" => Ok("/library"),
        "purchase" | "orders" => Ok("/orders"),
        "gift" | "gifts" => Ok("/library/gifts"),
        "free" => Ok("/library/free_downloads"),
        _ => bail!("INVALID_SOURCE"),
    }
}
pub fn source_name(source: &str) -> Result<&'static str> {
    match source {
        "library" => Ok("library"),
        "purchase" | "orders" => Ok("purchase"),
        "gift" | "gifts" => Ok("gift"),
        "free" => Ok("free"),
        _ => bail!("INVALID_SOURCE"),
    }
}
/// BOOTH's "recently updated first" Library order (announcement 970), the one sort a route takes.
pub const LIBRARY_UPDATED_SORT: &str = "file_updated_at_desc";
pub fn updated_route(page: u32) -> String {
    format!("/library?sort={LIBRARY_UPDATED_SORT}&page={page}")
}
/// Whether a canonical route from `route()` reads that order.
pub fn updated_order(route: &str) -> bool {
    route.starts_with("/library?sort=")
}
pub fn route(value: &str, source: &str) -> Result<Url> {
    let base = Url::parse("https://accounts.booth.pm").unwrap();
    let mut url = base
        .join(value)
        .map_err(|_| anyhow::anyhow!("INVALID_SOURCE_ROUTE"))?;
    if !secure(&url) || url.host_str() != Some("accounts.booth.pm") || url.fragment().is_some() {
        bail!("INVALID_SOURCE_ROUTE");
    }
    let path = source_path(source)?;
    if url.path() == path {
        // BOOTH's canonical first-page anchor/response may omit the query, and its pager
        // orders a sorted link's parameters its own way. Every route leaves here one way.
        let (mut page, mut sorted) = (None, false);
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "page"
                    if page.is_none()
                        && numeric(&value)
                        && value.parse::<u32>().unwrap_or(0) != 0 =>
                {
                    page = Some(value.into_owned());
                }
                "sort" if !sorted && path == "/library" && value == LIBRARY_UPDATED_SORT => {
                    sorted = true;
                }
                _ => bail!("INVALID_SOURCE_ROUTE"),
            }
        }
        if url.query().is_some() && page.is_none() && !sorted {
            bail!("INVALID_SOURCE_ROUTE");
        }
        let page = page.unwrap_or_else(|| "1".into());
        url.set_query(Some(&if sorted {
            format!("sort={LIBRARY_UPDATED_SORT}&page={page}")
        } else {
            format!("page={page}")
        }));
    } else if path != "/orders"
        || !url.path().strip_prefix("/orders/").is_some_and(numeric)
        || url.query().is_some()
    {
        bail!("INVALID_SOURCE_ROUTE");
    }
    Ok(url)
}
pub fn short(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(4000)
        .collect()
}

#[cfg(test)]
#[path = "safety_tests.rs"]
mod tests;
