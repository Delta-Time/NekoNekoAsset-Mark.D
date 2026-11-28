//! Public release metadata and signed installer updates. No catalogue or credential data is sent.
use anyhow::{ensure, Context, Result};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;
use url::Url;

const RELEASE_INDEX: &str =
    "https://api.github.com/repos/Delta-Time/NekoNekoAsset-Mark.D/releases?per_page=100";
const RELEASE_ROOT: &str = "https://github.com/Delta-Time/NekoNekoAsset-Mark.D/releases";
const APP_MANIFEST: &str = "app-update.json";

#[derive(Deserialize)]
struct PublishedRelease {
    tag_name: String,
    draft: bool,
    assets: Vec<PublishedAsset>,
}

#[derive(Deserialize)]
struct PublishedAsset {
    name: String,
    state: String,
}

impl PublishedRelease {
    fn has_asset(&self, name: &str) -> bool {
        self.assets
            .iter()
            .any(|asset| asset.name == name && asset.state == "uploaded")
    }
}

#[derive(Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppRelease {
    version: String,
    notes: String,
    release_url: String,
}

#[derive(Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BridgeRelease {
    version: String,
    unitypackage_url: String,
    release_url: String,
}

#[derive(Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppCheck {
    current_version: String,
    install_supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    available: Option<AppRelease>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    error_code: Option<String>,
}

/// Whether newer App and Bridge releases exist.
#[derive(Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct UpdateCheck {
    app: AppCheck,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    bridge: Option<BridgeRelease>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    bridge_error: Option<String>,
}

/// An App update's download and install progress: the `updates:progress` event.
#[derive(Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub(crate) struct UpdateProgress {
    #[ts(type = "\"downloading\" | \"verifying\" | \"installing\"")]
    stage: String,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
}

#[derive(Default)]
pub(crate) struct UpdateState {
    checked: Mutex<Option<Update>>,
    checking: Mutex<()>,
    installing: Arc<AtomicBool>,
}

/// The caller also holds the global command gate and the catalogue connection through installer exit.
pub(crate) struct InstallPermit(Arc<AtomicBool>);
impl Drop for InstallPermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn portable_executable(executable: &Path) -> bool {
    executable
        .parent()
        .is_some_and(|parent| parent.join("portable.flag").is_file())
}

fn install_supported(app: &AppHandle) -> bool {
    cfg!(all(windows, target_arch = "x86_64"))
        && !app.config().identifier.contains(".portable.")
        && std::env::current_exe().is_ok_and(|path| !portable_executable(&path))
}

fn release_url(version: &str) -> String {
    format!("{RELEASE_ROOT}/tag/v{version}")
}

fn mark_d_version(version: &Version) -> Option<(u64, u64, u64, u64)> {
    let digits = version.build.as_str().strip_prefix('D')?;
    let revision = digits.parse().ok()?;
    if !version.pre.is_empty()
        || digits.starts_with('0')
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    Some((version.major, version.minor, version.patch, revision))
}

fn asset_url(version: &str, name: &str) -> String {
    format!("{RELEASE_ROOT}/download/v{version}/{name}")
}

fn installer_name(version: &str) -> String {
    format!("NekoNekoAsset_{version}_x64-setup.exe")
}

fn bridge_name(version: &str) -> String {
    format!("com.yank.nekonekoasset.bridge-{version}.unitypackage")
}

fn valid_installer_url(value: &Url, version: &str) -> bool {
    value.as_str().replace("%2B", "+").replace("%2b", "+")
        == asset_url(version, &installer_name(version))
}

fn latest_release(
    releases: &[PublishedRelease],
    has_assets: impl Fn(&PublishedRelease, &str) -> bool,
) -> Option<Version> {
    releases
        .iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let tag = release.tag_name.strip_prefix('v')?;
            let version = Version::parse(tag).ok()?;
            let order = mark_d_version(&version)?;
            has_assets(release, tag).then_some((order, version))
        })
        .max_by_key(|(order, _)| *order)
        .map(|(_, version)| version)
}

async fn published_releases() -> Result<Vec<PublishedRelease>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("updates_unavailable")?;
    let mut response = client
        .get(RELEASE_INDEX)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "NekoNekoAsset-Mark.D")
        .send()
        .await
        .context("updates_unavailable")?
        .error_for_status()
        .context("updates_unavailable")?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.context("updates_unavailable")? {
        ensure!(
            bytes.len() + chunk.len() <= 4 * 1024 * 1024,
            "updates_invalid_manifest"
        );
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).context("updates_invalid_manifest")
}

fn public_error(error: &anyhow::Error) -> String {
    if error.to_string().starts_with("updates_invalid_manifest") {
        "updates_invalid_manifest".into()
    } else {
        "updates_unavailable".into()
    }
}

fn download_error(error: tauri_plugin_updater::Error) -> anyhow::Error {
    use tauri_plugin_updater::Error;
    let code = match error {
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => "updates_signature_invalid",
        _ => "updates_download_failed",
    };
    anyhow::anyhow!(code)
}

fn progress(app: &AppHandle, stage: &str, downloaded: u64, total: Option<u64>) {
    let _ = app.emit_to(
        "main",
        "updates:progress",
        UpdateProgress {
            stage: stage.to_owned(),
            downloaded_bytes: downloaded,
            total_bytes: total,
        },
    );
}

impl UpdateState {
    pub(crate) fn begin_install(&self) -> Result<InstallPermit> {
        ensure!(
            !self.installing.swap(true, Ordering::AcqRel),
            "updates_busy"
        );
        Ok(InstallPermit(self.installing.clone()))
    }

    pub(crate) async fn check(&self, app: &AppHandle) -> Result<UpdateCheck> {
        ensure!(!self.installing.load(Ordering::Acquire), "updates_busy");
        let _check = self.checking.lock().await;
        let releases = published_releases().await;
        let check_app = async {
            if !cfg!(all(windows, target_arch = "x86_64")) {
                return Ok(None);
            }
            let releases = releases
                .as_ref()
                .map_err(|error| anyhow::anyhow!(public_error(error)))?;
            let Some(latest) = latest_release(releases, |release, version| {
                let installer = installer_name(version);
                release.has_asset(APP_MANIFEST)
                    && release.has_asset(&installer)
                    && release.has_asset(&format!("{installer}.sig"))
            }) else {
                return Ok(None);
            };
            let current = app.package_info().version.clone();
            if mark_d_version(&latest) <= mark_d_version(&current) {
                return Ok(None);
            }

            let version = latest.to_string();
            let endpoint = Url::parse(&asset_url(&version, APP_MANIFEST))?;
            let updater = app
                .updater_builder()
                .endpoints(vec![endpoint])
                .context("updates_unavailable")?
                .version_comparator(move |_, release| release.version == latest)
                // Windows launch can fail after this callback; leave the App usable for retry.
                .on_before_exit(|| {})
                .timeout(Duration::from_secs(15))
                .header("Cache-Control", "no-cache")
                .context("updates_unavailable")?
                .build()
                .context("updates_unavailable")?;
            let update = updater.check().await.context("updates_unavailable")?;
            ensure!(
                update.as_ref().is_some_and(|update| {
                    valid_installer_url(&update.download_url, &version)
                        && !update.signature.trim().is_empty()
                }),
                "updates_invalid_manifest"
            );
            Ok::<_, anyhow::Error>(update)
        };
        let (update, app_error) = match check_app.await {
            Ok(update) => (update, None),
            Err(error) => (None, Some(public_error(&error))),
        };
        let available = update.as_ref().map(|update| AppRelease {
            version: update.version.clone(),
            notes: update.body.clone().unwrap_or_default(),
            release_url: release_url(&update.version),
        });
        *self.checked.lock().await = update;
        let (bridge, bridge_error) = match releases {
            Ok(releases) => {
                let bridge = latest_release(&releases, |release, version| {
                    release.has_asset(&bridge_name(version))
                })
                .map(|version| {
                    let version = version.to_string();
                    BridgeRelease {
                        unitypackage_url: asset_url(&version, &bridge_name(&version)),
                        release_url: release_url(&version),
                        version,
                    }
                });
                (bridge, None)
            }
            Err(error) => (None, Some(public_error(&error))),
        };
        Ok(UpdateCheck {
            app: AppCheck {
                current_version: app.package_info().version.to_string(),
                install_supported: install_supported(app),
                available,
                error_code: app_error,
            },
            bridge,
            bridge_error,
        })
    }

    /// A signed download never mutates the catalogue, so other work continues meanwhile.
    pub(crate) async fn download(
        &self,
        app: &AppHandle,
        expected_version: &str,
    ) -> Result<PreparedInstall> {
        ensure!(self.installing.load(Ordering::Acquire), "updates_busy");
        ensure!(install_supported(app), "updates_portable_unsupported");
        let mut update = self.checked.lock().await.clone().context("updates_stale")?;
        ensure!(update.version == expected_version, "updates_stale");
        ensure!(
            valid_installer_url(&update.download_url, &update.version),
            "updates_invalid_manifest"
        );
        update.timeout = Some(Duration::from_secs(600));
        progress(app, "downloading", 0, None);
        let mut downloaded = 0u64;
        let mut last_progress = Instant::now();
        let bytes = update
            .download(
                |chunk, total| {
                    downloaded = downloaded.saturating_add(chunk as u64);
                    if last_progress.elapsed() >= Duration::from_millis(150)
                        || Some(downloaded) == total
                    {
                        progress(app, "downloading", downloaded, total);
                        last_progress = Instant::now();
                    }
                },
                || {
                    progress(app, "verifying", 0, None);
                },
            )
            .await
            .map_err(download_error)?;
        progress(
            app,
            "installing",
            bytes.len() as u64,
            Some(bytes.len() as u64),
        );
        Ok(PreparedInstall { update, bytes })
    }
}

pub(crate) struct PreparedInstall {
    update: Update,
    bytes: Vec<u8>,
}

impl PreparedInstall {
    /// Windows exits this process only after launching NSIS.
    pub(crate) fn launch(&self) -> Result<()> {
        self.update
            .install(&self.bytes)
            .map_err(|_| anyhow::anyhow!("updates_install_failed"))
    }
}

/// Store sync/update and license jobs resume from their checkpoints when the updated App
/// starts, and downloads and local imports are queued again. Any other active job names why
/// installation has to wait.
pub(crate) fn blocking_job(job: &serde_json::Value) -> Option<&'static str> {
    if !matches!(job["state"].as_str(), Some("queued" | "running")) {
        return None;
    }
    let kind = job["type"].as_str().unwrap_or("");
    if crate::storage::RESUMED_AT_STARTUP.contains(&kind) {
        return None;
    }
    Some(match kind {
        "boothSync"
        | "boothUpdate"
        | "gumroadSync"
        | "gumroadUpdate"
        | "jinxxySync"
        | "jinxxyUpdate"
        | crate::licenses::JOB_TYPE => return None,
        "boothLogin" | "gumroadLogin" | "jinxxyLogin" => "updates_busy_login",
        "storage:migrate_root" => "updates_busy_move",
        "storage:export_backup" => "updates_busy_backup",
        "storage:restore_backup" => "updates_busy_restore",
        "storage:export_zip" => "updates_busy_export",
        "storage:delete_local_products" => "updates_busy_delete",
        "unity:import" => "updates_busy_unity",
        _ if kind.starts_with("ai:") => "updates_busy_ai",
        _ => "updates_busy_job",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_assets_are_bound_to_mark_d_repo_version_and_format() {
        let version = "0.4.0+D1";
        let good = asset_url(version, &installer_name(version));
        for url in [&good, &good.replace('+', "%2B")] {
            assert!(valid_installer_url(&Url::parse(url).unwrap(), version));
        }
        for bad in [
            good.replace("https:", "http:"),
            good.replace(
                "Delta-Time/NekoNekoAsset-Mark.D",
                "Yan-K/NekoNekoAsset-Public",
            ),
            good.replace("v0.4.0+D1/", "v0.4.0+D2/"),
            format!("{good}?source=elsewhere"),
            good.replace("-setup.exe", "-portable.zip"),
        ] {
            assert!(!valid_installer_url(&Url::parse(&bad).unwrap(), version));
        }
    }

    #[test]
    fn latest_release_requires_mark_d_and_uploaded_assets_and_orders_revisions_numerically() {
        let releases = [
            ("v1.0.0", false, "uploaded"),
            ("v0.4.0+D2", false, "uploaded"),
            ("v0.4.0+D10", false, "uploaded"),
            ("v0.4.1+D1", false, "new"),
            ("v0.4.2+D1", true, "uploaded"),
        ]
        .map(|(tag, draft, state)| PublishedRelease {
            tag_name: tag.into(),
            draft,
            assets: vec![PublishedAsset {
                name: APP_MANIFEST.into(),
                state: state.into(),
            }],
        });
        let latest = latest_release(&releases, |release, _| release.has_asset(APP_MANIFEST));
        assert_eq!(latest, Some(Version::parse("0.4.0+D10").unwrap()));
    }

    #[test]
    fn only_active_jobs_that_do_not_resume_block_installation() {
        let job = |kind: &str, state: &str| serde_json::json!({"type":kind,"state":state});
        for kind in [
            "boothSync",
            "boothUpdate",
            "gumroadSync",
            "gumroadUpdate",
            "jinxxySync",
            "jinxxyUpdate",
            "download",
            "storage:import_local",
        ] {
            assert_eq!(blocking_job(&job(kind, "running")), None);
            assert_eq!(blocking_job(&job(kind, "queued")), None);
        }
        assert_eq!(
            blocking_job(&job("storage:export_zip", "queued")),
            Some("updates_busy_export")
        );
        assert_eq!(
            blocking_job(&job("unity:import", "running")),
            Some("updates_busy_unity")
        );
        assert_eq!(
            blocking_job(&job("boothLogin", "running")),
            Some("updates_busy_login")
        );
        assert_eq!(
            blocking_job(&job("storage:restore_backup", "running")),
            Some("updates_busy_restore")
        );
        assert_eq!(
            blocking_job(&job("ai:classify", "running")),
            Some("updates_busy_ai")
        );
        assert_eq!(
            blocking_job(&job("storage:future", "running")),
            Some("updates_busy_job")
        );
        for state in ["completed", "failed", "needsAction", "cancelled"] {
            assert_eq!(blocking_job(&job("storage:export_zip", state)), None);
        }
    }
    #[test]
    fn failed_or_finished_install_releases_single_operation_slot() {
        let state = UpdateState::default();
        let first = state.begin_install().unwrap();
        assert!(state.begin_install().is_err());
        drop(first);
        assert!(state.begin_install().is_ok());
    }
}
