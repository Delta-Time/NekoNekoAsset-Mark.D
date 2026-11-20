//! Device-only public raster preview cache. No cookies, catalogue records or URLs are persisted.
use anyhow::{anyhow, ensure, Context, Result};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File, FileTimes},
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
    time::{Duration, SystemTime},
};
use tokio::{
    io::AsyncWriteExt,
    sync::{Mutex as AsyncMutex, Semaphore},
};
use url::Url;

#[path = "path_safety.rs"]
pub(crate) mod path_safety;

const MAX_FILE: u64 = 16 * 1024 * 1024;
const MAX_BYTES: u64 = 512 * 1024 * 1024;
const MAX_FILES: usize = 2048;
const PREFIX: &str = "nna-preview-";
const EXTENSIONS: [&str; 4] = ["png", "jpg", "gif", "webp"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CacheUsage {
    bytes: u64,
    files: usize,
}
impl CacheUsage {
    fn needs_reservation(&self) -> bool {
        self.bytes > MAX_BYTES - 2 * MAX_FILE || self.files > MAX_FILES - 2
    }
    fn reserve(&mut self, root: &Path) -> Result<()> {
        if self.needs_reservation() {
            *self = trim(root, MAX_BYTES - 2 * MAX_FILE, MAX_FILES - 2, None)?;
        }
        Ok(())
    }
    fn published(&mut self, bytes: u64) {
        // This is an upper bound between scans. A rescan may already have
        // counted part of an active stream, or an existing matching destination.
        // Counting it again only causes earlier maintenance; it cannot consume
        // the space reserved for the other bounded stream.
        self.bytes = self.bytes.saturating_add(bytes);
        self.files = self.files.saturating_add(1);
    }
}

pub struct PreviewCache {
    owner: path_safety::PinnedRoot,
    client: reqwest::Client,
    slots: Semaphore,
    keys: Mutex<HashMap<String, Weak<AsyncMutex<()>>>>,
    disk: Arc<Mutex<Option<CacheUsage>>>,
}

impl PreviewCache {
    pub fn new(data_dir: PathBuf) -> Result<Self> {
        ensure!(data_dir.is_absolute(), "preview_path_refused");
        // Cache failures belong to preview requests, not application startup.
        let client = reqwest::Client::builder()
            .cookie_store(false)
            .referer(false)
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 || permitted_url(attempt.url()).is_err() {
                    attempt.error("preview_url_refused")
                } else {
                    attempt.follow()
                }
            }))
            .build()
            .map_err(|_| anyhow!("preview_client_failed"))?;
        Ok(Self {
            owner: path_safety::PinnedRoot::new(data_dir),
            client,
            slots: Semaphore::new(2),
            keys: Mutex::new(HashMap::new()),
            disk: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn get(&self, raw: String) -> Result<PathBuf> {
        let url = parse_public_url(&raw)?;
        let key = hex::encode(Sha256::digest(url.as_str().as_bytes()));
        let key_lock = {
            let mut keys = self
                .keys
                .lock()
                .map_err(|_| anyhow!("preview_cache_lock"))?;
            keys.retain(|_, entry| entry.strong_count() > 0);
            if let Some(lock) = keys.get(&key).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(AsyncMutex::new(()));
                keys.insert(key.clone(), Arc::downgrade(&lock));
                lock
            }
        };
        let _key_guard = key_lock.lock().await;
        let root = self.owner.resolve("cache/previews").map_err(path_error)?;
        let lookup_root = root.clone();
        let lookup_key = key.clone();
        let disk = self.disk.clone();
        let cached = tokio::task::spawn_blocking(move || {
            let root = lookup_root;
            let mut usage = disk.lock().map_err(|_| anyhow!("preview_cache_lock"))?;
            if usage.is_none() {
                no_links(&root)?;
                fs::create_dir_all(&root).context("preview_cache_io")?;
                no_links(&root)?;
                // Recent partials may still belong to another request. Only
                // hour-old abandoned partials are eligible for maintenance.
                *usage = Some(trim(&root, MAX_BYTES, MAX_FILES, None)?);
            }
            find_cached(&root, &lookup_key)
        })
        .await
        .map_err(|_| anyhow!("preview_cache_worker"))??;
        if let Some(path) = cached {
            return Ok(path);
        }

        let _slot = self
            .slots
            .acquire()
            .await
            .map_err(|_| anyhow!("preview_cache_closed"))?;
        let reservation_root = root.clone();
        let disk = self.disk.clone();
        tokio::task::spawn_blocking(move || {
            let root = reservation_root;
            let mut usage = disk.lock().map_err(|_| anyhow!("preview_cache_lock"))?;
            // Reserve room for both bounded streams, including their partials.
            usage
                .as_mut()
                .ok_or_else(|| anyhow!("preview_cache_lock"))?
                .reserve(&root)
        })
        .await
        .map_err(|_| anyhow!("preview_cache_worker"))??;
        let temporary = root.join(format!("{PREFIX}{key}-{}.part", uuid::Uuid::new_v4()));
        let mut guard = PartialGuard(PathBuf::new());
        let (extension, bytes) = tokio::time::timeout(
            Duration::from_secs(30),
            self.fetch(&root, url, &temporary, &mut guard),
        )
        .await
        .map_err(|_| anyhow!("preview_timeout"))??;
        let disk = self.disk.clone();
        let temp = temporary.clone();
        let path = tokio::task::spawn_blocking(move || {
            let mut usage = disk.lock().map_err(|_| anyhow!("preview_cache_lock"))?;
            let path = publish(&root, &temp, &key, extension)?;
            // The pre-fetch reservation covers both bounded concurrent streams;
            // publishing their bytes cannot exceed it, so no second scan is needed.
            usage
                .as_mut()
                .ok_or_else(|| anyhow!("preview_cache_lock"))?
                .published(bytes);
            Ok::<_, anyhow::Error>(path)
        })
        .await
        .map_err(|_| anyhow!("preview_cache_worker"))??;
        guard.0 = PathBuf::new();
        Ok(path)
    }

    async fn fetch(
        &self,
        root: &Path,
        url: Url,
        temporary: &Path,
        guard: &mut PartialGuard,
    ) -> Result<(&'static str, u64)> {
        no_links(root)?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| anyhow!("preview_request_failed"))?;
        permitted_url(response.url())?;
        ensure!(response.status().is_success(), "preview_request_failed");
        if let Some(size) = response.content_length() {
            ensure!(size <= MAX_FILE, "preview_too_large");
        }
        let mut output = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temporary)
            .await
            .context("preview_cache_io")?;
        guard.0 = temporary.to_owned();
        let mut total = 0u64;
        let mut header = Vec::with_capacity(16);
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|_| anyhow!("preview_request_failed"))?;
            total = bounded_size(total, bytes.len())?;
            header.extend_from_slice(&bytes[..bytes.len().min(16 - header.len())]);
            output.write_all(&bytes).await.context("preview_cache_io")?;
        }
        let extension =
            raster_extension(&header).ok_or_else(|| anyhow!("preview_raster_required"))?;
        ensure!(total > 0, "preview_raster_required");
        output.flush().await.context("preview_cache_io")?;
        output.sync_all().await.context("preview_cache_io")?;
        drop(output); // Windows rename must happen after the writer is closed.
        Ok((extension, total))
    }
}

pub(crate) fn parse_public_url(raw: &str) -> Result<Url> {
    ensure!(
        raw.len() <= 8192 && raw.trim() == raw && !raw.contains('\\'),
        "preview_url_refused"
    );
    let (_, rest) = raw
        .split_once("://")
        .ok_or_else(|| anyhow!("preview_url_refused"))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    // Url normalizes an explicit :443 away; reject it before parsing as well.
    ensure!(!authority.contains([':', '@']), "preview_url_refused");
    let url = Url::parse(raw).map_err(|_| anyhow!("preview_url_refused"))?;
    permitted_url(&url)?;
    Ok(url)
}

fn permitted_url(url: &Url) -> Result<()> {
    ensure!(
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "preview_url_refused"
    );
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("preview_url_refused"))?;
    ensure!(
        (host.ends_with(".pximg.net")
            || matches!(host, "public-files.gumroad.com" | "jinxxy-c.jinxxy-cdn.com"))
            && host.len() <= 253,
        "preview_url_refused"
    );
    ensure!(
        host.split('.').all(|label| !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')),
        "preview_url_refused"
    );
    Ok(())
}

fn bounded_size(current: u64, incoming: usize) -> Result<u64> {
    let next = current
        .checked_add(incoming as u64)
        .ok_or_else(|| anyhow!("preview_too_large"))?;
    ensure!(next <= MAX_FILE, "preview_too_large");
    Ok(next)
}

fn raster_extension(header: &[u8]) -> Option<&'static str> {
    if header.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if header.starts_with(b"\xff\xd8\xff") {
        Some("jpg")
    } else if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
        Some("gif")
    } else if header.len() >= 12 && &header[..4] == b"RIFF" && &header[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

pub(crate) fn no_links(path: &Path) -> Result<()> {
    path_safety::no_links(path).map_err(path_error)
}

fn path_error(error: anyhow::Error) -> anyhow::Error {
    if error.downcast_ref::<std::io::Error>().is_some() {
        anyhow!("preview_cache_io")
    } else {
        anyhow!("preview_path_refused")
    }
}

fn managed_name(name: &str) -> bool {
    let Some(tail) = name.strip_prefix(PREFIX) else {
        return false;
    };
    let Some((hash, extension)) = tail.rsplit_once('.') else {
        return false;
    };
    hash.len() == 64
        && hash
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        && EXTENSIONS.contains(&extension)
}

fn partial_name(name: &str) -> bool {
    let Some(tail) = name
        .strip_prefix(PREFIX)
        .and_then(|s| s.strip_suffix(".part"))
    else {
        return false;
    };
    if tail.len() != 101 || tail.as_bytes()[64] != b'-' {
        return false;
    }
    tail[..64]
        .bytes()
        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        && uuid::Uuid::parse_str(&tail[65..]).is_ok()
}

fn valid_cached(path: &Path, extension: &str) -> Result<bool> {
    no_links(path)?;
    let metadata = fs::symlink_metadata(path).context("preview_cache_io")?;
    if !metadata.is_file() || metadata.len() > MAX_FILE {
        return Ok(false);
    }
    let mut header = [0u8; 16];
    let count = File::open(path)
        .context("preview_cache_io")?
        .read(&mut header)
        .context("preview_cache_io")?;
    Ok(raster_extension(&header[..count]) == Some(extension))
}

fn find_cached(root: &Path, key: &str) -> Result<Option<PathBuf>> {
    no_links(root)?;
    for extension in EXTENSIONS {
        let path = root.join(format!("{PREFIX}{key}.{extension}"));
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(anyhow!("preview_cache_io")),
            Ok(_) => (),
        }
        if valid_cached(&path, extension)? {
            let file = fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .context("preview_cache_io")?;
            // Approximate LRU with at most one timestamp write per minute per file.
            if file
                .metadata()
                .context("preview_cache_io")?
                .modified()
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_none_or(|age| age >= Duration::from_secs(60))
            {
                file.set_times(FileTimes::new().set_modified(SystemTime::now()))
                    .context("preview_cache_io")?;
            }
            return Ok(Some(path));
        }
        if fs::symlink_metadata(&path)
            .context("preview_cache_io")?
            .is_file()
        {
            fs::remove_file(path).context("preview_cache_io")?;
        }
    }
    Ok(None)
}

fn publish(root: &Path, temporary: &Path, key: &str, extension: &str) -> Result<PathBuf> {
    ensure!(
        temporary.parent() == Some(root)
            && temporary
                .file_name()
                .and_then(|v| v.to_str())
                .is_some_and(partial_name),
        "preview_path_refused"
    );
    let destination = root.join(format!("{PREFIX}{key}.{extension}"));
    ensure!(
        destination
            .file_name()
            .and_then(|v| v.to_str())
            .is_some_and(managed_name),
        "preview_path_refused"
    );
    no_links(temporary)?;
    no_links(&destination)?;
    if destination.exists() {
        ensure!(
            valid_cached(&destination, extension)?,
            "preview_cache_collision"
        );
        fs::remove_file(temporary).context("preview_cache_io")?;
    } else {
        fs::rename(temporary, &destination).context("preview_cache_io")?;
    }
    Ok(destination)
}

fn trim(
    root: &Path,
    budget: u64,
    count_limit: usize,
    protected: Option<&Path>,
) -> Result<CacheUsage> {
    no_links(root)?;
    let mut files = Vec::new();
    let mut total = 0u64;
    let mut partials = 0usize;
    for entry in fs::read_dir(root).context("preview_cache_io")? {
        let entry = entry.context("preview_cache_io")?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !managed_name(&name) && !partial_name(&name) {
            continue;
        }
        // DirEntry metadata does not follow links and reuses the Windows
        // directory enumeration data instead of another stat for each file.
        let metadata = entry.metadata().context("preview_cache_io")?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                continue;
            }
        }
        if partial_name(&name) {
            if metadata
                .modified()
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age >= Duration::from_secs(3600))
                && fs::remove_file(&path).is_ok()
            {
                continue;
            } else {
                total = total.saturating_add(metadata.len());
                partials += 1;
            }
            continue;
        }
        total = total.saturating_add(metadata.len());
        files.push((
            metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            path,
            metadata.len(),
        ));
    }
    files.sort_by_key(|(modified, _, _)| *modified);
    let mut count = files.len() + partials;
    for (_, path, size) in files {
        if total <= budget && count <= count_limit {
            break;
        }
        if protected == Some(path.as_path()) {
            continue;
        }
        no_links(&path)?;
        if fs::remove_file(path).is_err() {
            continue;
        }
        total = total.saturating_sub(size);
        count -= 1;
    }
    ensure!(
        total <= budget && count <= count_limit,
        "preview_cache_full"
    );
    Ok(CacheUsage {
        bytes: total,
        files: count,
    })
}

struct PartialGuard(PathBuf);
impl Drop for PartialGuard {
    fn drop(&mut self) {
        if !self.0.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.0);
        }
    }
}

#[cfg(test)]
#[path = "preview_tests.rs"]
mod tests;
