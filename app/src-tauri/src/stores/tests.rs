#[cfg(windows)]
#[test]
fn shared_account_views_follow_device_credentials_without_changing_ownership() {
    use tauri::webview::Cookie;
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!(
            "nna-authored-shared-session-{}",
            uuid::Uuid::new_v4()
        ));
    let library = base.join("library");
    let first_device = base.join("first-device");
    let second_device = base.join("second-device");
    std::fs::create_dir_all(&library).unwrap();
    let first = Catalogue::open_bound(&library, &first_device).unwrap();
    let service = StoreService::new(
        first.clone(),
        Arc::new(BoothService::new(first.clone()).unwrap()),
    )
    .unwrap();
    let mut expected = Vec::new();
    for provider in [
        StoreProvider::Booth,
        StoreProvider::Gumroad,
        StoreProvider::Jinxxy,
    ] {
        let id = uuid::Uuid::new_v4().to_string();
        let account = json!({"id":id,"store":provider.label(),"name":"Authored shared name","status":"connected","state":"connected","identityVerified":true,"identityHash":"authored-binding","disconnected":false});
        let entitlement = json!({"id":id,"accountId":id,"productId":"authored-product","fileIds":["authored-file"]});
        first.put("account", &id, account.clone()).unwrap();
        first.put("entitlement", &id, entitlement.clone()).unwrap();
        let host = match provider {
            StoreProvider::Booth => "booth.pm",
            StoreProvider::Gumroad => "gumroad.com",
            StoreProvider::Jinxxy => "jinxxy.com",
        };
        let profile = session::profile_path(&first.device_dir(), provider, &id).unwrap();
        session_cache::save(
            &profile,
            provider,
            &id,
            vec![Cookie::build(("authored_session", "AUTHORED_COOKIE_VALUE"))
                .domain(host)
                .path("/")
                .secure(true)
                .build()],
        )
        .unwrap();
        assert!(service.has_local_session(&account));
        expected.push((provider, id, account, entitlement));
    }
    assert!(!first.data_dir().join("auth").exists());
    drop(service);
    drop(first);
    let second = Catalogue::open_bound(&library, &second_device).unwrap();
    let service = StoreService::new(
        second.clone(),
        Arc::new(BoothService::new(second.clone()).unwrap()),
    )
    .unwrap();
    let views = service.account_views(second.list("account").unwrap());
    for (_, id, account, entitlement) in expected {
        let view = views.iter().find(|view| view["id"] == id).unwrap();
        assert_eq!(view["status"], "expired");
        assert_eq!(view["localSessionAvailable"], false);
        assert_eq!(view["disconnected"], false);
        assert_eq!(view["name"], account["name"]);
        assert_eq!(second.get("account", &id).unwrap(), Some(account));
        assert_eq!(second.get("entitlement", &id).unwrap(), Some(entitlement));
    }
    drop(service);
    drop(second);
    std::fs::remove_dir_all(base).unwrap();
}

#[tokio::test]
async fn store_execution_queue_is_shared_by_accounts_but_not_by_other_providers() {
    let path = std::env::temp_dir().join(format!("store-queues-{}", uuid::Uuid::new_v4()));
    let catalogue = crate::catalogue::Catalogue::open(&path).unwrap();
    let service = super::StoreService::new(
        catalogue.clone(),
        std::sync::Arc::new(crate::booth::BoothService::new(catalogue.clone()).unwrap()),
    )
    .unwrap();
    // These are the same gates acquired by the actual provider executors.
    let first_booth_account = service.booth.execution_gate().lock().await;
    assert!(service.booth.execution_gate().try_lock().is_err());
    let first_gumroad_account = service.gumroad.execution_gate().try_lock().unwrap();
    assert!(service.gumroad.execution_gate().try_lock().is_err());
    drop(first_booth_account);
    assert!(service.booth.execution_gate().try_lock().is_ok());
    drop(first_gumroad_account);
    assert!(service.gumroad.execution_gate().try_lock().is_ok());
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(path).unwrap();
}
use super::*;
#[test]
fn account_display_name_validation_is_bounded() {
    assert_eq!(
        normalized_account_name(&json!("  Authored\n name  ")).unwrap(),
        "Authored name"
    );
    assert!(normalized_account_name(&json!(" \n\t ")).is_err());
    assert!(normalized_account_name(&Value::Null).is_err());
    assert_eq!(
        normalized_account_name(&json!("猫".repeat(201)))
            .unwrap()
            .chars()
            .count(),
        200
    );
}
#[test]
fn account_rename_preserves_native_identity_and_entitlements_across_restart() {
    let root = std::env::var("NNA_STORE_EVIDENCE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    let path = root.join(format!("rename-{}", uuid::Uuid::new_v4()));
    let catalogue = Catalogue::open(&path).unwrap();
    let service = StoreService::new(
        catalogue.clone(),
        Arc::new(BoothService::new(catalogue.clone()).unwrap()),
    )
    .unwrap();
    let mut expected = Vec::new();
    for provider in [
        StoreProvider::Booth,
        StoreProvider::Gumroad,
        StoreProvider::Jinxxy,
    ] {
        let id = uuid::Uuid::new_v4().to_string();
        let mut account = json!({"id":id,"store":provider.label(),"name":"Before","label":"Before","identityHash":"authored-identity","identityVerified":true,"state":"connected","status":"connected","disconnected":false});
        catalogue.put("account", &id, account.clone()).unwrap();
        let entitlement = json!({"id":id,"accountId":id,"productId":"authored-product","manifestComplete":true,"fileIds":["authored-file"]});
        catalogue
            .put("entitlement", &id, entitlement.clone())
            .unwrap();
        service.rename_account(&id, "After").unwrap();
        account["name"] = json!("After");
        account["label"] = json!("After");
        expected.push((id, account, entitlement));
    }
    drop(service);
    drop(catalogue);
    let reopened = Catalogue::open(&path).unwrap();
    for (id, account, entitlement) in expected {
        assert_eq!(reopened.get("account", &id).unwrap(), Some(account));
        assert_eq!(reopened.get("entitlement", &id).unwrap(), Some(entitlement));
    }
}

#[test]
fn explicit_store_cannot_override_persisted_account_or_job_provider() {
    for provider in [
        StoreProvider::Booth,
        StoreProvider::Gumroad,
        StoreProvider::Jinxxy,
    ] {
        assert_eq!(require_provider(None, provider).unwrap(), provider);
        for requested in [
            StoreProvider::Booth,
            StoreProvider::Gumroad,
            StoreProvider::Jinxxy,
        ] {
            assert_eq!(
                require_provider(Some(requested), provider).is_ok(),
                requested == provider
            );
        }
    }
    assert!(requested_provider(&json!({"store":null})).is_err());
    assert!(requested_provider(&json!({"store":"unknown"})).is_err());
    assert!(requested_provider(&json!({"store":"BOOTH","provider":"gumroad"})).is_err());
}

#[test]
fn mixed_selected_update_groups_are_namespace_bound_and_preflight_all_accounts() {
    let path = std::env::temp_dir().join(format!(
        "nna-authored-store-groups-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&path).unwrap();
    let catalogue = Catalogue::open(&path).unwrap();
    let service = StoreService::new(
        catalogue.clone(),
        Arc::new(BoothService::new(catalogue.clone()).unwrap()),
    )
    .unwrap();
    for (id, store) in [
        ("900001", "BOOTH"),
        ("gumroad:900001", "Gumroad"),
        ("jinxxy:900001", "Jinxxy"),
    ] {
        catalogue
            .put(
                "product",
                id,
                json!({"id":id,"store":store,"title":"Authored product"}),
            )
            .unwrap();
    }
    let local_id = "local:aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    catalogue.put("product", local_id, json!({"id":local_id,"provider":"local","store":"Local Files","title":"Authored local product"})).unwrap();
    let args = json!({"productIds":["900001","gumroad:900001","jinxxy:900001","900001",local_id]});
    let groups = service.selected_groups(&args).unwrap();
    assert_eq!(groups.len(), 3);
    assert_eq!(
        groups[&StoreProvider::Booth],
        BTreeSet::from(["900001".into()])
    );
    assert_eq!(
        groups[&StoreProvider::Gumroad],
        BTreeSet::from(["gumroad:900001".into()])
    );
    assert_eq!(
        groups[&StoreProvider::Jinxxy],
        BTreeSet::from(["jinxxy:900001".into()])
    );
    let local_groups = service
        .selected_groups(&json!({"productIds":[local_id]}))
        .unwrap();
    assert!(local_groups.is_empty());
    assert!(service.preflight_updates(&local_groups, &json!({})).is_ok());
    for (id, provider, store) in [
        ("local:conflicting", "local", "Gumroad"),
        ("gumroad:conflicting", "local", "Local Files"),
        ("local:unknown", "unknown", "Local Files"),
        ("unknown:product", "unknown", "Unknown Store"),
    ] {
        catalogue.put("product", id, json!({"id":id,"provider":provider,"store":store,"title":"Authored invalid identity"})).unwrap();
        assert!(service
            .selected_groups(&json!({"productIds":[local_id,id,"900001"]}))
            .is_err());
    }
    let mut conflicting = args;
    conflicting["store"] = json!("BOOTH");
    assert!(service.selected_groups(&conflicting).is_err());
    assert!(service.preflight_updates(&groups, &json!({})).is_err());
    assert!(catalogue.list("job").unwrap().is_empty());
    let accounts = [
        (
            "ffffffff-ffff-4fff-8fff-ffffffffffff",
            "Gumroad",
            "gumroad:900001",
        ),
        (
            "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
            "Jinxxy",
            "jinxxy:900001",
        ),
    ];
    for (account, store, product) in accounts {
        catalogue.put("account", account, json!({"id":account,"store":store,"status":"connected","state":"connected","identityVerified":true,"identityHash":"authored-buyer-binding"})).unwrap();
        catalogue.put("entitlement", account, json!({"id":account,"accountId":account,"productId":product,"fileIds":[],"manifestComplete":false})).unwrap();
    }
    let historical = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
    catalogue.put("account", historical, json!({"id":historical,"store":"Gumroad","status":"disconnected","state":"disconnected","identityVerified":false,"identityHash":"authored-old-binding"})).unwrap();
    catalogue.put("entitlement", historical, json!({"id":historical,"accountId":historical,"productId":"gumroad:900001","fileIds":[]})).unwrap();
    assert!(service.preflight_updates(&groups, &json!({})).is_ok());
    assert_eq!(
        service
            .preflight_updates(&groups, &json!({"accountId":historical}))
            .unwrap_err()
            .to_string(),
        "AUTH_REQUIRED"
    );
    catalogue.put("job", "authored-job", json!({"id":"authored-job","type":"gumroadUpdate","store":"Gumroad","accountId":accounts[0].0,"state":"running"})).unwrap();
    assert_eq!(
        service
            .preflight_updates(&groups, &json!({}))
            .unwrap_err()
            .to_string(),
        "ACCOUNT_BUSY"
    );
    drop(service);
    drop(catalogue);
    let _ = std::fs::remove_dir_all(path);
}
