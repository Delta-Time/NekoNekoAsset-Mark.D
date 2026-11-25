//! Summaries, filters, sorting, paging, preferences and scale.
use super::*;

#[test]
fn all_summary_queries_include_results_after_10000_without_unbounding_file_payloads() {
    let fixture = Fixture::new();
    let c = fixture.open();
    c.inner.lock().unwrap().execute_batch("WITH RECURSIVE n(value) AS (VALUES(1) UNION ALL SELECT value+1 FROM n WHERE value<10001) INSERT INTO products(id,body,category,status,sensitive,downloaded,title,search,in_all,summary) SELECT CAST(value AS TEXT),json_object('id',CAST(value AS TEXT),'files',json_array('AUTHORED_DETAIL')),'Tool','notDownloaded',0,0,printf('%05d',value),'authored',1,json_object('id',CAST(value AS TEXT),'filesLoaded',0) FROM n").unwrap();
    let query = |args| c.command("query", args).unwrap();
    let all = query(
        json!({"allSummaries":true,"includeFiles":false,"presentation":"list","sort":"title"}),
    );
    assert_eq!(all["matched"], 10001);
    assert_eq!(all["products"].as_array().unwrap().len(), 10001);
    assert_eq!(all["products"][10000]["id"], "10001");
    let details =
        query(json!({"allSummaries":true,"includeFiles":true,"presentation":"list","limit":20000}));
    assert_eq!(details["products"].as_array().unwrap().len(), 10000);
}

#[test]
fn status_filter_survives_every_sort_order() {
    let fixture = Fixture::new();
    let c = fixture.open();
    product(&c, "wanted");
    product(&c, "other");
    // The record a failed download of its file left.
    c.put(
        "entitlement",
        "e",
        json!({"id":"e","productId":"wanted","fileIds":["f"],"manifestComplete":true}),
    )
    .unwrap();
    c.put(
        "remote_file",
        "f",
        json!({"id":"f","productId":"wanted","name":"authored.zip","revision":"r1"}),
    )
    .unwrap();
    c.put("artifact","pending-f",json!({"id":"pending-f","productId":"wanted","fileId":"f","revision":null,"status":"notDownloaded","errorCode":"network_failed","originalAvailable":false})).unwrap();
    for sort in ["title", "acquired", "price", "status"] {
        assert_eq!(
            c.command("query", json!({"status":"attention","sort":sort}))
                .unwrap()["matchedIds"],
            json!(["wanted"])
        );
    }
}

#[test]
fn undated_free_downloads_sort_after_every_date_by_their_free_history_order() {
    let fixture = Fixture::new();
    let c = fixture.open();
    let acquired = |product_id: &str, source: &str, fields: Value| {
        product(&c, product_id);
        let id = format!("a:{source}:{product_id}");
        let mut row = json!({"id":id,"productId":product_id,"accountId":"a","source":source});
        row.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        c.put("acquisition", &id, row).unwrap();
    };
    acquired(
        "dated-old",
        "purchase",
        json!({"acquiredDateText":"2026/09/01"}),
    );
    acquired(
        "dated-new",
        "purchase",
        json!({"acquiredDateText":"2026/10/01"}),
    );
    // One pass orders by position, newest first; a later pass sorts after it.
    let order = |pass: &str, position: u64| format!("{pass}{:012}", 999_999_999_999 - position);
    acquired(
        "free-page-2",
        "free",
        json!({"freeHistoryOrder":order("20261101000000", 1000)}),
    );
    acquired(
        "free-page-1",
        "free",
        json!({"freeHistoryOrder":order("20261101000000", 0)}),
    );
    acquired(
        "free-next-pass",
        "free",
        json!({"freeHistoryOrder":order("20261102000000", 7)}),
    );
    // A date wins over a free-history order on the same product.
    acquired(
        "bought-too",
        "free",
        json!({"freeHistoryOrder":order("20261103000000", 0)}),
    );
    acquired(
        "bought-too",
        "purchase",
        json!({"acquiredDateText":"2026/09/15"}),
    );
    product(&c, "unknown");
    for (accounts, last) in [(json!(null), vec!["unknown"]), (json!(["a"]), vec![])] {
        let ids = |direction: &str| {
            c.command(
                "query",
                json!({"sort":"acquired","direction":direction,"accountIds":accounts}),
            )
            .unwrap()["matchedIds"]
                .clone()
        };
        let expected = |ids: &[&str]| json!([ids, &last[..]].concat());
        assert_eq!(
            ids("desc"),
            expected(&[
                "dated-new",
                "bought-too",
                "dated-old",
                "free-next-pass",
                "free-page-1",
                "free-page-2"
            ])
        );
        assert_eq!(
            ids("asc"),
            expected(&[
                "dated-old",
                "bought-too",
                "dated-new",
                "free-page-2",
                "free-page-1",
                "free-next-pass"
            ])
        );
    }
    // Nothing is shown as a date.
    assert_eq!(projection(&c, "free-page-1")["acquired"], "");
}

#[test]
fn summary_status_progress_updates_leave_search_indexes_intact() {
    let f = Fixture::new();
    let c = f.open();
    c.put("product","p",json!({"id":"p","title":"Synthetic casual Hair","shop":"Synthetic shop","sourceShop":"Synthetic source shop","store":"BOOTH","sourceKind":1,"sourceUrl":"https://booth.pm/ja/items/900001","sourceCategory":{"name":"Hair"},"sourceTags":["Mafuyu"],"sourceDescription":"large detail only","price":800,"publicVariants":[{"id":"v","name":"Owned","price":800,"currency":"JPY"}]})).unwrap();
    c.put("remote_file","file",json!({"id":"file","productId":"p","accountId":"a","name":"Needle_Manuka.unitypackage","revision":"r","variantId":"v"})).unwrap();
    c.put("entitlement","ent",json!({"id":"ent","productId":"p","accountId":"a","fileIds":["file"],"manifestComplete":true})).unwrap();
    c.put("acquisition","acq",json!({"id":"acq","productId":"p","accountId":"a","source":"purchase","acquiredDateText":"2026/10/01","paidPrice":500,"paidCurrency":"JPY","paidPriceBasis":"productLineTotal"})).unwrap();
    c.command("overlay", json!({"ids":["p"],"patch":{"title":"Edited Synthetic Hair","tags":["Synthetic List Tag","Mafuyu"],"rejectedTags":["Synthetic Rejected Tag"],"note":"Keep this editor baseline"}})).unwrap();
    // A write to either index now fails deterministically, proving local status
    // updates do not rebuild filename/tag indexes.
    c.inner.lock().unwrap().execute_batch("CREATE TRIGGER synthetic_preserve_grams BEFORE DELETE ON search_grams BEGIN SELECT RAISE(FAIL,'index changed'); END;").unwrap();
    c.put(
        "job",
        "j",
        json!({"id":"j","productId":"p","type":"download","state":"running","progress":1}),
    )
    .unwrap();
    let before = c.change_domains().unwrap();
    c.put("job","j",json!({"id":"j","productId":"p","type":"download","state":"running","progress":99,"updatedAt":"later"})).unwrap();
    let after = c.change_domains().unwrap();
    assert!(after.0 > before.0);
    assert_eq!(after.1, before.1);
    c.put("artifact","artifact",json!({"id":"artifact","productId":"p","fileId":"file","revision":"r","status":"ready","originalAvailable":true,"extractedInventory":[{"path":"Detail only"}]})).unwrap();
    c.put(
        "job",
        "j",
        json!({"id":"j","productId":"p","type":"download","state":"completed"}),
    )
    .unwrap();
    let query = c
        .command(
            "query",
            json!({"search":"Needle","includeFiles":false,"accountIds":["a"]}),
        )
        .unwrap();
    let summary = &query["products"][0];
    assert_eq!(summary["status"], "ready");
    assert_eq!(summary["paidPrice"], 500.0);
    assert_eq!(summary["paidCurrency"], "JPY");
    assert_eq!(summary["price"], 800.0);
    assert_eq!(summary["acquired"], "2026/10/01");
    assert_eq!(summary["title"], "Edited Synthetic Hair");
    assert_eq!(summary["sourceTitle"], "Synthetic casual Hair");
    assert_eq!(summary["titleOverride"], true);
    assert_eq!(summary["sourceTags"], json!(["Mafuyu"]));
    assert_eq!(summary["rejectedTags"], json!(["Synthetic Rejected Tag"]));
    assert_eq!(summary["accountIds"], json!(["a"]));
    assert_eq!(summary["manifestComplete"], true);
    assert_eq!(summary["fullyDownloaded"], true);
    assert_eq!(summary["downloaded"], 1);
    assert_eq!(summary["total"], 1);
    assert!(!summary["tagSuggestions"].as_array().unwrap().is_empty());
    assert!(summary["tagSuggestions"][0]["basis"].is_string());
    let evidence = summary["ownedAvatarEvidence"].as_array().unwrap();
    assert!(evidence
        .iter()
        .any(|row| row["accountId"] == "a" && row["avatarId"].is_string()));
    assert!(evidence
        .iter()
        .any(|row| row["source"] == "sourceTag" && row["entitlementEstablished"] == false));
    assert!(summary["tags"]
        .as_array()
        .unwrap()
        .contains(&json!("Synthetic List Tag")));
    assert_eq!(summary["note"], "Keep this editor baseline");
    assert_eq!(summary["filesLoaded"], false);
    assert!(summary["files"].as_array().unwrap().is_empty());
    assert!(summary.get("sourceDescription").is_none());
    assert!(!summary["provenance"].as_array().unwrap().is_empty());
    let list = c
        .command(
            "query",
            json!({"search":"Needle","includeFiles":false,"presentation":"list","accountIds":["a"]}),
        )
        .unwrap();
    let mut expected_list = query.clone();
    for field in [
        "provenance",
        "categoryInference",
        "sources",
        "sourceShop",
        "sourceOriginUrl",
        "publicPageAvailability",
        "store",
        "sourceKind",
    ] {
        expected_list["products"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
    }
    expected_list["products"][0]["tagSuggestions"] = json!(summary["tagSuggestions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| json!({"label":row["label"]}))
        .collect::<Vec<_>>());
    expected_list["products"][0]["ownedAvatarEvidence"] = json!(evidence
        .iter()
        .map(|row| {
            let mut kept = json!({"avatar":row["avatar"],"source":row["source"]});
            if let Some(account) = row.get("accountId") {
                kept["accountId"] = account.clone();
            }
            kept
        })
        .collect::<Vec<_>>());
    // Preserve scoped paid/date values, matching IDs/counts, effective tags and
    // compatibility facts, and every other existing editor baseline.
    assert_eq!(list, expected_list);
    let detail = c
        .command("product_details", json!({"id":"p","accountIds":["a"]}))
        .unwrap();
    for field in [
        "tagSuggestions",
        "sourceShop",
        "sourceOriginUrl",
        "publicPageAvailability",
        "store",
        "sourceKind",
        "price",
        "paidPrice",
        "paidCurrency",
        "acquired",
        "tags",
        "ownedAvatars",
        "listingAvatars",
    ] {
        assert_eq!(detail[field], summary[field], "{field}");
    }
    assert!(detail["ownedAvatarEvidence"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["avatarId"].is_string()));
    let full = c
        .command(
            "query",
            json!({"search":"Needle","presentation":"list","accountIds":["a"]}),
        )
        .unwrap();
    assert_eq!(full["products"][0], detail);
    assert_eq!(
        c.command(
            "query",
            json!({"search":"Needle","includeFiles":false,"accountIds":["a"]})
        )
        .unwrap(),
        query
    );
    let avatar = evidence.iter().find(|row| row["accountId"] == "a").unwrap()["avatarId"].clone();
    let avatar_list = c
        .command(
            "query",
            json!({"avatar":avatar,"includeFiles":false,"presentation":"list","accountIds":["a"]}),
        )
        .unwrap();
    assert_eq!(avatar_list["matchedIds"], json!(["p"]));
    assert_eq!(avatar_list["products"], list["products"]);
    assert_eq!(
        detail["files"][0]["extractedInventory"][0]["path"],
        "Detail only"
    );
    assert_eq!(
        c.command("bootstrap", json!({"includeProducts":false}))
            .unwrap()["total"],
        1
    );
}

#[test]
fn indexed_all_scope_excludes_foreign_apps_and_respects_manual_categories() {
    let f = Fixture::new();
    let c = f.open();
    for (id, tags, description) in [
        (
            "windows",
            json!(["Windows", "ツール"]),
            "Export generated output for VRChat",
        ),
        (
            "unity",
            json!(["Windows", "Unity", "ツール"]),
            "Unity editor tool",
        ),
        (
            "vr",
            json!(["Windows", "VRChat", "ツール"]),
            "Authored tool",
        ),
        (
            "ae-shader",
            json!(["AfterEffects", "Shader"]),
            "Authored AE effect",
        ),
        ("unknown-vr", json!(["VRChat"]), ""),
    ] {
        c.put("product",id,json!({"id":id,"sourceTitle":"Synthetic product","sourceTags":tags,"sourceDescription":description})).unwrap();
    }
    let q = c.command("query", json!({})).unwrap();
    assert_eq!(q["matched"], 3);
    assert_eq!(q["counts"]["all"], 3);
    assert!(!strings(&q["matchedIds"]).contains(&"windows".to_owned()));
    assert!(!strings(&q["matchedIds"]).contains(&"ae-shader".to_owned()));
    assert_eq!(
        c.command("query", json!({"category":"Shader"})).unwrap()["matchedIds"],
        json!(["ae-shader"])
    );
    c.command(
        "overlay",
        json!({"ids":["ae-shader"],"patch":{"category":"Shader"}}),
    )
    .unwrap();
    c.command(
        "overlay",
        json!({"ids":["unknown-vr"],"patch":{"category":"NonVRChat"}}),
    )
    .unwrap();
    let q = c.command("query", json!({})).unwrap();
    assert_eq!(q["matched"], 3);
    assert_eq!(q["counts"]["all"], 3);
    assert!(strings(&q["matchedIds"]).contains(&"ae-shader".to_owned()));
    assert!(!strings(&q["matchedIds"]).contains(&"unknown-vr".to_owned()));
    drop(c);
    let c = f.open();
    assert_eq!(c.command("query", json!({})).unwrap()["counts"]["all"], 3);
}

#[test]
fn defaults_scope_r18_and_unknown_last() {
    let f = Fixture::new();
    let c = f.open();
    product(&c, "1");
    c.put(
        "product",
        "2",
        json!({"id":"2","title":"Synthetic illustration","tags":[],"price":1}),
    )
    .unwrap();
    assert_eq!(
        c.command("query", json!({})).unwrap()["matchedIds"],
        json!(["1"])
    );
    assert_eq!(
        c.command("query", json!({"category":"NonVRChat"})).unwrap()["matchedIds"],
        json!(["2"])
    );
    c.command("overlay", json!({"ids":["1"],"patch":{"sensitive":true}}))
        .unwrap();
    assert_eq!(
        c.command("query", json!({"sensitive":"hide"})).unwrap()["matchedIds"],
        json!([])
    );
    assert_eq!(
        c.command("query", json!({"sensitive":"only"})).unwrap()["matchedIds"],
        json!(["1"])
    );
    assert_eq!(
        c.get("preferences", "app").unwrap().unwrap()["autoDownloadAll"],
        false
    );
    let prefs = c
        .command(
            "preferences",
            json!({"patch":{"autoDownloadAll":false,"downloadConcurrency":99}}),
        )
        .unwrap();
    assert_eq!(prefs["autoDownloadAll"], false);
    assert_eq!(prefs["downloadConcurrency"], 6);
    c.command("preferences", json!({"patch":{"autoDownloadAll":true}}))
        .unwrap();
    {
        let field = "setupComplete";
        assert_eq!(
            c.command("preferences", json!({"patch":{field:true}}))
                .unwrap_err()
                .to_string(),
            "library_not_configured"
        );
    }
    c.put(
        "root",
        "setup-root",
        json!({"id":"setup-root","path":f.0,"status":"needsMapping","active":true}),
    )
    .unwrap();
    c.command(
        "preferences",
        json!({"patch":{"libraryRootId":"setup-root"}}),
    )
    .unwrap();
    assert!(c
        .command("preferences", json!({"patch":{"setupComplete":true}}))
        .is_err());
    c.put(
        "root",
        "setup-root",
        json!({"id":"setup-root","path":f.0,"status":"online","active":true}),
    )
    .unwrap();
    let complete = c
        .command("preferences", json!({"patch":{"setupComplete":true}}))
        .unwrap();
    assert_eq!(complete["autoDownloadAll"], true);
    c.put(
        "root",
        "setup-root",
        json!({"id":"setup-root","status":"offline"}),
    )
    .unwrap();
    assert!(c
        .command("preferences", json!({"patch":{"theme":"dark"}}))
        .is_ok());
    drop(c);
    assert_eq!(
        f.open().get("preferences", "app").unwrap().unwrap()["autoDownloadAll"],
        true
    );
}

#[test]
#[ignore = "Manual release-scale verification: independently authored 10k products / 300k file records"]
fn release_scale_10000_products_300000_files() {
    use std::time::Instant;
    // Explicit ignored runs retain synthetic fixtures even after a panic, so a
    // failed sync measurement never requires another twelve-minute population.
    let reuse = std::env::var("NNA_SCALE_REUSE_FIXTURE").ok();
    let f = std::mem::ManuallyDrop::new(if let Some(name) = &reuse {
        let suffix = name
            .strip_prefix("nna-catalogue-synthetic-")
            .expect("synthetic fixture name required");
        Uuid::parse_str(suffix).expect("synthetic fixture UUID required");
        assert!(!name.contains(['/', '\\', ':']));
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/catalogue-synthetic")
            .canonicalize()
            .unwrap();
        let path = base.join(name).canonicalize().unwrap();
        assert_eq!(path.parent(), Some(base.as_path()));
        // The lead explicitly identified this completed pre-marker fixture.
        assert!(
            path.join("scale-fixture.json").is_file()
                || suffix == "6263b886-7762-4c8d-bc19-4d87fce1d25d"
        );
        Fixture(path)
    } else {
        Fixture::new()
    });
    println!(
        "synthetic_fixture={}",
        f.0.file_name().unwrap().to_string_lossy()
    );
    let opened = Instant::now();
    let c = f.open();
    println!(
        "database_open_ms={} reused={}",
        opened.elapsed().as_millis(),
        reuse.is_some()
    );
    let start = Instant::now();
    for batch in 0..if reuse.is_some() { 0 } else { 100 } {
        let mut records = vec![];
        for n in batch * 100..(batch + 1) * 100 {
            let id = format!("{}", 900000000 + n);
            let mut file_ids = vec![];
            records.push(("product".into(),id.clone(),json!({"id":id,"title":format!("Synthetic ＡＢＣ 髪型 商品 {n:05}"),"shop":format!("Synthetic Maker {}",n%50),"sourceCategory":{"name":"Hair"},"tags":["hair","Casual"],"price":if n%9==0{None}else{Some(n%1000)},"sensitive":n%7==0})));
            for file in 0..30 {
                let fid = format!("{id}:file:{file}");
                file_ids.push(fid.clone());
                records.push(("remote_file".into(),fid.clone(),json!({"id":fid,"productId":id,"accountId":"synthetic","name":format!("Synthetic_説明_{n:05}_{file:02}.unitypackage"),"revision":"synthetic-v1","sizeBytes":12345})));
            }
            records.push(("entitlement".into(),id.clone(),json!({"id":id,"productId":id,"accountId":"synthetic","fileIds":file_ids,"manifestComplete":true})));
        }
        c.put_many(records).unwrap();
        if batch % 10 == 9 {
            println!(
                "seed_products={} seed_files={} elapsed_ms={}",
                (batch + 1) * 100,
                (batch + 1) * 3000,
                start.elapsed().as_millis()
            );
        }
    }
    println!("synthetic_population_ms={}", start.elapsed().as_millis());
    {
        let conn = c.inner.lock().unwrap();
        for (kind, expected) in [
            ("product", 10000),
            ("entitlement", 10000),
            ("remote_file", 300000),
        ] {
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM entities WHERE kind=?1", [kind], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(count, expected);
        }
        let products: i64 = conn.query_row("SELECT COUNT(*) FROM entities WHERE kind='product' AND CAST(id AS INTEGER) BETWEEN 900000000 AND 900009999 AND json_extract(body,'$.title') GLOB 'Synthetic ＡＢＣ 髪型 商品 *'", [], |r| r.get(0)).unwrap();
        let files: i64 = conn.query_row("SELECT COUNT(*) FROM entities WHERE kind='remote_file' AND json_extract(body,'$.accountId')='synthetic' AND json_extract(body,'$.revision')='synthetic-v1' AND json_extract(body,'$.name') GLOB 'Synthetic_説明_*.unitypackage'", [], |r| r.get(0)).unwrap();
        let projections: i64 = conn
            .query_row("SELECT COUNT(*) FROM products", [], |r| r.get(0))
            .unwrap();
        assert_eq!((products, files, projections), (10000, 300000, 10000));
    }
    let marker = json!({"kind":"independently-authored-scale-fixture","version":1,"products":10000,"remoteFiles":300000});
    let marker_path = f.0.join("scale-fixture.json");
    if marker_path.exists() {
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(&marker_path).unwrap()).unwrap(),
            marker
        );
    } else {
        std::fs::write(&marker_path, serde_json::to_vec_pretty(&marker).unwrap()).unwrap();
    }
    let begin = Instant::now();
    let boot = c
        .command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    assert!(boot["products"].as_array().unwrap().is_empty());
    println!("lean_bootstrap_ms={}", begin.elapsed().as_millis());
    let mut timings = vec![];
    for i in 0..15 {
        let search = ["髪", "ＡＢＣ", "説明_00001", "casual", "maker 3"][i % 5];
        let begin = Instant::now();
        let result=c.command("query",json!({"search":search,"includeFiles":false,"limit":100,"sort":(["title","acquired","price"][i%3]),"direction":if i%2==0{"asc"}else{"desc"}})).unwrap();
        let serialized = serde_json::to_vec(&result).unwrap();
        assert!(!serialized.is_empty());
        timings.push(begin.elapsed().as_micros());
    }
    timings.sort();
    let p95 = timings[14];
    println!(
        "query_samples=15 query_p95_us={p95} query_min_us={} query_median_us={}",
        timings[0], timings[7]
    );
    // Cold history validation is a background task after restart; catalogue
    // reads must remain available while its strict decoder is working.
    drop(c);
    let c = f.open();
    let begin = Instant::now();
    c.command("collection",json!({"operation":"create","name":"Synthetic cold collection","id":format!("scale-cold-{}",Uuid::new_v4()),"productIds":["900000001"]})).unwrap();
    let cold_collection_ms = begin.elapsed().as_millis();
    let pending: i64 = c
        .inner
        .lock()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM sync_outbox", [], |row| row.get(0))
        .unwrap();
    println!("cold_collection_commit_ms={cold_collection_ms} pending_sync_intents={pending}");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let complete = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let reader = c.clone();
    let reader_barrier = barrier.clone();
    let reader_complete = complete.clone();
    let concurrent = std::thread::spawn(move || {
        let mut samples = vec![];
        reader_barrier.wait();
        while !reader_complete.load(std::sync::atomic::Ordering::Relaxed) || samples.len() < 15 {
            let started = Instant::now();
            let result = reader
                .command(
                    "query",
                    json!({"search":"髪","includeFiles":false,"limit":100}),
                )
                .unwrap();
            let _bytes = serde_json::to_vec(&result).unwrap();
            samples.push(started.elapsed().as_micros());
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        samples
    });
    barrier.wait();
    let begin = Instant::now();
    let prepared = c.ensure_document();
    let cold_ms = begin.elapsed().as_millis();
    complete.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut cold_samples = concurrent.join().unwrap();
    let (copy_us, decode_us) = prepared.unwrap();
    cold_samples.sort();
    println!("cold_cache_prepare_ms={cold_ms} cold_copy_and_lock_wait_us={copy_us} cold_decode_outside_lock_us={decode_us} cold_query_samples={} cold_query_p95_us={} cold_query_max_us={}",cold_samples.len(),cold_samples[((cold_samples.len()*95).div_ceil(100)).saturating_sub(1)],cold_samples.last().unwrap());
    drop(c);
    let begin = Instant::now();
    let reopened = f.open();
    println!("local_open_ms={}", begin.elapsed().as_millis());
    let begin = Instant::now();
    reopened
        .command("bootstrap", json!({"includeProducts":false}))
        .unwrap();
    println!("restart_lean_bootstrap_ms={}", begin.elapsed().as_millis());
    println!("target_query_p95_under_200ms={}", p95 <= 200_000);
}

#[test]
fn related_lookup_plan_bounds_product_and_preserves_order() {
    let f = Fixture::new();
    let c = f.open();
    let conn = c.inner.lock().unwrap();
    for (id, product) in [("z", "1"), ("a", "1"), ("other", "2")] {
        set_raw(
            &conn,
            "remote_file",
            id,
            &json!({"id":id,"productId":product,"name":"Synthetic.unitypackage"}),
        )
        .unwrap();
    }
    let details: Vec<String> = conn.prepare("EXPLAIN QUERY PLAN SELECT body FROM entities WHERE kind=?1 AND product_id=?2 ORDER BY id").unwrap()
        .query_map(params!["remote_file","1"], |r| r.get(3)).unwrap()
        .collect::<rusqlite::Result<_>>().unwrap();
    assert!(
        details
            .iter()
            .any(|detail| detail.contains("entities_product_order")
                && detail.contains("kind=? AND product_id=?")),
        "{details:?}"
    );
    assert!(
        !details.iter().any(|detail| detail.contains("TEMP B-TREE")),
        "{details:?}"
    );
    let found = related(&conn, "remote_file", "1").unwrap();
    assert_eq!(
        found.iter().map(|v| text(v, "id")).collect::<Vec<_>>(),
        ["a", "z"]
    );
}

#[test]
fn current_ui_preferences_validate_explicit_patches_without_backfill() {
    let fixture = Fixture::new();
    let c = fixture.open();
    let initial = c.get("preferences", "app").unwrap().unwrap();
    assert_eq!(initial["downloadConcurrency"], 3);
    assert_eq!(initial["aiFeaturesEnabled"], false);
    assert_eq!(initial["sensitiveMode"], "blur");
    assert_eq!(initial["batchActions"], default_batch_actions());
    assert_eq!(initial["cardColumns"], json!(["shop"]));
    assert_eq!(
        initial["listColumns"],
        json!(["shop", "category", "files", "acquired", "price"])
    );
    let saved = c.command("preferences",json!({"patch":{"aiFeaturesEnabled":true,"sensitiveMode":"only","batchActions":{"spending":false},"cardColumns":["price","category"]}})).unwrap();
    assert_eq!(saved["batchActions"]["spending"], false);
    assert_eq!(saved["batchActions"]["download"], true);
    assert_eq!(saved["cardColumns"], json!(["price", "category"]));
    assert_eq!(saved["listColumns"], initial["listColumns"]);
    let saved = c
        .command("preferences", json!({"patch":{"listColumns":[]}}))
        .unwrap();
    assert_eq!(saved["cardColumns"], json!(["price", "category"]));
    assert_eq!(saved["listColumns"], json!([]));
    for patch in [
        json!({"aiFeaturesEnabled":"true"}),
        json!({"sensitiveMode":"invalid"}),
        json!({"batchActions":{"madeUp":true}}),
        json!({"batchActions":{"edit":0}}),
        json!({"cardColumns":"shop"}),
        json!({"cardColumns":["unknown"]}),
        json!({"cardColumns":["shop","shop"]}),
        json!({"listColumns":["price",false]}),
        json!({"listColumns":null}),
    ] {
        assert!(c.command("preferences", json!({"patch":patch})).is_err());
        assert_eq!(c.get("preferences", "app").unwrap().unwrap(), saved);
    }
    drop(c);
    let c = fixture.open();
    assert_eq!(c.get("preferences", "app").unwrap().unwrap(), saved);
    // Sparse explicitly authored preferences stay sparse on open.
    c.put("preferences", "app", json!({"id":"app","locale":"ja"}))
        .unwrap();
    drop(c);
    let c = fixture.open();
    let sparse = c.get("preferences", "app").unwrap().unwrap();
    for key in ["batchActions", "cardColumns", "listColumns"] {
        assert!(sparse.get(key).is_none());
    }
}

#[test]
fn query_all_downloaded_covers_all_matches_before_pagination() {
    let fixture = Fixture::new();
    let c = fixture.open();
    c.put(
        "root",
        "r",
        json!({"id":"r","path":"X:/authored","status":"online"}),
    )
    .unwrap();
    for id in ["a-ready", "z-pending"] {
        product(&c, id);
        c.put(
            "remote_file",
            id,
            json!({"id":id,"productId":id,"name":"authored.unitypackage","revision":"r1"}),
        )
        .unwrap();
        c.put(
            "entitlement",
            id,
            json!({"id":id,"productId":id,"fileIds":[id],"manifestComplete":true}),
        )
        .unwrap();
    }
    let artifact = |id: &str| json!({"id":id,"productId":id,"fileId":id,"rootId":"r","revision":"r1","status":"ready","relativePath":format!("{id}/authored.unitypackage")});
    c.put("artifact", "a-ready", artifact("a-ready")).unwrap();
    let args = json!({"sort":"title","limit":1,"includeFiles":false});
    let first = c.command("query", args.clone()).unwrap();
    assert_eq!(first["products"].as_array().unwrap().len(), 1);
    assert_eq!(first["products"][0]["id"], "a-ready");
    assert_eq!(first["products"][0]["fullyDownloaded"], true);
    assert_eq!(first["allDownloaded"], false);
    c.put("artifact", "z-pending", artifact("z-pending"))
        .unwrap();
    assert_eq!(c.command("query", args).unwrap()["allDownloaded"], true);
    assert_eq!(
        c.command("query", json!({"offset":100,"limit":0})).unwrap()["allDownloaded"],
        true
    );
    c.put("entitlement","z-pending",json!({"id":"z-pending","productId":"z-pending","fileIds":["z-pending"],"manifestComplete":false})).unwrap();
    assert_eq!(
        c.command("query", json!({"limit":1})).unwrap()["allDownloaded"],
        true
    );
    assert_eq!(
        c.command("query", json!({"search":"nothing-matches"}))
            .unwrap()["allDownloaded"],
        false
    );
}

#[test]
fn warm_list_actions_skip_stored_rows_and_edits_reread_only_their_row() {
    // Every list column follows the file-heavy body, so reading a stored row is
    // the network cost on a shared library. Warm searches, avatar and account
    // filters read the TEMP copy; an edit rereads only the edited product.
    let f = Fixture::new();
    let c = f.open();
    let mut records = vec![];
    for n in 0..60 {
        let id = format!("{}", 920000000 + n);
        let account = if n % 2 == 0 { "b" } else { "a" };
        let avatar = ["Manuka", "Kikyo", "Shinano"][n % 3];
        records.push(("product".into(), id.clone(), json!({"id":id,"title":format!("Authored outfit {n:02}"),"sourceCategory":{"name":"Outfit"},"tags":[avatar,"Casual"],"description":"Authored description. ".repeat(80)})));
        let file_ids: Vec<_> = (0..30).map(|file| format!("{id}:file:{file}")).collect();
        for file in &file_ids {
            records.push(("remote_file".into(), file.clone(), json!({"id":file,"productId":id,"accountId":account,"name":format!("{file}_authored.unitypackage"),"revision":"authored-v1","sizeBytes":123456})));
        }
        records.push(("entitlement".into(), format!("{account}:{id}"), json!({"id":format!("{account}:{id}"),"productId":id,"accountId":account,"fileIds":file_ids,"manifestComplete":true})));
    }
    c.put_many(records).unwrap();
    // A tiny library cache turns each stored-row read into a miss; the TEMP copy stays cached.
    let stored: i64 = {
        let conn = c.inner.lock().unwrap();
        conn.execute_batch("PRAGMA main.cache_size=8; PRAGMA temp.cache_size=-65536;")
            .unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM dbstat WHERE name='products'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    let list = |patch: Value, known: &Value| {
        let misses = || {
            let conn = c.inner.lock().unwrap();
            let (mut current, mut high) = (0, 0);
            unsafe {
                rusqlite::ffi::sqlite3_db_status(
                    conn.handle(),
                    rusqlite::ffi::SQLITE_DBSTATUS_CACHE_MISS,
                    &mut current,
                    &mut high,
                    0,
                )
            };
            i64::from(current)
        };
        let mut args = json!({"limit":10000,"includeFiles":false,"presentation":"list","allSummaries":true,"knownSummaries":known});
        for (key, value) in patch.as_object().unwrap() {
            args[key] = value.clone();
        }
        let before = misses();
        let result = c.command("query", args).unwrap();
        (result, misses() - before)
    };
    let (first, _) = list(json!({}), &json!({}));
    let known = first["fingerprints"].clone();
    assert_eq!(known.as_object().unwrap().len(), 60);
    for patch in [
        json!({"search":"outfit 1"}),
        json!({"avatar":"Manuka"}),
        json!({"accountIds":["a"],"includeOffline":true,"sort":"price"}),
        json!({"accountIds":["b"],"includeOffline":true,"avatar":"Kikyo"}),
    ] {
        let (result, read) = list(patch.clone(), &known);
        assert!(result["matched"].as_u64().unwrap() > 0, "{patch}");
        assert!(
            read * 10 < stored,
            "{patch} read {read} of {stored} product pages"
        );
    }
    c.command(
        "overlay",
        json!({"changes":[{"id":"920000007","patch":{"title":"Edited authored outfit"}}]}),
    )
    .unwrap();
    let (result, read) = list(json!({}), &known);
    let sent: Vec<_> = result["products"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| text(p, "id"))
        .collect();
    assert_eq!(sent, ["920000007"]);
    assert_eq!(result["products"][0]["title"], "Edited authored outfit");
    assert!(
        read * 10 < stored,
        "an edit reread {read} of {stored} product pages"
    );
}
