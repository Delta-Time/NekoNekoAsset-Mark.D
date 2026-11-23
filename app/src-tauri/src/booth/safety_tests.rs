use super::*;
#[test]
fn sign_in_follows_provider_hops_without_expanding_purchase_scope() {
    for value in [
        "https://accounts.booth.pm/users/sign_in",
        "https://oauth.secure.pixiv.net/v2/auth/authorize",
        "https://accounts.pixiv.net/pigya/proxy/authorize",
        "https://booth.pm/users/auth/pixiv/callback",
    ] {
        assert!(auth_navigation(&Url::parse(value).unwrap()), "{value}");
    }
    // pixiv's provider choices and the hops they were observed or reported to take.
    for value in [
        "https://appleid.apple.com/auth/authorize",
        "https://accounts.google.com/v3/signin/identifier",
        "https://accounts.youtube.com/accounts/SetSID",
        "https://accounts.google.com.tw/accounts/SetSID",
        "https://www.facebook.com/login.php",
        "https://api.weibo.com/oauth2/authorize",
        "https://api.x.com/oauth/authenticate",
    ] {
        let url = Url::parse(value).unwrap();
        assert!(auth_navigation(&url), "{value}");
        assert!(!booth_host(url.host_str().unwrap()));
    }
    for value in [
        "http://accounts.google.com/o/oauth2/v2/auth",
        "https://accounts.google.com@evil.invalid/o/oauth2/v2/auth",
        "https://accounts.google.com:444/o/oauth2/v2/auth",
        "https://127.0.0.1/login",
        "https://[::1]/login",
        "pixiv://account/login",
        "https://booth.pm/ja/items/900001",
        "https://synthetic.booth.pm/items/900001",
        "https://booth.pm/downloadables/700001",
        "https://accounts.booth.pm/gifts/700001",
        "https://booth.pm/cart",
        "https://checkout.booth.pm/checkout",
    ] {
        assert!(!auth_navigation(&Url::parse(value).unwrap()), "{value}");
    }
}
#[test]
fn observed_deeplink_retains_only_canonical_file_and_variant_identity() {
    let canonical =
        observed_resolver("https://booth.pm/downloadables/700001?variation_id=800001").unwrap();
    assert_eq!(observed_resolver("https://booth.pm/downloadables/700001/deeplink?client=booth-library-manager&variation_id=800001&type=zip"), Some(canonical));
    assert!(
        resolver("https://booth.pm/downloadables/700001/deeplink?variation_id=800001").is_none()
    );
    for url in [
        "https://booth.pm/downloadables/700001/deeplink/extra?variation_id=800001",
        "https://booth.pm/downloadables/700001/deeplink/deeplink?variation_id=800001",
        "https://booth.pm/downloadables/not-numeric/deeplink?variation_id=800001",
        "https://booth.pm/downloadables/700001/deeplink?variation_id=800001&variation_id=800002",
        "https://other.invalid/downloadables/700001/deeplink?variation_id=800001",
        "http://booth.pm/downloadables/700001/deeplink?variation_id=800001",
    ] {
        assert!(observed_resolver(url).is_none(), "{url}");
    }
}
#[test]
fn free_item_queue_routes_are_canonical_and_separate_from_history_routes() {
    let route = free_item_route("900001").unwrap();
    assert_eq!(free_item_id(&route), Some("900001"));
    assert!(super::route(&route, "free").is_err());
    for route in [
        "https://booth.pm/items/900001",
        "https://synthetic.booth.pm/ja/items/900001",
        "https://booth.pm/ja/items/900001?token=x",
        "https://booth.pm/ja/items/900001/",
        "https://booth.pm/ja/items/%39",
        "https://booth.pm:443/ja/items/900001",
    ] {
        assert!(free_item_id(route).is_none());
    }
    assert!(free_item_route("not-numeric").is_err());
}
#[test]
fn public_item_url_normalization_preserves_shop_host_and_explicit_locale() {
    for (input, expected) in [
        (
            "https://booth.pm/items/900001",
            "https://booth.pm/ja/items/900001",
        ),
        (
            "https://booth.pm/ja/items/900001",
            "https://booth.pm/ja/items/900001",
        ),
        (
            "https://booth.pm/en/items/900001",
            "https://booth.pm/en/items/900001",
        ),
        (
            "https://synthetic.booth.pm/items/900001",
            "https://synthetic.booth.pm/items/900001",
        ),
        (
            "https://synthetic.booth.pm/en/items/900001",
            "https://synthetic.booth.pm/en/items/900001",
        ),
    ] {
        let normalized = public_url(input, "item").unwrap();
        assert_eq!(normalized, expected);
        assert_eq!(item_id(&normalized).as_deref(), Some("900001"));
        assert_eq!(public_url(&normalized, "item").as_deref(), Some(expected));
    }
    for input in [
        "https://accounts.booth.pm/items/900001",
        "https://manage.booth.pm/items/900001",
        "https://booth.pm.evil.test/items/900001",
        "https://user@booth.pm/items/900001",
        "https://booth.pm:444/items/900001",
        "https://booth.pm/items/not-numeric",
        "https://booth.pm/downloadables/900001",
    ] {
        assert!(public_url(input, "item").is_none());
    }
    assert_eq!(
        public_url("https://synthetic.booth.pm/", "shop").as_deref(),
        Some("https://synthetic.booth.pm/")
    );
}
#[test]
fn network_boundaries() {
    for value in [
        "http://booth.pm/downloadables/1",
        "https://booth.pm.evil.test/downloadables/1",
        "https://booth.pm/downloadables/1?token=secret",
        "https://booth.pm/downloadables/1?type=flac",
        "https://booth.pm/downloadables/1?type=mp3&type=mp3",
        "https://user@booth.pm/downloadables/1",
        "https://booth.pm:444/downloadables/1",
    ] {
        assert!(resolver(value).is_none());
    }
    let track = resolver("https://booth.pm/downloadables/32?variation_id=8&type=mp3").unwrap();
    assert_eq!(track.2, "https://booth.pm/downloadables/32?variation_id=8");
    assert_eq!(
        resolver("https://booth.pm/downloadables/32?variation_id=8")
            .unwrap()
            .1
            .as_deref(),
        Some("8")
    );
    assert!(route("/orders/12", "purchase").is_ok());
    assert_eq!(
        route("/library", "library").unwrap().as_str(),
        "https://accounts.booth.pm/library?page=1"
    );
    assert!(route("/library?", "library").is_err());
    // The one sort a route takes comes out one way, whichever way BOOTH's pager wrote it.
    for value in [
        "/library?sort=file_updated_at_desc",
        "/library?page=1&sort=file_updated_at_desc",
        "/library?sort=file_updated_at_desc&page=1",
    ] {
        let url = route(value, "library").unwrap();
        assert_eq!(
            url.as_str(),
            "https://accounts.booth.pm/library?sort=file_updated_at_desc&page=1"
        );
        let canonical = format!("{}?{}", url.path(), url.query().unwrap());
        assert_eq!(canonical, updated_route(1));
        assert!(updated_order(&canonical));
    }
    assert!(!updated_order("/library?page=1"));
    for (value, source) in [
        ("/library?sort=bogus&page=1", "library"),
        (
            "/library?sort=file_updated_at_desc&sort=file_updated_at_desc",
            "library",
        ),
        ("/library?sort=file_updated_at_desc&page=0", "library"),
        (
            "/library?sort=file_updated_at_desc&page=1&q=authored",
            "library",
        ),
        ("/library?page=1&page=2", "library"),
        ("/library/gifts?sort=file_updated_at_desc&page=1", "gift"),
        (
            "/library/free_downloads?sort=file_updated_at_desc&page=1",
            "free",
        ),
        ("/orders?sort=file_updated_at_desc&page=1", "purchase"),
    ] {
        assert!(route(value, source).is_err(), "{value}");
    }
    assert!(route("/orders/12/receipt", "purchase").is_err());
    assert!(route("https://evil.test/library?page=1", "library").is_err());
    assert!(route("/library?page=1&token=x", "library").is_err());
    assert!(!download_destination(
        &Url::parse("https://127.0.0.1/a").unwrap()
    ));
}
#[test]
fn downloads_follow_any_https_host_and_only_booth_signs_in() {
    for value in [
        "https://booth.pm/downloadables/700001",
        "https://booth-download.pximg.net/authored/file.zip",
        "https://authored-cdn.example/signed/file.zip?token=authored",
    ] {
        assert!(download_destination(&Url::parse(value).unwrap()), "{value}");
    }
    for value in [
        "http://booth-download.pximg.net/authored/file.zip",
        "https://user@authored-cdn.example/file.zip",
        "https://authored-cdn.example:8443/file.zip",
        "https://192.0.2.1/file.zip",
        "https://[2001:db8::1]/file.zip",
        "https://localhost/file.zip",
        "https://nas.local/file.zip",
        "https://intranet/file.zip",
    ] {
        assert!(
            !download_destination(&Url::parse(value).unwrap()),
            "{value}"
        );
    }
    for (value, sign_in) in [
        ("https://accounts.booth.pm/users/sign_in", true),
        ("https://booth.pm/users/login", true),
        (
            "https://booth-download.pximg.net/authored/login_bonus.zip",
            false,
        ),
        ("https://authored-cdn.example/sign_in/file.zip", false),
        ("https://booth.pm/downloadables/700001", false),
    ] {
        assert_eq!(
            sign_in_page(&Url::parse(value).unwrap()),
            sign_in,
            "{value}"
        );
    }
}
