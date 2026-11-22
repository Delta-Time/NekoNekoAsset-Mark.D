use super::*;
use super::{
    acquisition_diagnostics::select_entry_diagnostic_route,
    checkpoints::source_traversal_verified,
    publication::publication_entry_certifiable,
    routes::{next_route, next_routes},
};
#[test]
fn incremental_purchase_lists_every_order_but_reuses_known_receipts() {
    let known_row = parser::Row {
        source_record_id: "purchase:known".into(),
        product_id: Some("900001".into()),
        order_id: Some("910001".into()),
        manifest_complete: true,
        ..Default::default()
    };
    let known = BTreeMap::from([
        ("/orders/910001".to_owned(), vec![known_row]),
        ("/orders/910002".to_owned(), vec![]),
    ]);
    let listing = |route: &str, details: &[&str], next: &[&str]| parser::Page {
        source: "purchase".into(),
        route: route.into(),
        rows: vec![],
        next: next.iter().map(|route| route.to_string()).collect(),
        details: details.iter().map(|route| route.to_string()).collect(),
        terminal: next.is_empty(),
        unresolved: 0,
        fingerprint: route.into(),
        is_detail: false,
        ordering: parser::Ordering::Unknown,
        free_item: None,
    };
    let mut checkpoint = Checkpoint::fresh("synthetic", "purchase", "incremental").unwrap();
    let first = listing(
        "/orders?page=1",
        &["/orders/910001", "/orders/910002", "/orders/910009"],
        &["/orders?page=2"],
    );
    record_source_page(&mut checkpoint, "purchase", "/orders?page=1", &first, None).unwrap();
    reuse_known_receipts(&mut checkpoint, &known);
    assert_eq!(checkpoint.pending, ["/orders?page=2", "/orders/910009"]);
    assert_eq!(
        checkpoint.reused,
        BTreeSet::from(["/orders/910001".into(), "/orders/910002".into()])
    );
    assert!(checkpoint.product_ids.contains("900001"));
    // An order shifted onto the next page is neither queued nor reused twice.
    let second = listing("/orders?page=2", &["/orders/910002"], &[]);
    record_source_page(&mut checkpoint, "purchase", "/orders?page=2", &second, None).unwrap();
    reuse_known_receipts(&mut checkpoint, &known);
    assert_eq!(checkpoint.pending, ["/orders/910009"]);
    assert_eq!(checkpoint.reused.len(), 2);
    // Checkpoints saved before reuse existed still resume.
    let mut saved =
        serde_json::to_value(Checkpoint::fresh("synthetic", "purchase", "incremental").unwrap())
            .unwrap();
    saved.as_object_mut().unwrap().remove("reused");
    assert!(resumable_checkpoint(Some(saved), "incremental", false)
        .unwrap()
        .unwrap()
        .reused
        .is_empty());
}
#[test]
fn unchanged_reobservation_rewrites_no_catalogue_record() {
    let path = std::env::temp_dir().join(format!("nna-booth-unchanged-{}", uuid::Uuid::new_v4()));
    let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
    let service = BoothService::new(catalogue.clone()).unwrap();
    let account = "synthetic-account";
    let job = json!({"id":"synthetic-job","type":"boothSync","accountId":account,"state":"running","progress":{}});
    catalogue.put("job", "synthetic-job", job.clone()).unwrap();
    let receipt = parser::Row {
        source_record_id: "purchase:synthetic-variant".into(),
        product_id: Some("900001".into()),
        order_id: Some("910001".into()),
        name: Some("Synthetic source title".into()),
        manifest_complete: true,
        product_line_total_jpy: Some(100),
        acquired_date_text: Some("2026/10/11 09:30".into()),
        files: vec![parser::File {
            id: "700001".into(),
            variant_id: Some("800001".into()),
            name: Some("authored.zip".into()),
            size_bytes: Some(10),
            source_url: "https://booth.pm/downloadables/700001?variation_id=800001".into(),
            revision: "authored".into(),
            format: None,
        }],
        ..Default::default()
    };
    let mut library = receipt.clone();
    library.source_record_id = "library:synthetic-variant".into();
    library.order_id = None;
    library.product_line_total_jpy = None;
    let draft = json!({"id":"900001","store":"BOOTH","title":"Synthetic source title","publicMetadataState":"complete"});
    let files = BTreeSet::from(["synthetic-account:700001:800001".to_owned()]);
    let publish = |source: &str, route: &str, row: &parser::Row| {
        let key = format!("booth:{account}:{source}");
        let mut checkpoint = Checkpoint::fresh(account, source, "incremental").unwrap();
        let routes = BTreeMap::from([(row.source_record_id.clone(), route.to_owned())]);
        service
            .publish_product(
                &job,
                account,
                source,
                "900001",
                &[row],
                &routes,
                &draft,
                &files,
                &key,
                &mut checkpoint,
                false,
            )
            .unwrap();
    };
    let facts = || -> Vec<Value> {
        ["product", "acquisition", "entitlement", "remote_file"]
            .into_iter()
            .flat_map(|kind| catalogue.list(kind).unwrap())
            .collect()
    };
    publish("purchase", "/orders/910001", &receipt);
    publish("library", "/library?page=1", &library);
    let first = facts();
    let file = catalogue
        .get("remote_file", "synthetic-account:700001:800001")
        .unwrap()
        .unwrap();
    assert_eq!(file["resolverPage"], "/orders/910001");
    std::thread::sleep(std::time::Duration::from_millis(5));
    // Later passes observe the same facts at a later time, from both listings.
    publish("purchase", "/orders/910001", &receipt);
    publish("library", "/library?page=1", &library);
    assert_eq!(facts(), first);
    let entry = |source: &str, row: &parser::Row, route: &str| ProductPublication {
        account: account.into(),
        source: source.into(),
        product: "900001".into(),
        rows: vec![row.clone()],
        routes: BTreeMap::from([(row.source_record_id.clone(), route.to_owned())]),
        draft: draft.clone(),
        files: files.clone(),
        checkpoint_key: format!("booth:{account}:{source}"),
        force_verify: false,
        needs_enrichment: false,
    };
    let mut checkpoints = BTreeMap::from([(
        format!("booth:{account}:purchase"),
        Checkpoint::fresh(account, "purchase", "incremental").unwrap(),
    )]);
    // An additive pass over a certified unchanged grant keeps its basis.
    assert!(!service
        .commit_product_group_mode(
            &job,
            "900001",
            &[entry("purchase", &receipt, "/orders/910001")],
            &mut checkpoints,
            false
        )
        .unwrap());
    assert_eq!(facts(), first);
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn explicit_fresh_traversal_ignores_an_unusable_previous_checkpoint() {
    let malformed = json!({"mode":"incremental","pending":"unusable"});
    assert!(resumable_checkpoint(Some(malformed.clone()), "incremental", false).is_err());
    assert!(
        resumable_checkpoint(Some(malformed.clone()), "forced", false)
            .unwrap()
            .is_none()
    );
    assert!(resumable_checkpoint(Some(malformed), "incremental", true)
        .unwrap()
        .is_none());
    let saved = Checkpoint::fresh("authored-account", "library", "incremental").unwrap();
    assert!(resumable_checkpoint(
        Some(serde_json::to_value(saved).unwrap()),
        "incremental",
        false
    )
    .unwrap()
    .is_some());
}
#[test]
fn only_source_local_failures_allow_other_source_scans_to_continue() {
    for code in [
        "COLLECTOR_UNSUPPORTED",
        "SOURCE_UNSUPPORTED",
        "CHECKPOINT_INVALID",
        "NO_RECOGNIZED_ROWS",
        "SOURCE_MARKER_UNKNOWN",
        "DIGITAL_PROOF_UNPROVEN",
        "SOURCE_TRAVERSAL_LIMIT",
        "METADATA_TOO_LARGE",
    ] {
        assert!(source_local_failure(code));
    }
    for code in [
        "AUTH_REQUIRED",
        "AUTH_CHALLENGE",
        "PERMISSION_DENIED",
        "RATE_LIMITED",
        "NETWORK_UNAVAILABLE",
        "NETWORK_TIMEOUT",
        "SOURCE_HTTP_FAILED",
        "CANCELLED",
        "ACCOUNT_UNAVAILABLE",
        "CONCURRENT_UPDATE",
        "catalogue.lock",
    ] {
        assert!(!source_local_failure(code));
    }
}
#[test]
fn one_assets_refused_or_unreadable_page_stays_local_and_outages_stop_the_job() {
    for code in [
        "AUTH_REQUIRED",
        "PERMISSION_DENIED",
        "RESPONSE_ROUTE_CHANGED",
        "UNSAFE_REDIRECT",
        "METADATA_TOO_LARGE",
        "METADATA_ENCODING",
        "PUBLIC_METADATA_INVALID",
        "PUBLIC_ID_MISMATCH",
    ] {
        assert!(product_local_failure(code), "{code}");
    }
    for code in [
        "NETWORK_UNAVAILABLE",
        "NETWORK_TIMEOUT",
        "RATE_LIMITED",
        "AUTH_CHALLENGE",
        "SOURCE_HTTP_FAILED",
        "PUBLIC_METADATA_FAILED",
        "CANCELLED",
        "CHECKPOINT_INVALID",
        "catalogue.lock",
    ] {
        assert!(!product_local_failure(code), "{code}");
    }
    use reqwest::StatusCode;
    for status in [
        StatusCode::BAD_REQUEST,
        StatusCode::NOT_ACCEPTABLE,
        StatusCode::GONE,
    ] {
        assert!(
            product_local_failure(public_status_code(status)),
            "{status}"
        );
    }
    for status in [StatusCode::INTERNAL_SERVER_ERROR, StatusCode::BAD_GATEWAY] {
        assert_eq!(public_status_code(status), "PUBLIC_METADATA_FAILED");
    }
    // Only a transient public failure is held back for the one later attempt.
    let mut retry = PublicRetry::default();
    let request = |product: &str| PublicRequest {
        product: product.into(),
        draft: json!({}),
        account: "authored".into(),
        source: "library".into(),
        run_id: "current".into(),
    };
    let refused: Result<Value, String> =
        Err(public_status_code(StatusCode::BAD_REQUEST).to_owned());
    assert_eq!(retry.settle(request("900001"), refused).len(), 1);
    let failed: Result<Value, String> = Err(public_status_code(StatusCode::BAD_GATEWAY).to_owned());
    assert!(retry.settle(request("900002"), failed).is_empty());
}
#[test]
fn a_moved_listing_or_asset_gaps_complete_the_job_but_an_unproven_or_unbound_listing_does_not() {
    let mut checkpoint = Checkpoint::fresh("authored", "library", "full").unwrap();
    checkpoint.terminal = true;
    checkpoint.pending.clear();
    checkpoint.head = "head".into();
    checkpoint.product_ids.insert("900001".into());
    let local = |c: &Checkpoint, head: &str, unproven, assets, rows| {
        source_gaps_local(c, head, unproven, assets, rows)
    };
    // Nothing left open has nothing to excuse.
    assert!(!local(&checkpoint, "head", false, false, 0));
    // A purchase arrived while the pass ran, or assets and rows were left as they were.
    assert!(local(&checkpoint, "moved", false, false, 0));
    assert!(local(&checkpoint, "head", false, true, 0));
    assert!(local(&checkpoint, "head", false, false, 2));
    // The end of the listing was never proven.
    assert!(!local(&checkpoint, "moved", true, false, 0));
    let mut open = checkpoint.clone();
    open.pending.push("/library?page=2".into());
    assert!(!local(&open, "head", false, true, 0));
    open = checkpoint.clone();
    open.terminal = false;
    assert!(!local(&open, "moved", false, false, 0));
    // Rows but no row bound to an asset: the parser no longer understands the listing.
    checkpoint.product_ids.clear();
    assert!(!local(&checkpoint, "head", false, false, 3));
    assert!(local(&checkpoint, "moved", false, false, 0));
}
#[test]
fn unreadable_receipt_or_free_item_keeps_membership_open_without_inventing_rows() {
    let receipt = unreadable_page("purchase", "/orders/910001", "PERMISSION_DENIED").unwrap();
    assert!(receipt.is_detail && receipt.rows.is_empty() && receipt.free_item.is_none());
    assert!(receipt.next.is_empty() && receipt.details.is_empty() && !receipt.terminal);
    assert_eq!(
        (receipt.route.as_str(), receipt.unresolved),
        ("/orders/910001", 1)
    );
    // An unread receipt could hold any known asset: one unbound row, no retirements.
    assert_eq!(unbound_row_count(&receipt), 1);
    let journal =
        json!({"accountId":"authored","source":"purchase","runId":"current","page":receipt});
    assert!(checked_journal_page(&journal, "authored", "purchase", "current").is_ok());
    let mut checkpoint = Checkpoint::fresh("authored", "purchase", "full").unwrap();
    checkpoint.pending.push("/orders/910001".into());
    record_source_page(
        &mut checkpoint,
        "purchase",
        "/orders/910001",
        &receipt,
        None,
    )
    .unwrap();
    assert_eq!(checkpoint.pending, vec!["/orders?page=1".to_owned()]);
    assert_eq!(
        (checkpoint.details_completed, checkpoint.unresolved),
        (1, 1)
    );
    assert!(!checkpoint.terminal);
    let mut history = BTreeMap::new();
    remember_free_history(&free_history_page(), &mut history);
    let route = safety::free_item_route("900001").unwrap();
    for code in [
        "AUTH_REQUIRED",
        "PERMISSION_DENIED",
        "RESPONSE_ROUTE_CHANGED",
        "SOURCE_HTTP_FAILED",
    ] {
        let page = unreadable_page("free", &route, code).unwrap();
        let evidence = page.free_item.as_ref().unwrap();
        assert_eq!(evidence.error_code.as_deref(), Some(code));
        assert!(!evidence.manifest_complete && !evidence.unavailable);
        assert!(evidence.files.is_empty() && page.rows.is_empty());
        assert_eq!(page.unresolved, 1);
        // The item stays incomplete; it never settles as an unavailable offer.
        assert!(!settled_free_absence(&page, &history));
        let journal = json!({"accountId":"authored","source":"free","runId":"current","page":page});
        assert!(checked_journal_page(&journal, "authored", "free", "current").is_ok());
    }
    // Rows the parser could not bind count as rows; withheld IDs are not gaps.
    let mut listing = free_history_page();
    listing.unresolved = 1;
    listing.rows[0].product_id = None;
    listing.rows[0].error_code = Some("PRODUCT_ID_UNRESOLVED".into());
    let mut withheld = listing.rows[0].clone();
    withheld.error_code = Some("BOOTH_WITHHELD_PRODUCT_ID".into());
    listing.rows.push(withheld);
    assert_eq!(unbound_row_count(&listing), 1);
}
#[test]
fn only_an_unclosed_listing_keeps_the_job_incomplete_and_left_assets_are_counted() {
    let scan = |settled, gaps_local| SourceScan {
        settled,
        gaps_local,
        ..SourceScan::default()
    };
    assert!(!scan(true, false).incomplete());
    assert!(!scan(false, true).incomplete());
    assert!(scan(false, false).incomplete());
    let mut job = json!({"progress":{"stage":"finalizing","incompleteProducts":9}});
    let left = BTreeSet::from(["900001".to_owned(), "900002".to_owned()]);
    record_incomplete(&mut job, &left, 1);
    assert_eq!(job["progress"]["incompleteProducts"], 3);
    record_incomplete(&mut job, &BTreeSet::new(), 0);
    assert!(job["progress"].get("incompleteProducts").is_none());
    assert_eq!(job["progress"]["stage"], "finalizing");
}
#[test]
fn selected_update_cannot_recreate_a_removed_accounts_grants() {
    let path = std::env::temp_dir().join(format!(
        "nna-booth-removed-account-{}",
        uuid::Uuid::new_v4()
    ));
    let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
    let service = BoothService::new(catalogue.clone()).unwrap();
    let account = "10000000-0000-4000-8000-000000000001";
    let job = json!({"id":"authored-update","type":"boothUpdate","accountId":account,"state":"running","args":{"accountId":account,"productIds":["900001"]}});
    catalogue
        .put("job", "authored-update", job.clone())
        .unwrap();
    let entry = ProductPublication {
        account: account.into(),
        source: "library".into(),
        product: "900001".into(),
        rows: vec![],
        routes: BTreeMap::new(),
        draft: json!({"id":"900001"}),
        files: BTreeSet::new(),
        checkpoint_key: "authored-source".into(),
        force_verify: true,
        needs_enrichment: false,
    };
    let mut checkpoints = BTreeMap::from([(
        "authored-source".to_owned(),
        Checkpoint::fresh(account, "library", "full").unwrap(),
    )]);
    assert_eq!(
        service
            .commit_product_group_mode(&job, "900001", &[entry], &mut checkpoints, true)
            .unwrap_err()
            .to_string(),
        "ACCOUNT_UNAVAILABLE"
    );
    for kind in ["product", "acquisition", "entitlement", "remote_file"] {
        assert!(catalogue.list(kind).unwrap().is_empty());
    }
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}

#[tokio::test]
async fn source_diagnostics_keep_checked_journal_reasons_and_changed_head() {
    let page = parser::Page {
        source: "purchase".into(),
        route: "/orders/910001".into(),
        rows: vec![parser::Row {
            source_record_id: "AUTHORED_SOURCE_MARKER".into(),
            product_id: Some("900001".into()),
            name: Some("AUTHORED_TITLE_MARKER".into()),
            error_code: Some("DIGITAL_CONTROLS_UNRESOLVED".into()),
            ..Default::default()
        }],
        next: vec![],
        details: vec![],
        terminal: false,
        unresolved: 1,
        fingerprint: "AUTHORED_CHANGED_HEAD".into(),
        is_detail: true,
        ordering: parser::Ordering::Unknown,
        free_item: None,
    };
    let mut checkpoint =
        Checkpoint::fresh("AUTHORED_ACCOUNT_MARKER", "purchase", "forced").unwrap();
    checkpoint.head = "AUTHORED_ORIGINAL_HEAD".into();
    let (_, trace) = crate::support_trace::capture(async {
        assert!(!trace_checked_journal(&page, false));
        // A resumed journal observation is counted again, not deduplicated
        // into an invented number of unique orders.
        assert!(!trace_checked_journal(&page, false));
        record_source_head(&mut checkpoint, &page);
        let mut missing_terminal = page.clone();
        missing_terminal.source = "library".into();
        missing_terminal.is_detail = false;
        missing_terminal.rows.clear();
        missing_terminal.unresolved = 0;
        assert!(trace_checked_journal(&missing_terminal, true));
        let free = free_item::Evidence::unknown("900002", "FREE_FILE_CONTROLS_UNAVAILABLE", false)
            .page()
            .unwrap();
        trace_checked_journal(&free, false);
    })
    .await;
    assert_eq!(checkpoint.unresolved, 1);
    assert_eq!(
        trace["sources"]["purchase"]["reasons"]["DIGITAL_CONTROLS_UNRESOLVED"],
        2
    );
    assert_eq!(
        trace["sources"]["purchase"]["reasons"]["SOURCE_HEAD_CHANGED"],
        1
    );
    assert_eq!(
        trace["sources"]["library"]["reasons"]["SOURCE_TERMINAL_UNPROVEN"],
        1
    );
    assert_eq!(
        trace["sources"]["library"]["reasons"]["SOURCE_MEMBERSHIP_UNBOUND"],
        1
    );
    assert_eq!(
        trace["sources"]["free"]["reasons"]["FREE_FILE_CONTROLS_UNAVAILABLE"],
        1
    );
    let text = crate::support::report(
        &json!({"type":"boothSync","supportDiagnostics":trace}),
        &[],
        &[],
    );
    assert!(text.contains("checkedJournalAndAssemblyObservationsThisAttempt"));
    for marker in ["AUTHORED_", "/orders/", "900001", "900002", "910001"] {
        assert!(!text.contains(marker));
    }
}
#[tokio::test]
async fn source_diagnostics_refresh_closure_without_recounting_incomplete_manifests() {
    let account = "AUTHORED_ACCOUNT_MARKER";
    let mut checkpoint = Checkpoint::fresh(account, "purchase", "forced").unwrap();
    checkpoint.unresolved = 3;
    checkpoint
        .enrichment_pending
        .extend(["900001".into(), "900002".into()]);
    let mut publications = Publications::default();
    publications
        .source_closures
        .insert((account.into(), "purchase".into()), (true, false));
    publications.incomplete_scopes.extend([
        (account.into(), "purchase".into(), "900001".into()),
        (account.into(), "purchase".into(), "900002".into()),
        (account.into(), "gift".into(), "900003".into()),
        (
            "AUTHORED_OTHER_ACCOUNT".into(),
            "purchase".into(),
            "900004".into(),
        ),
    ]);
    let before = serde_json::to_value(&checkpoint).unwrap();
    let (_, trace) = crate::support_trace::capture(async {
        trace_source_assembly(&checkpoint, &publications, false);
        assert_eq!(serde_json::to_value(&checkpoint).unwrap(), before);
        checkpoint.enrichment_pending.clear();
        trace_source_closure(&checkpoint, &publications);
    })
    .await;
    let source = &trace["sources"]["purchase"];
    assert_eq!(source["reasons"]["SOURCE_MANIFEST_INCOMPLETE"], 2);
    assert_eq!(source["reasons"]["SOURCE_TERMINAL_UNPROVEN"], 1);
    assert_eq!(
        source["lastClosure"],
        json!({
            "scope":"latestObservedSourceClosure", "terminalSeen":false,
            "pendingRequests":1,"unresolvedRows":3,"enrichmentPending":0,
            "incompleteProducts":2,"membershipUnbound":true,"headStable":false
        })
    );
    assert!(source.get("state").is_none());
    let text = serde_json::to_string(&trace).unwrap();
    for marker in ["AUTHORED_", "900001", "900002", "900003", "900004"] {
        assert!(!text.contains(marker));
    }
}
#[test]
fn verification_prefix_drains_details_without_certifying_source_end() {
    let mut checkpoint = Checkpoint::fresh("authored-account", "purchase", "forced").unwrap();
    checkpoint.pages = 4;
    checkpoint.pending = vec!["/orders?page=5".into(), "/orders/900001".into()];
    limit_listing_pages(&mut checkpoint, 5);
    assert_eq!(checkpoint.pending.len(), 2);
    checkpoint.pages = 5;
    checkpoint.pending[0] = "/orders?page=6".into();
    limit_listing_pages(&mut checkpoint, 5);
    assert_eq!(checkpoint.pending, vec!["/orders/900001"]);
    assert!(!checkpoint.terminal);
    assert_eq!(checkpoint.unresolved, 1);
    assert!(!collection_membership_complete(
        &checkpoint,
        &BTreeSet::new()
    ));

    let mut natural_end = Checkpoint::fresh("authored-account", "purchase", "forced").unwrap();
    natural_end.pages = 2;
    natural_end.terminal = true;
    natural_end.pending.clear();
    limit_listing_pages(&mut natural_end, 5);
    assert!(collection_membership_complete(
        &natural_end,
        &BTreeSet::new()
    ));
}
fn support_page_fixture(description: &str) -> String {
    format!("<html><head><link rel='canonical' href='https://booth.pm/ja/items/900001'><meta property='og:url' content='https://booth.pm/ja/items/900001'></head><body><div id='items' class='market' data-product-id='900001'><ul id='variations'><li class='variation-item'><div class='cart-button-wrap'><a title='authored.zip' href='https://booth.pm/downloadables/700001?variation_id=800001'><span class='cmd-label'>無料ダウンロード</span></a><div class='text-14 text-text-default font-normal text-left mb-8'>authored.zip</div></div></li></ul>{description}</div><p>PRIVATE_AUTH_BODY_SENTINEL</p></body></html>")
}
#[test]
fn free_support_page_reuse_is_product_scoped_and_keeps_only_public_fields() {
    let body = support_page_fixture("<section class='shop__text'><h2>対応アバター</h2><p>しなの https://ponderogen.booth.pm/items/6106863</p></section>");
    let evidence = free_item::parse(&body, "900001").unwrap();
    assert!(evidence.manifest_complete);
    let original_files = evidence.files.clone();
    let mut pages = SupportPages::default();
    pages.remember(&evidence, &body).unwrap();
    let mut product =
        json!({"id":"900001","note":"Human note","publicSupportAvatars":[{"id":"old"}]});
    assert!(!pages.apply("900002", &mut product));
    assert!(pages.apply("900001", &mut product));
    assert_eq!(product["publicSupportAvatars"][0]["id"], "booth:6106863");
    assert_eq!(
        product["publicSupportAvatars"][0]["entitlementEstablished"],
        false
    );
    assert_eq!(product["note"], "Human note");
    assert_eq!(evidence.files, original_files);
    assert_eq!(pages.reused, 1);
    assert!(!SupportPages::default().apply("900001", &mut product));
    let observation = &pages.observations["900001"];
    assert_eq!(observation.as_object().unwrap().len(), 2);
    let serialized = serde_json::to_string(observation).unwrap();
    for private in [
        "PRIVATE_AUTH_BODY_SENTINEL",
        "700001",
        "authored.zip",
        "<html",
        "entitlementId",
    ] {
        assert!(!serialized.contains(private), "{private}");
    }
}
#[test]
fn free_support_reuse_distinguishes_observed_empty_from_unproven_pages() {
    let empty = support_page_fixture(
        "<section class='shop__text'><p>No supported avatars listed</p></section>",
    );
    let mut pages = SupportPages::default();
    pages
        .remember(&free_item::parse(&empty, "900001").unwrap(), &empty)
        .unwrap();
    let mut product = json!({"id":"900001","publicSupportAvatars":[{"id":"prior"}]});
    assert!(pages.apply("900001", &mut product));
    assert_eq!(product["publicSupportAvatars"], json!([]));
    for body in [
        support_page_fixture(""),
        empty.replace("900001", "900002"),
        empty.replace("<ul id='variations'>", "<ul id='unknown'>"),
    ] {
        let evidence = free_item::parse(&body, "900001").unwrap();
        let mut pages = SupportPages::default();
        pages.remember(&evidence, &body).unwrap();
        let mut product = json!({"id":"900001","publicSupportAvatars":[{"id":"prior"}]});
        assert!(!pages.apply("900001", &mut product));
        assert_eq!(product["publicSupportAvatars"][0]["id"], "prior");
        if !evidence.manifest_complete {
            assert!(evidence.files.is_empty());
            assert_eq!(evidence.page().unwrap().unresolved, 1);
        }
    }
    for marker in [
        "<form action='/sign_in'><input type='password'></form>",
        "<div id='challenge-form'></div>",
    ] {
        assert!(free_item::parse(
            &empty.replace("<body>", &format!("<body>{marker}")),
            "900001"
        )
        .is_err());
    }
}
#[tokio::test]
async fn receipt_page_pipeline_publishes_before_next_listing_and_merges_later_variant() {
    let path = std::env::temp_dir().join(format!("booth-page-pipeline-{}", uuid::Uuid::new_v4()));
    let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
    let service = BoothService::new(catalogue.clone()).unwrap();
    catalogue.put("account", "authored-account", json!({"id":"authored-account","store":"BOOTH","state":"connected","identityVerified":true})).unwrap();
    let job = json!({"id":"authored-page-job","type":"boothSync","state":"running"});
    catalogue
        .put("job", "authored-page-job", job.clone())
        .unwrap();
    let mut checkpoint = Checkpoint::fresh("authored-account", "purchase", "full").unwrap();
    let (requests, _receiver) = mpsc::unbounded_channel();
    let mut early = EarlyPublications {
        remaining_sources: BTreeSet::from(["library".into(), "purchase".into()]),
        ..Default::default()
    };
    let mut public = metadata::public_metadata(
        &json!({
        "id":900001,"name":"Authored product","description":"Authored clothing description",
        "shop":{"name":"Authored shop","url":"https://invented.booth.pm"},
        "category":{"id":1,"name":"3D衣装"},
        "tags":[{"name":"VRChat"},{"name":"Authored source tag"},{"name":"Authored rejected tag"}],
        "images":[],"price":"¥ 50~","variations":[
            {"id":800001,"name":null,"type":"digital","price":700},
            {"id":800002,"name":"Second authored variant","type":"digital","price":900},
            {"id":800099,"name":"Unowned listing minimum","type":"digital","price":50}
        ]}),
        "900001",
    )
    .unwrap();
    public["publicMetadataState"] = json!("complete");
    public["publicMetadataObservedAt"] = json!("2026-10-12T00:00:00Z");
    early.observations.insert("900001".into(), Ok(public));
    let prefetch = PublicPrefetch {
        requests,
        enqueued: Default::default(),
        support_pages: Default::default(),
        early: std::sync::Arc::new(Mutex::new(early)),
    };
    let receipt = |order: &str, file: &str, variant: &str, amount: u64| {
        let dropdown = serde_json::to_string(&json!([
            {"deeplinkDownloadableUrl":format!("https://booth.pm/downloadables/{file}/deeplink?variation_id={variant}&client=booth-library-manager")},
            {"deeplinkDownloadableUrl":format!("https://booth.pm/downloadables/{file}/deeplink?variation_id={variant}&client=vroid&type=zip")}
        ])).unwrap();
        parser::parse(&format!("<main><div class='sheet' data-product-id='900001'><section><div><div><b><a href='https://invented.booth.pm/items/900001'>Authored product</a></b></div><div>¥ {amount}</div><div><i class='icon-boost'></i><span>0</span></div></div></section><div class='legacy-list-item'><div class='min-w-0'><b>authored-{file}.zip</b></div><a href='https://booth.pm/downloadables/{file}'>Download</a><button data-test='other-downloads-button' data-dropdown-items='{dropdown}'></button></div></div><div class='sheet'><div><span>注文番号</span><span>{order}</span></div><div><span>注文日時</span><span>2026/10/10 09:30</span></div></div></main>"),"purchase",&format!("/orders/{order}")).unwrap()
    };
    let first_listing = parser::parse("<main><h1>Orders</h1><a href='/orders/910001'><div class='sheet'>Authored order</div></a><nav class='pagination'><a aria-current='page' href='/orders?page=1'>1</a><a href='/orders?page=2'>2</a></nav></main>","purchase","/orders?page=1").unwrap();
    let consume = |page: &parser::Page, checkpoint: &mut Checkpoint| {
        record_source_page(checkpoint, "purchase", &page.route, page, None).unwrap();
        for row in &page.rows {
            if let Some(id) = &row.product_id {
                checkpoint.product_ids.insert(id.clone());
                checkpoint.enrichment_pending.insert(id.clone());
            }
        }
        let id = format!("authored-journal:{}", safety::hash(&page.route));
        checkpoint.journal_ids.push(id.clone());
        let journal = json!({"id":id,"accountId":"authored-account","source":"purchase","runId":checkpoint.run_id,"page":page});
        service
            .commit_source_page(&id, &journal, "authored-checkpoint", checkpoint)
            .unwrap();
        service.stage_discovered_page(
            &job,
            "authored-account",
            "purchase",
            "full",
            "authored-checkpoint",
            checkpoint,
            page,
            &prefetch,
        )
    };
    assert!(consume(&first_listing, &mut checkpoint).unwrap().is_empty());
    let routes = available_routes(&checkpoint.pending, &BTreeSet::new()).unwrap();
    assert_eq!(routes, vec!["/orders?page=2", "/orders/910001"]);
    let (release_next, wait_next) = tokio::sync::oneshot::channel::<()>();
    let mut wait_next = Some(wait_next);
    let mut reads = FuturesUnordered::new();
    for route in routes {
        let blocked = route.contains('?').then(|| wait_next.take().unwrap());
        let page = receipt("910001", "700001", "800001", 100);
        reads.push(async move {
            if let Some(blocked) = blocked {
                blocked.await.unwrap();
                None
            } else {
                Some(page)
            }
        });
    }
    let page = reads.next().await.unwrap().unwrap();
    assert_eq!(consume(&page, &mut checkpoint).unwrap(), vec!["900001"]);
    assert_eq!((checkpoint.pages, checkpoint.details_completed), (1, 1));
    assert!(checkpoint.pending.contains(&"/orders?page=2".into()));
    assert!(!checkpoint.complete());
    {
        let early = prefetch.early.lock().unwrap();
        let aggregate = publication_progress(&early.publications, "finalizing", 1, 1);
        assert_eq!(aggregate["pages"], 1);
        assert_eq!(aggregate["detailsCompleted"], 1);
        assert_eq!(aggregate["requestsCompleted"], 2);
        assert_eq!(aggregate["products"], 1);
        assert_eq!(aggregate["metadataCompleted"], 1);
        assert_eq!(aggregate["published"], 1);
    }
    assert!(!prefetch.early.lock().unwrap().remaining_sources.is_empty());
    assert_eq!(catalogue.related("remote_file", "900001").unwrap().len(), 1);
    let publication_draft = format!("booth-product:{}:900001", checkpoint.run_id);
    assert!(catalogue
        .get("checkpoint", &publication_draft)
        .unwrap()
        .is_none_or(|draft| draft["published"] != true));
    assert_eq!(
        catalogue.related("acquisition", "900001").unwrap()[0]["paidPrice"],
        100
    );
    let source_details = catalogue
        .command("product_details", json!({"id":"900001"}))
        .unwrap();
    assert!(source_details["tags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tag| tag == "Authored source tag"));
    catalogue
        .command(
            "overlay",
            json!({"ids":["900001"],"patch":{
                "title":"Human product name","note":"Human note","category":"Hair",
                "tags":["Authored manual tag"],"rejectedTags":["Authored rejected tag"]
            }}),
        )
        .unwrap();
    let first_details = catalogue
        .command("product_details", json!({"id":"900001"}))
        .unwrap();
    assert_eq!(first_details["price"], 700.0);
    assert_eq!(
        first_details["ownedCurrentPrices"][0]["variantId"],
        "800001"
    );
    assert_eq!(first_details["ownedPaidPrices"][0]["price"], 100.0);
    let mut first_file = catalogue
        .related("remote_file", "900001")
        .unwrap()
        .remove(0);
    first_file["revision"] = json!("verified-A");
    first_file["revisionBasis"] = json!("contentHash");
    first_file["requiresContentVerification"] = json!(false);
    catalogue
        .put(
            "remote_file",
            first_file["id"].as_str().unwrap(),
            first_file.clone(),
        )
        .unwrap();
    catalogue.put("artifact", "authored-A", json!({"id":"authored-A","productId":"900001","fileId":first_file["id"],"revision":first_file["revision"],"status":"ready","hash":"authored-hash-A","relativePath":"authored-A.zip","originalAvailable":true,"extractedPath":"authored-A","extractedInventory":[{"path":"Authored-A.unitypackage","size":10,"hash":"authored-member-A"}]})).unwrap();
    let details = || {
        catalogue
            .command("product_details", json!({"id":"900001"}))
            .unwrap()
    };
    assert_eq!(details()["files"][0]["status"], "ready");
    assert_eq!(details()["manifestComplete"], false);
    assert_eq!(details()["status"], "ready");
    assert_ne!(details()["status"], "attention");
    release_next.send(()).unwrap();
    assert!(reads.next().await.unwrap().is_none());
    let second = receipt("910002", "700002", "800002", 200);
    assert_eq!(consume(&second, &mut checkpoint).unwrap(), vec!["900001"]);
    let replayed_first = catalogue
        .get("remote_file", first_file["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(replayed_first["revision"], "verified-A");
    assert_eq!(replayed_first["requiresContentVerification"], false);
    assert_eq!(
        replayed_first["sourceObservationToken"],
        first_file["sourceObservationToken"]
    );
    let entitlement = catalogue
        .related("entitlement", "900001")
        .unwrap()
        .remove(0);
    assert_eq!(entitlement["fileIds"].as_array().unwrap().len(), 2);
    assert_eq!(entitlement["variantIds"].as_array().unwrap().len(), 2);
    assert_eq!(details()["status"], "partial");
    let paid: u64 = catalogue
        .related("acquisition", "900001")
        .unwrap()
        .iter()
        .filter_map(|row| row["paidPrice"].as_u64())
        .sum();
    assert_eq!(paid, 300);
    let assert_facts = |catalogue: &crate::catalogue::Catalogue| {
        let value = catalogue
            .command("product_details", json!({"id":"900001"}))
            .unwrap();
        assert!(value["price"].is_null()); // No listing minimum or multi-variant allocation.
        let current = value["ownedCurrentPrices"].as_array().unwrap();
        assert_eq!(current.len(), 2);
        for (variant, amount, paid) in [("800001", 700.0, 100.0), ("800002", 900.0, 200.0)] {
            let quote = current.iter().find(|v| v["variantId"] == variant).unwrap();
            assert_eq!(quote["price"], amount);
            assert_eq!(quote["currency"], "JPY");
            assert_eq!(quote["accountIds"], json!(["authored-account"]));
            let payment = value["ownedPaidPrices"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["variantId"] == variant)
                .unwrap();
            assert_eq!(payment["price"], paid);
            assert_eq!(payment["accountId"], "authored-account");
        }
        assert!(value["paidPrice"].is_null()); // Distinct actual charges remain separate.
        assert_eq!(value["accountIds"], json!(["authored-account"]));
        assert_eq!(
            value["sourceTags"],
            json!(["VRChat", "Authored source tag", "Authored rejected tag"])
        );
        assert!(value["tags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tag| tag == "Authored manual tag"));
        assert!(!value["tags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tag| tag == "Authored rejected tag"));
        let rejected: BTreeSet<_> = value["rejectedTags"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(
            rejected,
            BTreeSet::from(["Authored rejected tag", "Authored source tag", "VRChat"])
        );
        assert_eq!(value["title"], "Human product name");
        assert_eq!(value["sourceTitle"], "Authored product");
        assert_eq!(value["note"], "Human note");
        assert_eq!(value["category"], "Hair");
        let acquisitions = catalogue.related("acquisition", "900001").unwrap();
        assert_eq!(acquisitions.len(), 2);
        assert!(acquisitions
            .iter()
            .all(|row| row["accountId"] == "authored-account"
                && row["paidPriceBasis"] == "productLineTotal"
                && row["acquiredDateText"] == "2026/10/10 09:30"));
    };
    assert_facts(&catalogue);
    assert_eq!(details()["manifestComplete"], false);
    assert_ne!(details()["status"], "ready");
    // Verified closure certifies the same cumulative facts atomically.
    let entries = prefetch.early.lock().unwrap().publications.products["900001"].clone();
    let mut closed_checkpoints = prefetch
        .early
        .lock()
        .unwrap()
        .publications
        .checkpoints
        .clone();
    service
        .commit_product_group(&job, "900001", &entries, &mut closed_checkpoints)
        .unwrap();
    assert_eq!(details()["manifestComplete"], true);
    assert_ne!(details()["status"], "ready");
    assert_facts(&catalogue);
    let second_file = catalogue
        .related("remote_file", "900001")
        .unwrap()
        .into_iter()
        .find(|row| row["id"] != first_file["id"])
        .unwrap();
    assert_eq!(second_file["requiresContentVerification"], true);
    let mut second_file = second_file;
    second_file["revision"] = json!("verified-B");
    second_file["revisionBasis"] = json!("etag");
    second_file["requiresContentVerification"] = json!(false);
    catalogue
        .put(
            "remote_file",
            second_file["id"].as_str().unwrap(),
            second_file.clone(),
        )
        .unwrap();
    catalogue.put("artifact", "authored-B", json!({"id":"authored-B","productId":"900001","fileId":second_file["id"],"revision":second_file["revision"],"status":"ready","hash":"authored-hash-B","relativePath":"authored-B.zip"})).unwrap();
    service
        .commit_product_group(&job, "900001", &entries, &mut closed_checkpoints)
        .unwrap();
    assert_eq!(details()["manifestComplete"], true);
    assert_eq!(details()["status"], "ready");
    assert_facts(&catalogue);
    // A different full refresh must verify even these unchanged descriptors.
    let next_job = json!({"id":"authored-next-job","type":"boothSync","state":"running"});
    catalogue
        .put("job", "authored-next-job", next_job.clone())
        .unwrap();
    let mut next_checkpoints = BTreeMap::from([(
        entries[0].checkpoint_key.clone(),
        Checkpoint::fresh("authored-account", "purchase", "full").unwrap(),
    )]);
    service
        .commit_product_group_mode(&next_job, "900001", &entries, &mut next_checkpoints, false)
        .unwrap();
    let next_first = catalogue
        .get("remote_file", first_file["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(next_first["requiresContentVerification"], true);
    assert_ne!(
        next_first["sourceObservationToken"],
        first_file["sourceObservationToken"]
    );
    assert_ne!(details()["status"], "ready");
    service
        .commit_product_group_mode(&next_job, "900001", &entries, &mut next_checkpoints, false)
        .unwrap();
    assert_eq!(
        catalogue
            .get("remote_file", first_file["id"].as_str().unwrap())
            .unwrap()
            .unwrap()["requiresContentVerification"],
        true
    );
    // Positive verification during that job cannot mask changed source facts.
    let mut verified = next_first;
    verified["requiresContentVerification"] = json!(false);
    catalogue
        .put("remote_file", first_file["id"].as_str().unwrap(), verified)
        .unwrap();
    let mut changed = entries.clone();
    changed[0].rows[0].files[0].size_bytes = Some(99);
    service
        .commit_product_group_mode(&next_job, "900001", &changed, &mut next_checkpoints, false)
        .unwrap();
    let changed_first = catalogue
        .get("remote_file", first_file["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(changed_first["requiresContentVerification"], true);
    assert_eq!(changed_first["sizeBytes"], 99);
    assert_facts(&catalogue);
    // A genuinely changed descriptor requires a new positive verification.
    // These authored artifact records stand for storage's verified extraction;
    // this source test does not perform archive/filesystem verification.
    for (label, original) in [("A2", &first_file), ("B2", &second_file)] {
        let mut remote = catalogue
            .get("remote_file", original["id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        remote["revision"] = json!(format!("verified-{label}"));
        remote["revisionBasis"] = json!("contentHash");
        remote["requiresContentVerification"] = json!(false);
        let artifact_id = format!("authored-{label}");
        catalogue
            .put(
                "remote_file",
                remote["id"].as_str().unwrap(),
                remote.clone(),
            )
            .unwrap();
        catalogue.put("artifact", &artifact_id, json!({
            "id":artifact_id,"productId":"900001","fileId":remote["id"],
            "revision":remote["revision"],"status":"ready","hash":format!("authored-hash-{label}"),
            "relativePath":format!("authored-{label}.zip"),"originalAvailable":true,
            "extractedPath":format!("authored-{label}"),
            "extractedInventory":[{"path":format!("Authored-{label}.unitypackage"),"size":10,"hash":format!("authored-member-{label}")}]
        })).unwrap();
    }
    service
        .commit_product_group(&next_job, "900001", &changed, &mut next_checkpoints)
        .unwrap();
    assert_eq!(details()["status"], "ready");
    assert_facts(&catalogue);
    // A sparse incremental observation keeps previously proven financial
    // facts, public quotes/tags and unchanged verified local revisions.
    let incremental_job =
        json!({"id":"authored-incremental-job","type":"boothSync","state":"running"});
    catalogue
        .put("job", "authored-incremental-job", incremental_job.clone())
        .unwrap();
    let mut incremental_entries = changed.clone();
    for entry in &mut incremental_entries {
        entry.force_verify = false;
        entry.draft = json!({"id":"900001","publicMetadataState":"complete"});
        for row in &mut entry.rows {
            row.product_line_total_jpy = None;
            row.paid_amount_text = None;
            row.acquired_date_text = None;
        }
    }
    let mut incremental_checkpoints = BTreeMap::from([(
        entries[0].checkpoint_key.clone(),
        Checkpoint::fresh("authored-account", "purchase", "incremental").unwrap(),
    )]);
    service
        .commit_product_group_mode(
            &incremental_job,
            "900001",
            &incremental_entries,
            &mut incremental_checkpoints,
            false,
        )
        .unwrap();
    assert_eq!(details()["status"], "ready");
    service
        .commit_product_group(
            &incremental_job,
            "900001",
            &incremental_entries,
            &mut incremental_checkpoints,
        )
        .unwrap();
    assert_eq!(details()["manifestComplete"], true);
    assert_eq!(details()["status"], "ready");
    assert_facts(&catalogue);
    let retained_artifact = catalogue.get("artifact", "authored-A2").unwrap().unwrap();
    catalogue
        .put(
            "job",
            "authored-page-job",
            json!({"id":"authored-page-job","state":"cancelled"}),
        )
        .unwrap();
    let third = receipt("910003", "700003", "800003", 300);
    assert_eq!(
        consume(&third, &mut checkpoint).unwrap_err().to_string(),
        "CANCELLED"
    );
    assert_eq!(catalogue.related("remote_file", "900001").unwrap().len(), 2);
    drop(service);
    drop(catalogue);
    let reopened = crate::catalogue::Catalogue::open(&path).unwrap();
    assert_facts(&reopened);
    let reopened_details = reopened
        .command("product_details", json!({"id":"900001"}))
        .unwrap();
    assert_eq!(reopened_details["manifestComplete"], true);
    assert_eq!(reopened_details["status"], "ready");
    assert_eq!(
        reopened.get("artifact", "authored-A2").unwrap().unwrap(),
        retained_artifact
    );
    drop(reopened);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn authored_complete_product_publishes_before_unrelated_drafts_and_public_details() {
    let root = std::env::var("NNA_STORE_EVIDENCE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    let path = root.join(format!("booth-early-{}", uuid::Uuid::new_v4()));
    let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
    let service = BoothService::new(catalogue.clone()).unwrap();
    let job = json!({"id":"authored-early-job","type":"boothSync","state":"running","progress":{}});
    catalogue
        .put("job", "authored-early-job", job.clone())
        .unwrap();
    let entry = |product: &str, source: &str| {
        let row = parser::Row {
            source_record_id: format!("{source}:{product}"),
            product_id: Some(product.into()),
            order_id: (source == "purchase").then(|| "910001".into()),
            name: Some(format!("Authored {product}")),
            manifest_complete: true,
            product_line_total_jpy: (source == "purchase").then_some(100),
            files: vec![parser::File {
                id: format!("7{product}"),
                variant_id: Some("800001".into()),
                name: Some("authored.zip".into()),
                size_bytes: Some(10),
                source_url: format!(
                    "https://booth.pm/downloadables/7{product}?variation_id=800001"
                ),
                revision: "authored".into(),
                format: None,
            }],
            ..Default::default()
        };
        ProductPublication {
            account: "authored-account".into(),
            source: source.into(),
            product: product.into(),
            rows: vec![row.clone()],
            routes: BTreeMap::from([(
                row.source_record_id,
                if source == "purchase" {
                    "/orders/910001".into()
                } else {
                    "/library?page=1".into()
                },
            )]),
            draft: json!({"id":product,"store":"BOOTH","title":format!("Authored {product}")}),
            files: BTreeSet::from([format!("authored-account:7{product}:800001")]),
            checkpoint_key: source.into(),
            force_verify: true,
            needs_enrichment: true,
        }
    };
    let mut early = EarlyPublications {
        remaining_sources: BTreeSet::from(["purchase".into()]),
        ..Default::default()
    };
    for source in ["library", "purchase"] {
        early.publications.checkpoints.insert(
            source.into(),
            Checkpoint::fresh("authored-account", source, "full").unwrap(),
        );
    }
    early
        .publications
        .products
        .insert("900001".into(), vec![entry("900001", "library")]);
    early.observations.insert(
        "900001".into(),
        Ok(json!({"publicMetadataState":"complete","sourceTitle":"Authored complete"})),
    );
    assert!(service
        .publish_early_records(&job, &mut early, None)
        .unwrap()
        .is_empty());
    assert!(service
        .publish_early_records(&job, &mut early, None)
        .unwrap()
        .is_empty()); // receipt grant still absent, discovery remains open
    let began = std::time::Instant::now();
    early
        .publications
        .products
        .get_mut("900001")
        .unwrap()
        .push(entry("900001", "purchase"));
    let first = service
        .publish_early_records(&job, &mut early, None)
        .unwrap();
    assert_eq!(first, vec!["900001"]);
    let first_ms = began.elapsed().as_millis();
    assert!(catalogue.get("product", "900001").unwrap().is_some());
    assert_eq!(catalogue.related("entitlement", "900001").unwrap().len(), 2);
    assert_eq!(catalogue.related("remote_file", "900001").unwrap().len(), 1);
    assert!(catalogue
        .related("acquisition", "900001")
        .unwrap()
        .iter()
        .any(|row| row["paidPrice"] == 100));
    assert!(catalogue.get("product", "900002").unwrap().is_none());
    // Unrelated product source preparation continues after the first atomic commit.
    early.publications.products.insert(
        "900002".into(),
        vec![entry("900002", "library"), entry("900002", "purchase")],
    );
    early
        .observations
        .insert("900002".into(), Err("PUBLIC_METADATA_FAILED".into()));
    assert_eq!(
        service
            .publish_early_records(&job, &mut early, None)
            .unwrap(),
        vec!["900002"]
    ); // complete observed grants do not wait for public metadata
    assert_ne!(
        catalogue.get("product", "900002").unwrap().unwrap()["publicMetadataState"],
        "complete"
    );
    assert_eq!(
        catalogue
            .get("entitlement", "authored-account:purchase:900002")
            .unwrap()
            .unwrap()["manifestComplete"],
        false
    );
    assert!(early.publications.products["900002"]
        .iter()
        .all(|entry| entry.needs_enrichment));
    let mut closed = Publications {
        checkpoints: early.publications.checkpoints.clone(),
        products: BTreeMap::from([(
            "900002".into(),
            early.publications.products["900002"].clone(),
        )]),
        ..Default::default()
    };
    for checkpoint in closed.checkpoints.values_mut() {
        checkpoint.pending.clear();
        checkpoint.terminal = true;
    }
    let (certified, additive) = ready_product_entries(&closed, "900002");
    assert!(certified.is_empty());
    assert_eq!(additive.len(), 2); // source closure cannot stand in for missing metadata
    for checkpoint in closed.checkpoints.values() {
        let marker = format!("booth-product:{}:900002", checkpoint.run_id);
        assert!(catalogue.get("checkpoint", &marker).unwrap().is_none());
    }
    let mut incomplete = entry("900003", "purchase");
    incomplete.rows[0].manifest_complete = false;
    early
        .publications
        .products
        .insert("900003".into(), vec![incomplete]);
    assert!(service
        .publish_early_records(&job, &mut early, None)
        .unwrap()
        .is_empty());
    assert!(catalogue.get("product", "900003").unwrap().is_none());
    early.observations.insert(
        "900002".into(),
        Ok(json!({"publicMetadataState":"complete"})),
    );
    assert_eq!(
        service
            .publish_early_records(&job, &mut early, None)
            .unwrap(),
        vec!["900002"]
    );
    assert!(service
        .publish_early_records(&job, &mut early, None)
        .unwrap()
        .is_empty()); // no duplicate commit
    if let Ok(root) = std::env::var("NNA_STORE_EVIDENCE_DIR") {
        std::fs::write(std::path::Path::new(&root).join("booth-publication-timing.json"),serde_json::to_vec_pretty(&json!({"firstPublishMs":first_ms,"secondPublishMs":began.elapsed().as_millis(),"firstPublicationProducts":1,"firstPublicationEntitlements":2,"unrelatedSourceDraftsPendingAtFirstPublish":true,"unrelatedPublicDetailsPendingAtFirstPublish":true,"identityEnumerationClosedBeforeFirstPublish":false,"realStoreTiming":false})).unwrap()).unwrap();
    }
    drop(service);
    drop(catalogue);
}
#[test]
fn single_candidate_publication_does_not_revisit_unrelated_observations() {
    let path = std::env::temp_dir().join(format!("nna-booth-scoped-{}", uuid::Uuid::new_v4()));
    let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
    let service = BoothService::new(catalogue.clone()).unwrap();
    let job =
        json!({"id":"authored-scoped-job","type":"boothSync","state":"running","progress":{}});
    catalogue
        .put("job", "authored-scoped-job", job.clone())
        .unwrap();
    let mut early = EarlyPublications::default();
    early.publications.checkpoints.insert(
        "purchase".into(),
        Checkpoint::fresh("authored-account", "purchase", "full").unwrap(),
    );
    for product in ["900001", "900002"] {
        let row = parser::Row {
            source_record_id: format!("purchase:{product}"),
            product_id: Some(product.into()),
            order_id: Some("910001".into()),
            name: Some("Authored product".into()),
            manifest_complete: true,
            product_line_total_jpy: Some(100),
            ..Default::default()
        };
        early.publications.products.insert(
            product.into(),
            vec![ProductPublication {
                account: "authored-account".into(),
                source: "purchase".into(),
                product: product.into(),
                routes: BTreeMap::from([(row.source_record_id.clone(), "/orders/910001".into())]),
                rows: vec![row],
                draft: json!({"id":product,"store":"BOOTH","title":"Authored product"}),
                files: BTreeSet::new(),
                checkpoint_key: "purchase".into(),
                force_verify: true,
                needs_enrichment: true,
            }],
        );
        early.observations.insert(
            product.into(),
            Ok(json!({"publicMetadataState":"complete"})),
        );
    }
    let selected = BTreeSet::from(["900001".into()]);
    assert_eq!(
        service
            .publish_early_records(&job, &mut early, Some(&selected))
            .unwrap(),
        vec!["900001"]
    );
    assert!(early.publications.products["900002"][0].needs_enrichment);
    assert!(!early.publications.additive_versions.contains_key("900002"));
    assert!(catalogue.get("product", "900002").unwrap().is_none());
    // An unrelated completed observation is not reconsidered on this callback.
    early
        .observations
        .insert("900002".into(), Err("PUBLIC_METADATA_FAILED".into()));
    assert!(service
        .publish_early_records(&job, &mut early, Some(&selected))
        .unwrap()
        .is_empty());
    assert!(early.publications.incomplete_scopes.is_empty());
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn product_draft_restart_and_cancel_preserve_published_snapshot_until_atomic_publish() {
    let path = std::env::temp_dir().join(format!("nna-booth-draft-{}", uuid::Uuid::new_v4()));
    let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
    let service = BoothService::new(catalogue.clone()).unwrap();
    let account = "synthetic-account";
    let job = json!({"id":"synthetic-job","type":"boothSync","accountId":account,"state":"running","progress":{}});
    catalogue.put("job", "synthetic-job", job.clone()).unwrap();
    let prior = json!({"id":"900001","store":"BOOTH","title":"Previous published","price":50,"sourceTags":["previous"]});
    let old_entitlement = json!({"id":"synthetic-account:purchase:900001","productId":"900001","accountId":account,"source":"purchase","fileIds":["old-file"],"manifestComplete":true});
    catalogue.put("product", "900001", prior.clone()).unwrap();
    catalogue
        .put(
            "entitlement",
            old_entitlement["id"].as_str().unwrap(),
            old_entitlement.clone(),
        )
        .unwrap();
    catalogue
        .put(
            "overlay",
            "900001",
            json!({"id":"900001","title":"Manual title","note":"Manual note"}),
        )
        .unwrap();
    let row = parser::Row {
        source_record_id: "purchase:synthetic-variant".into(),
        product_id: Some("900001".into()),
        order_id: Some("910001".into()),
        name: Some("New source title".into()),
        manifest_complete: true,
        product_line_total_jpy: Some(0),
        acquired_date_text: Some("2026/10/11 09:30".into()),
        files: vec![parser::File {
            id: "700001".into(),
            variant_id: Some("800001".into()),
            name: Some("authored.zip".into()),
            size_bytes: Some(10),
            source_url: "https://booth.pm/downloadables/700001?variation_id=800001".into(),
            revision: "authored".into(),
            format: None,
        }],
        ..Default::default()
    };
    let mut checkpoint = Checkpoint::fresh(account, "purchase", "full").unwrap();
    checkpoint.pending.clear();
    checkpoint.terminal = true;
    checkpoint.head = "authored-head".into();
    checkpoint.product_ids.insert("900001".into());
    checkpoint.enrichment_pending.insert("900001".into());
    let key = "booth:synthetic-account:purchase";
    service
        .commit_source_page(
            "synthetic-page",
            &json!({"id":"synthetic-page","rows":[row]}),
            key,
            &checkpoint,
        )
        .unwrap();
    let collected_checkpoint = catalogue.get("checkpoint", key).unwrap().unwrap();
    let publication_draft = format!("booth-product:{}:900001", checkpoint.run_id);
    let draft = json!({"id":"900001","store":"BOOTH","title":"Enriched source","price":100,"sourceTags":["complete"],"publicMetadataState":"complete"});
    catalogue
        .put(
            "checkpoint",
            "synthetic-draft",
            json!({"id":"synthetic-draft","product":draft,"enriched":true}),
        )
        .unwrap();
    assert_eq!(
        catalogue.get("product", "900001").unwrap(),
        Some(prior.clone())
    );
    assert_eq!(
        catalogue
            .get("entitlement", old_entitlement["id"].as_str().unwrap())
            .unwrap(),
        Some(old_entitlement.clone())
    );
    assert!(catalogue.list("acquisition").unwrap().is_empty());
    assert!(catalogue.list("remote_file").unwrap().is_empty());
    drop(service);
    drop(catalogue);
    let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
    let service = BoothService::new(catalogue.clone()).unwrap();
    checkpoint =
        serde_json::from_value(catalogue.get("checkpoint", key).unwrap().unwrap()).unwrap();
    assert!(checkpoint.published_products.is_empty());
    let routes = BTreeMap::from([(row.source_record_id.clone(), "/orders/910001".into())]);
    let files = BTreeSet::from(["synthetic-account:700001:800001".into()]);
    let mut cancelled = job.clone();
    cancelled["state"] = json!("cancelled");
    catalogue.put("job", "synthetic-job", cancelled).unwrap();
    assert_eq!(
        service
            .publish_product(
                &job,
                account,
                "purchase",
                "900001",
                &[&row],
                &routes,
                &draft,
                &files,
                key,
                &mut checkpoint,
                true
            )
            .unwrap_err()
            .to_string(),
        "CANCELLED"
    );
    assert_eq!(
        catalogue.get("product", "900001").unwrap(),
        Some(prior.clone())
    );
    assert_eq!(
        catalogue
            .get("entitlement", old_entitlement["id"].as_str().unwrap())
            .unwrap(),
        Some(old_entitlement.clone())
    );
    assert!(catalogue
        .get("checkpoint", &publication_draft)
        .unwrap()
        .is_none());
    catalogue.put("job", "synthetic-job", job.clone()).unwrap();
    let mut library_row = row.clone();
    library_row.source_record_id = "library:synthetic-variant".into();
    library_row.order_id = None;
    library_row.product_line_total_jpy = None;
    let library_key = "booth:synthetic-account:library";
    let mut library_checkpoint = Checkpoint::fresh(account, "library", "full").unwrap();
    library_checkpoint.pending.clear();
    library_checkpoint.terminal = true;
    library_checkpoint.head = "authored-library-head".into();
    library_checkpoint.product_ids.insert("900001".into());
    library_checkpoint
        .enrichment_pending
        .insert("900001".into());
    let mut checkpoints = BTreeMap::from([
        (key.to_owned(), checkpoint.clone()),
        (library_key.to_owned(), library_checkpoint),
    ]);
    let entries = vec![
        ProductPublication {
            account: account.into(),
            source: "library".into(),
            product: "900001".into(),
            rows: vec![library_row.clone()],
            routes: BTreeMap::from([(
                library_row.source_record_id.clone(),
                "/library?page=1".into(),
            )]),
            draft: draft.clone(),
            files: files.clone(),
            checkpoint_key: library_key.into(),
            force_verify: true,
            needs_enrichment: false,
        },
        ProductPublication {
            account: account.into(),
            source: "purchase".into(),
            product: "900001".into(),
            rows: vec![row.clone()],
            routes: routes.clone(),
            draft: draft.clone(),
            files: files.clone(),
            checkpoint_key: key.into(),
            force_verify: true,
            needs_enrichment: false,
        },
    ];
    let free_gap = BTreeSet::from([(account.to_owned(), "free".to_owned())]);
    assert!(!publication_membership_proven(&entries, &free_gap));
    let mut endpoint_entries = entries;
    endpoint_entries.push(ProductPublication {
        account: account.into(),
        source: "free".into(),
        product: "900001".into(),
        rows: vec![],
        routes: BTreeMap::new(),
        draft: draft.clone(),
        files: files.clone(),
        checkpoint_key: "authored-endpoint".into(),
        force_verify: false,
        needs_enrichment: false,
    });
    assert!(publication_membership_proven(&endpoint_entries, &free_gap));
    assert!(!publication_membership_proven(
        &endpoint_entries,
        &BTreeSet::from([(account.to_owned(), "purchase".to_owned())])
    ));
    assert!(publication_membership_proven(
        &endpoint_entries,
        &BTreeSet::from([("different-account".into(), "purchase".into())])
    ));
    endpoint_entries.pop();
    let mut entries = endpoint_entries;
    entries[0].needs_enrichment = true;
    assert_eq!(
        service
            .commit_product_group(&job, "900001", &entries, &mut checkpoints)
            .unwrap_err()
            .to_string(),
        "PUBLIC_METADATA_FAILED"
    );
    assert_eq!(catalogue.get("product", "900001").unwrap().unwrap(), prior);
    assert_eq!(
        catalogue
            .get("entitlement", old_entitlement["id"].as_str().unwrap())
            .unwrap()
            .unwrap(),
        old_entitlement
    );
    entries[0].needs_enrichment = false;
    service
        .commit_product_group(&job, "900001", &entries, &mut checkpoints)
        .unwrap();
    checkpoint = checkpoints.remove(key).unwrap();
    assert_eq!(catalogue.list("acquisition").unwrap().len(), 2);
    assert!(catalogue
        .list("entitlement")
        .unwrap()
        .iter()
        .all(|value| value["manifestComplete"] == true));
    assert_eq!(
        catalogue.get("product", "900001").unwrap().unwrap()["sourceTags"],
        json!(["complete"])
    );
    let entitlement = catalogue
        .get("entitlement", "synthetic-account:purchase:900001")
        .unwrap()
        .unwrap();
    assert_eq!(entitlement["manifestComplete"], true);
    assert_eq!(
        entitlement["fileIds"],
        json!(["synthetic-account:700001:800001"])
    );
    let acquired = catalogue
        .list("acquisition")
        .unwrap()
        .into_iter()
        .find(|value| value["source"] == "purchase")
        .unwrap();
    assert_eq!(acquired["variantIds"], json!(["800001"]));
    assert_eq!(acquired["variantPayments"][0]["price"], 0);
    assert_eq!(catalogue.list("remote_file").unwrap().len(), 1);
    assert_eq!(
        catalogue.get("overlay", "900001").unwrap().unwrap()["note"],
        "Manual note"
    );
    assert!(checkpoint.published_products.contains("900001"));
    assert!(checkpoint.enrichment_pending.is_empty());
    // Publication no longer rewrites the large source checkpoint per item.
    assert_eq!(
        catalogue.get("checkpoint", key).unwrap().unwrap(),
        collected_checkpoint
    );
    let marker = catalogue
        .get("checkpoint", &publication_draft)
        .unwrap()
        .unwrap();
    assert_eq!(marker["published"], true);
    assert_eq!(
        marker["publishedManifest"],
        publication_manifest(account, "purchase", "900001", [&row], &routes, &files).unwrap()
    );
    // A later metadata callback keeps the certified marker, with its read
    // participating in the same expected-value transaction.
    let mut later_draft = marker.clone();
    later_draft.as_object_mut().unwrap().remove("published");
    later_draft
        .as_object_mut()
        .unwrap()
        .remove("publishedManifest");
    later_draft["product"]["title"] = json!("Later metadata draft");
    service
        .save_product_draft(&publication_draft, later_draft)
        .unwrap();
    let marker = catalogue
        .get("checkpoint", &publication_draft)
        .unwrap()
        .unwrap();
    assert_eq!(marker["published"], true);
    assert_eq!(marker["product"]["title"], "Later metadata draft");
    drop(service);
    drop(catalogue);
    let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
    let service = BoothService::new(catalogue.clone()).unwrap();
    let mut resumed: Checkpoint =
        serde_json::from_value(catalogue.get("checkpoint", key).unwrap().unwrap()).unwrap();
    assert!(resumed.published_products.is_empty());
    assert!(resumed.enrichment_pending.contains("900001"));
    let restore = |checkpoint: &mut Checkpoint, saved: &Value, complete| {
        service
            .restore_product_publication(
                "900001",
                &[&row],
                &routes,
                &files,
                complete,
                Some(saved),
                checkpoint,
            )
            .unwrap()
    };
    assert!(!restore(&mut resumed, &marker, false));
    for key in [
        "accountId",
        "source",
        "runId",
        "productId",
        "publishedManifest",
    ] {
        let mut other = marker.clone();
        other[key] = json!("other");
        assert!(!restore(&mut resumed, &other, true), "{key}");
    }
    let mut prepared_only = marker.clone();
    prepared_only["published"] = json!(false);
    assert!(!restore(&mut resumed, &prepared_only, true));
    let mut mismatched_product = marker.clone();
    mismatched_product["product"]["id"] = json!("900002");
    assert!(!restore(&mut resumed, &mismatched_product, true));
    let mut changed_row = row.clone();
    changed_row.product_line_total_jpy = Some(100);
    assert!(!service
        .restore_product_publication(
            "900001",
            &[&changed_row],
            &routes,
            &files,
            true,
            Some(&marker),
            &mut resumed
        )
        .unwrap());
    let mut fresh_forced = Checkpoint::fresh(account, "purchase", "forced").unwrap();
    assert!(!restore(&mut fresh_forced, &marker, true));
    let mut changed_entitlement = entitlement.clone();
    changed_entitlement["manifestComplete"] = json!(false);
    catalogue
        .put(
            "entitlement",
            &format!("{account}:purchase:900001"),
            changed_entitlement,
        )
        .unwrap();
    assert!(!restore(&mut resumed, &marker, true));
    catalogue
        .put(
            "entitlement",
            &format!("{account}:purchase:900001"),
            entitlement,
        )
        .unwrap();
    assert!(restore(&mut resumed, &marker, true));
    assert!(resumed.published_products.contains("900001"));
    assert!(resumed.enrichment_pending.is_empty());
    assert_eq!(catalogue.list("acquisition").unwrap().len(), 2);
    assert_eq!(catalogue.list("remote_file").unwrap().len(), 1);
    assert_eq!(
        catalogue.get("checkpoint", key).unwrap().unwrap(),
        collected_checkpoint
    );
    // Restoring one source omits only its already certified entry. A later
    // publication for the other source must retain those committed facts.
    let recovered_entitlement = catalogue
        .get("entitlement", &format!("{account}:purchase:900001"))
        .unwrap()
        .unwrap();
    let mut next_library_row = library_row.clone();
    next_library_row.files[0].id = "700002".into();
    next_library_row.files[0].variant_id = Some("800002".into());
    next_library_row.files[0].source_url =
        "https://booth.pm/downloadables/700002?variation_id=800002".into();
    let mut next_library = Checkpoint::fresh(account, "library", "full").unwrap();
    next_library.pending.clear();
    next_library.terminal = true;
    next_library.product_ids.insert("900001".into());
    next_library.enrichment_pending.insert("900001".into());
    let mut next_checkpoints = BTreeMap::from([(library_key.to_owned(), next_library)]);
    let next_entry = ProductPublication {
        account: account.into(),
        source: "library".into(),
        product: "900001".into(),
        routes: BTreeMap::from([(
            next_library_row.source_record_id.clone(),
            "/library?page=1".into(),
        )]),
        rows: vec![next_library_row],
        draft: draft.clone(),
        files: BTreeSet::from(["synthetic-account:700002:800002".into()]),
        checkpoint_key: library_key.into(),
        force_verify: true,
        needs_enrichment: false,
    };
    service
        .commit_product_group(&job, "900001", &[next_entry], &mut next_checkpoints)
        .unwrap();
    assert_eq!(
        catalogue
            .get("entitlement", &format!("{account}:purchase:900001"))
            .unwrap()
            .unwrap(),
        recovered_entitlement
    );
    assert!(resumed.complete());
    assert!(next_checkpoints[library_key].complete());
    assert!(restore(&mut resumed, &marker, true));
    assert_eq!(catalogue.list("remote_file").unwrap().len(), 2);
    assert_eq!(catalogue.list("acquisition").unwrap().len(), 2);
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}
/// A resumed run restores a product only when its saved signature still matches, so the
/// signature of the same rows must never move between App versions.
#[test]
fn publication_signatures_saved_by_earlier_versions_still_match() {
    let full = parser::Row {
        source_record_id: "purchase:pinned-a".into(),
        product_id: Some("900101".into()),
        order_id: Some("910101".into()),
        name: Some("Authored item".into()),
        shop: Some("Authored shop".into()),
        shop_url: Some("https://authored.booth.pm/".into()),
        preview_url: Some("https://booth.pximg.net/authored.png".into()),
        acquired_date_text: Some("2026/01/02 03:04".into()),
        acquired_at: None,
        paid_amount_text: Some("¥ 500".into()),
        product_line_total_jpy: Some(500),
        variant_name: Some("Standard".into()),
        files: vec![parser::File {
            id: "920101".into(),
            variant_id: Some("930101".into()),
            name: Some("authored.zip".into()),
            size_bytes: Some(1024),
            source_url: "https://booth.pm/downloadables/920101".into(),
            revision: "unknown".into(),
            format: Some("mp3".into()),
        }],
        manifest_complete: true,
        unavailable: false,
        error_code: None,
    };
    let sparse = parser::Row {
        source_record_id: "free:pinned-b".into(),
        product_id: Some("900101".into()),
        files: vec![
            parser::File {
                id: "920102".into(),
                source_url: "https://booth.pm/downloadables/920102".into(),
                revision: "rev-2".into(),
                ..full.files[0].clone()
            },
            parser::File {
                id: "920103".into(),
                variant_id: None,
                name: None,
                size_bytes: None,
                source_url: "https://booth.pm/downloadables/920103".into(),
                revision: "unknown".into(),
                format: None,
            },
        ],
        unavailable: true,
        error_code: Some("FREE_OFFER_UNAVAILABLE".into()),
        ..Default::default()
    };
    let routes = BTreeMap::from([
        ("purchase:pinned-a".to_owned(), "/orders/910101".to_owned()),
        (
            "free:pinned-b".to_owned(),
            "/library/free?page=2".to_owned(),
        ),
    ]);
    let files = BTreeSet::from(["account-a:920101:930101".to_owned()]);
    let one = publication_manifest("account-a", "purchase", "900101", [&full], &routes, &files);
    let both = publication_manifest(
        "account-a",
        "free",
        "900101",
        [&sparse, &full],
        &routes,
        &BTreeSet::new(),
    );
    assert_eq!(
        one.unwrap(),
        "4ada86e091668494a583e81527b9f22ff42403944df23b94c2831a4fafbafcc5"
    );
    assert_eq!(
        both.unwrap(),
        "5f5347181ff82721261f8130c2c260e790164229344c147b1b7522b6867d3970"
    );
}
#[test]
fn restored_free_scope_preserves_unbound_exception_and_completed_totals_without_replay() {
    let path = std::env::temp_dir().join(format!("booth-restored-free-{}", uuid::Uuid::new_v4()));
    let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
    let service = BoothService::new(catalogue.clone()).unwrap();
    let account = "authored-account";
    let job =
        json!({"id":"authored-restored-job","type":"boothSync","state":"running","progress":{}});
    catalogue
        .put("job", "authored-restored-job", job.clone())
        .unwrap();
    let entry = |source: &str| {
        let file = if source == "free" { "700001" } else { "700002" };
        let row = parser::Row {
            source_record_id: format!("{source}:900001"),
            product_id: Some("900001".into()),
            order_id: (source == "purchase").then(|| "910001".into()),
            product_line_total_jpy: (source == "purchase").then_some(100),
            manifest_complete: true,
            files: vec![parser::File {
                id: file.into(),
                variant_id: None,
                name: Some("authored.zip".into()),
                size_bytes: Some(10),
                source_url: format!("https://booth.pm/downloadables/{file}"),
                revision: "authored".into(),
                format: None,
            }],
            ..Default::default()
        };
        ProductPublication {
            account: account.into(),
            source: source.into(),
            product: "900001".into(),
            routes: BTreeMap::from([(
                row.source_record_id.clone(),
                if source == "free" {
                    safety::free_item_route("900001").unwrap()
                } else {
                    "/orders/910001".into()
                },
            )]),
            rows: vec![row],
            draft: json!({"id":"900001","store":"BOOTH","title":"Authored complete","publicMetadataState":"complete"}),
            files: BTreeSet::from([format!("{account}:{file}:")]),
            checkpoint_key: source.into(),
            force_verify: true,
            needs_enrichment: false,
        }
    };
    let free_entry = entry("free");
    let purchase_entry = entry("purchase");
    let mut checkpoints = BTreeMap::new();
    for source in ["free", "purchase"] {
        let mut checkpoint = Checkpoint::fresh(account, source, "full").unwrap();
        checkpoint.pending.clear();
        checkpoint.terminal = true;
        checkpoint.head = format!("authored-{source}-head");
        checkpoint.product_ids.insert("900001".into());
        checkpoint.enrichment_pending.insert("900001".into());
        // The exact free endpoint proves this product; an unrelated
        // unbound historical row still keeps source-wide coverage incomplete.
        checkpoint.unresolved = if source == "free" { 1 } else { 0 };
        service.save_checkpoint(source, &checkpoint).unwrap();
        checkpoints.insert(source.into(), checkpoint);
    }
    service
        .commit_product_group(
            &job,
            "900001",
            std::slice::from_ref(&free_entry),
            &mut checkpoints,
        )
        .unwrap();
    let free_entitlement = catalogue
        .get("entitlement", &format!("{account}:free:900001"))
        .unwrap()
        .unwrap();
    let draft_id = format!("booth-product:{}:900001", checkpoints["free"].run_id);
    let marker = catalogue.get("checkpoint", &draft_id).unwrap().unwrap();
    let mut resumed: Checkpoint =
        serde_json::from_value(catalogue.get("checkpoint", "free").unwrap().unwrap()).unwrap();
    assert!(resumed.published_products.is_empty());
    let mut publications = Publications {
        checkpoints: BTreeMap::from([
            ("free".into(), resumed.clone()),
            ("purchase".into(), checkpoints.remove("purchase").unwrap()),
        ]),
        unbound_sources: BTreeSet::from([(account.into(), "free".into())]),
        ..Default::default()
    };
    let (requests, _receiver) = mpsc::unbounded_channel();
    let prefetch = PublicPrefetch {
        requests,
        enqueued: Default::default(),
        support_pages: Default::default(),
        early: std::sync::Arc::new(Mutex::new(EarlyPublications {
            publications: Publications {
                checkpoints: publications.checkpoints.clone(),
                unbound_sources: publications.unbound_sources.clone(),
                ..Default::default()
            },
            ..Default::default()
        })),
    };
    assert!(service
        .restore_product_publication(
            "900001",
            &[&free_entry.rows[0]],
            &free_entry.routes,
            &free_entry.files,
            true,
            Some(&marker),
            &mut resumed
        )
        .unwrap());
    publications
        .checkpoints
        .insert("free".into(), resumed.clone());
    service
        .retain_published_context(&mut publications, Some(&prefetch), free_entry.clone())
        .unwrap();
    remember_product_context(&mut publications, purchase_entry.clone());
    remember_product_context(
        &mut prefetch.early.lock().unwrap().publications,
        purchase_entry.clone(),
    );
    assert!(publication_membership_proven(
        &publications.products["900001"],
        &publications.unbound_sources
    ));
    assert!(!publication_membership_proven(
        std::slice::from_ref(&purchase_entry),
        &publications.unbound_sources
    ));
    assert!(!publication_membership_proven(
        &publications.products["900001"],
        &BTreeSet::from([(account.into(), "purchase".into())])
    ));
    assert_eq!(published_product_count(&publications), 0); // Other scope still awaits certification.
    assert_eq!(
        publication_progress(&publications, "enriching", 1, 0)["metadataTotal"],
        1
    );
    let entries = publications.products["900001"].clone();
    service
        .commit_product_group(&job, "900001", &entries, &mut publications.checkpoints)
        .unwrap();
    assert_eq!(
        catalogue
            .get("entitlement", &format!("{account}:free:900001"))
            .unwrap()
            .unwrap(),
        free_entitlement
    );
    assert_eq!(
        catalogue.get("checkpoint", &draft_id).unwrap().unwrap(),
        marker
    );
    assert!(!publications.checkpoints["free"].complete());
    assert!(publications.checkpoints["purchase"].complete());
    assert_eq!(published_product_count(&publications), 1);
    // This also covers the subsequent resume through published_products,
    // rather than through a newly hydrated marker.
    assert!(service
        .restore_product_publication(
            "900001",
            &[&free_entry.rows[0]],
            &free_entry.routes,
            &free_entry.files,
            true,
            None,
            &mut resumed
        )
        .unwrap());
    let mut early = prefetch.early.lock().unwrap();
    early.publications.checkpoints = publications.checkpoints.clone();
    let revision = catalogue.revision().unwrap();
    assert!(service
        .publish_early_records(&job, &mut early, None)
        .unwrap()
        .is_empty());
    assert!(service
        .publish_early_records(&job, &mut early, None)
        .unwrap()
        .is_empty());
    service
        .commit_product_group(&job, "900001", &entries, &mut publications.checkpoints)
        .unwrap();
    assert_eq!(catalogue.revision().unwrap(), revision);
    let progress = publication_progress(
        &early.publications,
        "enriching",
        1,
        published_product_count(&early.publications),
    );
    assert_eq!(progress["products"], 1);
    assert_eq!(progress["metadataTotal"], 1);
    assert_eq!(progress["metadataCompleted"], 1);
    assert_eq!(progress["published"], 1);
    assert!(early.published.contains("900001"));
    assert_eq!(
        catalogue
            .get("entitlement", &format!("{account}:free:900001"))
            .unwrap()
            .unwrap()["manifestBasis"],
        "verifiedAccountProductFileList"
    );
    drop(early);
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn discovery_interleaves_listing_and_details_and_public_patch_keeps_manual_fields() {
    let mut checkpoint = Checkpoint::fresh("synthetic", "purchase", "full").unwrap();
    checkpoint.pending.clear();
    checkpoint.terminal = true;
    checkpoint.product_ids.insert("900001".into());
    checkpoint.enrichment_pending.insert("900001".into());
    assert!(collection_membership_complete(
        &checkpoint,
        &BTreeSet::new()
    ));
    assert!(!checkpoint.complete()); // collected does not mean published
    assert!(!collection_membership_complete(
        &checkpoint,
        &BTreeSet::from([("synthetic".into(), "purchase".into(), "900001".into())])
    ));
    assert!(collection_membership_complete(
        &checkpoint,
        &BTreeSet::from([("synthetic".into(), "gift".into(), "900001".into())])
    ));
    let incomplete = BTreeSet::from([("synthetic".into(), "purchase".into(), "900001".into())]);
    assert!(checkpoint_finished_after_collection(
        &checkpoint,
        false,
        &incomplete
    ));
    checkpoint.pending.push("/orders?page=2".into());
    assert!(!checkpoint_finished_after_collection(
        &checkpoint,
        false,
        &incomplete
    ));
    checkpoint.pending.clear();
    checkpoint.enrichment_pending.clear();
    checkpoint.published_products.insert("900001".into());
    assert!(checkpoint.complete());
    let pending = vec![
        "/orders?page=3".into(),
        "/orders/910001".into(),
        "/orders?page=2".into(),
        "/orders/910002".into(),
    ];
    assert_eq!(
        next_routes(&pending).unwrap(),
        vec!["/orders?page=2", "/orders/910001", "/orders/910002"]
    );
    assert_eq!(next_routes(&pending[3..]).unwrap(), vec!["/orders/910002"]);
    let free = vec![
        "/library/free_downloads?page=2".into(),
        safety::free_item_route("900001").unwrap(),
    ];
    assert_eq!(next_routes(&free).unwrap(), free);
    let before =
        json!({"id":"900001","note":"Human note","sourceTags":["old"],"tags":["old"],"price":10});
    let mut after = before.clone();
    after["sourceTags"] = json!(["new"]);
    after["price"] = json!(0);
    let patch = public_patch(&before, &after);
    assert!(patch.get("note").is_none());
    assert!(patch.get("id").is_none());
    assert_eq!(patch["price"], 0);
    let mut newer = before;
    newer["note"] = json!("Edited while fetching");
    newer
        .as_object_mut()
        .unwrap()
        .extend(patch.as_object().unwrap().clone());
    assert_eq!(newer["note"], "Edited while fetching");
    assert_eq!(newer["sourceTags"], json!(["new"]));
    let request = PublicRequest {
        product: "900001".into(),
        draft: newer.clone(),
        account: "synthetic".into(),
        source: "purchase".into(),
        run_id: "current-run".into(),
    };
    let mut saved = json!({"accountId":"synthetic","source":"purchase","runId":"current-run",
        "productId":"900001","product":newer,"enriched":true});
    assert!(request.saved_product(&saved).is_some());
    for key in ["accountId", "source", "runId", "productId"] {
        let mut other = saved.clone();
        other[key] = json!("other");
        assert!(request.saved_product(&other).is_none(), "{key}");
    }
    saved["enriched"] = json!(false);
    assert!(request.saved_product(&saved).is_none());
    let before =
        json!({"note":"Current human note","publicMetadataErrorCode":"PUBLIC_METADATA_FAILED"});
    let after = json!({"note":"Old human note","publicMetadataState":"complete"});
    let patch = public_patch(&before, &after);
    assert!(patch.get("note").is_none());
    assert_eq!(patch.get("publicMetadataErrorCode"), Some(&Value::Null));
    let public = json!({"id":"900001","sourceTitle":"Current public title",
        "title":"Current public title","previewUrl":"https://booth.pximg.net/current.png",
        "publicMetadataState":"complete","price":null,"publicVariants":[]});
    let mut receipt = json!({"id":"900001","sourceTitle":"Older receipt title",
        "title":"Older receipt title","previewUrl":"https://booth.pximg.net/old.png",
        "note":"Human note","publicMetadataErrorCode":"PUBLIC_METADATA_FAILED"});
    let observation = public_patch(&public, &public);
    receipt
        .as_object_mut()
        .unwrap()
        .extend(observation.as_object().unwrap().clone());
    assert_eq!(receipt["sourceTitle"], public["sourceTitle"]);
    assert_eq!(receipt["title"], public["title"]);
    assert_eq!(receipt["previewUrl"], public["previewUrl"]);
    assert!(receipt["price"].is_null());
    assert_eq!(receipt["publicVariants"], json!([]));
    assert!(receipt["publicMetadataErrorCode"].is_null());
    assert_eq!(receipt["note"], "Human note");
}
#[test]
fn a_list_reading_empty_after_a_pass_that_found_something_certifies_nothing() {
    let page = |rows: Vec<parser::Row>, is_detail, terminal, unresolved| parser::Page {
        source: "library".into(),
        route: "/library?page=1".into(),
        rows,
        next: vec![],
        details: vec![],
        terminal,
        unresolved,
        fingerprint: "authored".into(),
        is_detail,
        ordering: parser::Ordering::AcquiredNewestFirst,
        free_item: None,
    };
    let empty = page(vec![], false, true, 0);
    let found = Baseline {
        rows: vec![parser::Row::default()],
        ..Default::default()
    };
    let receipts_only = Baseline {
        details: BTreeSet::from(["/orders/910001".to_owned()]),
        ..Default::default()
    };
    assert!(contradicts_last_pass(&empty, Some(&found)));
    assert!(contradicts_last_pass(&empty, Some(&receipts_only)));
    // A list that was empty before, or never read, may simply be empty.
    assert!(!contradicts_last_pass(&empty, Some(&Baseline::default())));
    assert!(!contradicts_last_pass(&empty, None));
    // The last page of orders has no rows of its own, only receipts to read.
    let mut orders = page(vec![], false, true, 0);
    orders.details = vec!["/orders/910001".to_owned()];
    assert!(!contradicts_last_pass(&orders, Some(&receipts_only)));
    // Only a page saying the whole list is empty: rows, a receipt, a page the parser
    // already doubts or a list that has not ended say something else.
    for other in [
        page(vec![parser::Row::default()], false, true, 0),
        page(vec![], true, true, 0),
        page(vec![], false, true, 1),
        page(vec![], false, false, 0),
    ] {
        assert!(!contradicts_last_pass(&other, Some(&found)));
    }
}
#[test]
fn a_run_keeps_when_it_began_and_settles_only_what_it_lists() {
    let mut c = Checkpoint::fresh("synthetic", "library", "incremental").unwrap();
    assert!(!c.started_at.is_empty());
    assert_eq!(c.read_from(), c.started_at);
    let saved = serde_json::to_value(&c).unwrap();
    assert_eq!(saved["startedAt"], json!(c.started_at));
    // A run saved before this was recorded still loads, and has only the time it finishes.
    let mut older = saved;
    older.as_object_mut().unwrap().remove("startedAt");
    let older: Checkpoint = serde_json::from_value(older).unwrap();
    assert!(older.started_at.is_empty());
    assert!(chrono::DateTime::parse_from_rfc3339(&older.read_from()).is_ok());
    // Work for a product no list shows now cannot finish, so it does not hold the source open.
    c.pending.clear();
    c.terminal = true;
    c.enrichment_pending
        .extend(["900001".to_owned(), "900002".to_owned()]);
    c.forget_unlisted(|id| id == "900001");
    assert!(c.enrichment_pending.contains("900001") && !c.complete());
    c.forget_unlisted(|_| false);
    assert!(c.complete());
}
#[test]
fn unresolved_digital_candidates_prevent_manifest_certification() {
    let mut checkpoint = Checkpoint {
        terminal: true,
        head: "same-head".into(),
        ..Default::default()
    };
    let head = parser::parse(
        "<main><h1>Library</h1><div class='bg-white text-center'>No items</div></main>",
        "library",
        "/library?page=1",
    )
    .unwrap();
    checkpoint.head = head.fingerprint.clone();
    assert!(source_traversal_verified(&checkpoint, &head));
    checkpoint.unresolved = 1;
    assert!(!source_traversal_verified(&checkpoint, &head));
    checkpoint.unresolved = 0;
    checkpoint.pending.push("/library?page=2".into());
    assert!(!source_traversal_verified(&checkpoint, &head));
}
#[test]
fn acquisition_identity_reconciliation_rejects_ambiguous_or_foreign_bindings() {
    let mut row = parser::Row {
        product_id: Some("900001".into()),
        order_id: Some("910001".into()),
        source_record_id: format!(
            "purchase:{}",
            safety::hash("910001:900001::2026/10/10 09:30:")
        ),
        acquired_date_text: Some("2026/10/10 09:30".into()),
        ..Default::default()
    };
    let old_record = format!("purchase:{}", safety::hash("910001:900001:::"));
    let saved = json!({"id":format!("a:{old_record}"),"sourceRecordId":old_record,"accountId":"a","source":"purchase","productId":"900001","orderId":"910001","variantName":null,"acquiredDateText":"2026/10/10 09:30"});
    assert_eq!(
        unique_previous_acquisition_record(std::slice::from_ref(&saved), "a", "purchase", &row),
        Some(old_record.clone())
    );
    assert!(unique_previous_acquisition_record(
        &[saved.clone(), saved.clone()],
        "a",
        "purchase",
        &row
    )
    .is_none());
    assert!(unique_previous_acquisition_record(
        std::slice::from_ref(&saved),
        "b",
        "purchase",
        &row
    )
    .is_none());
    row.variant_name = Some("Different variant".into());
    assert!(unique_previous_acquisition_record(
        std::slice::from_ref(&saved),
        "a",
        "purchase",
        &row
    )
    .is_none());
    row.variant_name = None;
    row.source_record_id = format!("purchase:{}", safety::hash("line:920001"));
    assert!(unique_previous_acquisition_record(
        std::slice::from_ref(&saved),
        "a",
        "purchase",
        &row
    )
    .is_none());
}
#[test]
fn only_undated_gift_rows_read_their_item_and_the_date_reaches_the_existing_record() {
    let path = std::env::temp_dir().join(format!("nna-booth-gift-date-{}", uuid::Uuid::new_v4()));
    let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
    let service = BoothService::new(catalogue.clone()).unwrap();
    let account = "synthetic-account";
    let row = |id: &str| parser::Row {
        source_record_id: format!("gift:{}", safety::hash(format!(":{id}:::"))),
        product_id: Some(id.into()),
        name: Some("Authored gift".into()),
        ..Default::default()
    };
    let page = |rows: Vec<parser::Row>| parser::Page {
        source: "gift".into(),
        route: "/library/gifts?page=1".into(),
        rows,
        next: vec![],
        details: vec![],
        terminal: true,
        unresolved: 0,
        fingerprint: "authored".into(),
        is_detail: false,
        ordering: parser::Ordering::Unknown,
        free_item: None,
    };
    let needed = |mode: &str, selected: Option<&BTreeSet<String>>, page: &parser::Page| {
        service
            .undated_gift_products(account, mode, selected, page)
            .unwrap()
            .into_iter()
            .collect::<Vec<_>>()
    };
    let saved_date = |id: &str, date: &str| {
        let record = format!("{account}:{}", row(id).source_record_id);
        catalogue
            .put("acquisition", &record, json!({"id":record,"accountId":account,"source":"gift","productId":id,"acquiredDateText":date}))
            .unwrap();
    };
    saved_date("900002", "2026/11/01");
    saved_date("900003", " ");
    catalogue
        .put(
            "product",
            "900005",
            json!({"id":"900005","publicPageAvailability":"unavailable"}),
        )
        .unwrap();
    let mut second_gift = row("900001");
    second_gift.source_record_id.push_str(":occurrence:1");
    let mut listing_dated = row("900004");
    listing_dated.acquired_date_text = Some("2026/11/01".into());
    let rows = page(vec![
        row("900001"),
        second_gift,
        row("900002"),
        row("900003"),
        listing_dated,
        row("900005"),
        row("not-numeric"),
        parser::Row {
            source_record_id: "gift:withheld".into(),
            ..Default::default()
        },
    ]);
    assert_eq!(needed("incremental", None, &rows), ["900001", "900003"]);
    // Only Full asks again about a listing proven removed.
    assert_eq!(needed("full", None, &rows), ["900001", "900003", "900005"]);
    assert_eq!(
        needed("forced", None, &rows),
        ["900001", "900003", "900005"]
    );
    let selected = BTreeSet::from(["900003".to_owned(), "900004".to_owned()]);
    assert_eq!(needed("full", Some(&selected), &rows), ["900003"]);

    // A library synced before this fix gains the date on the record it already has.
    let mut gift = row("900006");
    gift.files = vec![parser::File {
        id: "700001".into(),
        variant_id: None,
        name: Some("authored.zip".into()),
        size_bytes: None,
        source_url: "https://booth.pm/downloadables/700001".into(),
        revision: "authored".into(),
        format: None,
    }];
    gift.manifest_complete = true;
    let route = "/library/gifts?page=1";
    service
        .record_row(account, "gift", &gift, route, false)
        .unwrap();
    let record = format!("{account}:{}", gift.source_record_id);
    let stored = || catalogue.get("acquisition", &record).unwrap().unwrap();
    assert!(stored()["acquiredDateText"].is_null());
    let undated_page = page(vec![gift.clone()]);
    assert_eq!(needed("incremental", None, &undated_page), ["900006"]);
    let mut dated = gift.clone();
    dated.acquired_date_text = Some("2026/11/01".into());
    service
        .record_row(account, "gift", &dated, route, false)
        .unwrap();
    assert_eq!(stored()["acquiredDateText"], "2026/11/01");
    assert_eq!(stored()["sourceRecordId"], json!(gift.source_record_id));
    let products = catalogue.command("bootstrap", json!({})).unwrap()["products"].clone();
    let projected = products
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "900006")
        .unwrap();
    assert_eq!(projected["acquired"], "2026/11/01");
    // The next listing read has no date of its own: it costs no request and loses nothing.
    assert!(needed("full", None, &undated_page).is_empty());
    service
        .record_row(account, "gift", &gift, route, false)
        .unwrap();
    assert_eq!(stored()["acquiredDateText"], "2026/11/01");
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn shelf_date_supplementation_preserves_unique_fallback_but_not_distinct_dated_events() {
    for source in ["library", "gift", "free"] {
        let old_record = format!("{source}:{}", safety::hash(":900001:::"));
        let mut row = parser::Row {
            product_id: Some("900001".into()),
            source_record_id: format!("{source}:{}", safety::hash(":900001::2026/10/10 09:30:")),
            acquired_date_text: Some("2026/10/10 09:30".into()),
            ..Default::default()
        };
        let mut saved = json!({"id":format!("a:{old_record}"),"sourceRecordId":old_record,"accountId":"a","source":source,"productId":"900001","orderId":null,"variantName":null,"acquiredDateText":null});
        assert_eq!(
            unique_previous_acquisition_record(std::slice::from_ref(&saved), "a", source, &row),
            Some(old_record)
        );
        saved["acquiredDateText"] = json!("2026/10/09 09:30");
        let dated_record = format!("{source}:{}", safety::hash(":900001::2026/10/09 09:30:"));
        saved["sourceRecordId"] = json!(dated_record);
        saved["id"] = json!(format!("a:{dated_record}"));
        assert!(unique_previous_acquisition_record(
            std::slice::from_ref(&saved),
            "a",
            source,
            &row
        )
        .is_none());
        row.source_record_id.push_str(":occurrence:1");
        assert!(unique_previous_acquisition_record(
            std::slice::from_ref(&saved),
            "a",
            source,
            &row
        )
        .is_none());
    }
}
#[test]
fn selected_receipts_use_unique_bound_routes_and_leave_unknown_products_out() {
    let acquisitions = vec![
        json!({"accountId":"a","source":"purchase","productId":"900001","orderId":"910001"}),
        json!({"accountId":"a","source":"purchase","productId":"900001","orderId":"910001"}),
        json!({"accountId":"b","source":"purchase","productId":"900001","orderId":"910999"}),
        json!({"accountId":"a","source":"gift","productId":"900001","orderId":"910998"}),
    ];
    let remote = vec![
        json!({"accountId":"a","source":"purchase","productId":"900002","resolverPage":"/orders/910001"}),
        json!({"accountId":"a","source":"purchase","productId":"900002","resolverPage":"/orders?page=1"}),
    ];
    let selected = BTreeSet::from(["900001".into(), "900002".into()]);
    assert_eq!(
        selected_purchase_routes(&acquisitions, &remote, "a", &selected),
        BTreeMap::from([("/orders/910001".into(), selected.clone())])
    );
    // 900002 has no receipt for account b; 900001 still settles from its own.
    assert_eq!(
        selected_purchase_routes(&acquisitions, &remote, "b", &selected),
        BTreeMap::from([("/orders/910999".into(), BTreeSet::from(["900001".into()]))])
    );
    assert!(selected_purchase_routes(&[], &[], "a", &selected).is_empty());
}
#[test]
fn a_free_update_rebuilds_only_its_accounts_usable_history_rows() {
    let acquisitions = vec![
        json!({"accountId":"a","source":"free","sourceRecordId":"free:1","productId":"900001","observedProductId":"900001","acquiredDateText":"2026/10/01","unavailable":false,"errorCode":null}),
        json!({"accountId":"a","source":"free","sourceRecordId":"free:2","productId":"900001","observedProductId":"900001","unavailable":false,"errorCode":null}),
        json!({"accountId":"a","source":"free","sourceRecordId":"free:3","productId":"900001","unavailable":true}),
        json!({"accountId":"a","source":"free","sourceRecordId":"free:4","productId":"900001","errorCode":"PRODUCT_TITLE_UNRESOLVED"}),
        json!({"accountId":"a","source":"free","sourceRecordId":"","productId":"900001"}),
        json!({"accountId":"b","source":"free","sourceRecordId":"free:5","productId":"900001"}),
        json!({"accountId":"a","source":"gift","sourceRecordId":"gift:6","productId":"900001"}),
        json!({"accountId":"a","source":"free","sourceRecordId":"free:7","productId":"900002"}),
    ];
    let rows = saved_free_history(&acquisitions, "a", "900001");
    assert_eq!(
        rows.iter()
            .map(|row| row.source_record_id.as_str())
            .collect::<Vec<_>>(),
        ["free:1", "free:2"]
    );
    assert!(rows
        .iter()
        .all(|row| row.product_id.as_deref() == Some("900001")
            && row.files.is_empty()
            && !row.manifest_complete
            && !row.unavailable
            && row.error_code.is_none()));
    assert_eq!(rows[0].acquired_date_text.as_deref(), Some("2026/10/01"));
    assert!(saved_free_history(&acquisitions, "a", "900003").is_empty());
}
#[test]
fn targeted_paid_patch_preserves_all_other_fields_and_records() {
    let path =
        std::env::temp_dir().join(format!("nna-paid-patch-synthetic-{}", uuid::Uuid::new_v4()));
    let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
    let mut row = parser::Row {
        source_record_id: "purchase:authored-product".into(),
        product_id: Some("900001".into()),
        order_id: Some("910001".into()),
        name: Some("Authored product".into()),
        manifest_complete: true,
        product_line_total_jpy: Some(0),
        acquired_date_text: Some("2026/10/10 09:30".into()),
        files: vec![parser::File {
            id: "700001".into(),
            variant_id: None,
            name: Some("authored.zip".into()),
            size_bytes: None,
            source_url: "https://booth.pm/downloadables/700001".into(),
            revision: "authored-revision".into(),
            format: None,
        }],
        ..Default::default()
    };
    let existing = json!({"id":"synthetic-account:purchase:authored-product", "accountId":"synthetic-account", "source":"purchase",
        "sourceRecordId":row.source_record_id, "orderId":"910001", "productId":"900001", "observedProductId":"900001",
        "acquiredDateText":"authored-prior-date", "paidAmountText":"authored-prior-text", "variantName":"authored-variant", "observedAt":"authored-time", "freeHistoryOrder":"authored-order"});
    catalogue
        .put(
            "acquisition",
            existing["id"].as_str().unwrap(),
            existing.clone(),
        )
        .unwrap();
    let protected = [
        "entitlement",
        "product",
        "remote_file",
        "artifact",
        "checkpoint",
        "source_coverage",
    ];
    for kind in protected {
        catalogue
            .put(
                kind,
                "authored-protected",
                json!({"id":"authored-protected", "manifestComplete":true, "authoredKeep":kind}),
            )
            .unwrap();
    }
    let before: Vec<_> = protected
        .iter()
        .map(|kind| catalogue.list(kind).unwrap())
        .collect();
    assert!(saved_product_line_binding(std::slice::from_ref(&row), &row));
    let patched =
        patched_product_fields(&existing, "synthetic-account", &row, "/orders/910001").unwrap();
    let mut expected = existing.clone();
    expected["paidPrice"] = json!(0);
    expected["paidCurrency"] = json!("JPY");
    expected["paidPriceBasis"] = json!("productLineTotal");
    expected["acquiredDateText"] = json!("2026/10/10 09:30");
    assert_eq!(patched, expected);
    let mut batch = super::super::batch::RecordBatch::new(&catalogue);
    batch
        .stage(
            "acquisition",
            existing["id"].as_str().unwrap(),
            patched.clone(),
        )
        .unwrap();
    assert_eq!(batch.try_commit().unwrap(), Some(1));
    assert_eq!(
        catalogue.list("acquisition").unwrap(),
        vec![patched.clone()]
    );
    for (kind, prior) in protected.iter().zip(before) {
        assert_eq!(catalogue.list(kind).unwrap(), prior);
    }
    for key in [
        "id",
        "accountId",
        "source",
        "sourceRecordId",
        "orderId",
        "productId",
        "observedProductId",
    ] {
        let mut conflict = existing.clone();
        conflict[key] = json!("authored-conflict");
        assert!(
            patched_product_fields(&conflict, "synthetic-account", &row, "/orders/910001")
                .is_none()
        );
    }
    let mut stale = row.clone();
    stale.files[0].id = "700002".into();
    assert!(!saved_product_line_binding(
        std::slice::from_ref(&row),
        &stale
    ));
    assert!(!saved_product_line_binding(
        &[row.clone(), row.clone()],
        &row
    ));
    row.acquired_date_text = Some(" ".into());
    let unchanged =
        patched_product_fields(&patched, "synthetic-account", &row, "/orders/910001").unwrap();
    assert_eq!(unchanged, patched);
    let mut replay = super::super::batch::RecordBatch::new(&catalogue);
    replay
        .stage("acquisition", existing["id"].as_str().unwrap(), unchanged)
        .unwrap();
    assert_eq!(replay.try_commit().unwrap(), Some(0));
    drop(catalogue);
    assert!(path.starts_with(std::env::temp_dir()));
    std::fs::remove_dir_all(path).unwrap();
}
fn diagnostic_sample_fixture(count: usize) -> (Checkpoint, Vec<Value>) {
    let mut checkpoint = Checkpoint::fresh("synthetic-account", "purchase", "full").unwrap();
    let mut details = Vec::new();
    for index in 0..count {
        let page = parser::parse(&format!("<main><div class='sheet'><a href='https://invented.booth.pm/items/{}'>Authored</a><div data-test='downloadable-list'><b data-test='filename'>authored.zip</b><a href='https://booth.pm/downloadables/{}'>Download</a></div></div></main>", 900001 + index, 700001 + index), "purchase", &format!("/orders/{}", 910001 + index)).unwrap();
        details.push(page);
    }
    let mut listing = details[0].clone();
    listing.route = "/orders?page=1".into();
    listing.is_detail = false;
    listing.rows.clear();
    listing.details = details.iter().map(|page| page.route.clone()).collect();
    listing.fingerprint = "synthetic-listing".into();
    let mut journals = Vec::new();
    for (index, page) in std::iter::once(listing).chain(details).enumerate() {
        let id = format!("journal-{index}");
        checkpoint.journal_ids.push(id.clone());
        checkpoint
            .committed
            .insert(page.route.clone(), page.fingerprint.clone());
        journals.push(json!({"id":id,"accountId":checkpoint.account_id,"source":"purchase","runId":checkpoint.run_id,"page":page}));
    }
    (checkpoint, journals)
}
#[test]
fn targeted_receipt_selection_is_exact_and_validates_other_journals() {
    let (checkpoint, journals) = diagnostic_sample_fixture(3);
    let selected =
        select_targeted_receipt_proof(&checkpoint, &journals, "synthetic-account", "910003")
            .unwrap()
            .unwrap();
    assert!(selected
        .iter()
        .all(|row| row.order_id.as_deref() == Some("910003")));
    assert!(
        select_targeted_receipt_proof(&checkpoint, &journals, "synthetic-account", "910999")
            .unwrap()
            .is_none()
    );
    for index in 0..journals.len() {
        let mut invalid = journals.clone();
        invalid[index]["runId"] = json!("other-run");
        assert_eq!(
            select_targeted_receipt_proof(&checkpoint, &invalid, "synthetic-account", "910003")
                .unwrap_err()
                .to_string(),
            "CHECKPOINT_INVALID"
        );
    }
    assert_eq!(
        select_targeted_receipt_proof(&checkpoint, &journals, "different-account", "910003")
            .unwrap_err()
            .to_string(),
        "CHECKPOINT_INVALID"
    );
    assert_eq!(
        select_targeted_receipt_proof(&checkpoint, &journals, "synthetic-account", "910003?x=1")
            .unwrap_err()
            .to_string(),
        "INVALID_DIAGNOSTIC_ARGUMENTS"
    );
    let mut unwitnessed = journals.clone();
    unwitnessed[0]["page"]["details"] = json!(["/orders/910001"]);
    assert!(select_targeted_receipt_proof(
        &checkpoint,
        &unwitnessed,
        "synthetic-account",
        "910003"
    )
    .unwrap()
    .is_none());
}
#[test]
fn acquisition_diagnostic_detail_is_current_run_and_listing_witness_bound() {
    let detail=parser::parse("<main><div class='sheet'><a href='https://invented.booth.pm/items/900001'>Authored</a><div data-test='downloadable-list'><b data-test='filename'>authored.zip</b><a href='https://booth.pm/downloadables/700001'>Download</a></div></div></main>","purchase","/orders/910001").unwrap();
    let mut listing = detail.clone();
    listing.route = "/orders?page=1".into();
    listing.is_detail = false;
    listing.rows.clear();
    listing.details = vec![detail.route.clone()];
    listing.fingerprint = "synthetic-listing".into();
    let mut checkpoint = Checkpoint::fresh("synthetic-account", "purchase", "full").unwrap();
    checkpoint.journal_ids = vec!["listing".into(), "detail".into()];
    checkpoint
        .committed
        .insert(listing.route.clone(), listing.fingerprint.clone());
    checkpoint
        .committed
        .insert(detail.route.clone(), detail.fingerprint.clone());
    let journals = vec![
        json!({"id":"listing","accountId":checkpoint.account_id,"source":"purchase","runId":checkpoint.run_id,"page":listing}),
        json!({"id":"detail","accountId":checkpoint.account_id,"source":"purchase","runId":checkpoint.run_id,"page":detail}),
    ];
    let (route, rows) =
        select_acquisition_diagnostic_detail(&checkpoint, &journals, "synthetic-account").unwrap();
    assert_eq!(route, "/orders/910001");
    assert_eq!(rows.len(), 1);
    let mut rejected = journals.clone();
    rejected[1]["runId"] = json!("old-run");
    assert_eq!(
        select_acquisition_diagnostic_detail(&checkpoint, &rejected, "synthetic-account")
            .unwrap_err()
            .to_string(),
        "CHECKPOINT_INVALID"
    );
    let mut rejected = journals.clone();
    rejected[0]["page"]["details"] = json!([]);
    assert_eq!(
        select_acquisition_diagnostic_detail(&checkpoint, &rejected, "synthetic-account")
            .unwrap_err()
            .to_string(),
        "DIAGNOSTIC_ENTRY_NOT_FOUND"
    );
    let mut rejected = journals.clone();
    rejected[1]["page"]["rows"][0]["manifestComplete"] = json!(false);
    assert_eq!(
        select_acquisition_diagnostic_detail(&checkpoint, &rejected, "synthetic-account")
            .unwrap_err()
            .to_string(),
        "DIAGNOSTIC_ENTRY_NOT_FOUND"
    );
    let mut rejected = checkpoint.clone();
    rejected.committed.clear();
    assert_eq!(
        select_acquisition_diagnostic_detail(&rejected, &journals, "synthetic-account")
            .unwrap_err()
            .to_string(),
        "CHECKPOINT_INVALID"
    );
}
#[test]
fn diagnostic_reservation_atomically_blocks_duplicates_and_account_jobs() {
    let running = Mutex::new(BTreeSet::new());
    let first = DiagnosticReservation::acquire(&running, "synthetic-account").unwrap();
    assert_eq!(
        *running.lock().unwrap(),
        BTreeSet::from([
            "booth-global".to_owned(),
            "account:synthetic-account".to_owned()
        ])
    );
    for account in ["synthetic-account", "other-account"] {
        assert_eq!(
            DiagnosticReservation::acquire(&running, account)
                .err()
                .unwrap()
                .to_string(),
            "ACCOUNT_BUSY"
        );
    }
    drop(first);
    assert!(running.lock().unwrap().is_empty());
    running
        .lock()
        .unwrap()
        .insert("account:other-account".into());
    assert_eq!(
        DiagnosticReservation::acquire(&running, "synthetic-account")
            .err()
            .unwrap()
            .to_string(),
        "ACCOUNT_BUSY"
    );
    assert_eq!(
        *running.lock().unwrap(),
        BTreeSet::from(["account:other-account".to_owned()])
    );
}
#[test]
fn diagnostic_reservation_releases_its_keys_on_error() {
    let running = Mutex::new(BTreeSet::new());
    let result: Result<()> = (|| {
        let _reservation = DiagnosticReservation::acquire(&running, "synthetic-account")?;
        running.lock().unwrap().insert("unrelated-key".into());
        bail!("SYNTHETIC_EARLY_ERROR");
    })();
    assert_eq!(result.unwrap_err().to_string(), "SYNTHETIC_EARLY_ERROR");
    assert_eq!(
        *running.lock().unwrap(),
        BTreeSet::from(["unrelated-key".to_owned()])
    );
    running.lock().unwrap().clear();
    let next = DiagnosticReservation::acquire(&running, "synthetic-account").unwrap();
    drop(next);
    assert!(running.lock().unwrap().is_empty());
}
#[tokio::test]
async fn cancelling_pending_diagnostic_future_releases_both_keys() {
    let running = Mutex::new(BTreeSet::new());
    let mut future = Box::pin(async {
        let _reservation = DiagnosticReservation::acquire(&running, "synthetic-account").unwrap();
        std::future::pending::<()>().await;
    });
    std::future::poll_fn(|context| {
        assert!(std::future::Future::poll(future.as_mut(), context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    assert_eq!(running.lock().unwrap().len(), 2);
    drop(future);
    assert!(running.lock().unwrap().is_empty());
    let next = DiagnosticReservation::acquire(&running, "other-account").unwrap();
    drop(next);
    assert!(running.lock().unwrap().is_empty());
}
#[test]
fn identity_diagnostic_selection_requires_its_fixed_issue_and_current_history() {
    let mut checkpoint = Checkpoint::fresh("synthetic-account", "free", "full").unwrap();
    let history = free_history_page();
    let identity = free_item::Evidence::unknown("900001", "FREE_ITEM_IDENTITY_UNPROVEN", false)
        .page()
        .unwrap();
    checkpoint.journal_ids = vec!["synthetic-history".into(), "synthetic-identity".into()];
    checkpoint
        .committed
        .insert(history.route.clone(), history.fingerprint.clone());
    checkpoint
        .committed
        .insert(identity.route.clone(), identity.fingerprint.clone());
    let journals = vec![
        json!({"id":"synthetic-history","accountId":checkpoint.account_id,"source":"free","runId":checkpoint.run_id,"page":history}),
        json!({"id":"synthetic-identity","accountId":checkpoint.account_id,"source":"free","runId":checkpoint.run_id,"page":identity}),
    ];
    assert_eq!(
        select_free_item_diagnostic_route(
            &checkpoint,
            &journals,
            "synthetic-account",
            FreeItemDiagnostic::Identity
        )
        .unwrap(),
        "https://booth.pm/ja/items/900001"
    );
    assert_eq!(
        select_entry_diagnostic_route(&checkpoint, &journals, "synthetic-account")
            .unwrap_err()
            .to_string(),
        "DIAGNOSTIC_ENTRY_NOT_FOUND"
    );
    for (index, field, value) in [
        (1, "runId", "old-run"),
        (0, "accountId", "other-account"),
        (1, "source", "purchase"),
    ] {
        let mut rejected = journals.clone();
        rejected[index][field] = json!(value);
        assert_eq!(
            select_free_item_diagnostic_route(
                &checkpoint,
                &rejected,
                "synthetic-account",
                FreeItemDiagnostic::Identity
            )
            .unwrap_err()
            .to_string(),
            "CHECKPOINT_INVALID"
        );
    }
    for (index, field, value) in [
        (0, "unavailable", json!(true)),
        (0, "errorCode", json!("ROW_UNPROVEN")),
        (1, "errorCode", json!("FREE_ITEM_ENTRY_UNPROVEN")),
    ] {
        let mut rejected = journals.clone();
        if index == 0 {
            rejected[0]["page"]["rows"][0][field] = value;
        } else {
            rejected[1]["page"]["freeItem"][field] = value;
        }
        assert_eq!(
            select_free_item_diagnostic_route(
                &checkpoint,
                &rejected,
                "synthetic-account",
                FreeItemDiagnostic::Identity
            )
            .unwrap_err()
            .to_string(),
            "DIAGNOSTIC_ENTRY_NOT_FOUND"
        );
    }
}
#[test]
fn entry_diagnostic_selection_is_current_account_run_and_history_bound() {
    let mut checkpoint = Checkpoint::fresh("synthetic-account", "free", "full").unwrap();
    let history = free_history_page();
    let entry = free_item::Evidence::unknown("900001", "FREE_ITEM_ENTRY_UNPROVEN", false)
        .page()
        .unwrap();
    checkpoint.journal_ids = vec!["synthetic-history".into(), "synthetic-entry".into()];
    checkpoint
        .committed
        .insert(history.route.clone(), history.fingerprint.clone());
    checkpoint
        .committed
        .insert(entry.route.clone(), entry.fingerprint.clone());
    let journals = vec![
        json!({"id":"synthetic-history","accountId":checkpoint.account_id,"source":"free","runId":checkpoint.run_id,"page":history}),
        json!({"id":"synthetic-entry","accountId":checkpoint.account_id,"source":"free","runId":checkpoint.run_id,"page":entry}),
    ];
    assert_eq!(
        select_entry_diagnostic_route(&checkpoint, &journals, "synthetic-account").unwrap(),
        "https://booth.pm/ja/items/900001"
    );
    assert_eq!(
        select_entry_diagnostic_route(&checkpoint, &journals, "different-account")
            .unwrap_err()
            .to_string(),
        "CHECKPOINT_INVALID"
    );
    for (index, field, value) in [
        (0, "accountId", "other-account"),
        (1, "runId", "old-run"),
        (1, "source", "purchase"),
        (1, "id", "unlisted-entry"),
    ] {
        let mut rejected = journals.clone();
        rejected[index][field] = json!(value);
        assert_eq!(
            select_entry_diagnostic_route(&checkpoint, &rejected, "synthetic-account")
                .unwrap_err()
                .to_string(),
            "CHECKPOINT_INVALID"
        );
    }
    let mut no_history = journals.clone();
    no_history[0]["page"]["rows"][0]["unavailable"] = json!(true);
    assert_eq!(
        select_entry_diagnostic_route(&checkpoint, &no_history, "synthetic-account")
            .unwrap_err()
            .to_string(),
        "DIAGNOSTIC_ENTRY_NOT_FOUND"
    );
    let mut wrong_reason = journals.clone();
    wrong_reason[1]["page"]["freeItem"]["errorCode"] = json!("FREE_ITEM_ROLE_UNPROVEN");
    assert_eq!(
        select_entry_diagnostic_route(&checkpoint, &wrong_reason, "synthetic-account")
            .unwrap_err()
            .to_string(),
        "DIAGNOSTIC_ENTRY_NOT_FOUND"
    );
    for issue in [FreeItemDiagnostic::Role, FreeItemDiagnostic::Controls] {
        let mut bound = journals.clone();
        bound[1]["page"]["freeItem"]["errorCode"] = json!(issue.error_code());
        assert_eq!(
            select_free_item_diagnostic_route(&checkpoint, &bound, "synthetic-account", issue,)
                .unwrap(),
            "https://booth.pm/ja/items/900001"
        );
        bound[0]["page"]["rows"][0]["unavailable"] = json!(true);
        assert_eq!(
            select_free_item_diagnostic_route(&checkpoint, &bound, "synthetic-account", issue,)
                .unwrap_err()
                .to_string(),
            "DIAGNOSTIC_ENTRY_NOT_FOUND"
        );
    }
    for version in [0, 1] {
        let mut old = checkpoint.clone();
        old.free_item_adapter_version = version;
        assert_eq!(
            select_entry_diagnostic_route(&old, &journals, "synthetic-account")
                .unwrap_err()
                .to_string(),
            "CHECKPOINT_INVALID"
        );
    }
    let mut old = checkpoint.clone();
    old.committed.clear();
    assert_eq!(
        select_entry_diagnostic_route(&old, &journals, "synthetic-account")
            .unwrap_err()
            .to_string(),
        "CHECKPOINT_INVALID"
    );
}
#[test]
fn selected_free_update_preserves_global_coverage_and_unselected_product() {
    let account = "10000000-0000-4000-8000-000000000001";
    let coverage_id = format!("{account}:free");
    let selected = BTreeSet::from(["900001".to_owned()]);
    for existing_coverage in [true, false] {
        let path = std::env::temp_dir().join(format!(
            "nna-booth-synthetic-selected-{}",
            uuid::Uuid::new_v4()
        ));
        let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
        let original = json!({"id":coverage_id,"accountId":account,"source":"free","status":"incomplete","coverageComplete":false,"productCount":2,"unresolvedRows":1,"completedAt":"authored-prior-proof"});
        if existing_coverage {
            catalogue
                .put("source_coverage", &coverage_id, original.clone())
                .unwrap();
        }
        let mut history = free_history_page();
        let mut unselected = history.rows[0].clone();
        unselected.source_record_id = "free:synthetic-other".into();
        unselected.product_id = Some("900002".into());
        history.rows.push(unselected);
        assert_eq!(eligible_free_rows(&history).count(), 2);
        let a_id = format!("{account}:free:900001");
        let b_id = format!("{account}:free:900002");
        let a_before = json!({"id":a_id,"productId":"900001","accountId":account,"source":"free","fileIds":[],"manifestComplete":false});
        let b_before = json!({"id":b_id,"productId":"900002","accountId":account,"source":"free","fileIds":[],"manifestComplete":false});
        catalogue
            .put_many(vec![
                ("entitlement".into(), a_id.clone(), a_before),
                ("entitlement".into(), b_id.clone(), b_before.clone()),
            ])
            .unwrap();
        let before = catalogue.get("source_coverage", &coverage_id).unwrap();
        let running =
            json!({"id":coverage_id,"accountId":account,"source":"free","status":"running"});
        if let Some((kind, id, value)) =
            source_coverage_record(Some(&selected), &coverage_id, running)
        {
            catalogue.put(&kind, &id, value).unwrap();
        }
        assert_eq!(
            catalogue.get("source_coverage", &coverage_id).unwrap(),
            before
        );
        let mut a_updated = catalogue.get("entitlement", &a_id).unwrap().unwrap();
        a_updated["manifestComplete"] = json!(true);
        a_updated["fileIds"] = json!([format!("{account}:700001:800001")]);
        let complete = json!({"id":coverage_id,"accountId":account,"source":"free","status":"complete","coverageComplete":true,"productCount":2});
        let mut records = vec![("entitlement".into(), a_id.clone(), a_updated.clone())];
        records.extend(source_coverage_record(
            Some(&selected),
            &coverage_id,
            complete.clone(),
        ));
        catalogue.put_many(records).unwrap();
        assert_eq!(
            catalogue.get("source_coverage", &coverage_id).unwrap(),
            before
        );
        assert_eq!(
            catalogue.get("entitlement", &a_id).unwrap(),
            Some(a_updated)
        );
        assert_eq!(catalogue.get("entitlement", &b_id).unwrap(), Some(b_before));
        let unrestricted = source_coverage_record(None, &coverage_id, complete.clone()).unwrap();
        catalogue.put_many(vec![unrestricted]).unwrap();
        assert_eq!(
            catalogue.get("source_coverage", &coverage_id).unwrap(),
            Some(complete)
        );
        drop(catalogue);
        std::fs::remove_dir_all(path).unwrap();
    }
}
fn free_history_page() -> parser::Page {
    parser::Page {
        source: "free".into(),
        route: "/library/free_downloads?page=1".into(),
        rows: vec![parser::Row {
            source_record_id: "free:synthetic-original".into(),
            product_id: Some("900001".into()),
            name: Some("Authored history name".into()),
            acquired_date_text: Some("synthetic-original-date".into()),
            ..parser::Row::default()
        }],
        next: vec![],
        details: vec![],
        terminal: true,
        unresolved: 0,
        fingerprint: "synthetic".into(),
        is_detail: false,
        ordering: parser::Ordering::DownloadedNewestFirst,
        free_item: None,
    }
}
#[test]
fn settled_free_absence_requires_bound_history_and_preserves_incomplete_coverage() {
    let mut history = BTreeMap::new();
    remember_free_history(&free_history_page(), &mut history);
    for code in ["FREE_OFFER_UNAVAILABLE", "PUBLIC_METADATA_UNAVAILABLE"] {
        let page = free_item::Evidence::unknown("900001", code, true)
            .page()
            .unwrap();
        assert!(settled_free_absence(&page, &history));
        assert!(!settled_free_absence(&page, &BTreeMap::new()));
        let mut wrong = page.clone();
        wrong.route = safety::free_item_route("900002").unwrap();
        assert!(!settled_free_absence(&wrong, &history));
        let mut wrong = history.clone();
        wrong.get_mut("900001").unwrap()[0].product_id = Some("900002".into());
        assert!(!settled_free_absence(&page, &wrong));
        let journal = json!({"accountId":"authored","source":"free","runId":"current","page":page});
        assert!(checked_journal_page(&journal, "authored", "free", "current").is_ok());
        for (account, source, run) in [
            ("other", "free", "current"),
            ("authored", "library", "current"),
            ("authored", "free", "old"),
        ] {
            assert!(checked_journal_page(&journal, account, source, run).is_err());
        }
    }
    for (code, unavailable) in [
        ("FREE_FILE_CONTROLS_UNAVAILABLE", false),
        ("FREE_OFFER_UNAVAILABLE", false),
        ("SOURCE_HTTP_FAILED", true),
        ("AUTH_REQUIRED", true),
        ("RATE_LIMITED", true),
    ] {
        assert!(!settled_free_absence(
            &free_item::Evidence::unknown("900001", code, unavailable)
                .page()
                .unwrap(),
            &history
        ));
    }
    let mut checkpoint = Checkpoint::fresh("authored", "free", "full").unwrap();
    checkpoint.pending.clear();
    checkpoint.terminal = true;
    checkpoint.head = "authored-head".into();
    checkpoint.unresolved = 1;
    assert!(!checkpoint.complete());
    assert!(!collection_membership_complete(
        &checkpoint,
        &BTreeSet::new()
    ));
    assert!(collection_settled(&checkpoint, false, true, false, 1));
    assert!(!collection_settled(&checkpoint, false, false, false, 1));
    assert!(!collection_settled(&checkpoint, false, true, true, 1));
    checkpoint.unresolved = 2;
    assert!(!collection_settled(&checkpoint, false, true, false, 1));
    checkpoint.unresolved = 1;
    checkpoint
        .pending
        .push(safety::free_item_route("900002").unwrap());
    assert!(!collection_settled(&checkpoint, false, true, false, 1));
}
#[test]
fn settled_free_history_publication_preserves_downloads_and_visible_ownership() {
    let account = "10000000-0000-4000-8000-000000000001";
    for existing_grant in [false, true] {
        let path =
            std::env::temp_dir().join(format!("booth-settled-history-{}", uuid::Uuid::new_v4()));
        let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
        let service = BoothService::new(catalogue.clone()).unwrap();
        let mut job = json!({"id":"authored-warning-job","type":"boothSync","accountId":account,"state":"running"});
        catalogue
            .put("job", "authored-warning-job", job.clone())
            .unwrap();
        catalogue
            .put(
                "account",
                account,
                json!({"id":account,"provider":"booth","name":"Authored account"}),
            )
            .unwrap();
        let entitlement_id = format!("{account}:free:900001");
        let remote_id = format!("{account}:700001:");
        let old_entitlement = json!({"id":entitlement_id,"productId":"900001","accountId":account,"source":"free","fileIds":[remote_id],"manifestComplete":true,"manifestBasis":"verifiedAccountProductFileList"});
        let old_file = json!({"id":remote_id,"productId":"900001","accountId":account,"source":"free","sourceFileId":"700001","name":"authored-owned.zip","revision":"owned-v1"});
        let old_artifact = json!({"id":"authored-ready","productId":"900001","fileId":remote_id,"revision":"owned-v1","status":"ready"});
        if existing_grant {
            catalogue
                .put_many(vec![
                    (
                        "entitlement".into(),
                        entitlement_id.clone(),
                        old_entitlement.clone(),
                    ),
                    ("remote_file".into(), remote_id.clone(), old_file.clone()),
                    (
                        "artifact".into(),
                        "authored-ready".into(),
                        old_artifact.clone(),
                    ),
                ])
                .unwrap();
        }
        let mut row = free_history_page().rows.remove(0);
        // Even historical file hints must not update a current certified grant.
        row.files.push(parser::File {
            id: "700001".into(),
            variant_id: None,
            name: Some("obsolete-hint.zip".into()),
            size_bytes: Some(20),
            source_url: "https://booth.pm/downloadables/700001".into(),
            revision: "obsolete-hint".into(),
            format: None,
        });
        let mut draft = json!({"id":"900001","store":"BOOTH","title":"Authored history name","sourceTags":[],"sourceCategory":null});
        if !existing_grant {
            metadata::apply_public_response(&mut draft, "900001", 404, None).unwrap();
        }
        let entry = ProductPublication {
            account: account.into(),
            source: "free".into(),
            product: "900001".into(),
            routes: BTreeMap::from([(
                row.source_record_id.clone(),
                safety::free_item_route("900001").unwrap(),
            )]),
            rows: vec![row],
            draft,
            files: BTreeSet::new(),
            checkpoint_key: "free".into(),
            force_verify: true,
            needs_enrichment: false,
        };
        let mut checkpoint = Checkpoint::fresh(account, "free", "full").unwrap();
        checkpoint.pending.clear();
        checkpoint.terminal = true;
        checkpoint.head = "authored-head".into();
        checkpoint.unresolved = 1;
        let mut publications = Publications {
            availability: BTreeMap::from([("900001".into(), vec![entry.clone()])]),
            checkpoints: BTreeMap::from([("free".into(), checkpoint)]),
            ..Default::default()
        };
        assert_eq!(published_product_count(&publications), 0);
        let mut not_ready = entry.clone();
        not_ready.needs_enrichment = true;
        assert!(service
            .commit_availability_product(&job, "900001", &[not_ready], &publications.checkpoints)
            .is_err());
        assert!(catalogue.get("product", "900001").unwrap().is_none());
        service
            .commit_availability_product(
                &job,
                "900001",
                std::slice::from_ref(&entry),
                &publications.checkpoints,
            )
            .unwrap();
        publications.availability_published.insert("900001".into());
        assert_eq!(published_product_count(&publications), 1);
        let details = catalogue
            .command("product_details", json!({"id":"900001"}))
            .unwrap();
        assert_eq!(details["accountIds"], json!([account]));
        let query = catalogue
            .command(
                "query",
                json!({"accountIds":[account],"category":details["category"]}),
            )
            .unwrap();
        assert_eq!(query["matchedIds"], json!(["900001"]));
        let acquisition = catalogue
            .get(
                "acquisition",
                &format!("{account}:{}", entry.rows[0].source_record_id),
            )
            .unwrap()
            .unwrap();
        assert_eq!(acquisition["acquiredDateText"], "synthetic-original-date");
        assert_eq!(acquisition["unavailable"], false);
        if existing_grant {
            assert_eq!(
                catalogue.get("entitlement", &entitlement_id).unwrap(),
                Some(old_entitlement)
            );
            assert_eq!(
                catalogue.get("remote_file", &remote_id).unwrap(),
                Some(old_file)
            );
            assert_eq!(
                catalogue.get("artifact", "authored-ready").unwrap(),
                Some(old_artifact)
            );
            assert_eq!(details["manifestComplete"], true);
            assert_eq!(details["status"], "ready");
        } else {
            assert!(catalogue.list("entitlement").unwrap().is_empty());
            assert!(catalogue.list("remote_file").unwrap().is_empty());
            assert!(catalogue.list("artifact").unwrap().is_empty());
            assert_eq!(details["manifestComplete"], false);
            assert_eq!(details["status"], "notDownloaded");
            assert_eq!(details["publicPageAvailability"], "unavailable");
            assert!(details["sourceCategory"].is_null());
        }
        assert!(catalogue.list("checkpoint").unwrap().is_empty());
        assert!(publications.checkpoints["free"]
            .published_products
            .is_empty());
        let coverage_id = format!("{account}:free");
        let coverage = json!({"id":coverage_id,"accountId":account,"source":"free","status":"incomplete","coverageComplete":false,"unresolvedRows":1,"availabilityWarnings":1});
        catalogue
            .put("source_coverage", &coverage_id, coverage.clone())
            .unwrap();
        job["progress"] = publication_progress(&publications, "finalizing", 1, 1);
        catalogue
            .put("job", "authored-warning-job", job.clone())
            .unwrap();
        service.finish_job(&job, None).unwrap();
        let completed = catalogue
            .get("job", "authored-warning-job")
            .unwrap()
            .unwrap();
        assert_eq!(completed["state"], "completed");
        assert_eq!(completed["progress"]["availabilityWarnings"], 1);
        assert_eq!(
            catalogue.get("source_coverage", &coverage_id).unwrap(),
            Some(coverage.clone())
        );
        let selected = BTreeSet::from(["900001".into()]);
        assert!(source_coverage_record(Some(&selected), &coverage_id, coverage).is_none());
        let cancelled = json!({"id":"authored-cancelled","state":"cancelled"});
        catalogue
            .put("job", "authored-cancelled", cancelled.clone())
            .unwrap();
        let before = catalogue.get("product", "900001").unwrap();
        assert_eq!(
            service
                .commit_availability_product(
                    &cancelled,
                    "900001",
                    &[entry],
                    &publications.checkpoints
                )
                .unwrap_err()
                .to_string(),
            "CANCELLED"
        );
        assert_eq!(catalogue.get("product", "900001").unwrap(), before);
        drop(service);
        drop(catalogue);
        std::fs::remove_dir_all(path).unwrap();
    }
}
#[test]
fn settled_free_offer_does_not_block_other_source_or_close_unbound_membership() {
    let path = std::env::temp_dir().join(format!(
        "booth-settled-cross-source-{}",
        uuid::Uuid::new_v4()
    ));
    let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
    let service = BoothService::new(catalogue.clone()).unwrap();
    let job = json!({"id":"authored-cross-source","type":"boothSync","state":"running"});
    catalogue
        .put("job", "authored-cross-source", job.clone())
        .unwrap();
    let account = "authored-account";
    let mut free = Checkpoint::fresh(account, "free", "full").unwrap();
    free.unresolved = 1;
    let (requests, _receiver) = mpsc::unbounded_channel();
    let prefetch = PublicPrefetch {
        requests,
        enqueued: Default::default(),
        support_pages: Default::default(),
        early: Default::default(),
    };
    let mut history = free_history_page();
    history.rows[0].files.push(parser::File {
        id: "700001".into(),
        variant_id: None,
        name: Some("historical-hint.zip".into()),
        size_bytes: None,
        source_url: "https://booth.pm/downloadables/700001".into(),
        revision: "historical".into(),
        format: None,
    });
    // Pending historical controls and confirmed absent offers cannot poison
    // the whole product before a different source's grant is published.
    service
        .stage_discovered_page(
            &job, account, "free", "full", "free", &free, &history, &prefetch,
        )
        .unwrap();
    let mut absent = free_item::Evidence::unknown("900001", "FREE_OFFER_UNAVAILABLE", true)
        .page()
        .unwrap();
    absent.rows = history.rows.clone();
    service
        .stage_discovered_page(
            &job, account, "free", "full", "free", &free, &absent, &prefetch,
        )
        .unwrap();
    assert!(prefetch
        .early
        .lock()
        .unwrap()
        .publications
        .incomplete_scopes
        .is_empty());
    let mut publications = Publications {
        checkpoints: BTreeMap::from([("free".into(), free)]),
        ..Default::default()
    };
    let history_row = history.rows.remove(0);
    let availability = ProductPublication {
        account: account.into(),
        source: "free".into(),
        product: "900001".into(),
        routes: BTreeMap::from([(history_row.source_record_id.clone(), absent.route.clone())]),
        rows: vec![history_row],
        draft: json!({"id":"900001","store":"BOOTH","title":"Authored history","publicMetadataState":"complete"}),
        files: BTreeSet::new(),
        checkpoint_key: "free".into(),
        force_verify: true,
        needs_enrichment: false,
    };
    publications
        .availability
        .insert("900001".into(), vec![availability.clone()]);
    let purchase_row = parser::Row {
        source_record_id: "purchase:910001".into(),
        product_id: Some("900001".into()),
        order_id: Some("910001".into()),
        product_line_total_jpy: Some(100),
        manifest_complete: true,
        files: vec![parser::File {
            id: "700002".into(),
            variant_id: None,
            name: Some("purchased.zip".into()),
            size_bytes: Some(10),
            source_url: "https://booth.pm/downloadables/700002".into(),
            revision: "purchased".into(),
            format: None,
        }],
        ..Default::default()
    };
    let purchase = ProductPublication {
        account: account.into(),
        source: "purchase".into(),
        product: "900001".into(),
        routes: BTreeMap::from([(
            purchase_row.source_record_id.clone(),
            "/orders/910001".into(),
        )]),
        rows: vec![purchase_row],
        draft: availability.draft.clone(),
        files: BTreeSet::from([format!("{account}:700002:")]),
        checkpoint_key: "purchase".into(),
        force_verify: true,
        needs_enrichment: false,
    };
    publications.checkpoints.insert(
        "purchase".into(),
        Checkpoint::fresh(account, "purchase", "full").unwrap(),
    );
    remember_product_context(&mut publications, purchase.clone());
    assert!(publication_membership_proven(
        &publications.products["900001"],
        &BTreeSet::new()
    ));
    assert!(!publication_membership_proven(
        &publications.products["900001"],
        &BTreeSet::from([(account.into(), "free".into())])
    ));
    assert_eq!(
        publication_progress(&publications, "enriching", 1, 0)["metadataTotal"],
        1
    );
    assert_eq!(
        publication_progress(&publications, "enriching", 1, 0)["availabilityWarnings"],
        1
    );
    service
        .commit_availability_product(&job, "900001", &[availability], &publications.checkpoints)
        .unwrap();
    publications.availability_published.insert("900001".into());
    assert_eq!(published_product_count(&publications), 0); // Purchase still pending.
    service
        .commit_product_group(&job, "900001", &[purchase], &mut publications.checkpoints)
        .unwrap();
    assert_eq!(published_product_count(&publications), 1);
    assert!(catalogue
        .get("entitlement", &format!("{account}:free:900001"))
        .unwrap()
        .is_none());
    assert_eq!(
        catalogue
            .get("entitlement", &format!("{account}:purchase:900001"))
            .unwrap()
            .unwrap()["manifestComplete"],
        true
    );
    assert!(catalogue
        .get("remote_file", &format!("{account}:700001:"))
        .unwrap()
        .is_none());
    assert!(publications.checkpoints["free"]
        .published_products
        .is_empty());
    let free_draft = format!(
        "booth-product:{}:900001",
        publications.checkpoints["free"].run_id
    );
    assert!(catalogue.get("checkpoint", &free_draft).unwrap().is_none());
    let mut unknown = absent;
    unknown.free_item = Some(free_item::Evidence::unknown(
        "900001",
        "FREE_FILE_CONTROLS_UNAVAILABLE",
        false,
    ));
    service
        .stage_discovered_page(
            &job,
            account,
            "free",
            "full",
            "free",
            &publications.checkpoints["free"],
            &unknown,
            &prefetch,
        )
        .unwrap();
    assert!(prefetch
        .early
        .lock()
        .unwrap()
        .publications
        .incomplete_scopes
        .contains(&(account.into(), "free".into(), "900001".into())));
    drop(prefetch);
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn incomplete_gift_grant_does_not_hide_paid_receipt_in_either_arrival_order() {
    for gift_first in [true, false] {
        let path =
            std::env::temp_dir().join(format!("booth-independent-grants-{}", uuid::Uuid::new_v4()));
        let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
        let service = BoothService::new(catalogue.clone()).unwrap();
        let account = "authored-account";
        let job = json!({"id":"authored-independent-grants","type":"boothSync","state":"running"});
        catalogue
            .put("job", "authored-independent-grants", job.clone())
            .unwrap();
        let (requests, _receiver) = mpsc::unbounded_channel();
        let prefetch = PublicPrefetch {
            requests,
            enqueued: Default::default(),
            support_pages: Default::default(),
            early: Default::default(),
        };
        let gift = Checkpoint::fresh(account, "gift", "full").unwrap();
        let purchase = Checkpoint::fresh(account, "purchase", "full").unwrap();
        let file = |id: &str, variant: &str| parser::File {
            id: id.into(),
            variant_id: Some(variant.into()),
            name: Some(format!("authored-{id}.zip")),
            size_bytes: Some(10),
            source_url: format!("https://booth.pm/downloadables/{id}?variation_id={variant}"),
            revision: "authored".into(),
            format: None,
        };
        let gift_row = parser::Row {
            source_record_id: "gift:910002".into(),
            product_id: Some("900001".into()),
            name: Some("Authored product".into()),
            files: vec![file("700001", "800001")],
            manifest_complete: false,
            error_code: Some("DIGITAL_CONTROLS_UNRESOLVED".into()),
            ..Default::default()
        };
        let complete_gift_row = parser::Row {
            source_record_id: "gift:910003".into(),
            product_id: Some("900001".into()),
            name: Some("Authored product".into()),
            files: vec![file("700003", "800003")],
            manifest_complete: true,
            ..Default::default()
        };
        let purchase_row = parser::Row {
            source_record_id: "purchase:910001".into(),
            product_id: Some("900001".into()),
            order_id: Some("910001".into()),
            name: Some("Authored product".into()),
            acquired_date_text: Some("2026/10/10 09:30".into()),
            product_line_total_jpy: Some(900),
            files: vec![file("700002", "800002")],
            manifest_complete: true,
            ..Default::default()
        };
        let page = |source: &str, route: &str, row: parser::Row| parser::Page {
            source: source.into(),
            route: route.into(),
            rows: vec![row],
            next: vec![],
            details: vec![],
            terminal: true,
            unresolved: 0,
            fingerprint: "authored".into(),
            is_detail: true,
            ordering: parser::Ordering::Unknown,
            free_item: None,
        };
        let mut bad = page("gift", "/gifts?page=1", gift_row.clone());
        bad.rows.push(complete_gift_row);
        let paid = page("purchase", "/orders/910001", purchase_row);
        let stage = |source: &str, checkpoint: &Checkpoint, page: &parser::Page| {
            service
                .stage_discovered_page(
                    &job, account, source, "full", source, checkpoint, page, &prefetch,
                )
                .unwrap();
        };
        if gift_first {
            stage("gift", &gift, &bad);
            stage("purchase", &purchase, &paid);
        } else {
            stage("purchase", &purchase, &paid);
            stage("gift", &gift, &bad);
        }
        let paid_acquisition = catalogue
            .get("acquisition", "authored-account:purchase:910001")
            .unwrap()
            .unwrap();
        assert_eq!(paid_acquisition["paidPrice"], 900);
        assert_eq!(paid_acquisition["acquiredDateText"], "2026/10/10 09:30");
        assert_eq!(paid_acquisition["variantIds"], json!(["800002"]));
        assert!(catalogue
            .get("remote_file", "authored-account:700002:800002")
            .unwrap()
            .is_some());
        assert!(catalogue
            .get("remote_file", "authored-account:700001:800001")
            .unwrap()
            .is_none());
        assert!(catalogue
            .get("remote_file", "authored-account:700003:800003")
            .unwrap()
            .is_some());
        assert_eq!(
            catalogue
                .get("entitlement", "authored-account:gift:900001")
                .unwrap()
                .unwrap()["manifestComplete"],
            false
        );
        assert!(prefetch
            .early
            .lock()
            .unwrap()
            .publications
            .incomplete_scopes
            .contains(&(account.into(), "gift".into(), "900001".into())));
        {
            let early = prefetch.early.lock().unwrap();
            let mut closed_purchase = purchase.clone();
            closed_purchase.pending.clear();
            closed_purchase.terminal = true;
            let final_scope = Publications {
                checkpoints: BTreeMap::from([
                    ("purchase".into(), closed_purchase),
                    ("gift".into(), gift.clone()),
                ]),
                incomplete_scopes: early.publications.incomplete_scopes.clone(),
                ..Default::default()
            };
            let entries = &early.publications.products["900001"];
            assert!(publication_entry_certifiable(
                entries
                    .iter()
                    .find(|entry| entry.source == "purchase")
                    .unwrap(),
                &final_scope,
            ));
            assert!(!publication_entry_certifiable(
                entries.iter().find(|entry| entry.source == "gift").unwrap(),
                &final_scope,
            ));
        }
        let mut complete_gift = gift_row;
        complete_gift.manifest_complete = true;
        complete_gift.error_code = None;
        stage("gift", &gift, &page("gift", "/gifts?page=1", complete_gift));
        assert_eq!(catalogue.related("acquisition", "900001").unwrap().len(), 3);
        assert_eq!(catalogue.related("remote_file", "900001").unwrap().len(), 3);
        assert_eq!(
            catalogue
                .get("acquisition", "authored-account:purchase:910001")
                .unwrap()
                .unwrap()["paidPrice"],
            900
        );
        drop(prefetch);
        drop(service);
        drop(catalogue);
        std::fs::remove_dir_all(path).unwrap();
    }
}
#[test]
fn late_metadata_keeps_complete_receipt_from_mixed_incomplete_product() {
    let path =
        std::env::temp_dir().join(format!("booth-partial-purchase-{}", uuid::Uuid::new_v4()));
    let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
    let service = BoothService::new(catalogue.clone()).unwrap();
    let job = json!({"id":"authored-partial-purchase","type":"boothSync","state":"running"});
    catalogue
        .put("job", "authored-partial-purchase", job.clone())
        .unwrap();
    let account = "authored-account";
    let key = format!("booth:{account}:purchase");
    let mut checkpoint = Checkpoint::fresh(account, "purchase", "full").unwrap();
    checkpoint.pending.clear();
    checkpoint.terminal = true;
    checkpoint.unresolved = 1;
    checkpoint.product_ids.insert("900002".into());
    let incomplete_row = parser::Row {
        source_record_id: "purchase:910001".into(),
        product_id: Some("900002".into()),
        order_id: Some("910001".into()),
        files: vec![parser::File {
            id: "700001".into(),
            variant_id: Some("800001".into()),
            name: Some("unresolved.zip".into()),
            size_bytes: None,
            source_url: "https://booth.pm/downloadables/700001?variation_id=800001".into(),
            revision: "unknown".into(),
            format: None,
        }],
        manifest_complete: false,
        error_code: Some("DIGITAL_CONTROLS_UNRESOLVED".into()),
        ..Default::default()
    };
    let row = parser::Row {
        source_record_id: "purchase:910002".into(),
        product_id: Some("900002".into()),
        order_id: Some("910002".into()),
        name: Some("Authored paid item".into()),
        acquired_date_text: Some("2026/10/10 09:30".into()),
        product_line_total_jpy: Some(900),
        files: vec![parser::File {
            id: "700002".into(),
            variant_id: Some("800002".into()),
            name: Some("authored.zip".into()),
            size_bytes: Some(10),
            source_url: "https://booth.pm/downloadables/700002?variation_id=800002".into(),
            revision: "authored".into(),
            format: None,
        }],
        manifest_complete: true,
        ..Default::default()
    };
    let ancillary_row = parser::Row {
        source_record_id: "purchase:910003".into(),
        product_id: Some("900002".into()),
        order_id: Some("910003".into()),
        files: vec![],
        manifest_complete: false,
        ..Default::default()
    };
    let observed = observed_grant_rows(
        "purchase",
        &[&incomplete_row, &row, &ancillary_row],
        false,
        false,
    );
    assert_eq!(observed.len(), 2);
    assert!(observed
        .iter()
        .any(|row| row.source_record_id == "purchase:910002"));
    assert!(observed
        .iter()
        .any(|row| row.source_record_id == "purchase:910003"));
    let entry = ProductPublication {
        account: account.into(),
        source: "purchase".into(),
        product: "900002".into(),
        rows: observed.into_iter().cloned().collect(),
        routes: BTreeMap::from([
            ("purchase:910002".into(), "/orders/910002".into()),
            ("purchase:910003".into(), "/orders/910003".into()),
        ]),
        draft: json!({"id":"900002","store":"BOOTH","title":"Authored paid item"}),
        files: BTreeSet::from([format!("{account}:700002:800002")]),
        checkpoint_key: key.clone(),
        force_verify: true,
        needs_enrichment: true,
    };
    let mut publications = Publications {
        products: BTreeMap::from([("900002".into(), vec![entry.clone()])]),
        checkpoints: BTreeMap::from([(key.clone(), checkpoint)]),
        incomplete_scopes: BTreeSet::from([(account.into(), "purchase".into(), "900002".into())]),
        ..Default::default()
    };
    let (authoritative, additive) = ready_product_entries(&publications, "900002");
    assert!(authoritative.is_empty());
    assert_eq!(additive.len(), 1);
    assert!(additive[0].needs_enrichment);
    service
        .commit_product_group_mode(
            &job,
            "900002",
            &additive,
            &mut publications.checkpoints,
            false,
        )
        .unwrap();
    assert_eq!(
        catalogue
            .get("acquisition", "authored-account:purchase:910002")
            .unwrap()
            .unwrap()["paidPrice"],
        900
    );
    assert_eq!(
        catalogue
            .get("entitlement", "authored-account:purchase:900002")
            .unwrap()
            .unwrap()["manifestComplete"],
        false
    );
    assert!(catalogue
        .get("remote_file", "authored-account:700001:800001")
        .unwrap()
        .is_none());
    assert!(publications.checkpoints[&key].published_products.is_empty());
    publications.products.get_mut("900002").unwrap()[0].draft["publicMetadataState"] =
        json!("complete");
    publications.products.get_mut("900002").unwrap()[0].needs_enrichment = false;
    let (authoritative, additive) = ready_product_entries(&publications, "900002");
    assert!(authoritative.is_empty());
    assert_eq!(additive.len(), 1);
    service
        .commit_product_group_mode(
            &job,
            "900002",
            &additive,
            &mut publications.checkpoints,
            false,
        )
        .unwrap();
    assert_eq!(
        catalogue
            .get("acquisition", "authored-account:purchase:910002")
            .unwrap()
            .unwrap()["paidPrice"],
        900
    );
    assert!(catalogue
        .get("acquisition", "authored-account:purchase:910003")
        .unwrap()
        .is_some());
    assert!(catalogue
        .get("remote_file", "authored-account:700002:800002")
        .unwrap()
        .is_some());
    assert_eq!(
        catalogue
            .get("entitlement", "authored-account:purchase:900002")
            .unwrap()
            .unwrap()["manifestComplete"],
        false
    );
    assert!(catalogue
        .get("remote_file", "authored-account:700001:800001")
        .unwrap()
        .is_none());
    assert!(publications.checkpoints[&key].published_products.is_empty());
    let prior = additive_version(&additive).unwrap();
    let mut changed = additive[0].clone();
    changed
        .routes
        .insert("purchase:910002".into(), "/orders/910003".into());
    assert_ne!(prior, additive_version(&[changed.clone()]).unwrap());
    changed = additive[0].clone();
    changed.files.insert(format!("{account}:700003:800003"));
    assert_ne!(prior, additive_version(&[changed]).unwrap());
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn free_history_witness_and_augmented_rows_preserve_original_acquisition_facts() {
    let page = free_history_page();
    let mut history = BTreeMap::new();
    remember_free_history(&page, &mut history);
    assert_eq!(history.len(), 1);
    assert_eq!(history["900001"].len(), 1);
    remember_free_history(&page, &mut history);
    assert_eq!(history["900001"].len(), 1);
    let mut row = page.rows[0].clone();
    let before = serde_json::to_value(&row).unwrap();
    let evidence = free_item::Evidence {
        product_id: "900001".into(),
        files: vec![parser::File {
            id: "700001".into(),
            variant_id: Some("800001".into()),
            name: Some("authored.zip".into()),
            size_bytes: None,
            source_url: "https://booth.pm/downloadables/700001?variation_id=800001".into(),
            revision: "unknown".into(),
            format: None,
        }],
        manifest_complete: true,
        unavailable: false,
        error_code: None,
    };
    augment_free_row(&mut row, &evidence);
    assert!(row.manifest_complete);
    assert_eq!(row.files.len(), 1);
    let after = serde_json::to_value(&row).unwrap();
    for (key, value) in before.as_object().unwrap() {
        if !matches!(key.as_str(), "files" | "manifestComplete") {
            assert_eq!(&after[key], value);
        }
    }
    augment_free_row(
        &mut row,
        &free_item::Evidence::unknown("900001", "FREE_FILE_CONTROLS_UNAVAILABLE", false),
    );
    assert!(!row.manifest_complete);
    assert_eq!(row.files.len(), 1);
    let mut mismatched = page.rows[0].clone();
    augment_free_row(
        &mut mismatched,
        &free_item::Evidence {
            product_id: "900002".into(),
            ..evidence
        },
    );
    assert!(mismatched.files.is_empty());
    let mut rejected = page.clone();
    rejected.rows[0].error_code = Some("SYNTHETIC_UNSUPPORTED".into());
    assert_eq!(eligible_free_rows(&rejected).count(), 0);
    let mut rejected = page.clone();
    rejected.rows[0].unavailable = true;
    assert_eq!(eligible_free_rows(&rejected).count(), 0);
    let mut rejected = page.clone();
    rejected.source = "library".into();
    assert_eq!(eligible_free_rows(&rejected).count(), 0);
    let mut rejected = page.clone();
    rejected.is_detail = true;
    assert_eq!(eligible_free_rows(&rejected).count(), 0);
}
/// A free item added by its listing takes the record ID its free-history row will have, so
/// the sync that later reads the history merges with it instead of adding a second row.
#[test]
fn a_free_item_added_by_its_listing_takes_its_history_rows_record() {
    let route = "/library/free_downloads?page=1";
    let html = format!("<html><head><meta name='js_const_user_signed_in' content='true'></head><body><main role='main'><nav><a class='nav-item current' href='https://accounts.booth.pm{route}'>Authored tab</a></nav><article data-test='library-item'><a href='https://invented.booth.pm/items/900001'>Authored</a><div data-test='downloadable-list'><b data-test='filename'>authored.zip</b><a href='https://booth.pm/downloadables/700001?variation_id=800001'>Download</a></div></article><div class='pager border-0'><nav><ul><li class='current'><a class='nav-item' href='https://accounts.booth.pm{route}'>1</a></li></ul></nav></div></main></body></html>");
    let page = parser::parse(&html, "free", route).unwrap();
    assert_eq!(page.rows.len(), 1);
    assert_eq!(page.rows[0].product_id.as_deref(), Some("900001"));
    assert_eq!(
        page.rows[0].source_record_id,
        free_history::free_history_record_id("900001")
    );
}
#[test]
fn undated_free_downloads_keep_the_order_of_the_pass_that_first_listed_them() {
    let path = std::env::temp_dir().join(format!("booth-free-order-{}", uuid::Uuid::new_v4()));
    let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
    let service = BoothService::new(catalogue.clone()).unwrap();
    let row = |id: &str| parser::Row {
        source_record_id: format!("free:{id}"),
        product_id: Some(id.into()),
        name: Some("Authored free item".into()),
        ..parser::Row::default()
    };
    let page = |number: u32, ids: &[&str]| parser::Page {
        route: format!("/library/free_downloads?page={number}"),
        rows: ids.iter().map(|id| row(id)).collect(),
        ..free_history_page()
    };
    // BOOTH lists the newest download first, page after page.
    let positions: Vec<_> = [page(1, &["900002", "900001"]), page(2, &["900003"])]
        .iter()
        .flat_map(free_history_positions)
        .collect();
    assert_eq!(
        positions,
        [
            ("free:900002".to_owned(), 0),
            ("free:900001".to_owned(), 1),
            ("free:900003".to_owned(), 1000)
        ]
    );
    // An item's own page or another list has no place in the free history.
    let detail = parser::Page {
        is_detail: true,
        ..page(1, &["900001"])
    };
    let library = parser::Page {
        source: "library".into(),
        route: "/library?page=1".into(),
        ..page(1, &["900001"])
    };
    assert!(free_history_positions(&detail).is_empty());
    assert!(free_history_positions(&library).is_empty());
    let mut orders: BTreeMap<_, _> = positions
        .iter()
        .map(|(record, position)| {
            let order = free_history_order("2026-11-01T09:00:00.123+09:00", *position).unwrap();
            (format!("a:{record}"), order)
        })
        .collect();
    assert!(orders["a:free:900002"] > orders["a:free:900001"]);
    assert!(orders["a:free:900001"] > orders["a:free:900003"]);
    // A later pass sorts after everything an earlier one listed, wherever its rows are.
    assert!(free_history_order("2026-11-02T00:00:00Z", 5000).unwrap() > orders["a:free:900002"]);
    for id in ["900001", "900002", "900003"] {
        let acquisition = format!("a:free:{id}");
        catalogue.put("acquisition", &acquisition, json!({"id":acquisition,"sourceRecordId":format!("free:{id}"),"productId":id,"accountId":"a","source":"free","variantIds":[]})).unwrap();
    }
    // An order once given stays; a purchase or a record that does not exist gets none.
    let kept = "20261020000000999999999999";
    catalogue.put("acquisition","a:free:900003",json!({"id":"a:free:900003","sourceRecordId":"free:900003","productId":"900003","accountId":"a","source":"free","variantIds":[],"freeHistoryOrder":kept})).unwrap();
    catalogue.put("acquisition","a:purchase:900001",json!({"id":"a:purchase:900001","productId":"900001","accountId":"a","source":"purchase","variantIds":[]})).unwrap();
    orders.insert("a:purchase:900001".into(), kept.into());
    orders.insert("a:free:900009".into(), kept.into());
    let mut batch = crate::booth::RecordBatch::new(&catalogue);
    assert_eq!(
        stage_free_orders(&mut batch, &orders).unwrap(),
        BTreeSet::from(["900001".to_owned(), "900002".to_owned()])
    );
    batch.commit().unwrap();
    let order =
        |id: &str| catalogue.get("acquisition", id).unwrap().unwrap()["freeHistoryOrder"].clone();
    assert_eq!(order("a:free:900002"), json!(orders["a:free:900002"]));
    assert_eq!(order("a:free:900003"), json!(kept));
    assert!(order("a:purchase:900001").is_null());
    // The next observation of the row rebuilds the record and keeps its order.
    service
        .record_row(
            "a",
            "free",
            &row("900001"),
            "https://booth.pm/ja/items/900001",
            false,
        )
        .unwrap();
    assert_eq!(order("a:free:900001"), json!(orders["a:free:900001"]));
    let _ = std::fs::remove_dir_all(path);
}
#[test]
fn current_free_checkpoint_roundtrips_and_does_not_upgrade_missing_version() {
    assert_eq!(FREE_ITEM_ADAPTER_VERSION, 3);
    let checkpoint = Checkpoint::fresh("synthetic", "free", "full").unwrap();
    assert_eq!(
        checkpoint.free_item_adapter_version,
        FREE_ITEM_ADAPTER_VERSION
    );
    let mut old = serde_json::to_value(&checkpoint).unwrap();
    old.as_object_mut()
        .unwrap()
        .remove("freeItemAdapterVersion");
    assert!(serde_json::from_value::<Checkpoint>(old).is_err());
    let mut legacy = serde_json::to_value(&checkpoint).unwrap();
    legacy["freeItemAdapterVersion"] = json!(1);
    let restored: Checkpoint = serde_json::from_value(legacy).unwrap();
    assert_eq!(restored.free_item_adapter_version, 1);
    assert_ne!(
        restored.free_item_adapter_version,
        FREE_ITEM_ADAPTER_VERSION
    );
    assert_eq!(
        Checkpoint::fresh("synthetic", "library", "full")
            .unwrap()
            .free_item_adapter_version,
        0
    );
    let mut old = serde_json::to_value(free_history_page()).unwrap();
    old.as_object_mut().unwrap().remove("freeItem");
    assert!(serde_json::from_value::<parser::Page>(old)
        .unwrap()
        .free_item
        .is_none());
    let pending = vec![
        safety::free_item_route("900001").unwrap(),
        "/library/free_downloads?page=2".into(),
    ];
    assert_eq!(
        next_route(&pending).unwrap(),
        "/library/free_downloads?page=2"
    );
}
#[test]
fn free_history_journals_cannot_cross_account_source_or_current_run() {
    let journal = json!({"accountId":"synthetic-account","source":"free","runId":"synthetic-run","page":free_history_page()});
    assert!(checked_journal_page(&journal, "synthetic-account", "free", "synthetic-run").is_ok());
    for (account, source, run) in [
        ("other-account", "free", "synthetic-run"),
        ("synthetic-account", "library", "synthetic-run"),
        ("synthetic-account", "free", "old-run"),
    ] {
        assert_eq!(
            checked_journal_page(&journal, account, source, run)
                .unwrap_err()
                .to_string(),
            "CHECKPOINT_INVALID"
        );
    }
}
#[test]
fn checkpoint_completion_requires_terminal_and_all_rows() {
    let mut c = Checkpoint::fresh("synthetic", "free", "full").unwrap();
    assert!(!c.complete());
    c.pending.clear();
    c.terminal = true;
    assert!(c.complete());
    c.unresolved = 1;
    assert!(!c.complete());
    c.unresolved = 0;
    c.enrichment_pending.insert("900001".into());
    assert!(!c.complete());
}
#[test]
fn transient_public_failure_waits_for_one_later_attempt() {
    let request = |product: &str| PublicRequest {
        product: product.into(),
        draft: json!({"id":product}),
        account: "authored-account".into(),
        source: "library".into(),
        run_id: "authored-run".into(),
    };
    let failed = |code: &str| Err::<Value, String>(code.into());
    let mut retry = PublicRetry::default();
    assert_eq!(
        retry
            .settle(request("900001"), Ok(json!({"title":"Authored"})))
            .len(),
        1
    );
    assert_eq!(
        retry.settle(request("900002"), failed("PUBLIC_METADATA_INVALID")),
        vec![("900002".into(), failed("PUBLIC_METADATA_INVALID"))]
    );
    for (product, code) in [
        ("900003", "NETWORK_UNAVAILABLE"),
        ("900004", "PUBLIC_METADATA_FAILED"),
        ("900005", "NETWORK_UNAVAILABLE"),
    ] {
        assert!(retry.settle(request(product), failed(code)).is_empty());
    }
    let next = retry.next().unwrap();
    assert_eq!(next.product, "900003");
    assert_eq!(
        retry.settle(next, Ok(json!({"title":"Recovered"}))).len(),
        1
    );
    // A repeated failure settles the rest without another request.
    let next = retry.next().unwrap();
    assert_eq!(next.product, "900004");
    assert_eq!(
        retry.settle(next, failed("NETWORK_UNAVAILABLE")),
        vec![
            ("900004".into(), failed("NETWORK_UNAVAILABLE")),
            ("900005".into(), failed("NETWORK_UNAVAILABLE")),
        ]
    );
    assert!(retry.next().is_none());
}
#[test]
fn incremental_public_refresh_skips_settled_and_removed_pages() {
    assert!(public_refresh_needed(None));
    for (product, needed) in [
        (json!({"id":"900001"}), true),
        (json!({"publicPageAvailability":"unknown"}), true),
        (
            json!({"publicMetadataObservedAt":"2026-10-30T00:00:00Z","publicMetadataState":"complete"}),
            false,
        ),
        (
            json!({"publicMetadataObservedAt":"2026-10-30T00:00:00Z","publicMetadataState":"incomplete"}),
            true,
        ),
        (
            json!({"publicPageAvailability":"unavailable","publicMetadataErrorCode":"PUBLIC_METADATA_UNAVAILABLE"}),
            false,
        ),
        (
            json!({"publicPageAvailability":"unavailable","publicMetadataState":"incomplete"}),
            true,
        ),
    ] {
        assert_eq!(public_refresh_needed(Some(&product)), needed, "{product}");
    }
}
#[test]
fn source_queue_numeric_not_lexical() {
    assert_eq!(
        next_route(&[
            "/library?page=10".into(),
            "/library?page=2".into(),
            "/orders/1".into()
        ])
        .unwrap(),
        "/library?page=2"
    );
}
#[test]
fn resume_overlap_guards_shifted_head_and_boundary() {
    let head = parser::Page {
        source: "library".into(),
        route: "/library?page=1".into(),
        rows: vec![],
        next: vec![],
        details: vec![],
        terminal: false,
        unresolved: 0,
        fingerprint: "head".into(),
        is_detail: false,
        ordering: parser::Ordering::Unknown,
        free_item: None,
    };
    let mut boundary = head.clone();
    boundary.route = "/library?page=2".into();
    boundary.fingerprint = "boundary".into();
    let mut c = Checkpoint::fresh("synthetic", "library", "full").unwrap();
    c.head = "head".into();
    c.boundary = Some(boundary.route.clone());
    c.committed
        .insert(boundary.route.clone(), boundary.fingerprint.clone());
    assert!(!overlap_changed(&c, &head, Some(&boundary)));
    assert!(overlap_changed(&c, &head, None));
    boundary.fingerprint = "shifted".into();
    assert!(overlap_changed(&c, &head, Some(&boundary)));
    c.head = "old-head".into();
    assert!(overlap_changed(&c, &head, Some(&boundary)));
}
#[test]
fn the_updated_order_adds_to_a_library_pass_without_changing_its_order_or_head() {
    let page = |route: String, ordering, next: Vec<String>| parser::Page {
        source: "library".into(),
        route,
        rows: vec![],
        next,
        details: vec![],
        terminal: false,
        unresolved: 0,
        fingerprint: "authored".into(),
        is_detail: false,
        ordering,
        free_item: None,
    };
    let acquired = parser::Ordering::AcquiredNewestFirst;
    let updated = parser::Ordering::FileUpdatedNewestFirst;
    let mut c = Checkpoint::fresh("synthetic", "library", "incremental").unwrap();
    for number in 1..=2 {
        let route = format!("/library?page={number}");
        let next = vec![format!("/library?page={}", number + 1)];
        record_source_page(
            &mut c,
            "library",
            &route,
            &page(route.clone(), acquired, next),
            None,
        )
        .unwrap();
    }
    let head = c.head.clone();
    // The overlap held: the plain pages stop and the updated order starts.
    c.pending = vec![safety::updated_route(1)];
    c.terminal = true;
    let first = parser::Page {
        fingerprint: "sorted".into(),
        unresolved: 1,
        ..page(
            safety::updated_route(1),
            updated,
            vec![safety::updated_route(2), safety::updated_route(9)],
        )
    };
    record_source_page(&mut c, "library", &safety::updated_route(1), &first, None).unwrap();
    assert_eq!(c.ordering, acquired);
    assert_eq!(c.head, head);
    assert!(c.terminal);
    assert_eq!(c.unresolved, 0);
    assert_eq!(next_routes(&c.pending).unwrap(), [safety::updated_route(2)]);
    // Only what is about this order gives up on it, and only on this order.
    for code in [
        "SOURCE_HTTP_FAILED",
        "RESPONSE_ROUTE_CHANGED",
        "NO_RECOGNIZED_ROWS",
    ] {
        assert!(
            refused_updated_order(&safety::updated_route(1), &anyhow::anyhow!("{code}")),
            "{code}"
        );
        assert!(
            !refused_updated_order("/library?page=3", &anyhow::anyhow!("{code}")),
            "{code}"
        );
    }
    for code in [
        "AUTH_REQUIRED",
        "AUTH_CHALLENGE",
        "RATE_LIMITED",
        "NETWORK_UNAVAILABLE",
        "CANCELLED",
    ] {
        assert!(
            !refused_updated_order(&safety::updated_route(1), &anyhow::anyhow!("{code}")),
            "{code}"
        );
    }
    // Something did not fit: the plain pages go on from where they stopped, and stay on.
    read_every_page(&mut c);
    assert!(c.read_every_page && !c.terminal && c.overlap_end.is_none());
    assert_eq!(c.pending, ["/library?page=3"]);
    let saved = serde_json::to_value(&c).unwrap();
    assert_eq!(saved["readEveryPage"], true);
    assert!(
        serde_json::from_value::<Checkpoint>(saved)
            .unwrap()
            .read_every_page
    );
    // A checkpoint saved before this field existed still loads.
    let mut older =
        serde_json::to_value(Checkpoint::fresh("synthetic", "library", "incremental").unwrap())
            .unwrap();
    assert!(older.get("readEveryPage").is_none());
    older["ordering"] = serde_json::json!("unknown");
    assert!(
        !serde_json::from_value::<Checkpoint>(older)
            .unwrap()
            .read_every_page
    );
}
#[test]
fn revision_is_content_evidence_not_signed_url_or_clock() {
    let mut f = parser::File {
        id: "700001".into(),
        variant_id: None,
        name: Some("authored.zip".into()),
        size_bytes: Some(9),
        source_url: "https://booth.pm/downloadables/700001".into(),
        revision: "unknown".into(),
        format: None,
    };
    let old = json!({"sourceFileId":"700001","variantId":null,"name":"authored.zip","sizeBytes":9,"revision":"sha256:positive-content","revisionBasis":"contentHash"});
    assert_eq!(
        revision_observation(Some(&old), &f, false),
        (
            "sha256:positive-content".into(),
            "contentHash".into(),
            false
        )
    );
    assert!(revision_observation(Some(&old), &f, true).2);
    f.size_bytes = Some(10);
    assert_eq!(revision_observation(Some(&old), &f, false).0, "unknown");
    f.revision = "source:actual-provider-revision".into();
    assert!(!revision_observation(Some(&old), &f, true).2);
}
