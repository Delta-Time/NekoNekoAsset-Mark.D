
use super::*;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nna-preview-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn hash(n: u8) -> String {
    format!("{n:064x}")
}
fn png() -> &'static [u8] {
    b"\x89PNG\r\n\x1a\nsynthetic signature fixture"
}
fn cache_file(root: &Path, n: u8) -> PathBuf {
    let path = root.join(format!("{PREFIX}{}.png", hash(n)));
    fs::write(&path, png()).unwrap();
    path
}

#[test]
fn quota_reserves_two_maximum_streams_before_another_fetch() {
    let mut usage = CacheUsage {
        bytes: MAX_BYTES - 2 * MAX_FILE,
        files: MAX_FILES - 2,
    };
    assert!(!usage.needs_reservation());
    let mut over_bytes = usage;
    over_bytes.bytes += 1;
    assert!(over_bytes.needs_reservation());
    let mut over_files = usage;
    over_files.files += 1;
    assert!(over_files.needs_reservation());
    // Both slots may finish before the next reservation. Their maximum
    // combined payload still fits the bytes and entry-count bounds.
    usage.published(MAX_FILE);
    usage.published(MAX_FILE);
    assert_eq!(
        usage,
        CacheUsage {
            bytes: MAX_BYTES,
            files: MAX_FILES
        }
    );
    assert!(usage.needs_reservation());
}

#[test]
fn quota_keeps_safe_overcounts_until_maintenance_and_reclaims_stale_partials() {
    let fixture = Fixture::new();
    cache_file(&fixture.0, 1);
    let corrupt = cache_file(&fixture.0, 2);
    fs::write(&corrupt, b"authored invalid raster").unwrap();
    let mut usage = trim(&fixture.0, MAX_BYTES, MAX_FILES, None).unwrap();
    assert_eq!(usage.files, 2);
    assert!(find_cached(&fixture.0, &hash(2)).unwrap().is_none());
    let before = usage;
    usage.reserve(&fixture.0).unwrap();
    // Ordinary misses do not scan the directory: an already removed
    // corrupt entry remains an upper-bound count until quota maintenance.
    assert_eq!(usage, before);

    let key = hash(3);
    let partial = fixture
        .0
        .join(format!("{PREFIX}{key}-{}.part", uuid::Uuid::new_v4()));
    fs::write(&partial, png()).unwrap();
    usage = trim(&fixture.0, MAX_BYTES, MAX_FILES, None).unwrap();
    assert_eq!(usage.files, 2); // Includes the active partial.
    let destination = publish(&fixture.0, &partial, &key, "png").unwrap();
    usage.published(png().len() as u64);
    assert_eq!(usage.files, 3);
    // A matching cached destination is harmless to count again as well.
    fs::write(&partial, png()).unwrap();
    assert_eq!(
        publish(&fixture.0, &partial, &key, "png").unwrap(),
        destination
    );
    usage.published(png().len() as u64);
    assert_eq!(usage.files, 4);

    let abandoned = fixture
        .0
        .join(format!("{PREFIX}{}-{}.part", hash(4), uuid::Uuid::new_v4()));
    fs::write(&abandoned, png()).unwrap();
    File::options()
        .write(true)
        .open(&abandoned)
        .unwrap()
        .set_times(FileTimes::new().set_modified(SystemTime::UNIX_EPOCH))
        .unwrap();
    usage.bytes = MAX_BYTES;
    usage.reserve(&fixture.0).unwrap();
    assert_eq!(
        usage,
        CacheUsage {
            bytes: 2 * png().len() as u64,
            files: 2
        }
    );
    assert!(!abandoned.exists());
    assert!(destination.exists());
}

#[tokio::test]
async fn optional_cache_initializes_on_demand_and_retries_before_network() {
    let fixture = Fixture::new();
    let parent = fixture.0.join("cache");
    fs::write(&parent, b"authored path blocker").unwrap();
    let cache = PreviewCache::new(fixture.0.clone()).unwrap();
    let url = "https://booth.pximg.net/authored-preview.png";
    assert!(cache.get(url.into()).await.is_err());
    assert_eq!(fs::read(&parent).unwrap(), b"authored path blocker");
    fs::remove_file(&parent).unwrap();
    let root = parent.join("previews");
    fs::create_dir_all(&root).unwrap();
    let key = hex::encode(Sha256::digest(url.as_bytes()));
    let image = root.join(format!("{PREFIX}{key}.png"));
    fs::write(&image, png()).unwrap();
    let cached = cache.get(url.into()).await.unwrap();
    assert_eq!(cached, image.canonicalize().unwrap());
    assert_eq!(fs::read(cached).unwrap(), png());
}

#[cfg(windows)]
#[test]
fn eviction_skips_locked_files_when_other_entries_can_meet_the_budget() {
    use std::os::windows::fs::OpenOptionsExt;
    let fixture = Fixture::new();
    let locked = cache_file(&fixture.0, 1);
    File::options()
        .write(true)
        .open(&locked)
        .unwrap()
        .set_times(FileTimes::new().set_modified(SystemTime::UNIX_EPOCH))
        .unwrap();
    // Permit reads/metadata, but keep Windows deletion sharing disabled.
    let _handle = File::options()
        .read(true)
        .share_mode(3)
        .open(&locked)
        .unwrap();
    let removable = cache_file(&fixture.0, 2);
    trim(&fixture.0, MAX_BYTES, 1, None).unwrap();
    assert!(locked.exists());
    assert!(!removable.exists());
}

#[test]
fn public_url_boundary_rejects_credentials_ports_queries_and_lookalikes() {
    for raw in [
            "https://booth.pximg.net/a.png",
            "https://i.pximg.net/a/b.jpg",
            "https://public-files.gumroad.com/synthetic-preview",
            "https://jinxxy-c.jinxxy-cdn.com/cdn-cgi/image/format=auto,onerror=redirect/synthetic/thumbnail.webp",
        ] {
            assert!(parse_public_url(raw).is_ok());
        }
    for raw in [
        "http://booth.pximg.net/a",
        "https://booth.pximg.net:443/a",
        "https://booth.pximg.net:444/a",
        "https://u:p@booth.pximg.net/a",
        "https://@booth.pximg.net/a",
        "https://booth.pximg.net/a?token=x",
        "https://booth.pximg.net/a#x",
        "https://booth.pximg.net.attacker.test/a",
        "https://pximg.net/a",
        "https://bad_pximg.pximg.net/a",
        "https://-bad.pximg.net/a",
        "https://booth.pximg.net./a",
        "https://127.0.0.1/a",
        " https://booth.pximg.net/a",
        "https://booth.pximg.net\\a",
        "http://public-files.gumroad.com/a",
        "https://public-files.gumroad.com:443/a",
        "https://u:p@public-files.gumroad.com/a",
        "https://public-files.gumroad.com/a?Signature=synthetic",
        "https://public-files.gumroad.com/a#fragment",
        "https://public-files.gumroad.com.attacker.test/a",
        "https://other.gumroad.com/a",
        "https://jinxxy-c.jinxxy-cdn.com.attacker.test/a",
        "https://other.jinxxy-cdn.com/a",
        "https://jinxxy-c.jinxxy-cdn.com/a?token=synthetic",
        "https://jinxxy-c.jinxxy-cdn.com:443/a",
    ] {
        assert!(parse_public_url(raw).is_err(), "{raw}");
    }
}
#[test]
fn raster_signatures_and_stream_bounds_are_exact() {
    for (header, extension) in [
        (png(), "png"),
        (&b"\xff\xd8\xffdata"[..], "jpg"),
        (&b"GIF89a"[..], "gif"),
        (&b"RIFF1234WEBP"[..], "webp"),
    ] {
        assert_eq!(raster_extension(header), Some(extension));
    }
    assert_eq!(raster_extension(b"<svg>"), None);
    assert_eq!(raster_extension(b"RIFF1234WAVE"), None);
    assert_eq!(bounded_size(MAX_FILE - 1, 1).unwrap(), MAX_FILE);
    assert!(bounded_size(MAX_FILE, 1).is_err());
    assert!(bounded_size(u64::MAX, 1).is_err());
}
#[test]
fn closed_temporary_publishes_once_and_roundtrips_cache_hit() {
    let fixture = Fixture::new();
    let key = hash(1);
    let temp = fixture
        .0
        .join(format!("{PREFIX}{key}-{}.part", uuid::Uuid::new_v4()));
    {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .unwrap();
        use std::io::Write;
        (&file).write_all(png()).unwrap();
        file.sync_all().unwrap();
    }
    let published = publish(&fixture.0, &temp, &key, "png").unwrap();
    assert!(!temp.exists());
    assert_eq!(
        find_cached(&fixture.0, &key).unwrap(),
        Some(published.clone())
    );
    fs::write(&temp, png()).unwrap();
    assert_eq!(publish(&fixture.0, &temp, &key, "png").unwrap(), published);
    let unrelated = fixture.0.join("human-image.png");
    fs::write(&unrelated, png()).unwrap();
    assert!(publish(&fixture.0, &unrelated, &key, "png").is_err());
    assert!(unrelated.exists());
}
#[test]
fn eviction_only_removes_managed_files_and_preserves_recent_partial() {
    let fixture = Fixture::new();
    let first = cache_file(&fixture.0, 1);
    let second = cache_file(&fixture.0, 2);
    File::options()
        .write(true)
        .open(&first)
        .unwrap()
        .set_times(FileTimes::new().set_modified(SystemTime::UNIX_EPOCH))
        .unwrap();
    let unrelated = fixture.0.join("user-thumbnail.png");
    fs::write(&unrelated, png()).unwrap();
    let partial = fixture
        .0
        .join(format!("{PREFIX}{}-{}.part", hash(3), uuid::Uuid::new_v4()));
    fs::write(&partial, png()).unwrap();
    trim(&fixture.0, MAX_BYTES, 2, Some(&second)).unwrap();
    assert!(!first.exists());
    assert!(second.exists());
    assert!(unrelated.exists());
    assert!(partial.exists());
    assert!(trim(&fixture.0, 0, 0, None).is_err());
    assert!(!second.exists());
    assert!(unrelated.exists());
    File::options()
        .write(true)
        .open(&partial)
        .unwrap()
        .set_times(FileTimes::new().set_modified(SystemTime::UNIX_EPOCH))
        .unwrap();
    trim(&fixture.0, 0, 0, None).unwrap();
    assert!(!partial.exists());
}
#[test]
fn invalid_cached_content_is_removed_without_touching_other_names() {
    let fixture = Fixture::new();
    let path = cache_file(&fixture.0, 1);
    fs::write(&path, b"<html>not a raster</html>").unwrap();
    assert!(find_cached(&fixture.0, &hash(1)).unwrap().is_none());
    assert!(!path.exists());
    assert!(!managed_name("nna-preview-../media.png"));
    assert!(!partial_name("nna-preview-short.part"));
}
#[test]
fn service_is_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<PreviewCache>();
}
