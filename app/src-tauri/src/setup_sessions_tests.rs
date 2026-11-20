
use super::*;
use serde_json::json;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("nna-setup-auth-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn profile(&self, provider: StoreProvider, id: &str) -> PathBuf {
        let path = session::profile_path(&self.0, provider, id).unwrap();
        fs::create_dir_all(path.join("WebView/Cache")).unwrap();
        fs::write(path.join("session.bin"), b"authored protected fixture").unwrap();
        fs::write(
            path.join("WebView/Cache/authored"),
            b"authored browser fixture",
        )
        .unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = remove_owned_profile(&self.0, &self.0);
    }
}
#[test]
fn cleanup_requires_exact_pending_marker_and_released_lifetime_lease() {
    let fixture = Fixture::new();
    let sessions = SetupSessions::new(&fixture.0).unwrap();
    assert!(!fixture.0.join(MARKERS).exists());
    let id = uuid::Uuid::new_v4().to_string();
    sessions.arm(StoreProvider::Gumroad, &id).unwrap();
    let pending = fixture.profile(StoreProvider::Gumroad, &id);
    let old_id = uuid::Uuid::new_v4().to_string();
    let unmarked = fixture.profile(StoreProvider::Gumroad, &old_id);
    fs::write(
        fixture.0.join(MARKERS).join("malformed.json"),
        b"invalid marker",
    )
    .unwrap();
    assert_eq!(cleanup_abandoned(&fixture.0).unwrap(), 0);
    assert!(pending.join("session.bin").exists());
    drop(sessions);
    assert_eq!(cleanup_abandoned(&fixture.0).unwrap(), 1);
    assert!(!pending.exists());
    assert_eq!(
        fs::read(unmarked.join("session.bin")).unwrap(),
        b"authored protected fixture"
    );
    assert!(fixture.0.join(MARKERS).join("malformed.json").exists());
}
#[test]
fn released_marker_and_repeated_login_never_authorize_committed_profile_cleanup() {
    let fixture = Fixture::new();
    let sessions = SetupSessions::new(&fixture.0).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    sessions.arm(StoreProvider::Booth, &id).unwrap();
    let profile = fixture.profile(StoreProvider::Booth, &id);
    sessions
        .release(&[json!({"store":"BOOTH","id":id})])
        .unwrap();
    sessions.arm(StoreProvider::Booth, &id).unwrap();
    drop(sessions);
    assert_eq!(cleanup_abandoned(&fixture.0).unwrap(), 0);
    assert_eq!(
        fs::read(profile.join("session.bin")).unwrap(),
        b"authored protected fixture"
    );
    let next = SetupSessions::new(&fixture.0).unwrap();
    assert!(next.arm(StoreProvider::Booth, &id).is_err());
    assert!(profile.join("WebView/Cache/authored").exists());
}
#[test]
#[cfg(windows)]
fn abandoned_profile_with_a_junction_is_retained_without_touching_the_target() {
    use std::os::windows::process::CommandExt;
    let fixture = Fixture::new();
    let sessions = SetupSessions::new(&fixture.0).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    sessions.arm(StoreProvider::Jinxxy, &id).unwrap();
    let profile = fixture.profile(StoreProvider::Jinxxy, &id);
    let outside = fixture.0.join("unrelated");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("keep"), b"unrelated authored bytes").unwrap();
    let junction = profile.join("linked");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&outside)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "authored junction fixture must be available"
    );
    drop(sessions);
    let cleaned = cleanup_abandoned(&fixture.0).unwrap();
    let profile_retained = profile.join("session.bin").exists();
    let outside_bytes = fs::read(outside.join("keep")).unwrap();
    fs::remove_dir(&junction).unwrap();
    assert_eq!(cleaned, 0);
    assert!(profile_retained);
    assert_eq!(outside_bytes, b"unrelated authored bytes");
}
