//! Native device-local login profiles. Cookies never cross IPC or catalogue records.
use super::StoreProvider;
use anyhow::{bail, ensure, Result};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use url::Url;

#[cfg(windows)]
#[path = "native_cookies.rs"]
mod native_cookies;

// The deployed first-party language switch declares these hosts explicitly.
pub const JINXXY_HOSTS: &[&str] = &[
    "jinxxy.com",
    "www.jinxxy.com",
    "ja.jinxxy.com",
    "de.jinxxy.com",
    "es.jinxxy.com",
    "fr.jinxxy.com",
    "zh.jinxxy.com",
    "ko.jinxxy.com",
];

/// Jinxxy may add or move subdomains; its pages and cookies stay within jinxxy.com.
pub fn jinxxy_host(host: &str) -> bool {
    host == "jinxxy.com" || host.ends_with(".jinxxy.com")
}

fn account_id(account: &str) -> Result<String> {
    Ok(uuid::Uuid::parse_str(account)
        .map_err(|_| anyhow::anyhow!("INVALID_ACCOUNT_ID"))?
        .to_string())
}

fn secure(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
}

fn cookie_host(provider: StoreProvider, host: &str) -> bool {
    match provider {
        StoreProvider::Booth => host == "booth.pm" || host.ends_with(".booth.pm"),
        StoreProvider::Gumroad => host == "gumroad.com" || host.ends_with(".gumroad.com"),
        StoreProvider::Jinxxy => jinxxy_host(host),
    }
}

/// Sign-in may pass through any HTTPS provider, and providers add or rename their own hops
/// (Google's accounts.youtube.com and country-domain cookie hops, X's api.x.com), so no provider
/// host list is kept. The window has no IPC, refuses downloads and only the store's own cookies
/// are read; the store's checkout, cart, download and receipt pages stay closed in it.
pub fn navigation_allowed(provider: StoreProvider, url: &Url) -> bool {
    let Some(url::Host::Domain(host)) = url.host().filter(|_| secure(url)) else {
        return false;
    };
    !cookie_host(provider, host)
        || (!(provider == StoreProvider::Jinxxy && host == "api.jinxxy.com")
            && !["/checkout", "/cart", "/download", "/r/", "/d/"]
                .iter()
                .any(|path| url.path().contains(path)))
}

pub fn profile_path(data_dir: &Path, provider: StoreProvider, account: &str) -> Result<PathBuf> {
    Ok(data_dir
        .join("auth")
        .join(provider.key())
        .join(account_id(account)?))
}

/// Build request cookies without widening the native browser's host-only scope.
pub(crate) fn cookies_for_request(
    window: &WebviewWindow,
    url: &Url,
) -> Result<Vec<tauri::webview::Cookie<'static>>> {
    #[cfg(windows)]
    {
        Ok(native_cookies::for_url(window, url.as_str())?
            .into_iter()
            .map(super::session_cache::ScopedCookie::request_cookie)
            .collect())
    }
    #[cfg(not(windows))]
    {
        window
            .cookies_for_url(url.clone())
            .map_err(|_| anyhow::anyhow!("COOKIE_ACCESS_FAILED"))
    }
}

/// Save only verified first-party cookies, outside catalogue, IPC and backups.
pub async fn remember(
    window: WebviewWindow,
    data_dir: &Path,
    provider: StoreProvider,
    account: &str,
) -> Result<()> {
    let profile = profile_path(data_dir, provider, account)?;
    let account = account_id(account)?;
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        let cookies = native_cookies::list(&window)?;
        #[cfg(not(windows))]
        let cookies = window
            .cookies()
            .map_err(|_| anyhow::anyhow!("COOKIE_ACCESS_FAILED"))?
            .into_iter()
            .map(super::session_cache::ScopedCookie::from)
            .collect::<Vec<_>>();
        let cookies = cookies
            .into_iter()
            .filter(|scoped| {
                scoped
                    .domain
                    .as_deref()
                    .is_some_and(|domain| cookie_host(provider, domain.trim_start_matches('.')))
            })
            .collect();
        super::session_cache::save_scoped(&profile, provider, &account, cookies)
    })
    .await
    .map_err(|_| anyhow::anyhow!("AUTH_SESSION_STORAGE_FAILED"))?
}

/// Restore before any remote navigation, keeping newer cookies already in the native profile.
pub async fn restore(
    window: WebviewWindow,
    data_dir: &Path,
    provider: StoreProvider,
    account: &str,
) -> Result<()> {
    let profile = profile_path(data_dir, provider, account)?;
    let account = account_id(account)?;
    tauri::async_runtime::spawn_blocking(move || {
        let saved =
            super::session_cache::load_scoped(&profile, provider, &account).unwrap_or_default();
        #[cfg(windows)]
        let current = native_cookies::list(&window)?;
        #[cfg(not(windows))]
        let current = window
            .cookies()
            .map_err(|_| anyhow::anyhow!("COOKIE_ACCESS_FAILED"))?
            .into_iter()
            .map(super::session_cache::ScopedCookie::from)
            .collect::<Vec<_>>();
        let missing = saved
            .into_iter()
            .filter(|scoped| {
                scoped
                    .domain
                    .as_deref()
                    .is_some_and(|domain| cookie_host(provider, domain.trim_start_matches('.')))
                    && !current.iter().any(|existing| {
                        existing.cookie.name() == scoped.cookie.name()
                            && existing.domain == scoped.domain
                            && existing.cookie.path() == scoped.cookie.path()
                    })
            })
            .collect::<Vec<_>>();
        #[cfg(windows)]
        native_cookies::set(&window, missing)?;
        #[cfg(not(windows))]
        for cookie in missing {
            window
                .set_cookie(cookie.cookie)
                .map_err(|_| anyhow::anyhow!("COOKIE_ACCESS_FAILED"))?;
        }
        Ok(())
    })
    .await
    .map_err(|_| anyhow::anyhow!("COOKIE_ACCESS_FAILED"))?
}

pub fn forget(data_dir: &Path, provider: StoreProvider, account: &str) -> Result<()> {
    super::session_cache::clear(&profile_path(data_dir, provider, account)?)
}

/// What a sync or download does on AUTH_REQUIRED; see session_cache::mark_signed_out.
pub fn mark_signed_out(data_dir: &Path, provider: StoreProvider, account: &str) -> Result<()> {
    super::session_cache::mark_signed_out(&profile_path(data_dir, provider, account)?)
}

/// Only a confirmed discarded draft loses its account-scoped snapshot. Other
/// libraries' profiles and established accounts are never inferred as obsolete.
pub(crate) fn discard_unverified_account(
    catalogue: &crate::catalogue::Catalogue,
    provider: StoreProvider,
    account: &str,
) -> Result<bool> {
    let discarded = catalogue.discard_unverified_account(account)?;
    if discarded {
        forget(&catalogue.device_dir(), provider, account)?;
    }
    Ok(discarded)
}

/// Login polling must not recreate a window the human has closed.
pub fn existing_window(
    app: &AppHandle,
    provider: StoreProvider,
    account: &str,
) -> Result<Option<WebviewWindow>> {
    Ok(app.get_webview_window(&format!("{}-auth-{}", provider.key(), account_id(account)?)))
}

/// Keep the verified native session alive: closing its last WebView drops session cookies.
/// Failed/cancelled login still destroys the window; logout clears browsing data separately.
pub async fn finish_login(
    app: &AppHandle,
    provider: StoreProvider,
    account: &str,
    verified: bool,
) -> Result<()> {
    if let Some(window) = existing_window(app, provider, account)? {
        tauri::async_runtime::spawn_blocking(move || {
            if verified {
                window.hide()
            } else {
                window.close()
            }
        })
        .await
        .map_err(|_| anyhow::anyhow!("AUTH_WINDOW_FAILED"))?
        .map_err(|_| anyhow::anyhow!("AUTH_WINDOW_FAILED"))?;
    }
    Ok(())
}

/// Creating/showing a profile does not establish authentication. The adapter must verify identity.
pub async fn window(
    app: &AppHandle,
    data_dir: &Path,
    provider: StoreProvider,
    account: &str,
    entry: Url,
    visible: bool,
) -> Result<WebviewWindow> {
    // Existing BOOTH windows and Pixiv navigation retain their established implementation.
    ensure!(
        provider != StoreProvider::Booth,
        "LEGACY_BOOTH_SESSION_REQUIRED"
    );
    ensure!(
        navigation_allowed(provider, &entry)
            && entry
                .host_str()
                .is_some_and(|host| cookie_host(provider, host)),
        "INVALID_AUTH_URL"
    );
    let id = account_id(account)?;
    let label = format!("{}-auth-{id}", provider.key());
    if let Some(window) = app.get_webview_window(&label) {
        if visible {
            window
                .navigate(entry)
                .map_err(|_| anyhow::anyhow!("AUTH_WINDOW_FAILED"))?;
            window
                .show()
                .map_err(|_| anyhow::anyhow!("AUTH_WINDOW_FAILED"))?;
            window.set_focus().ok();
        }
        return Ok(window);
    }
    let profile = profile_path(data_dir, provider, &id)?;
    let app = app.clone();
    let restore_data = data_dir.to_path_buf();
    let entry_url = entry.clone();
    let created = tauri::async_runtime::spawn_blocking(move || {
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("/usr/bin/sw_vers")
                .arg("-productVersion")
                .output()
                .map_err(|_| anyhow::anyhow!("AUTH_ISOLATION_UNAVAILABLE"))?;
            let major = std::str::from_utf8(&output.stdout)
                .ok()
                .and_then(|value| value.trim().split('.').next())
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0);
            ensure!(
                output.status.success() && major >= 14,
                "AUTH_ISOLATION_UNAVAILABLE"
            );
        }
        std::fs::create_dir_all(&profile).map_err(|_| anyhow::anyhow!("PROFILE_UNAVAILABLE"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&profile, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| anyhow::anyhow!("PROFILE_UNAVAILABLE"))?;
        }
        let popup_app = app.clone();
        let popup_label = label.clone();
        let builder = WebviewWindowBuilder::new(
            &app,
            label,
            WebviewUrl::External(Url::parse("about:blank").unwrap()),
        )
        .title(provider.label())
        .inner_size(920.0, 760.0)
        .visible(false)
        .devtools(false)
        .data_directory(profile)
        .on_navigation(move |url| {
            url.as_str() == "about:blank" || navigation_allowed(provider, url)
        })
        .on_new_window(move |url, _| {
            if navigation_allowed(provider, &url) {
                if let Some(window) = popup_app.get_webview_window(&popup_label) {
                    let _ = window.navigate(url);
                }
            }
            tauri::webview::NewWindowResponse::Deny
        })
        .on_download(|_, _| false);
        #[cfg(target_os = "macos")]
        let builder = {
            use sha2::{Digest, Sha256};
            let digest = Sha256::digest(format!("{}:{id}", provider.key()).as_bytes());
            let mut store = [0u8; 16];
            store.copy_from_slice(&digest[..16]);
            builder.data_store_identifier(store)
        };
        builder
            .build()
            .map_err(|_| anyhow::anyhow!("AUTH_WINDOW_FAILED"))
    })
    .await
    .map_err(|_| anyhow::anyhow!("AUTH_WINDOW_FAILED"))??;
    restore(created.clone(), &restore_data, provider, account).await?;
    created
        .navigate(entry_url)
        .map_err(|_| anyhow::anyhow!("AUTH_WINDOW_FAILED"))?;
    if visible {
        created
            .show()
            .map_err(|_| anyhow::anyhow!("AUTH_WINDOW_FAILED"))?;
        created.set_focus().ok();
    }
    Ok(created)
}

pub async fn jar(
    window: WebviewWindow,
    provider: StoreProvider,
) -> Result<Arc<reqwest::cookie::Jar>> {
    let origins: Vec<Url> = match provider {
        StoreProvider::Booth => bail!("LEGACY_BOOTH_SESSION_REQUIRED"),
        StoreProvider::Gumroad => vec![Url::parse("https://gumroad.com/").unwrap()],
        StoreProvider::Jinxxy => JINXXY_HOSTS
            .iter()
            .copied()
            .chain(std::iter::once("api.jinxxy.com"))
            .map(|host| Url::parse(&format!("https://{host}/")).unwrap())
            .collect(),
    };
    tauri::async_runtime::spawn_blocking(move || {
        // A reopened hidden WebView loads its profile asynchronously, slowly on a busy
        // disk. No cookies means signed out only once the profile had time to load.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut wait = std::time::Duration::from_millis(100);
        loop {
            let jar = Arc::new(reqwest::cookie::Jar::default());
            let mut count = 0;
            for url in &origins {
                for cookie in cookies_for_request(&window, url)? {
                    if cookie.name().is_empty()
                        || cookie.domain().is_some_and(|domain| {
                            !cookie_host(provider, domain.trim_start_matches('.'))
                        })
                    {
                        continue;
                    }
                    jar.add_cookie_str(&cookie.to_string(), url);
                    count += 1;
                }
            }
            if count != 0 {
                return Ok(jar);
            }
            ensure!(std::time::Instant::now() + wait < deadline, "AUTH_REQUIRED");
            std::thread::sleep(wait);
            wait = (wait * 2).min(std::time::Duration::from_secs(1));
        }
    })
    .await
    .map_err(|_| anyhow::anyhow!("COOKIE_ACCESS_FAILED"))?
}

const JINXXY_ENGLISH_PREFERENCE: &str =
    "jx_locale=en; Domain=.jinxxy.com; Path=/; Max-Age=31536000; SameSite=Lax; Secure";

/// Called only after the adapter has parsed a protected buyer inventory.
/// Change the documented language preference in this profile; auth cookies keep their scope.
pub async fn jinxxy_english(window: WebviewWindow) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let inventory = Url::parse("https://jinxxy.com/my/inventory").unwrap();
        let jinxxy_page = || -> Result<Option<Url>> {
            let url = window
                .url()
                .map_err(|_| anyhow::anyhow!("AUTH_WINDOW_FAILED"))?;
            Ok((secure(&url) && url.host_str().is_some_and(jinxxy_host)).then_some(url))
        };
        let mut current = jinxxy_page()?;
        if current.is_none() {
            // A hidden window may still be loading or show another page, which says
            // nothing about the session. Open the inventory and set the preference there.
            window
                .navigate(inventory.clone())
                .map_err(|_| anyhow::anyhow!("AUTH_WINDOW_FAILED"))?;
            for _ in 0..100 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                current = jinxxy_page()?;
                if current.is_some() {
                    break;
                }
            }
        }
        let current = current.ok_or_else(|| anyhow::anyhow!("JINXXY_SOURCE_UNSUPPORTED"))?;
        let apex = Url::parse("https://jinxxy.com/").unwrap();
        let mut applied = false;
        // Use the site's own document.cookie mechanism: the Windows native
        // Cookie::domain conversion strips the leading dot of parent domains.
        // eval dispatch is asynchronous and a page still loading can drop it, so it is
        // repeated; confirm only this public preference before snapshotting the jar.
        // Never inspect or alter auth-cookie values.
        for attempt in 0..20 {
            if attempt % 5 == 0 {
                window
                    .eval(format!(
                        "document.cookie = {};",
                        serde_json::to_string(JINXXY_ENGLISH_PREFERENCE).unwrap()
                    ))
                    .map_err(|_| anyhow::anyhow!("COOKIE_ACCESS_FAILED"))?;
            }
            let cookies = window
                .cookies_for_url(apex.clone())
                .map_err(|_| anyhow::anyhow!("COOKIE_ACCESS_FAILED"))?;
            let preferences: Vec<_> = cookies
                .iter()
                .filter(|cookie| cookie.name() == "jx_locale" && cookie.path() == Some("/"))
                .collect();
            if !preferences.is_empty() && preferences.iter().all(|cookie| cookie.value() == "en") {
                applied = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        ensure!(applied, "JINXXY_SOURCE_UNSUPPORTED");
        if current.host_str() != Some("jinxxy.com") {
            window
                .navigate(inventory)
                .map_err(|_| anyhow::anyhow!("AUTH_WINDOW_FAILED"))?;
        }
        Ok(())
    })
    .await
    .map_err(|_| anyhow::anyhow!("AUTH_WINDOW_FAILED"))?
}

pub async fn disconnect(window: WebviewWindow) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        window
            .navigate(Url::parse("about:blank").unwrap())
            .map_err(|_| anyhow::anyhow!("PROFILE_CLEAR_FAILED"))?;
        window
            .clear_all_browsing_data()
            .map_err(|_| anyhow::anyhow!("PROFILE_CLEAR_FAILED"))?;
        // WebView2 submits clearing asynchronously. Do not report logout before cookies are gone.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if window
                .cookies()
                .map_err(|_| anyhow::anyhow!("PROFILE_CLEAR_FAILED"))?
                .is_empty()
            {
                break;
            }
            ensure!(std::time::Instant::now() < deadline, "PROFILE_CLEAR_FAILED");
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        window
            .close()
            .map_err(|_| anyhow::anyhow!("AUTH_WINDOW_FAILED"))?;
        Ok(())
    })
    .await
    .map_err(|_| anyhow::anyhow!("PROFILE_CLEAR_FAILED"))?
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
