
use super::*;
#[test]
fn discarded_draft_clears_only_its_snapshot_and_retains_established_profiles() {
    let root = std::env::temp_dir().join(format!(
        "nna-authored-draft-cleanup-{}",
        uuid::Uuid::new_v4()
    ));
    let catalogue = crate::catalogue::Catalogue::open(&root).unwrap();
    let draft = "10000000-0000-4000-8000-000000000001";
    let established = "10000000-0000-4000-8000-000000000002";
    catalogue
        .put(
            "account",
            draft,
            serde_json::json!({"id":draft,"store":"BOOTH"}),
        )
        .unwrap();
    catalogue
        .put(
            "account",
            established,
            serde_json::json!({"id":established,"store":"BOOTH","identityVerified":true}),
        )
        .unwrap();
    let draft_profile = profile_path(&catalogue.device_dir(), StoreProvider::Booth, draft).unwrap();
    let established_profile =
        profile_path(&catalogue.device_dir(), StoreProvider::Booth, established).unwrap();
    let other_provider =
        profile_path(&catalogue.device_dir(), StoreProvider::Gumroad, draft).unwrap();
    for profile in [&draft_profile, &established_profile, &other_provider] {
        std::fs::create_dir_all(profile).unwrap();
        std::fs::write(profile.join("session.bin"), b"authored snapshot marker").unwrap();
        std::fs::write(
            profile.join("browser-profile-marker"),
            b"preserve profile bytes",
        )
        .unwrap();
    }
    assert!(discard_unverified_account(&catalogue, StoreProvider::Booth, draft).unwrap());
    assert!(!draft_profile.join("session.bin").exists());
    assert!(draft_profile.join("browser-profile-marker").exists());
    assert!(!discard_unverified_account(&catalogue, StoreProvider::Booth, established).unwrap());
    assert!(established_profile.join("session.bin").exists());
    assert!(other_provider.join("session.bin").exists());
    drop(catalogue);
    crate::storage::validate_library_path(&root).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn jinxxy_hosts_cover_its_subdomains_and_set_only_the_language_preference() {
    for host in JINXXY_HOSTS
        .iter()
        .copied()
        .chain(["other.jinxxy.com", "jx.jinxxy.com"])
    {
        assert!(navigation_allowed(
            StoreProvider::Jinxxy,
            &Url::parse(&format!("https://{host}/my/inventory")).unwrap()
        ));
        assert!(jinxxy_host(host) && cookie_host(StoreProvider::Jinxxy, host));
    }
    assert!(cookie_host(StoreProvider::Jinxxy, "api.jinxxy.com"));
    for host in [
        "ja.jinxxy.com.evil.test",
        "evil-jinxxy.com",
        "jinxxy.co",
        "discord.com",
    ] {
        assert!(!jinxxy_host(host) && !cookie_host(StoreProvider::Jinxxy, host));
    }
    let cookie = tauri::webview::Cookie::parse(JINXXY_ENGLISH_PREFERENCE).unwrap();
    assert_eq!((cookie.name(), cookie.value()), ("jx_locale", "en"));
    assert_eq!(cookie.domain(), Some("jinxxy.com"));
    assert_eq!(cookie.path(), Some("/"));
    assert_eq!(cookie.max_age().unwrap().whole_seconds(), 31536000);
    assert_eq!(cookie.secure(), Some(true));
    assert_eq!(
        cookie.same_site(),
        Some(tauri::webview::cookie::SameSite::Lax)
    );
}

#[test]
fn native_profiles_separate_provider_and_account_and_reject_path_injection() {
    let root = Path::new("authored-root");
    let first = "ffffffff-ffff-4fff-8fff-ffffffffffff";
    let second = "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee";
    assert_ne!(
        profile_path(root, StoreProvider::Gumroad, first).unwrap(),
        profile_path(root, StoreProvider::Jinxxy, first).unwrap()
    );
    assert_ne!(
        profile_path(root, StoreProvider::Gumroad, first).unwrap(),
        profile_path(root, StoreProvider::Gumroad, second).unwrap()
    );
    assert!(profile_path(root, StoreProvider::Gumroad, "../other").is_err());
}

#[test]
fn native_sign_in_follows_provider_hops_but_not_store_purchases_or_downloads() {
    let allowed = |provider, value| navigation_allowed(provider, &Url::parse(value).unwrap());
    // The stores' provider choices and the hops they were observed or reported to take.
    for value in [
        "https://gumroad.com/login",
        "https://accounts.google.com/v3/signin/identifier",
        "https://accounts.youtube.com/accounts/SetSID",
        "https://accounts.google.co.jp/accounts/SetSID",
        "https://api.x.com/oauth/authenticate",
        "https://connect.stripe.com/oauth/v2/authorize",
        "https://gumroad.com/users/auth/google_oauth2/callback",
    ] {
        assert!(allowed(StoreProvider::Gumroad, value), "{value}");
    }
    for value in [
        "https://zh.jinxxy.com/login",
        "https://discord.com/oauth2/authorize",
        "https://jinxxy.com/connect/discord/callback",
    ] {
        assert!(allowed(StoreProvider::Jinxxy, value), "{value}");
    }
    // Only the store's own cookies are ever read from its window.
    for host in ["accounts.google.com", "api.x.com", "zh.jinxxy.com"] {
        assert!(!cookie_host(StoreProvider::Gumroad, host), "{host}");
    }
    for host in ["discord.com", "gumroad.com"] {
        assert!(!cookie_host(StoreProvider::Jinxxy, host), "{host}");
    }
    for value in [
        "http://gumroad.com/login",
        "https://user:secret@gumroad.com/login",
        "https://gumroad.com:8443/login",
        "https://gumroad.com/checkout",
        "https://gumroad.com/r/authored-token",
        "https://gumroad.com/d/authored-capability",
        "https://app.gumroad.com/download/fixture.zip",
        "http://accounts.google.com/o/oauth2/auth",
        "https://user:secret@accounts.google.com/o/oauth2/auth",
        "https://accounts.google.com:8443/o/oauth2/auth",
        "https://127.0.0.1/login",
        "https://10.0.0.1/login",
    ] {
        assert!(!allowed(StoreProvider::Gumroad, value), "{value}");
    }
    for value in [
        "https://api.jinxxy.com/login",
        "https://zh.jinxxy.com/download/fixture.zip",
        "https://secret@zh.jinxxy.com/login",
        "http://zh.jinxxy.com/login",
        "https://zh.jinxxy.com:8443/login",
        "discord://-/channels/@me",
    ] {
        assert!(!allowed(StoreProvider::Jinxxy, value), "{value}");
    }
}
