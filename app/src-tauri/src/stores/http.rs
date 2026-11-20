//! Provider-scoped native metadata transport, shared by the two new adapters.
use super::StoreProvider;
use anyhow::{bail, ensure, Result};
use reqwest::{Client, Method, Response, StatusCode};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Semaphore};
use url::Url;

/// Attempts for a dropped connection, a cut-off body, 5xx or 429, waiting 2, 4, 8 and 16 s:
/// long enough to ride out a Wi-Fi drop or waking from sleep, still bounded.
const ATTEMPTS: u32 = 5;

pub fn metadata_destination(provider: StoreProvider, url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && url.host_str().is_some_and(|host| match provider {
            StoreProvider::Booth => host == "booth.pm" || host.ends_with(".booth.pm"),
            StoreProvider::Gumroad => host == "gumroad.com" || host.ends_with(".gumroad.com"),
            StoreProvider::Jinxxy => super::session::jinxxy_host(host),
        })
}

/// Any public HTTPS host. Stores move pages, files and downloads between hosts and CDNs
/// without notice; local and private addresses stay excluded. Store cookies still reach
/// only the store's own domains (the jar matches by domain), and reqwest drops Cookie and
/// Authorization headers on a cross-host redirect.
pub fn public_https(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && match url.host() {
            Some(url::Host::Ipv4(ip)) => public_ip(ip.into()),
            Some(url::Host::Ipv6(ip)) => public_ip(ip.into()),
            Some(url::Host::Domain(host)) => {
                host.contains('.')
                    && !host.ends_with('.')
                    && !host.ends_with(".localhost")
                    && !host.ends_with(".local")
                    && !host.ends_with(".internal")
            }
            None => false,
        }
}

pub fn public_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => {
            let octets = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_broadcast()
                && !ip.is_documentation()
                && octets[0] != 0
                && octets[0] < 224
                && !(octets[0] == 100 && (64..=127).contains(&octets[1]))
                && !(octets[0] == 198 && matches!(octets[1], 18 | 19))
                && !(octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        }
        std::net::IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return public_ip(mapped.into());
            }
            let segments = ip.segments();
            // Native public unicast only; exclude documentation and transition ranges.
            segments[0] & 0xe000 == 0x2000
                && !(segments[0] == 0x2001 && segments[1] <= 0x1ff)
                && !(segments[0] == 0x2001 && segments[1] == 0xdb8)
                && segments[0] != 0x2002
        }
    }
}

/// A file reached from the store's own file link may sit on any public HTTPS host,
/// but the store's sign-in page is never a file.
pub fn download_destination(provider: StoreProvider, url: &Url) -> bool {
    public_https(url) && !(metadata_destination(provider, url) && login_path(url))
}

fn login_path(url: &Url) -> bool {
    matches!(url.path(), "/login" | "/two-factor" | "/sign_in")
}

fn resolver_login_redirect(provider: StoreProvider, response: &Response) -> bool {
    if provider != StoreProvider::Gumroad || !response.status().is_redirection() {
        return false;
    }
    response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| response.url().join(value).ok())
        .is_some_and(|url| {
            metadata_destination(provider, &url)
                && url.host_str() == Some("gumroad.com")
                && login_path(&url)
        })
}

pub fn build_client(provider: StoreProvider, jar: Arc<reqwest::cookie::Jar>) -> Result<Client> {
    build_provider_client(provider, jar, false, false)
}

/// Large payloads have an inactivity bound, never a whole-transfer deadline.
pub fn build_download_client(
    provider: StoreProvider,
    jar: Arc<reqwest::cookie::Jar>,
) -> Result<Client> {
    build_provider_client(provider, jar, false, true)
}

/// HEAD returns the source Location unchanged: its capability is signed for a later GET.
pub fn build_head_client(
    provider: StoreProvider,
    jar: Arc<reqwest::cookie::Jar>,
) -> Result<Client> {
    build_provider_client(provider, jar, true, false)
}

fn build_provider_client(
    provider: StoreProvider,
    jar: Arc<reqwest::cookie::Jar>,
    head: bool,
    download: bool,
) -> Result<Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    if provider == StoreProvider::Jinxxy {
        headers.insert(
            reqwest::header::ACCEPT_LANGUAGE,
            reqwest::header::HeaderValue::from_static("en-US"),
        );
    }
    // A hop may leave the store; a signed URL must not reach the next host as a Referer.
    let builder = Client::builder()
        .referer(false)
        .cookie_provider(jar)
        .default_headers(headers)
        .redirect(if head {
            reqwest::redirect::Policy::none()
        } else {
            // Session and download requests may follow any public HTTPS hop.
            reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 8 {
                    attempt.error("REDIRECT_LIMIT")
                } else if public_https(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.error("UNSAFE_REDIRECT")
                }
            })
        })
        .connect_timeout(Duration::from_secs(20));
    let builder = if download {
        builder.read_timeout(Duration::from_secs(60))
    } else {
        builder.timeout(Duration::from_secs(45))
    };
    builder
        .user_agent(concat!(
            "NekoNekoAsset/",
            env!("CARGO_PKG_VERSION"),
            " (private asset manager)"
        ))
        .build()
        .map_err(|_| anyhow::anyhow!("HTTP_CLIENT_FAILED"))
}

/// Gumroad's bound file controls and their redirects; see download_destination.
pub fn gumroad_file_destination(url: &Url) -> bool {
    download_destination(StoreProvider::Gumroad, url)
}

fn jinxxy_graphql_url(origin: &Url) -> Result<Url> {
    ensure!(
        metadata_destination(StoreProvider::Jinxxy, origin)
            && origin.path() == "/"
            && origin.query().is_none()
            && origin.fragment().is_none(),
        "INVALID_METADATA_URL"
    );
    origin
        .join("graphql")
        .map_err(|_| anyhow::anyhow!("INVALID_METADATA_URL"))
}

#[derive(Clone)]
pub struct MetadataHttp {
    provider: StoreProvider,
    pace: Arc<Mutex<Instant>>,
    metadata: Arc<Semaphore>,
    metrics: Arc<RequestMetrics>,
    classified: Arc<[RequestMetrics; 2]>,
    #[cfg(test)]
    authored_origin: Option<Url>,
}
#[derive(Default)]
struct RequestMetrics {
    requests: AtomicU64,
    wait_ms: AtomicU64,
    response_ms: AtomicU64,
    body_ms: AtomicU64,
    retries: AtomicU64,
    rate_limits: AtomicU64,
    server_errors: AtomicU64,
    transport_errors: AtomicU64,
    client_errors: AtomicU64,
    timeouts: AtomicU64,
    tls_errors: AtomicU64,
    dns_errors: AtomicU64,
    connect_errors: AtomicU64,
}
#[derive(Clone, Copy)]
pub enum RequestKind {
    Content,
    Public,
}
impl RequestMetrics {
    fn snapshot(&self) -> [u64; 13] {
        [
            &self.requests,
            &self.wait_ms,
            &self.response_ms,
            &self.body_ms,
            &self.retries,
            &self.rate_limits,
            &self.server_errors,
            &self.transport_errors,
            &self.client_errors,
            &self.timeouts,
            &self.tls_errors,
            &self.dns_errors,
            &self.connect_errors,
        ]
        .map(|counter| counter.load(Ordering::Relaxed))
    }
}

impl MetadataHttp {
    pub fn new(provider: StoreProvider) -> Self {
        Self {
            provider,
            pace: Arc::new(Mutex::new(Instant::now())),
            metadata: Arc::new(Semaphore::new(2)),
            metrics: Arc::new(RequestMetrics::default()),
            classified: Arc::new(std::array::from_fn(|_| RequestMetrics::default())),
            #[cfg(test)]
            authored_origin: None,
        }
    }
    pub fn classified_metrics(&self) -> [[u64; 13]; 2] {
        self.classified.each_ref().map(|metrics| metrics.snapshot())
    }
    pub fn classified_since(&self, before: [[u64; 13]; 2]) -> serde_json::Value {
        let after = self.classified_metrics();
        let names = [
            "requests",
            "pacingWaitMs",
            "responseWaitMs",
            "bodyReadMs",
            "retries",
            "rateLimits",
            "serverErrors",
            "transportErrors",
            "clientErrors",
            "timeouts",
            "tlsErrors",
            "dnsErrors",
            "connectErrors",
        ];
        let mut result = serde_json::Map::new();
        for (i, label) in ["content", "public"].iter().enumerate() {
            let fields: serde_json::Map<_, _> = names
                .iter()
                .enumerate()
                .map(|(field, name)| {
                    (
                        (*name).into(),
                        serde_json::json!(after[i][field].saturating_sub(before[i][field])),
                    )
                })
                .collect();
            result.insert((*label).into(), serde_json::Value::Object(fields));
        }
        serde_json::Value::Object(result)
    }
    fn record(&self, kind: Option<RequestKind>, update: impl Fn(&RequestMetrics)) {
        update(&self.metrics);
        if let Some(kind) = kind {
            update(&self.classified[kind as usize]);
        }
    }
    pub fn metrics(&self) -> [u64; 3] {
        [
            self.metrics.requests.load(Ordering::Relaxed),
            self.metrics.wait_ms.load(Ordering::Relaxed),
            self.metrics.response_ms.load(Ordering::Relaxed),
        ]
    }

    pub(super) fn start_spacing_ms(&self) -> u64 {
        if self.provider == StoreProvider::Gumroad {
            500
        } else {
            1000
        }
    }

    async fn wait_start(&self) {
        loop {
            let at = {
                let mut next = self.pace.lock().await;
                let now = Instant::now();
                if *next <= now {
                    *next = now + Duration::from_millis(self.start_spacing_ms());
                    return;
                }
                *next
            };
            // A concurrent response may extend Retry-After while this caller waits.
            tokio::time::sleep_until(tokio::time::Instant::from_std(at)).await;
        }
    }

    async fn defer(&self, wait: Duration) {
        let mut next = self.pace.lock().await;
        *next = (*next).max(Instant::now() + wait);
    }

    async fn request(
        &self,
        client: &Client,
        url: Url,
        resolver: bool,
        method: Method,
        body: Option<&serde_json::Value>,
        kind: Option<RequestKind>,
    ) -> Result<Response> {
        let destination = |url: &Url| {
            #[cfg(test)]
            if self
                .authored_origin
                .as_ref()
                .is_some_and(|origin| url.origin() == origin.origin())
            {
                return true;
            }
            metadata_destination(self.provider, url)
        };
        ensure!(destination(&url), "INVALID_METADATA_URL");
        for attempt in 0..ATTEMPTS {
            let waiting = Instant::now();
            self.wait_start().await;
            let wait_ms = waiting.elapsed().as_millis() as u64;
            self.record(kind, |metrics| {
                metrics.wait_ms.fetch_add(wait_ms, Ordering::Relaxed);
                metrics.requests.fetch_add(1, Ordering::Relaxed);
                if attempt > 0 {
                    metrics.retries.fetch_add(1, Ordering::Relaxed);
                }
            });
            let started = Instant::now();
            let mut request = client.request(method.clone(), url.clone());
            if let Some(body) = body {
                request = request.json(body);
            }
            let sent = request.send().await;
            let response_ms = started.elapsed().as_millis() as u64;
            let cause = sent.as_ref().err().map(crate::support_trace::network_cause);
            self.record(kind, |metrics| {
                metrics
                    .response_ms
                    .fetch_add(response_ms, Ordering::Relaxed);
                if let Some(cause) = cause {
                    metrics.transport_errors.fetch_add(1, Ordering::Relaxed);
                    match cause {
                        "timeout" => Some(&metrics.timeouts),
                        "tls" => Some(&metrics.tls_errors),
                        "dns" => Some(&metrics.dns_errors),
                        "connect" => Some(&metrics.connect_errors),
                        _ => None,
                    }
                    .map(|counter| counter.fetch_add(1, Ordering::Relaxed));
                }
            });
            let response = match sent {
                Ok(response) => response,
                Err(error) if error.is_redirect() => bail!("UNSAFE_REDIRECT"),
                Err(_) if attempt + 1 < ATTEMPTS => {
                    self.defer(Duration::from_secs(1 << (attempt + 1))).await;
                    continue;
                }
                Err(_) => bail!("NETWORK_UNAVAILABLE"),
            };
            self.record(kind, |metrics| {
                if response.status() == StatusCode::TOO_MANY_REQUESTS {
                    metrics.rate_limits.fetch_add(1, Ordering::Relaxed);
                }
                if response.status().is_server_error() {
                    metrics.server_errors.fetch_add(1, Ordering::Relaxed);
                }
                if response.status().is_client_error() {
                    metrics.client_errors.fetch_add(1, Ordering::Relaxed);
                }
            });
            let store = destination(response.url());
            ensure!(store || public_https(response.url()), "UNSAFE_REDIRECT");
            // Only the store's own answer can say that its session is signed out.
            if (store
                && (login_path(response.url())
                    || (!resolver && response.status() == StatusCode::UNAUTHORIZED)))
                || (resolver && resolver_login_redirect(self.provider, &response))
            {
                bail!("AUTH_REQUIRED");
            }
            if resolver && response.status() == StatusCode::UNAUTHORIZED {
                // A single file capability can be denied independently of the
                // buyer session. Only account metadata/login evidence expires it.
                bail!("DOWNLOAD_RESOLVER_FAILED");
            }
            if response.status() == StatusCode::FORBIDDEN {
                bail!("PERMISSION_DENIED");
            }
            if response.status() == StatusCode::TOO_MANY_REQUESTS
                || response.status().is_server_error()
            {
                let wait = retry_after(
                    response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|value| value.to_str().ok()),
                );
                self.defer(wait.max(Duration::from_secs(1 << (attempt + 1))))
                    .await;
                if attempt + 1 == ATTEMPTS || wait > Duration::from_secs(120) {
                    if response.status() == StatusCode::TOO_MANY_REQUESTS {
                        bail!("RATE_LIMITED");
                    }
                    bail!("NETWORK_UNAVAILABLE");
                }
                continue;
            }
            return Ok(response);
        }
        bail!("NETWORK_UNAVAILABLE")
    }

    /// Two bounded bodies can overlap; all starts share provider pacing and backoff.
    pub async fn get(&self, client: &Client, url: Url) -> Result<(StatusCode, String)> {
        let (status, body, _) = self.get_with_url(client, url).await?;
        Ok((status, body))
    }

    pub async fn get_with_url(
        &self,
        client: &Client,
        url: Url,
    ) -> Result<(StatusCode, String, Url)> {
        self.get_kind(client, url, None).await
    }
    pub async fn get_classified(
        &self,
        client: &Client,
        url: Url,
        kind: RequestKind,
    ) -> Result<(StatusCode, String)> {
        let (status, body, _) = self.get_kind(client, url, Some(kind)).await?;
        Ok((status, body))
    }
    async fn get_kind(
        &self,
        client: &Client,
        url: Url,
        kind: Option<RequestKind>,
    ) -> Result<(StatusCode, String, Url)> {
        let _permit = self
            .metadata
            .acquire()
            .await
            .map_err(|_| anyhow::anyhow!("HTTP_CLIENT_FAILED"))?;
        for attempt in 0..ATTEMPTS {
            let response = self
                .request(client, url.clone(), false, Method::GET, None, kind)
                .await?;
            let final_url = response.url().clone();
            let started = Instant::now();
            let read = Self::read_metadata(response).await;
            let body_ms = started.elapsed().as_millis() as u64;
            self.record(kind, |metrics| {
                metrics.body_ms.fetch_add(body_ms, Ordering::Relaxed);
            });
            match read {
                // A body cut off after its headers is the same transient failure as a
                // dropped request, with the same bounded retry.
                Err(error)
                    if error.to_string() == "NETWORK_UNAVAILABLE" && attempt + 1 < ATTEMPTS =>
                {
                    self.record(kind, |metrics| {
                        metrics.transport_errors.fetch_add(1, Ordering::Relaxed);
                        metrics.retries.fetch_add(1, Ordering::Relaxed);
                    });
                    self.defer(Duration::from_secs(1 << (attempt + 1))).await;
                }
                read => {
                    let (status, body) = read?;
                    return Ok((status, body, final_url));
                }
            }
        }
        bail!("NETWORK_UNAVAILABLE")
    }

    /// The one buyer download-capability mutation; no arbitrary service POST surface.
    pub async fn post_json(
        &self,
        client: &Client,
        origin: &Url,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        ensure!(
            self.provider == StoreProvider::Jinxxy,
            "INVALID_METADATA_URL"
        );
        let _permit = self
            .metadata
            .acquire()
            .await
            .map_err(|_| anyhow::anyhow!("HTTP_CLIENT_FAILED"))?;
        let url = jinxxy_graphql_url(origin)?;
        let response = self
            .request(client, url.clone(), false, Method::POST, Some(body), None)
            .await?;
        ensure!(response.url() == &url, "UNSAFE_REDIRECT");
        let (status, body) = Self::read_metadata(response).await?;
        ensure!(status.is_success(), "JINXXY_DOWNLOAD_UNAVAILABLE");
        serde_json::from_str(&body).map_err(|_| anyhow::anyhow!("JINXXY_DOWNLOAD_UNPROVEN"))
    }

    async fn read_metadata(mut response: Response) -> Result<(StatusCode, String)> {
        let status = response.status();
        ensure!(
            response
                .content_length()
                .is_none_or(|bytes| bytes <= 8 * 1024 * 1024),
            "METADATA_TOO_LARGE"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("NETWORK_UNAVAILABLE"))?
        {
            ensure!(
                bytes.len() + chunk.len() <= 8 * 1024 * 1024,
                "METADATA_TOO_LARGE"
            );
            bytes.extend_from_slice(&chunk);
        }
        Ok((
            status,
            String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("METADATA_ENCODING"))?,
        ))
    }

    /// Header-only capability resolution uses the same provider request-start spacing.
    pub async fn resolve_head(&self, client: &Client, url: Url) -> Result<Response> {
        ensure!(
            metadata_destination(self.provider, &url),
            "INVALID_METADATA_URL"
        );
        let _permit = self
            .metadata
            .acquire()
            .await
            .map_err(|_| anyhow::anyhow!("HTTP_CLIENT_FAILED"))?;
        self.request(client, url, true, Method::HEAD, None, None)
            .await
    }
}

fn retry_after(value: Option<&str>) -> Duration {
    value
        .and_then(|value| {
            value
                .parse::<u64>()
                .ok()
                .map(Duration::from_secs)
                .or_else(|| {
                    chrono::DateTime::parse_from_rfc2822(value).ok().map(|at| {
                        Duration::from_secs(
                            (at.timestamp() - chrono::Utc::now().timestamp()).max(0) as u64,
                        )
                    })
                })
        })
        // An unrepresentable header is invalid, just like malformed text.
        // defer() must be able to form its monotonic deadline before returning.
        .filter(|delay| Instant::now().checked_add(*delay).is_some())
        .unwrap_or(Duration::from_secs(5))
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
