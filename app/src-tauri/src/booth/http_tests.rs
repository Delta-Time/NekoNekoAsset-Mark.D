use super::*;
#[test]
fn listing_and_detail_slots_are_bounded_separately() {
    let http = MetadataHttp::new();
    assert_eq!(http.listings.available_permits(), 1);
    assert_eq!(http.metadata.available_permits(), 2);
    for path in [
        "/library",
        "/library/gifts",
        "/library/free_downloads",
        "/orders",
    ] {
        assert!(listing_url(
            &Url::parse(&format!("https://accounts.booth.pm{path}?page=2")).unwrap()
        ));
    }
    for raw in [
        "https://accounts.booth.pm/orders/910001",
        "https://booth.pm/ja/items/900001",
        "https://booth.pm/ja/items/900001.json",
    ] {
        assert!(!listing_url(&Url::parse(raw).unwrap()));
    }
}
#[tokio::test]
async fn pacing_waiters_obey_shared_start_interval_and_later_deferral() {
    let http = MetadataHttp::new();
    http.wait_start().await;
    let started = Instant::now();
    let waiter = http.wait_start();
    let defer = async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        http.defer(Duration::from_millis(1100)).await;
    };
    tokio::join!(waiter, defer);
    assert!(started.elapsed() >= Duration::from_millis(1200));
    let next = Instant::now();
    http.wait_start().await;
    assert!(next.elapsed() >= Duration::from_millis(350));
}
#[test]
fn start_spacing_jitter_has_bounded_exact_mean() {
    let intervals: Vec<_> = (0..=u8::MAX)
        .map(|sample| start_spacing(sample).as_millis())
        .collect();
    assert!(intervals.iter().all(|ms| (350..=450).contains(ms)));
    assert_eq!(intervals.first(), Some(&350));
    assert_eq!(intervals.last(), Some(&450));
    assert_eq!(intervals.iter().sum::<u128>(), 400 * 256);
    for sample in 0..=u8::MAX {
        assert_eq!(
            start_spacing(sample) + start_spacing(u8::MAX - sample),
            Duration::from_millis(800)
        );
    }
}
#[test]
fn free_item_response_accepts_any_https_booth_page_for_the_same_item() {
    let requested = Url::parse("https://booth.pm/ja/items/900001").unwrap();
    for actual in [
        "https://booth.pm/ja/items/900001",
        "https://synthetic.booth.pm/ja/items/900001",
        "https://synthetic.booth.pm/items/900001",
        "https://booth.pm/items/900001",
        "https://booth.pm/en/items/900001",
        "https://booth.pm/ja/items/900001?x=1",
    ] {
        assert!(
            validate_source_response(&requested, &Url::parse(actual).unwrap()).is_ok(),
            "{actual}"
        );
    }
    for actual in [
        "https://booth.pm/ja/items/900002",
        "https://synthetic.booth.pm/items/900002",
        "http://booth.pm/ja/items/900001",
        "https://booth.pm.evil.test/ja/items/900001",
        "https://authored.example/ja/items/900001",
        "https://booth.pm/ja/items/900001/reviews",
        "https://accounts.booth.pm/library/free_downloads?page=1",
    ] {
        assert_eq!(
            validate_source_response(&requested, &Url::parse(actual).unwrap())
                .unwrap_err()
                .to_string(),
            "RESPONSE_ROUTE_CHANGED"
        );
    }
}
#[test]
fn source_response_keeps_requested_route_and_only_canonicalizes_page_one() {
    let url = |path: &str| Url::parse(&format!("https://accounts.booth.pm{path}")).unwrap();
    for (requested, actual) in [
        ("/library?page=1", "/library"),
        ("/library", "/library?page=1"),
        ("/orders?page=1", "/orders"),
        ("/orders/900001", "/orders/900001"),
    ] {
        assert!(validate_source_response(&url(requested), &url(actual)).is_ok());
    }
    for (requested, actual) in [
        ("/library?page=2", "/library?page=1"),
        ("/library?page=2", "/library"),
        ("/orders?page=1", "/orders/900001"),
        ("/orders/900001", "/orders?page=1"),
        ("/library?page=1", "/library/gifts?page=1"),
        ("/library?page=1", "/library?page=1&filter=active"),
    ] {
        assert_eq!(
            validate_source_response(&url(requested), &url(actual))
                .unwrap_err()
                .to_string(),
            "RESPONSE_ROUTE_CHANGED"
        );
    }
    assert!(validate_source_response(
        &url("/library?page=1"),
        &Url::parse("https://booth.pm/library?page=1").unwrap()
    )
    .is_err());
}
#[test]
fn redirects_follow_any_https_host_and_keep_pixiv_as_the_sign_in_hop() {
    for value in [
        "https://accounts.booth.pm/library?page=1",
        "https://synthetic.booth.pm/items/900001",
        "https://booth-download.pximg.net/authored/file.zip",
        "https://authored-cdn.example/signed/file.zip",
    ] {
        assert_eq!(redirect_error(&Url::parse(value).unwrap()), None, "{value}");
    }
    for value in [
        "https://accounts.pixiv.net/login",
        "https://oauth.secure.pixiv.net/v2/auth/authorize",
    ] {
        assert_eq!(
            redirect_error(&Url::parse(value).unwrap()),
            Some("AUTH_REQUIRED")
        );
    }
    for value in [
        "http://authored-cdn.example/file.zip",
        "https://user@authored-cdn.example/file.zip",
        "https://authored-cdn.example:8443/file.zip",
        "https://192.0.2.1/file.zip",
    ] {
        assert_eq!(
            redirect_error(&Url::parse(value).unwrap()),
            Some("UNSAFE_REDIRECT")
        );
    }
}
#[test]
fn only_an_account_listing_can_say_the_session_is_signed_out() {
    for path in [
        "/library?page=3",
        "/library/gifts?page=1",
        "/library/free_downloads?page=1",
        "/orders?page=2",
    ] {
        let url = Url::parse(&format!("https://accounts.booth.pm{path}")).unwrap();
        assert_eq!(signed_out(&url), "AUTH_REQUIRED", "{path}");
    }
    for value in [
        "https://accounts.booth.pm/orders/910001",
        "https://booth.pm/ja/items/900001",
        "https://booth.pm/ja/items/900001.json",
    ] {
        assert_eq!(
            signed_out(&Url::parse(value).unwrap()),
            "PERMISSION_DENIED",
            "{value}"
        );
    }
}
#[test]
fn retry_after_obeys_dates_and_bounds() {
    let now = std::time::UNIX_EPOCH + Duration::from_secs(1000);
    assert_eq!(retry_after(Some("90"), now), Duration::from_secs(90));
    assert_eq!(retry_after(Some("600"), now), Duration::from_secs(600));
    assert_eq!(
        retry_after(Some("18446744073709551615"), now),
        Duration::from_secs(5)
    );
    let date = "Thu, 01 Jan 1970 00:17:10 GMT";
    assert_eq!(retry_after(Some(date), now), Duration::from_secs(30));
    assert!(retry_after(Some("999999"), now) > Duration::from_secs(120));
    assert_eq!(retry_after(Some("bad"), now), Duration::from_secs(5));
}
