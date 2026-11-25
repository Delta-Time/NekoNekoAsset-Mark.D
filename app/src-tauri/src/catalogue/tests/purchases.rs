//! Acquisition dates, prices, variants, account scopes and spending.
use super::*;

#[test]
fn acquisition_dates_accept_complete_provider_timestamps_without_rewriting_raw_text() {
    for (raw, expected) in [
        ("2026/2/1 09:08:07", "2026-02-01T09:08:07.000000000Z"),
        ("2026/2/1 09:08", "2026-02-01T09:08:00.000000000Z"),
        ("2026/2/1", "2026-02-01T00:00:00.000000000Z"),
    ] {
        assert_eq!(acquisition::date_key(raw).as_deref(), Some(expected));
    }
    for invalid in [
        "2026/2/30 09:08:07",
        "2026/2/1 25:08:07",
        "2026/2/1 09:99",
        "receipt 2026/2/1 09:08:07",
        "2026/2/1 09:08:07 trailing",
    ] {
        assert!(acquisition::date_key(invalid).is_none(), "{invalid}");
    }
    let raw = json!({"id":"a","accountId":"a","acquiredDateText":"","acquiredAt":"2026-02-01T09:08:07Z","paidAmountText":"Original provider text"});
    let mut p = json!({"provenance":acquisition::normalize_provenance(&[raw.clone()])});
    acquisition::apply_default(&mut p, "", None);
    assert_eq!(p["acquired"], raw["acquiredAt"]);
    for field in [
        "id",
        "accountId",
        "acquiredDateText",
        "acquiredAt",
        "paidAmountText",
    ] {
        assert_eq!(p["provenance"][0][field], raw[field]);
    }
    let mut p = json!({"provenance":acquisition::normalize_provenance(&[
        json!({"id":"z","acquiredDateText":"Later provider text"}),
        json!({"id":"a","acquiredDateText":"Unparsed provider text"}),
    ])});
    acquisition::apply_default(&mut p, "", None);
    assert_eq!(p["acquired"], "Unparsed provider text");
    assert!(p["acquiredSortKey"].is_null());
    // Equal dates use the stable acquisition id, not input traversal order.
    p["provenance"] = json!(acquisition::normalize_provenance(&[
        json!({"id":"z","acquiredDateText":"2026/2/1"}),
        json!({"id":"a","acquiredDateText":"2026年2月1日"}),
    ]));
    acquisition::apply_default(&mut p, "", None);
    assert_eq!(p["acquired"], "2026年2月1日");
}

#[test]
fn an_added_time_sorts_by_the_clock_it_shows() {
    // Store times in UTC key as before; a time with an offset keys by its own clock.
    for (raw, expected) in [
        (
            "2026-10-08T01:02:03+00:00",
            "2026-10-08T01:02:03.000000000Z",
        ),
        ("2026-11-23T05:05:07.123Z", "2026-11-23T05:05:07.123000000Z"),
        (
            "2026-11-23T20:00:00.001+09:00",
            "2026-11-23T20:00:00.001000000Z",
        ),
    ] {
        assert_eq!(acquisition::date_key(raw).as_deref(), Some(expected));
    }
    // Added in Japan at 20:00, it sorts after a receipt from 15:00 the same day.
    let rows = acquisition::normalize_provenance(&[
        json!({"id":"receipt","acquiredDateText":"2026/11/23 15:00"}),
        json!({"id":"added","acquiredDateText":"2026/11/23 20:00","acquiredAt":"2026-11-23T20:00:00.000+09:00"}),
    ]);
    assert!(rows[1]["acquiredSortKey"].as_str() > rows[0]["acquiredSortKey"].as_str());
    let mut local = json!({"provenance":[]});
    acquisition::apply_default(
        &mut local,
        "2026/11/23 20:00",
        Some("2026-11-23T20:00:00.001+09:00"),
    );
    assert_eq!(local["acquired"], "2026/11/23 20:00");
    assert!(local["acquiredSortKey"].as_str() > rows[0]["acquiredSortKey"].as_str());
    // A free BOOTH item is added on BOOTH's clock, whatever this device's.
    let at = chrono::DateTime::parse_from_rfc3339("2026-11-23T11:00:07.123Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let (shown, time) = acquisition::added_time(at, &acquisition::booth_clock());
    assert_eq!(shown, "2026/11/23 20:00");
    assert_eq!(time, "2026-11-23T20:00:07.123+09:00");
    // A local import uses this device's, and keys within the minute it shows.
    let (shown, time) = acquisition::added_time(at, &chrono::Local);
    let shown_key = acquisition::date_key(&shown).unwrap();
    let time_key = acquisition::date_key(&time).unwrap();
    assert_eq!(time_key[..16], shown_key[..16]);
    assert!(time_key > shown_key);
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(&time).unwrap(),
        at,
        "the same instant"
    );
}

#[test]
fn paid_price_requires_unambiguous_scoped_item_evidence_and_preserves_zero() {
    let proof = |id: &str, account: &str, amount: Value, currency: &str| json!({"id":id,"accountId":account,"paidPrice":amount,"paidCurrency":currency,"paidPriceBasis":"productLineTotal"});
    let unproven = json!({"id":"raw","accountId":"a","paidAmountText":"¥ 900","paidPrice":900,"paidCurrency":"JPY","orderTotal":900});
    let mut p = json!({"price":999,"provenance":acquisition::normalize_provenance(&[
        unproven.clone(), proof("1","a",json!(0),"JPY"), proof("2","a",json!(0),"JPY"),
        proof("3","b",json!(500),"JPY"), proof("negative","a",json!(-1),"JPY"),
        proof("fraction","a",json!(1.5),"JPY"),
    ])});
    acquisition::apply_default(&mut p, "", None);
    assert!(p["paidPrice"].is_null());
    let accounts = vec![json!("a")];
    acquisition::apply_scope(&mut p, Some(&accounts));
    assert_eq!(p["paidPrice"], 0.0);
    assert_eq!(p["paidCurrency"], "JPY");
    assert_eq!(p["price"], 999);
    assert_eq!(
        p["provenance"][0]["paidAmountText"],
        unproven["paidAmountText"]
    );
    let accounts = vec![json!("b")];
    acquisition::apply_scope(&mut p, Some(&accounts));
    assert_eq!(p["paidPrice"], 500.0);
    acquisition::apply_scope(&mut p, Some(&vec![]));
    assert!(p["paidPrice"].is_null());
    assert!(p["paidCurrency"].is_null());
    for rows in [
        vec![unproven],
        vec![
            json!({"id":"free","source":"free"}),
            json!({"id":"gift","source":"gift"}),
        ],
        vec![
            proof("1", "a", json!(500), "JPY"),
            proof("2", "a", json!(500), "USD"),
        ],
        vec![
            proof("1", "a", json!(500), "JPY"),
            proof("2", "a", json!(12.50), "USD"),
        ],
        vec![proof("unsafe", "a", json!(9_007_199_254_740_992u64), "JPY")],
    ] {
        p["provenance"] = json!(acquisition::normalize_provenance(&rows));
        acquisition::apply_default(&mut p, "", None);
        assert!(p["paidPrice"].is_null());
    }
    for (amount, currency) in [(12.50, "USD"), (0.0, "EUR")] {
        p["provenance"] = json!(acquisition::normalize_provenance(&[
            proof("one", "a", json!(amount), currency),
            proof("duplicate", "a", json!(amount), currency),
        ]));
        acquisition::apply_scope(&mut p, Some(&vec![json!("a")]));
        assert_eq!(p["paidPrice"], amount);
        assert_eq!(p["paidCurrency"], currency);
        acquisition::apply_scope(&mut p, Some(&vec![]));
        assert!(p["paidPrice"].is_null());
        assert!(p["paidCurrency"].is_null());
    }
}

#[test]
fn provider_prices_preserve_currency_identity_scope_and_portable_facts() {
    let f = Fixture::new();
    let c = f.open();
    for (id, provider, store, amount, currency) in [
        ("900001", "booth", "BOOTH", 800.0, "JPY"),
        ("gumroad:900001", "gumroad", "Gumroad", 12.50, "USD"),
        ("jinxxy:900001", "jinxxy", "Jinxxy", 99.25, "EUR"),
        ("gumroad:cheap", "gumroad", "Gumroad", 2.0, "USD"),
        ("jinxxy:unknown", "jinxxy", "Jinxxy", 5.0, ""),
    ] {
        let mut source = json!({"id":id,"provider":provider,"store":store,"title":"Same synthetic Hair","sourceCategory":{"name":"Hair"},"price":amount,"priceCurrency":currency});
        if provider == "gumroad" {
            source["sourceDescription"] = json!("Authored hair for VRChat.");
        }
        c.put("product", id, source).unwrap();
    }
    let mut booth = c.get("product", "900001").unwrap().unwrap();
    booth["publicVariants"] = json!([{"id":"owned","name":"Owned","price":800,"currency":"JPY"}]);
    c.put("product", "900001", booth).unwrap();
    c.put("acquisition","booth-owned",json!({"id":"booth-owned","productId":"900001","accountId":"booth-account","variantIds":["owned"]})).unwrap();
    c.put("account", "gum-account", json!({"id":"gum-account","name":"Synthetic","provider":"gumroad","store":"Gumroad","status":"connected","identityHash":"authored-stable-buyer-hash","identityVerified":true,"nativeProfile":"authored-device-only"})).unwrap();
    c.put("acquisition", "gum-paid", json!({"id":"gum-paid","productId":"gumroad:900001","provider":"gumroad","accountId":"gum-account","source":"purchase","acquiredDateText":"2026-10-01","paidPrice":10.25,"paidCurrency":"USD","paidPriceBasis":"productLineTotal"})).unwrap();
    c.command("overlay", json!({"ids":["gumroad:900001"],"patch":{"title":"Edited Gumroad","note":"Keep correction"}})).unwrap();
    let list = c
        .command(
            "query",
            json!({"sort":"price","direction":"asc","includeFiles":false,"presentation":"list"}),
        )
        .unwrap();
    assert_eq!(
        list["matchedIds"],
        json!([
            "jinxxy:900001",
            "900001",
            "gumroad:cheap",
            "gumroad:900001",
            "jinxxy:unknown"
        ])
    );
    let desc = c
        .command("query", json!({"sort":"price","direction":"desc"}))
        .unwrap();
    assert_eq!(
        desc["matchedIds"],
        json!([
            "jinxxy:900001",
            "900001",
            "gumroad:900001",
            "gumroad:cheap",
            "jinxxy:unknown"
        ])
    );
    let scoped = c
        .command(
            "query",
            json!({"accountIds":["gum-account"],"includeFiles":false,"presentation":"list"}),
        )
        .unwrap();
    let detail = c
        .command(
            "product_details",
            json!({"id":"gumroad:900001","accountIds":["gum-account"]}),
        )
        .unwrap();
    for field in [
        "id",
        "title",
        "note",
        "provider",
        "price",
        "priceCurrency",
        "currentPriceText",
        "paidPrice",
        "paidCurrency",
        "acquired",
    ] {
        assert_eq!(scoped["products"][0][field], detail[field], "{field}");
    }
    assert_eq!(detail["price"], 12.50);
    assert_eq!(detail["priceCurrency"], "USD");
    assert_eq!(detail["currentPriceText"], "USD 12.5");
    assert_eq!(detail["paidPrice"], 10.25);
    assert_eq!(detail["paidCurrency"], "USD");
    assert_eq!(dynamic_details(&c, "900001")["currentPriceText"], "¥ 800");
    assert!(dynamic_details(&c, "jinxxy:unknown")["price"].is_null());
    c.put("product", "jinxxy:unknown", json!({"id":"jinxxy:unknown","provider":"jinxxy","store":"Jinxxy","title":"Synthetic formatted quote","priceFormatted":"¥ 1200","priceSourceCurrency":"JPY"})).unwrap();
    let formatted = dynamic_details(&c, "jinxxy:unknown");
    assert!(formatted["price"].is_null());
    assert_eq!(formatted["priceCurrency"], "JPY");
    assert_eq!(formatted["currentPriceText"], "¥ 1200");
    let empty_scope = c
        .command(
            "product_details",
            json!({"id":"gumroad:900001","accountIds":[]}),
        )
        .unwrap();
    assert!(empty_scope["paidPrice"].is_null());
    assert_eq!(empty_scope["priceCurrency"], "USD");
    let manifest = c.manifest().unwrap();
    assert_eq!(manifest["records"]["account"][0]["provider"], "gumroad");
    assert_eq!(manifest["records"]["account"][0]["status"], "disconnected");
    assert_eq!(
        manifest["records"]["account"][0]["identityHash"],
        "authored-stable-buyer-hash"
    );
    assert!(manifest["records"]["account"][0]
        .get("identityVerified")
        .is_none());
    assert!(manifest["records"]["account"][0]
        .get("nativeProfile")
        .is_none());
    let fresh = Fixture::new();
    let other = fresh.open();
    other.restore_manifest(manifest).unwrap();
    let account = other.get("account", "gum-account").unwrap().unwrap();
    assert_eq!(account["identityHash"], "authored-stable-buyer-hash");
    assert_eq!(account["status"], "disconnected");
    assert_ne!(account["identityVerified"], true);
    assert_eq!(
        dynamic_details(&other, "gumroad:900001")["paidCurrency"],
        "USD"
    );
    assert_eq!(
        dynamic_details(&other, "gumroad:900001")["note"],
        "Keep correction"
    );
    assert_eq!(other.list("product").unwrap().len(), 5);
    let restored_formatted = dynamic_details(&other, "jinxxy:unknown");
    assert!(restored_formatted["price"].is_null());
    assert_eq!(restored_formatted["currentPriceText"], "¥ 1200");
}

#[test]
fn acquisition_filter_reads_each_accounts_own_route_before_paging() {
    let f = Fixture::new();
    let c = f.open();
    let proven = |amount: f64| json!({"paidPrice":amount,"paidCurrency":"JPY","paidPriceBasis":"productLineTotal"});
    let unproven = json!({});
    for (product, account, source, facts) in [
        ("bought", "a", "purchase", proven(500.0)),
        // The Library row beside a proven order adds nothing of its own.
        ("bought", "a", "library", unproven.clone()),
        ("zero-order", "a", "purchase", proven(0.0)),
        ("zero-order", "a", "library", unproven.clone()),
        ("free-history", "a", "free", unproven.clone()),
        ("gifted", "b", "gift", unproven.clone()),
        ("library-only", "a", "library", unproven.clone()),
        ("two-routes", "a", "purchase", proven(800.0)),
        ("two-routes", "b", "gift", unproven.clone()),
        // A gift beside that account's free variant or order stays a gift only.
        ("gift-and-free", "b", "gift", unproven.clone()),
        ("gift-and-free", "b", "free", unproven.clone()),
        ("gift-and-order", "b", "gift", unproven.clone()),
        ("gift-and-order", "b", "purchase", proven(300.0)),
        // A bought product's free download, such as its 0-yen license file, stays bought.
        ("order-and-free", "a", "purchase", proven(600.0)),
        ("order-and-free", "a", "library", unproven.clone()),
        ("order-and-free", "a", "free", unproven.clone()),
        ("unproven-order-and-free", "a", "purchase", unproven.clone()),
        ("unproven-order-and-free", "a", "free", unproven.clone()),
    ] {
        let id = format!("{product}:{account}:{source}");
        let mut row = json!({"id":id,"productId":product,"accountId":account,"source":source});
        row.as_object_mut()
            .unwrap()
            .extend(facts.as_object().unwrap().clone());
        c.put("acquisition", &id, row).unwrap();
    }
    for id in [
        "bought",
        "zero-order",
        "free-history",
        "gifted",
        "library-only",
        "two-routes",
        "gift-and-free",
        "gift-and-order",
        "order-and-free",
        "unproven-order-and-free",
        "no-record",
    ] {
        c.put(
            "product",
            id,
            json!({"id":id,"provider":"booth","title":id,"sourceCategory":{"name":"Hair"}}),
        )
        .unwrap();
    }
    c.put("product","local:file",json!({"id":"local:file","provider":"local","store":"Local Files","custom":true,"title":"local:file","sourceCategory":{"name":"Hair"}})).unwrap();
    let matched = |args: Value| {
        let mut args = args;
        args["sort"] = json!("title");
        args["direction"] = json!("asc");
        strings(&c.command("query", args).unwrap()["matchedIds"])
    };
    assert_eq!(matched(json!({})).len(), 12);
    assert_eq!(matched(json!({"acquisition":null})).len(), 12);
    assert_eq!(matched(json!({"acquisition":[]})), Vec::<String>::new());
    assert_eq!(
        matched(json!({"acquisition":["free"]})),
        ["free-history", "zero-order"]
    );
    assert_eq!(
        matched(json!({"acquisition":["gift"]})),
        ["gift-and-free", "gift-and-order", "gifted", "two-routes"]
    );
    assert_eq!(
        matched(json!({"acquisition":["paid"]})),
        [
            "bought",
            "library-only",
            "no-record",
            "order-and-free",
            "two-routes",
            "unproven-order-and-free"
        ]
    );
    assert_eq!(
        matched(json!({"acquisition":["free","gift"]})),
        [
            "free-history",
            "gift-and-free",
            "gift-and-order",
            "gifted",
            "two-routes",
            "zero-order"
        ]
    );
    // Only the selected account's own route counts; another account's gift does not.
    assert_eq!(
        matched(json!({"acquisition":["gift"],"accountIds":["a"]})),
        Vec::<String>::new()
    );
    assert_eq!(
        matched(json!({"acquisition":["paid"],"accountIds":["b"]})),
        Vec::<String>::new()
    );
    assert_eq!(
        matched(json!({"acquisition":["gift"],"accountIds":["b"]})),
        ["gift-and-free", "gift-and-order", "gifted", "two-routes"]
    );
    assert_eq!(
        matched(json!({"acquisition":["free"],"accountIds":[]})),
        Vec::<String>::new()
    );
    // An unproven row follows the price it can still show.
    let mut row = c
        .get("acquisition", "library-only:a:library")
        .unwrap()
        .unwrap();
    row["variantIds"] = json!(["800001"]);
    c.put("acquisition", "library-only:a:library", row).unwrap();
    let mut listing = c.get("product", "library-only").unwrap().unwrap();
    listing["publicVariants"] = json!([{"id":"800001","name":"","price":0,"currency":"JPY"}]);
    c.put("product", "library-only", listing).unwrap();
    assert_eq!(
        matched(json!({"acquisition":["free"]})),
        ["free-history", "library-only", "zero-order"]
    );
    // Two accounts holding different variants: each row follows its own account's price.
    for (account, variant) in [("a", "800002"), ("b", "800003")] {
        let id = format!("split:{account}:library");
        c.put("acquisition",&id,json!({"id":id,"productId":"split","accountId":account,"source":"library","variantIds":[variant]})).unwrap();
    }
    c.put("product","split",json!({"id":"split","provider":"booth","title":"split","sourceCategory":{"name":"Hair"},"publicVariants":[{"id":"800002","name":"Free","price":0,"currency":"JPY"},{"id":"800003","name":"Paid","price":500,"currency":"JPY"}]})).unwrap();
    assert!(
        matched(json!({"acquisition":["free"],"accountIds":["a"]})).contains(&"split".to_owned())
    );
    assert!(
        !matched(json!({"acquisition":["paid"],"accountIds":["a"]})).contains(&"split".to_owned())
    );
    assert!(
        matched(json!({"acquisition":["paid"],"accountIds":["b"]})).contains(&"split".to_owned())
    );
    assert!(
        !matched(json!({"acquisition":["free"],"accountIds":["b"]})).contains(&"split".to_owned())
    );
}

#[test]
fn offline_product_scope_preserves_accounts_paging_and_local_metadata() {
    let f = Fixture::new();
    let c = f.open();
    for (id, title, custom) in [
        ("owned-a", "A owned store", false),
        ("owned-b", "B owned store", false),
        ("edited-store", "C edited store", true),
        ("unassigned-store", "D unassigned store", false),
    ] {
        c.put("product",id,json!({"id":id,"provider":"booth","title":title,"custom":custom,"sourceCategory":{"name":"Hair"}})).unwrap();
    }
    for (id, account) in [("owned-a", "a"), ("owned-b", "b"), ("edited-store", "a")] {
        c.put("acquisition",id,json!({"id":id,"productId":id,"accountId":account,"source":"purchase","acquiredDateText":if id=="owned-a"{"2026/10/1"}else{""}})).unwrap();
    }
    for (id, title) in [
        ("local:blank", "E local blank"),
        ("local:dated", "F local dated"),
    ] {
        c.put("product",id,json!({"id":id,"provider":"local","store":"Local Files","custom":true,"title":title,"sourceTags":["Mafuyu"],"sourceCategory":{"name":"Hair"},"importedAt":"2026-10-10T00:00:00Z"})).unwrap();
    }
    // Authored observed acquisition metadata must remain independent of store scope.
    c.put("acquisition","local-observation",json!({"id":"local-observation","productId":"local:dated","provider":"local","source":"local","observationKind":"localImport","importedAt":"2026-10-10T00:00:00Z","acquiredDateText":"2026/1/15"})).unwrap();
    c.command("overlay",json!({"ids":["local:dated"],"patch":{"note":"Human local note","tags":["Human local tag"],"ownedAvatars":["Mafuyu"]}})).unwrap();

    let all = json!([
        "owned-a",
        "owned-b",
        "edited-store",
        "unassigned-store",
        "local:blank",
        "local:dated"
    ]);
    for (scope, expected) in [
        (json!({}), all.clone()),
        (json!({"accountIds":null}), all.clone()),
        (
            json!({"accountIds":null,"includeOffline":true}),
            all.clone(),
        ),
        (
            json!({"includeOffline":false}),
            json!(["owned-a", "owned-b", "edited-store", "unassigned-store"]),
        ),
        (
            json!({"accountIds":null,"includeOffline":false}),
            json!(["owned-a", "owned-b", "edited-store", "unassigned-store"]),
        ),
        (
            json!({"accountIds":["a"]}),
            json!(["owned-a", "edited-store"]),
        ),
        (
            json!({"accountIds":["a"],"includeOffline":false}),
            json!(["owned-a", "edited-store"]),
        ),
        (
            json!({"accountIds":["a"],"includeOffline":true}),
            json!(["owned-a", "edited-store", "local:blank", "local:dated"]),
        ),
        (
            json!({"accountIds":["b"],"includeOffline":true}),
            json!(["owned-b", "local:blank", "local:dated"]),
        ),
        (
            json!({"accountIds":["a","b"],"includeOffline":true}),
            json!([
                "owned-a",
                "owned-b",
                "edited-store",
                "local:blank",
                "local:dated"
            ]),
        ),
        (json!({"accountIds":[]}), json!([])),
        (json!({"accountIds":[],"includeOffline":false}), json!([])),
        (
            json!({"accountIds":[],"includeOffline":true}),
            json!(["local:blank", "local:dated"]),
        ),
    ] {
        let mut args = scope.clone();
        args["sort"] = json!("title");
        args["direction"] = json!("asc");
        let result = c.command("query", args).unwrap();
        assert_eq!(result["matchedIds"], expected, "{scope}");
        assert_eq!(
            result["matched"],
            expected.as_array().unwrap().len(),
            "{scope}"
        );
        assert_eq!(result["total"], 6, "{scope}");
    }
    let paged=c.command("query",json!({"accountIds":["a"],"includeOffline":true,"sort":"title","direction":"asc","limit":1,"offset":1,"includeFiles":false,"presentation":"list"})).unwrap();
    assert_eq!(
        paged["matchedIds"],
        json!(["owned-a", "edited-store", "local:blank", "local:dated"])
    );
    assert_eq!(paged["products"].as_array().unwrap().len(), 1);
    assert_eq!(paged["products"][0]["id"], "edited-store");
    assert_eq!(paged["allDownloaded"], false);
    let searched = c
        .command(
            "query",
            json!({"accountIds":["a"],"includeOffline":true,"search":"local","sort":"title"}),
        )
        .unwrap();
    assert_eq!(
        searched["matchedIds"],
        json!(["local:blank", "local:dated"])
    );
    let acquired = c
        .command(
            "query",
            json!({"accountIds":["a"],"includeOffline":true,"sort":"acquired","direction":"asc"}),
        )
        .unwrap();
    assert_eq!(
        acquired["matchedIds"],
        json!(["local:dated", "owned-a", "edited-store", "local:blank"])
    );

    let avatar=c.command("query",json!({"accountIds":[],"includeOffline":true,"avatar":"Mafuyu","sort":"title","includeFiles":false,"presentation":"list"})).unwrap();
    assert_eq!(avatar["matchedIds"], json!(["local:blank", "local:dated"]));
    let excluded_avatar = c
        .command(
            "query",
            json!({"accountIds":[],"includeOffline":false,"avatar":"Mafuyu"}),
        )
        .unwrap();
    assert_eq!(excluded_avatar["matchedIds"], json!([]));
    for id in ["local:blank", "local:dated"] {
        let unscoped = dynamic_details(&c, id);
        for accounts in [json!([]), json!(["a"])] {
            let details = c
                .command("product_details", json!({"id":id,"accountIds":accounts}))
                .unwrap();
            for field in [
                "title",
                "note",
                "tags",
                "ownedAvatars",
                "ownedAvatarEvidence",
                "acquired",
                "acquiredSortKey",
                "price",
                "priceCurrency",
                "currentPriceText",
                "paidPrice",
                "paidCurrency",
                "provenance",
            ] {
                assert_eq!(details[field], unscoped[field], "{id}: {field}");
            }
        }
        let summary = avatar["products"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == id)
            .unwrap();
        for field in [
            "acquired",
            "acquiredSortKey",
            "price",
            "priceCurrency",
            "currentPriceText",
            "paidPrice",
            "paidCurrency",
            "ownedAvatars",
        ] {
            assert_eq!(summary[field], unscoped[field], "{id}: {field}");
        }
        assert!(unscoped["price"].is_null());
        assert!(unscoped["paidPrice"].is_null());
    }
    assert_eq!(dynamic_details(&c, "local:blank")["acquired"], "");
    assert_eq!(dynamic_details(&c, "local:dated")["acquired"], "2026/1/15");
}

#[test]
fn scoped_acquisition_summary_and_sort_match_details_before_paging() {
    let f = Fixture::new();
    let c = f.open();
    for id in ["1", "2", "3"] {
        product(&c, id);
    }
    for (id, product_id, account, date, amount) in [
        ("1a", "1", "a", "2026/10/1 12:00:01", 300),
        ("1b", "1", "b", "2026/1/1 12:00:01", 700),
        ("2a", "2", "a", "2026/2/1 12:00:01", 400),
        ("3a", "3", "a", "Unparsed provider date", 0),
    ] {
        c.put("acquisition",id,json!({"id":id,"productId":product_id,"accountId":account,"source":"purchase","acquiredDateText":date,"paidPrice":amount,"paidCurrency":"JPY","paidPriceBasis":"productLineTotal","variantIds":["owned"]})).unwrap();
    }
    let mut source = c.get("product", "1").unwrap().unwrap();
    source["price"] = json!(900);
    source["publicVariants"] = json!([{"id":"owned","name":"Owned","price":900,"currency":"JPY"}]);
    c.put("product", "1", source).unwrap();
    let all = c
        .command("query", json!({"sort":"acquired","direction":"asc"}))
        .unwrap();
    assert_eq!(all["matchedIds"], json!(["1", "2", "3"]));
    let scoped = c
        .command(
            "query",
            json!({"accountIds":["a"],"sort":"acquired","direction":"asc","limit":1,"offset":1}),
        )
        .unwrap();
    assert_eq!(scoped["matchedIds"], json!(["2", "1", "3"]));
    assert_eq!(scoped["products"][0]["id"], "1");
    let details = c
        .command("product_details", json!({"id":"1","accountIds":["a"]}))
        .unwrap();
    for field in [
        "acquired",
        "acquiredSortKey",
        "paidPrice",
        "paidCurrency",
        "price",
        "currentPriceText",
        "provenance",
    ] {
        assert_eq!(details[field], scoped["products"][0][field], "{field}");
    }
    assert_eq!(details["acquired"], "2026/10/1 12:00:01");
    assert_eq!(details["paidPrice"], 300.0);
    assert_eq!(details["price"], 900.0);
    assert_eq!(details["provenance"].as_array().unwrap().len(), 2);
    let desc = c
        .command(
            "query",
            json!({"accountIds":["a"],"sort":"acquired","direction":"desc"}),
        )
        .unwrap();
    assert_eq!(desc["matchedIds"], json!(["1", "2", "3"]));
    assert_eq!(desc["products"][2]["acquired"], "Unparsed provider date");
    assert!(desc["products"][2]["acquiredSortKey"].is_null());
    let all_details = c
        .command("product_details", json!({"id":"1","accountIds":null}))
        .unwrap();
    assert_eq!(all_details["acquired"], "2026/1/1 12:00:01");
    assert!(all_details["paidPrice"].is_null());
    let none = c
        .command("product_details", json!({"id":"1","accountIds":[]}))
        .unwrap();
    assert_eq!(none["acquired"], "");
    assert!(none["paidPrice"].is_null());
    assert!(none["price"].is_null());
}

#[test]
fn pinned_base_does_not_borrow_excluded_account_acquisition_or_product_fallback() {
    let f = Fixture::new();
    let c = f.open();
    c.put("product","base",json!({"id":"base","sourceTitle":"SyntheticScopeAvatar","sourceCategory":{"name":"3Dアバター"},"acquired":"2000/1/1"})).unwrap();
    c.put("acquisition","base-b",json!({"id":"base-b","productId":"base","accountId":"b","acquiredDateText":"2026/2/1 10:20:30","paidPrice":500,"paidCurrency":"JPY","paidPriceBasis":"productLineTotal"})).unwrap();
    assert_eq!(dynamic_details(&c, "base")["acquired"], "2000/1/1");
    for accounts in [json!([]), json!(["a"])] {
        let q = c
            .command(
                "query",
                json!({"avatar":"product:base","accountIds":accounts}),
            )
            .unwrap();
        assert_eq!(q["matchedIds"], json!(["base"]));
        assert_eq!(q["products"][0]["acquired"], "");
        assert!(q["products"][0]["paidPrice"].is_null());
    }
    let b = c
        .command("product_details", json!({"id":"base","accountIds":["b"]}))
        .unwrap();
    assert_eq!(b["acquired"], "2026/2/1 10:20:30");
    assert_eq!(b["paidPrice"], 500.0);
}

#[test]
fn current_projection_prices_and_dates_survive_restart_without_source_history_changes() {
    let f = Fixture::new();
    let c = f.open();
    for (id, price, listing) in [
        ("exact", Value::Null, "¥ 1,200"),
        ("range", Value::Null, "¥ 500 ~"),
        ("closed", Value::Null, "¥ 500 ~ ¥ 900"),
        ("zero", json!(0), "¥ 900"),
        ("known", json!(700), "¥ 900"),
        ("fraction", json!(1.5), ""),
        ("negative", json!(-1), "invalid"),
        ("unsafe", json!(9_007_199_254_740_992u64), ""),
        ("fraction-fallback", json!(1.5), "¥ 800"),
        ("negative-fallback", json!(-1), "¥ 0"),
        ("unsafe-range", json!(9_007_199_254_740_992u64), "¥ 500 ~"),
    ] {
        c.put(
            "product",
            id,
            json!({"id":id,"title":id,"price":price,"listingPriceText":listing}),
        )
        .unwrap();
    }
    for (id, price) in [
        ("exact", 1200),
        ("zero", 0),
        ("known", 700),
        ("fraction-fallback", 800),
        ("negative-fallback", 0),
    ] {
        let mut source = c.get("product", id).unwrap().unwrap();
        source["publicVariants"] =
            json!([{"id":"owned","name":"Owned","price":price,"currency":"JPY"}]);
        c.put("product", id, source).unwrap();
        c.put("acquisition", &format!("owned-{id}"),json!({"id":format!("owned-{id}"),"productId":id,"accountId":"a","variantIds":["owned"]})).unwrap();
    }
    c.put("acquisition","retained",json!({"id":"retained","productId":"exact","accountId":"a","source":"purchase","sourceRecordId":"unchanged-record","acquiredDateText":"2026/2/1 10:20:30","paidAmountText":null})).unwrap();
    c.command(
        "overlay",
        json!({"ids":["exact"],"patch":{"title":"Human title","note":"Keep note"}}),
    )
    .unwrap();
    let raw_products = c.list("product").unwrap();
    let raw_acquisitions = c.list("acquisition").unwrap();
    let before = Catalogue::document_snapshot(&c.inner.lock().unwrap()).unwrap();
    drop(c);
    let c = f.open();
    assert_eq!(c.list("product").unwrap(), raw_products);
    assert_eq!(c.list("acquisition").unwrap(), raw_acquisitions);
    let after = Catalogue::document_snapshot(&c.inner.lock().unwrap()).unwrap();
    assert_eq!(after.1, before.1);
    assert_eq!(after.0 .0, before.0 .0);
    assert_eq!(after.0 .1, before.0 .1);
    let exact = dynamic_details(&c, "exact");
    assert_eq!(exact["price"], 1200.0);
    assert_eq!(exact["acquired"], "2026/2/1 10:20:30");
    assert_eq!(exact["title"], "Human title");
    assert_eq!(exact["note"], "Keep note");
    assert!(exact["paidPrice"].is_null());
    for id in ["range", "closed", "unsafe-range"] {
        let p = dynamic_details(&c, id);
        assert!(p["price"].is_null());
        assert!(p["currentPriceText"].is_null());
        assert_eq!(p["listingStartingPrice"], 500.0);
        assert!(!text(&c.get("product", id).unwrap().unwrap(), "listingPriceText").is_empty());
    }
    for id in ["fraction", "negative", "unsafe"] {
        let p = dynamic_details(&c, id);
        assert!(p["price"].is_null());
        assert!(p["currentPriceText"].is_null());
        let sql_price: Option<f64> = c
            .inner
            .lock()
            .unwrap()
            .query_row("SELECT price FROM products WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(sql_price.is_none());
    }
    for (id, expected) in [
        ("zero", 0.0),
        ("known", 700.0),
        ("fraction-fallback", 800.0),
        ("negative-fallback", 0.0),
    ] {
        assert_eq!(dynamic_details(&c, id)["price"].as_f64(), Some(expected));
        let sql_price: f64 = c
            .inner
            .lock()
            .unwrap()
            .query_row("SELECT price FROM products WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(sql_price, expected);
    }
}

#[test]
fn acquired_dates_sort_chronologically_preserving_provider_text() {
    let f = Fixture::new();
    let c = f.open();
    for id in ["1", "2", "3"] {
        product(&c, id);
    }
    for (id, date) in [
        ("1", "2026年2月1日"),
        ("2", "2026年10月1日"),
        ("3", "not a date"),
    ] {
        c.put(
            "acquisition",
            id,
            json!({"id":id,"productId":id,"source":"purchase","acquiredDateText":date}),
        )
        .unwrap();
    }
    assert_eq!(
        c.command("query", json!({"sort":"acquired","direction":"desc"}))
            .unwrap()["matchedIds"],
        json!(["2", "1", "3"])
    );
    assert_eq!(
        c.command("query", json!({"sort":"acquired","direction":"asc"}))
            .unwrap()["matchedIds"],
        json!(["1", "2", "3"])
    );
    assert_eq!(projection(&c, "1")["acquired"], "2026年2月1日");
    c.put(
        "acquisition",
        "earlier",
        json!({"id":"earlier","productId":"2","source":"gift","acquiredDateText":"2026年2月1日"}),
    )
    .unwrap();
    assert_eq!(projection(&c, "2")["acquired"], "2026年2月1日");
}

#[test]
fn acquired_subsecond_sort_key_is_fixed_precision() {
    let f = Fixture::new();
    let c = f.open();
    for (id, date) in [
        ("1", "2026-01-01T00:00:00Z"),
        ("2", "2026-01-01T00:00:00.500Z"),
        ("3", "2026年1月1日"),
    ] {
        product(&c, id);
        c.put(
            "acquisition",
            id,
            json!({"id":id,"productId":id,"source":"purchase","acquiredDateText":date}),
        )
        .unwrap();
    }
    assert_eq!(
        c.command("query", json!({"sort":"acquired","direction":"desc"}))
            .unwrap()["matchedIds"],
        json!(["2", "1", "3"])
    );
}

#[test]
fn unnamed_public_variant_price_requires_exact_owned_variant_identity() {
    let fixture = Fixture::new();
    let c = fixture.open();
    for (id, owned) in [
        ("matched-unnamed", Some("800001")),
        ("different-unnamed", Some("800002")),
        ("unowned-unnamed", None),
    ] {
        c.put("product", id, json!({"id":id,"store":"BOOTH","sourceTitle":"Authored clothing","sourceCategory":{"name":"3D衣装"},"sourceTags":["VRChat"],"price":6000,"listingPriceText":"¥ 6000","publicVariants":[{"id":"800001","type":"digital","price":6000,"currency":"JPY"}]})).unwrap();
        if let Some(variant) = owned {
            let file_id = format!("{id}-file");
            let acquisition_id = format!("{id}-order");
            c.put("remote_file", &file_id, json!({"id":file_id,"productId":id,"accountId":"a","variantId":variant,"name":"authored.zip","revision":"r1"})).unwrap();
            c.put(
                "acquisition",
                &acquisition_id,
                json!({"id":acquisition_id,"productId":id,"accountId":"a","variantIds":[variant]}),
            )
            .unwrap();
        }
    }
    let matched = dynamic_details(&c, "matched-unnamed");
    assert_eq!(matched["price"], 6000.0);
    assert_eq!(matched["priceCurrency"], "JPY");
    assert_eq!(matched["currentPriceText"], "¥ 6000");
    assert_eq!(matched["ownedCurrentPrices"].as_array().unwrap().len(), 1);
    assert_eq!(matched["ownedCurrentPrices"][0]["variantId"], "800001");
    assert_eq!(matched["ownedCurrentPrices"][0]["price"], 6000.0);
    assert!(matched["paidPrice"].is_null());
    let different = dynamic_details(&c, "different-unnamed");
    assert!(different["price"].is_null());
    assert!(different["currentPriceText"].is_null());
    assert_eq!(different["listingStartingPrice"], 6000.0);
    assert_eq!(different["ownedCurrentPrices"][0]["variantId"], "800002");
    assert!(different["ownedCurrentPrices"][0]["price"].is_null());
    let unowned = dynamic_details(&c, "unowned-unnamed");
    assert!(unowned["price"].is_null());
    assert_eq!(unowned["listingStartingPrice"], 6000.0);
    assert!(unowned["ownedCurrentPrices"].as_array().unwrap().is_empty());
}

#[test]
fn multi_provider_refresh_unlink_restart_preserves_grants_money_and_manual_tags() {
    use sha2::{Digest, Sha256};
    let fixture = Fixture::new();
    let c = fixture.open();
    c.put(
        "root",
        "authored-root",
        json!({"id":"authored-root","path":fixture.0,"status":"online"}),
    )
    .unwrap();
    let providers = [
        ("booth", "BOOTH", "authored-booth", "JPY"),
        ("gumroad", "Gumroad", "gumroad:authored", "USD"),
        ("jinxxy", "Jinxxy", "jinxxy:authored", "EUR"),
    ];
    for (provider, store, id, currency) in providers {
        let source = json!({"id":id,"provider":provider,"store":store,"sourceTitle":"Authored source Hair","sourceCategory":{"name":"Hair"},"sourceTags":["Source tag","Removed tag"],"price":if provider=="booth"{1.0}else{22.5},"priceCurrency":currency,"listingPriceText":"¥ 1","publicVariants":[{"id":"owned-a","name":"Owned A","type":"digital","price":1100,"currency":"JPY"},{"id":"owned-b","name":"Owned B","type":"digital","price":2200,"currency":"JPY"},{"id":"unowned-cheap","name":"Unowned minimum","type":"digital","price":1,"currency":"JPY"}]});
        c.put("product", id, source).unwrap();
        for (suffix, paid, date) in [("a", 8.0, "2026/10/01"), ("b", 16.0, "2026/10/02")] {
            let account = format!("{provider}-{suffix}");
            let file_id = format!("{provider}-file-{suffix}");
            let entitlement_id = format!("{provider}-grant-{suffix}");
            let acquisition_id = format!("{provider}-purchase-{suffix}");
            let variant = format!("owned-{suffix}");
            let relative = format!("{provider}/Authored-{suffix}.unitypackage");
            let bytes = format!("Independently authored {provider} {suffix} content");
            let path = fixture.0.join(&relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &bytes).unwrap();
            c.put_many(vec![
                ("account".into(),account.clone(),json!({"id":account,"provider":provider,"store":store,"identityVerified":true,"name":format!("Authored {suffix}")})),
                ("remote_file".into(),file_id.clone(),json!({"id":file_id,"productId":id,"accountId":account,"provider":provider,"sourceFileId":format!("source-{suffix}"),"variantId":variant,"name":format!("Authored-{suffix}.unitypackage"),"revision":"r1","sizeBytes":bytes.len(),"requiresContentVerification":false})),
                ("entitlement".into(),entitlement_id.clone(),json!({"id":entitlement_id,"productId":id,"accountId":account,"provider":provider,"variantIds":[variant],"fileIds":[file_id],"manifestComplete":true})),
                ("acquisition".into(),acquisition_id.clone(),json!({"id":acquisition_id,"productId":id,"accountId":account,"provider":provider,"source":"purchase","variantIds":[variant],"acquiredDateText":date,"paidPrice":paid,"paidCurrency":currency,"paidPriceBasis":"productLineTotal","variantPayments":[{"variantId":variant,"name":format!("Owned {suffix}"),"price":paid,"currency":currency,"quantity":1}]})),
                ("artifact".into(),file_id.clone(),json!({"id":file_id,"fileId":file_id,"productId":id,"revision":"r1","rootId":"authored-root","status":"ready","hash":format!("{:x}",Sha256::digest(bytes.as_bytes())),"sizeBytes":bytes.len(),"relativePath":relative,"originalAvailable":true})),
            ]).unwrap();
        }
        c.command("overlay", json!({"ids":[id],"patch":{"title":format!("Human label {provider}"),"category":"Accessory","note":"Persistent human note","tags":["Manual retained","Manual second"],"rejectedTags":["Removed tag"],"thumbnailHidden":true}})).unwrap();
        let initial = dynamic_details(&c, id);
        assert_eq!(initial["status"], "ready", "{provider}");
        assert_eq!(initial["total"], 2);
        assert!(
            initial["paidPrice"].is_null(),
            "Two distinct paid amounts must not become a guessed scalar"
        );
        if provider == "booth" {
            assert!(initial["price"].is_null());
            assert_eq!(initial["ownedCurrentPrices"].as_array().unwrap().len(), 2);
            assert!(!initial["ownedCurrentPrices"]
                .as_array()
                .unwrap()
                .iter()
                .any(|quote| quote["variantId"] == "unowned-cheap"));
        } else {
            assert_eq!(initial["price"], 22.5);
            assert_eq!(initial["priceCurrency"], currency);
        }
        for (suffix, amount) in [("a", 8.0), ("b", 16.0)] {
            let scoped = c
                .command(
                    "product_details",
                    json!({"id":id,"accountIds":[format!("{provider}-{suffix}")]}),
                )
                .unwrap();
            assert_eq!(scoped["paidPrice"], amount);
            assert_eq!(scoped["paidCurrency"], currency);
            if provider == "booth" {
                assert_eq!(
                    scoped["ownedCurrentPrices"][0]["variantId"],
                    format!("owned-{suffix}")
                );
                assert_eq!(scoped["price"], if suffix == "a" { 1100.0 } else { 2200.0 });
                assert_eq!(scoped["ownedPaidPrices"][0]["price"], amount);
            }
        }
    }
    // A real source change must invalidate the old artifact until the new bytes
    // and matching descriptor are committed; a complete second grant remains usable.
    for (provider, _, id, _) in providers {
        let mut source = c.get("product", id).unwrap().unwrap();
        source["sourceTitle"] = json!("Updated source Clothes");
        source["sourceCategory"] = json!({"name":"Clothes"});
        source["sourceTags"] = json!(["New source tag", "Removed tag"]);
        source["price"] = json!(if provider == "booth" { 1.0 } else { 27.5 });
        source["publicVariants"][0]["price"] = json!(1400);
        source["publicVariants"][1]["price"] = json!(2600);
        let file_id = format!("{provider}-file-a");
        let mut changed_file = c.get("remote_file", &file_id).unwrap().unwrap();
        changed_file["revision"] = json!("r2");
        changed_file["requiresContentVerification"] = json!(true);
        c.put_many(vec![
            ("product".into(), id.into(), source),
            ("remote_file".into(), file_id.clone(), changed_file.clone()),
        ])
        .unwrap();
        let pending = dynamic_details(&c, id);
        assert_eq!(pending["status"], "partial", "{provider}");
        assert_eq!(pending["downloaded"], 1);
        assert_eq!(pending["manifestComplete"], true);
        let mut verified = c.get("artifact", &file_id).unwrap().unwrap();
        let bytes = format!("Independently authored refreshed {provider} content");
        std::fs::write(fixture.0.join(text(&verified, "relativePath")), &bytes).unwrap();
        verified["revision"] = json!("r2");
        verified["hash"] = json!(format!("{:x}", Sha256::digest(bytes.as_bytes())));
        verified["sizeBytes"] = json!(bytes.len());
        changed_file["sizeBytes"] = json!(bytes.len());
        changed_file["requiresContentVerification"] = json!(false);
        c.put_many(vec![
            ("remote_file".into(), file_id.clone(), changed_file),
            ("artifact".into(), file_id, verified),
        ])
        .unwrap();
        assert_eq!(dynamic_details(&c, id)["status"], "ready", "{provider}");
    }
    for (provider, _, id, currency) in providers {
        let account_a = format!("{provider}-a");
        let account_b = format!("{provider}-b");
        let file_b = format!("{provider}-file-b");
        let retained = c.get("remote_file", &file_b).unwrap();
        let other_products: Vec<_> = providers
            .iter()
            .filter(|(_, _, other, _)| *other != id)
            .map(|(_, _, other, _)| ((*other).to_owned(), dynamic_details(&c, other)))
            .collect();
        assert_eq!(
            c.remove_account(&account_a, true).unwrap(),
            json!({"accountRemoved":1,"productRemoved":0,"retainedShared":1})
        );
        assert!(c
            .get("acquisition", &format!("{provider}-purchase-a"))
            .unwrap()
            .is_none());
        assert!(c
            .get("entitlement", &format!("{provider}-grant-a"))
            .unwrap()
            .is_none());
        assert!(c
            .get("remote_file", &format!("{provider}-file-a"))
            .unwrap()
            .is_none());
        assert_eq!(c.get("remote_file", &file_b).unwrap(), retained);
        for (other, expected) in other_products {
            assert_eq!(
                dynamic_details(&c, &other),
                expected,
                "Unlinking {provider} changed another provider"
            );
        }
        let kept = dynamic_details(&c, id);
        assert_eq!(kept["accountIds"], json!([account_b]));
        assert_eq!(kept["total"], 1);
        assert_eq!(kept["downloaded"], 1);
        assert_eq!(kept["status"], "ready");
        assert_eq!(kept["files"][0]["id"], file_b);
        assert_eq!(kept["files"][0]["provenance"][0]["accountId"], account_b);
        assert_eq!(kept["paidPrice"], 16.0);
        assert_eq!(kept["paidCurrency"], currency);
        assert_eq!(kept["acquired"], "2026/10/02");
        assert_eq!(
            kept["price"],
            if provider == "booth" { 2600.0 } else { 27.5 }
        );
        if provider == "booth" {
            assert_eq!(kept["ownedCurrentPrices"].as_array().unwrap().len(), 1);
            assert_eq!(kept["ownedCurrentPrices"][0]["variantId"], "owned-b");
            assert_eq!(kept["ownedPaidPrices"].as_array().unwrap().len(), 1);
            assert_eq!(kept["ownedPaidPrices"][0]["price"], 16.0);
        }
    }
    let expected: Vec<_> = providers
        .iter()
        .map(|(_, _, id, _)| ((*id).to_owned(), dynamic_details(&c, id)))
        .collect();
    drop(c);
    let reopened = fixture.open();
    let list = reopened
        .command("query", json!({"includeFiles":false,"presentation":"list"}))
        .unwrap();
    assert_eq!(list["total"], 3);
    assert_eq!(list["allDownloaded"], true);
    for (id, expected) in &expected {
        let details = dynamic_details(&reopened, id);
        assert_eq!(&details, expected);
        let summary = list["products"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == id.as_str())
            .unwrap();
        for field in [
            "title",
            "category",
            "note",
            "tags",
            "rejectedTags",
            "thumbnailHidden",
            "accountIds",
            "price",
            "priceCurrency",
            "paidPrice",
            "paidCurrency",
            "acquired",
            "status",
            "fullyDownloaded",
        ] {
            assert_eq!(
                summary[field], details[field],
                "Reopened list/detail {id}/{field}"
            );
        }
        assert_eq!(
            details["title"],
            format!("Human label {}", text(&details, "provider"))
        );
        assert_eq!(details["category"], "Accessory");
        assert_eq!(details["note"], "Persistent human note");
        assert_eq!(details["tags"], json!(["Manual retained", "Manual second"]));
        assert_eq!(
            details["rejectedTags"],
            json!(["Removed tag", "Source tag"])
        );
        assert_eq!(details["thumbnailHidden"], true);
    }
    // A missing owned quote stays unknown in exact-price fields. BOOTH exposes
    // the independently labelled listing start without changing paid facts.
    for (provider, _, id, _) in providers {
        let mut source = reopened.get("product", id).unwrap().unwrap();
        source["price"] = Value::Null;
        source["publicVariants"] = json!([{"id":"unowned-cheap","name":"Unowned minimum","type":"digital","price":1,"currency":"JPY"}]);
        reopened.put("product", id, source).unwrap();
        let unknown = dynamic_details(&reopened, id);
        assert!(
            unknown["price"].is_null(),
            "{provider} must not borrow a listing minimum"
        );
        assert!(unknown["currentPriceText"].is_null());
        assert_eq!(unknown["paidPrice"], 16.0);
        assert_eq!(unknown["status"], "ready");
        assert_eq!(unknown["tags"], json!(["Manual retained", "Manual second"]));
        if provider == "booth" {
            assert!(unknown["ownedCurrentPrices"][0]["price"].is_null());
            assert_eq!(unknown["listingStartingPrice"], 1.0);
        }
    }
    drop(reopened);
    let unknown_restart = fixture.open();
    for (_, _, id, currency) in providers {
        let p = dynamic_details(&unknown_restart, id);
        assert!(p["price"].is_null());
        assert!(p["currentPriceText"].is_null());
        assert_eq!(p["paidPrice"], 16.0);
        assert_eq!(p["paidCurrency"], currency);
        assert_eq!(p["status"], "ready");
    }
}

#[test]
fn public_variants_and_account_fullpack_remain_distinct_under_filters_and_overrides() {
    let fixture = Fixture::new();
    let c = fixture.open();
    let source = json!({"id":"900001","store":"BOOTH","sourceTitle":"Authored VRChat clothing","sourceCategory":{"name":"3D衣装"},"sourceTags":["VRChat"],"publicVariants":[
        {"id":"900010","type":"digital","name":"全部入りフルパック","price":3300,"currency":"JPY"},
        {"id":"900011","type":"digital","name":"しなの専用"},
        {"id":"900012","type":"digital","name":"マヌカ専用","price":800,"currency":"JPY"},
        {"id":"900013","type":"digital","name":"Lapwing専用"},
        {"id":"900014","type":"digital","name":"マリシア専用"}]});
    let a_file = json!({"id":"a-file","productId":"900001","accountId":"a","variantId":"900010","name":"opaque-bundle.zip","revision":"r1"});
    let b_file = json!({"id":"b-file","productId":"900001","accountId":"b","variantId":"900012","variantName":"マヌカ専用","name":"opaque-single.zip","revision":"r1"});
    c.put_many(vec![
        ("product".into(),"900001".into(),source.clone()),
        ("account".into(),"a".into(),json!({"id":"a","store":"BOOTH"})),
        ("account".into(),"b".into(),json!({"id":"b","store":"BOOTH"})),
        ("remote_file".into(),"a-file".into(),a_file.clone()),
        ("remote_file".into(),"b-file".into(),b_file.clone()),
        ("entitlement".into(),"a-ent".into(),json!({"id":"a-ent","productId":"900001","accountId":"a","fileIds":["a-file"],"manifestComplete":true})),
        ("entitlement".into(),"b-ent".into(),json!({"id":"b-ent","productId":"900001","accountId":"b","fileIds":["b-file"],"manifestComplete":true}))
    ]).unwrap();
    c.put("acquisition", "a-order", json!({"id":"a-order","productId":"900001","accountId":"a","variantIds":["900010"],"paidPrice":3000,"paidCurrency":"JPY","paidPriceBasis":"productLineTotal","variantPayments":[{"variantId":"900010","name":"全部入りフルパック","price":3000,"currency":"JPY","quantity":1}]})).unwrap();
    // The aggregate price across two variants is not a per-variant payment.
    c.put("acquisition", "b-order", json!({"id":"b-order","productId":"900001","accountId":"b","variantIds":["900012","900013"],"paidPrice":1600,"paidCurrency":"JPY","paidPriceBasis":"productLineTotal"})).unwrap();
    c.put("acquisition", "b-unproven", json!({"id":"b-unproven","productId":"900001","accountId":"b","variantIds":["900012"],"paidPrice":700,"paidCurrency":"JPY","paidPriceBasis":"unproven","variantPayments":[{"variantId":"900012","name":"マヌカ専用","price":700,"currency":"JPY"}]})).unwrap();
    let details = dynamic_details(&c, "900001");
    assert!(details["price"].is_null());
    assert_eq!(details["ownedCurrentPrices"].as_array().unwrap().len(), 3);
    assert_eq!(details["ownedPaidPrices"].as_array().unwrap().len(), 3);
    assert_eq!(details["ownedPaidPrices"][0]["price"], 3000.0);
    let scoped = c
        .command(
            "query",
            json!({"accountIds":["a"],"includeFiles":false,"presentation":"list"}),
        )
        .unwrap();
    assert_eq!(scoped["products"][0]["price"], 3300.0);
    assert_eq!(
        scoped["products"][0]["ownedCurrentPrices"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        scoped["products"][0]["ownedPaidPrices"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let scoped = c
        .command("product_details", json!({"id":"900001","accountIds":["b"]}))
        .unwrap();
    assert!(scoped["price"].is_null());
    assert_eq!(scoped["ownedPaidPrices"].as_array().unwrap().len(), 2);
    assert!(scoped["ownedPaidPrices"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| row["price"].is_null() && row["currency"].is_null()));
    assert!(scoped["ownedCurrentPrices"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["variantId"] == "900013" && v["price"].is_null()));
    assert_eq!(strings(&details["listingAvatars"]).len(), 4);
    assert_eq!(strings(&details["ownedAvatars"]).len(), 4);
    let account_a = details["ownedAvatarEvidence"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["accountId"] == "a")
        .count();
    let account_b = details["ownedAvatarEvidence"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["accountId"] == "b")
        .count();
    assert_eq!(account_a, 4);
    assert_eq!(account_b, 1);
    for (scope, matched) in [
        (json!(["a"]), 1),
        (json!(["b"]), 0),
        (json!([]), 0),
        (Value::Null, 1),
    ] {
        let result = c
            .command(
                "query",
                json!({"avatar":"booth:4993931","accountIds":scope}),
            )
            .unwrap();
        assert_eq!(result["matched"], matched);
    }
    let mut public_only = source.clone();
    public_only["id"] = json!("900002");
    c.put("product", "900002", public_only).unwrap();
    assert_eq!(
        strings(&dynamic_details(&c, "900002")["listingAvatars"]).len(),
        4
    );
    assert!(strings(&dynamic_details(&c, "900002")["ownedAvatars"]).is_empty());
    c.command(
        "overlay",
        json!({"ids":["900001"],"patch":{"ownedAvatars":["Human base"]}}),
    )
    .unwrap();
    assert_eq!(
        dynamic_details(&c, "900001")["ownedAvatars"],
        json!(["Human base"])
    );
    c.command(
        "overlay",
        json!({"ids":["900001"],"patch":{},"clearFields":["ownedAvatars"]}),
    )
    .unwrap();
    assert_eq!(
        strings(&dynamic_details(&c, "900001")["ownedAvatars"]).len(),
        4
    );
    assert_eq!(c.get("remote_file", "a-file").unwrap(), Some(a_file));
    assert_eq!(c.get("remote_file", "b-file").unwrap(), Some(b_file));
    drop(c);
    let c = fixture.open();
    assert_eq!(
        strings(&dynamic_details(&c, "900001")["ownedAvatars"]).len(),
        4
    );
    c.put("entitlement","a-ent",json!({"id":"a-ent","productId":"900001","accountId":"a","fileIds":[],"manifestComplete":true})).unwrap();
    assert_eq!(
        strings(&dynamic_details(&c, "900001")["ownedAvatars"]).len(),
        1
    );
    assert_eq!(
        strings(&dynamic_details(&c, "900001")["listingAvatars"]).len(),
        4
    );
    assert_eq!(
        c.command(
            "query",
            json!({"avatar":"booth:4993931","accountIds":["a"]})
        )
        .unwrap()["matched"],
        0
    );
}

#[test]
fn spending_summary_counts_only_unique_proven_payments_with_exact_currency_totals() {
    let fixture = Fixture::new();
    let c = fixture.open();
    for (id, provider) in [
        ("booth-paid", "booth"),
        ("usd-paid", "jinxxy"),
        ("unknown", "gumroad"),
        ("local:free", "local"),
    ] {
        c.put("product",id,json!({"id":id,"provider":provider,"title":format!("Authored {id}"),"price":999999,"priceCurrency":"USD"})).unwrap();
    }
    let add = |id: &str,
               product: &str,
               account: &str,
               fact: &str,
               amount: Value,
               currency: &str| {
        c.put("acquisition",id,json!({"id":id,"productId":product,"accountId":account,"source":"purchase","sourceRecordId":fact,"orderId":"same-order","paidPrice":amount,"paidCurrency":currency,"paidPriceBasis":"productLineTotal","variantPayments":[{"variantId":"v","price":amount,"currency":currency}]})).unwrap();
    };
    add("line-a", "booth-paid", "a", "line-a", json!(100), "JPY");
    add("duplicate", "booth-paid", "a", "line-a", json!(100), "JPY");
    add("line-b", "booth-paid", "a", "line-b", json!(200), "JPY");
    add(
        "other-account",
        "booth-paid",
        "b",
        "line-a",
        json!(50),
        "JPY",
    );
    add("usd-one", "usd-paid", "a", "one", json!(0.1), "USD");
    add("usd-two", "usd-paid", "a", "two", json!(0.2), "USD");
    add(
        "usd-duplicate-grant",
        "usd-paid",
        "a",
        "one",
        json!(0.1),
        "USD",
    );
    add(
        "local-false-paid",
        "local:free",
        "a",
        "local",
        json!(1000),
        "USD",
    );
    c.put("acquisition","unproven",json!({"id":"unproven","productId":"unknown","accountId":"a","source":"purchase","sourceRecordId":"unknown","paidPrice":777,"paidCurrency":"USD","paidPriceBasis":"listingPrice"})).unwrap();
    let selection =
        json!({"productIds":["booth-paid","usd-paid","unknown","local:free","booth-paid"]});
    let result = c.command("spending_summary", selection.clone()).unwrap();
    assert_eq!(
        result["totals"],
        json!([
            {"currency":"JPY","amount":"350","minorUnits":"350","fractionDigits":0},
            {"currency":"USD","amount":"0.30","minorUnits":"30","fractionDigits":2}
        ])
    );
    assert_eq!(result["skippedCount"], 2);
    assert_eq!(result["products"].as_array().unwrap().len(), 4);
    assert_eq!(result["products"][0]["title"], "Authored booth-paid");
    let mut scoped = selection.clone();
    scoped["accountIds"] = json!(["a"]);
    assert_eq!(
        c.command("spending_summary", scoped.clone()).unwrap()["totals"][0]["minorUnits"],
        "300"
    );
    scoped["accountIds"] = json!([]);
    assert!(c.command("spending_summary", scoped).unwrap()["totals"]
        .as_array()
        .unwrap()
        .is_empty());
    // Conflicting observations of one fact are unknown, never an invented sum.
    add("conflict", "usd-paid", "a", "one", json!(0.4), "USD");
    let conflicted = c.command("spending_summary", selection).unwrap();
    assert_eq!(conflicted["totals"][1]["amount"], "0.20");
    assert_eq!(
        conflicted["products"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["productId"] == "usd-paid")
            .unwrap()["unknownPaymentCount"],
        1
    );
    add(
        "fractional",
        "unknown",
        "a",
        "fractional",
        json!(0.001),
        "USD",
    );
    add("three-digits", "unknown", "a", "kwd", json!(1.234), "KWD");
    let precise = c
        .command("spending_summary", json!({"productIds":["unknown"]}))
        .unwrap();
    assert_eq!(
        precise["totals"],
        json!([{"currency":"KWD","amount":"1.234","minorUnits":"1234","fractionDigits":3}])
    );
    assert_eq!(precise["products"][0]["unknownPaymentCount"], 2);
}
