use super::safety;
use anyhow::{bail, Result};
use reqwest::{Client, StatusCode};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Semaphore};
use url::Url;

/// Attempts for a dropped connection, a cut-off body, 5xx or 429/503, waiting 2, 4, 8 and 16 s:
/// long enough to ride out a Wi-Fi drop or waking from sleep, still bounded.
const ATTEMPTS: u32 = 5;

/// One listing and two detail requests across accounts, with shared request-start pacing.
#[derive(Clone)]
pub struct MetadataHttp {
    pace: Arc<Mutex<Instant>>,
    metadata: Arc<Semaphore>,
    listings: Arc<Semaphore>,
}
impl MetadataHttp {
    async fn wait_start(&self) {
        loop {
            let wait = {
                let mut next = self.pace.lock().await;
                let now = Instant::now();
                if *next <= now {
                    *next = now + start_spacing(uuid::Uuid::new_v4().as_bytes()[0]);
                    return;
                }
                *next
            };
            // Recheck after sleeping: another request's Retry-After can defer
            // waiters that have not started yet. No timestamp lock spans I/O.
            tokio::time::sleep_until(tokio::time::Instant::from_std(wait)).await;
        }
    }
    async fn defer(&self, delay: Duration) {
        let mut next = self.pace.lock().await;
        *next = (*next).max(Instant::now() + delay);
    }
    pub fn new() -> Self {
        Self {
            pace: Arc::new(Mutex::new(Instant::now())),
            metadata: Arc::new(Semaphore::new(2)),
            listings: Arc::new(Semaphore::new(1)),
        }
    }
    pub async fn resolve(&self, client: &Client, url: Url) -> Result<reqwest::Response> {
        if safety::resolver(url.as_str()).is_none() {
            bail!("INVALID_RESOLVER");
        }
        let _request = self
            .metadata
            .acquire()
            .await
            .map_err(|_| anyhow::anyhow!("HTTP_CLIENT_FAILED"))?;
        for attempt in 0..ATTEMPTS {
            self.wait_start().await;
            let response = match client
                .get(url.clone())
                .timeout(Duration::from_secs(45))
                .send()
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    let code = transport_code(&error);
                    if code == "NETWORK_UNAVAILABLE" && attempt + 1 < ATTEMPTS {
                        self.defer(Duration::from_secs(2u64.pow(attempt + 1))).await;
                        continue;
                    }
                    // One file sent to sign in is refused alone: only an account-level
                    // listing can say the session is signed out.
                    bail!(
                        "{}",
                        match code {
                            "NETWORK_UNAVAILABLE" => "DOWNLOAD_RESOLVER_FAILED",
                            "AUTH_REQUIRED" => "PERMISSION_DENIED",
                            code => code,
                        }
                    );
                }
            };
            let status = response.status();
            if status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::SERVICE_UNAVAILABLE
            {
                let wait = retry_after(
                    response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok()),
                    std::time::SystemTime::now(),
                );
                self.defer(wait.max(Duration::from_secs(2u64.pow(attempt + 1))))
                    .await;
                if attempt + 1 == ATTEMPTS || wait > Duration::from_secs(120) {
                    bail!("RATE_LIMITED");
                }
                continue;
            }
            return Ok(response);
        }
        bail!("DOWNLOAD_RESOLVER_FAILED")
    }
    pub async fn get(&self, client: &Client, url: Url) -> Result<(StatusCode, String)> {
        if let Some(limit) = crate::verification::source_page_limit() {
            if listing_url(&url)
                && url.query_pairs().any(|(key, value)| {
                    key == "page" && value.parse::<usize>().is_ok_and(|page| page > limit)
                })
            {
                bail!("VERIFICATION_PAGE_LIMIT");
            }
        }
        if !safety::secure(&url) || !url.host_str().is_some_and(safety::booth_host) {
            bail!("INVALID_METADATA_URL");
        }
        let gate = if listing_url(&url) {
            &self.listings
        } else {
            &self.metadata
        };
        let _request = gate
            .acquire()
            .await
            .map_err(|_| anyhow::anyhow!("HTTP_CLIENT_FAILED"))?;
        for attempt in 0..ATTEMPTS {
            self.wait_start().await;
            crate::support_trace::request(&url, attempt);
            let response = match client.get(url.clone()).send().await {
                Ok(r) => r,
                Err(error) => {
                    let code = transport_code(&error);
                    crate::support_trace::transport(
                        &url,
                        crate::support_trace::network_cause(&error),
                    );
                    if code == "UNSAFE_REDIRECT" {
                        crate::support_trace::redirect_rejected(&url);
                    }
                    if code == "NETWORK_UNAVAILABLE" && attempt + 1 < ATTEMPTS {
                        let wait = Duration::from_secs(2u64.pow(attempt + 1));
                        crate::support_trace::backoff(&url, wait);
                        self.defer(wait).await;
                        continue;
                    }
                    bail!(
                        "{}",
                        if code == "AUTH_REQUIRED" {
                            signed_out(&url)
                        } else {
                            code
                        }
                    );
                }
            };
            crate::support_trace::response(&url, response.status().as_u16());
            if !safety::secure_domain(response.url()) {
                crate::support_trace::redirect_rejected(&url);
                bail!("UNSAFE_REDIRECT");
            }
            let status = response.status();
            if status == StatusCode::UNAUTHORIZED || safety::sign_in_page(response.url()) {
                bail!("{}", signed_out(&url));
            }
            if status == StatusCode::FORBIDDEN {
                bail!("PERMISSION_DENIED");
            }
            if let Err(error) = validate_source_response(&url, response.url()) {
                crate::support_trace::redirect_rejected(&url);
                return Err(error);
            }
            if status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::SERVICE_UNAVAILABLE
            {
                let wait = retry_after(
                    response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok()),
                    std::time::SystemTime::now(),
                );
                let wait = wait.max(Duration::from_secs(2u64.pow(attempt + 1)));
                crate::support_trace::backoff(&url, wait);
                self.defer(wait).await;
                if attempt + 1 == ATTEMPTS || wait > Duration::from_secs(120) {
                    bail!("RATE_LIMITED");
                }
                continue;
            }
            if status.is_server_error() && attempt + 1 < ATTEMPTS {
                let wait = Duration::from_secs(2u64.pow(attempt + 1));
                crate::support_trace::backoff(&url, wait);
                self.defer(wait).await;
                continue;
            }
            if response
                .content_length()
                .is_some_and(|n| n > 8 * 1024 * 1024)
            {
                bail!("METADATA_TOO_LARGE");
            }
            let mut body = Vec::new();
            let mut response = response;
            let interrupted = loop {
                match response.chunk().await {
                    Ok(Some(chunk)) => {
                        if body.len() + chunk.len() > 8 * 1024 * 1024 {
                            bail!("METADATA_TOO_LARGE");
                        }
                        body.extend_from_slice(&chunk);
                    }
                    Ok(None) => break false,
                    Err(error) => {
                        crate::support_trace::transport(
                            &url,
                            crate::support_trace::network_cause(&error),
                        );
                        break true;
                    }
                }
            };
            // A body cut off after its headers is the same transient failure
            // as a dropped request, with the same bounded retry.
            if interrupted {
                if attempt + 1 < ATTEMPTS {
                    let wait = Duration::from_secs(2u64.pow(attempt + 1));
                    crate::support_trace::backoff(&url, wait);
                    self.defer(wait).await;
                    continue;
                }
                bail!("NETWORK_UNAVAILABLE");
            }
            return Ok((
                status,
                String::from_utf8(body).map_err(|_| anyhow::anyhow!("METADATA_ENCODING"))?,
            ));
        }
        bail!("NETWORK_UNAVAILABLE")
    }
}
// Symmetric rounding keeps the mean at 400ms for a uniformly random byte.
fn start_spacing(sample: u8) -> Duration {
    Duration::from_millis(350 + (u64::from(sample) * 100 + 127) / 255)
}
fn listing_url(url: &Url) -> bool {
    url.host_str() == Some("accounts.booth.pm")
        && matches!(
            url.path(),
            "/library" | "/library/gifts" | "/library/free_downloads" | "/orders"
        )
}
/// Only an account-level listing can say the session is signed out. Any other page sent
/// to sign in, including every cookie-less public page, is refused for that page alone.
fn signed_out(requested: &Url) -> &'static str {
    if listing_url(requested) {
        "AUTH_REQUIRED"
    } else {
        "PERMISSION_DENIED"
    }
}
fn validate_source_response(requested: &Url, actual: &Url) -> Result<()> {
    if let Some(id) = safety::free_item_id(requested.as_str()) {
        // Any HTTPS BOOTH page for the same item: a locale prefix or the shop's own host.
        if safety::item_id(actual.as_str()).as_deref() != Some(id) {
            bail!("RESPONSE_ROUTE_CHANGED");
        }
        return Ok(());
    }
    if requested.host_str() != Some("accounts.booth.pm") {
        return Ok(());
    }
    let source = match requested.path() {
        "/library" => "library",
        "/library/gifts" => "gift",
        "/library/free_downloads" => "free",
        "/orders" => "purchase",
        path if path.strip_prefix("/orders/").is_some_and(safety::numeric) => "purchase",
        _ => bail!("INVALID_SOURCE_ROUTE"),
    };
    let requested = safety::route(requested.as_str(), source)?;
    let actual = safety::route(actual.as_str(), source)
        .map_err(|_| anyhow::anyhow!("RESPONSE_ROUTE_CHANGED"))?;
    if requested != actual {
        bail!("RESPONSE_ROUTE_CHANGED");
    }
    Ok(())
}
fn retry_after(value: Option<&str>, now: std::time::SystemTime) -> Duration {
    value
        .and_then(|s| {
            s.parse::<u64>().ok().map(Duration::from_secs).or_else(|| {
                chrono::DateTime::parse_from_rfc2822(s).ok().and_then(|d| {
                    let seconds = now.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
                    Some(Duration::from_secs(
                        d.timestamp().saturating_sub(seconds).max(0) as u64,
                    ))
                })
            })
        })
        // An unrepresentable header is invalid, just like malformed text.
        // defer() must be able to form its monotonic deadline before returning.
        .filter(|delay| Instant::now().checked_add(*delay).is_some())
        .unwrap_or(Duration::from_secs(5))
}
/// Pages and downloads may move to any HTTPS host; the jar sends BOOTH cookies only to
/// BOOTH. A hop to pixiv is BOOTH sending the request to sign in.
fn redirect_error(url: &Url) -> Option<&'static str> {
    if safety::auth_navigation(url)
        && url
            .host_str()
            .is_some_and(|h| h == "pixiv.net" || h.ends_with(".pixiv.net"))
    {
        Some("AUTH_REQUIRED")
    } else if safety::secure_domain(url) {
        None
    } else {
        Some("UNSAFE_REDIRECT")
    }
}
pub fn build_client(jar: Arc<reqwest::cookie::Jar>, download: bool) -> Result<Client> {
    let policy = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 8 {
            return attempt.error("REDIRECT_LIMIT");
        }
        match redirect_error(attempt.url()) {
            None => attempt.follow(),
            Some(code) => attempt.error(code),
        }
    });
    let builder = Client::builder()
        .cookie_provider(jar)
        .redirect(policy)
        // A hop may leave BOOTH; a signed URL must not reach the next host as a Referer.
        .referer(false)
        .connect_timeout(Duration::from_secs(20))
        .user_agent(concat!(
            "NekoNekoAsset/",
            env!("CARGO_PKG_VERSION"),
            " (Windows; private asset manager)"
        ));
    let builder = if download {
        builder.read_timeout(Duration::from_secs(60))
    } else {
        builder.timeout(Duration::from_secs(45))
    };
    builder
        .build()
        .map_err(|_| anyhow::anyhow!("HTTP_CLIENT_FAILED"))
}
fn transport_code(error: &reqwest::Error) -> &'static str {
    // Inspect fixed sentinel codes in the local error chain; never return URL-bearing messages.
    use std::error::Error;
    let mut source = error.source();
    while let Some(error) = source {
        let message = error.to_string();
        if message == "AUTH_REQUIRED" {
            return "AUTH_REQUIRED";
        }
        if message == "UNSAFE_REDIRECT" || message == "REDIRECT_LIMIT" {
            return "UNSAFE_REDIRECT";
        }
        source = error.source();
    }
    "NETWORK_UNAVAILABLE"
}
#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
