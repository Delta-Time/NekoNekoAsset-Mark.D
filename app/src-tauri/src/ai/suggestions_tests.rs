use super::*;
#[test]
fn personalization_prompt_applies_language_and_style_without_avatar() {
    let personal = Personalization {
        avatar_data_url: Some("AUTHORED_AVATAR_MARKER".into()),
        user_name: "Authored user".into(),
        reply_language: chat_store::ReplyLanguage::ZhHant,
        personality: Personality::OlderSister,
        custom_prompt: String::new(),
    };
    let system = conversation_system(
        "Authored custom purpose",
        &personal,
        Locale::En,
        &json!({"candidates":[]}),
    )
    .unwrap();
    assert!(system.contains("Traditional Chinese"));
    assert!(system.contains("older-sister-style"));
    assert!(system.contains("Authored user"));
    assert!(system.contains("Authored custom purpose"));
    assert!(!system.contains("AUTHORED_AVATAR_MARKER"));
    let app = Personalization::default();
    assert!(conversation_system("chat", &app, Locale::Ja, &json!({}))
        .unwrap()
        .contains("Reply in Japanese"));
}
#[test]
fn existing_chat_reads_current_custom_style_at_each_reply() {
    let path = std::env::temp_dir().join(format!("nna-ai-style-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&path).unwrap();
    let store = ChatStore::new(&path).unwrap();
    let chat = store.create_chat("chat").unwrap();
    let id = chat.chat.id.as_str();
    for marker in ["AUTHORED_STYLE_ONE", "AUTHORED_STYLE_TWO"] {
        let personal = Personalization {
            personality: Personality::Custom,
            custom_prompt: marker.into(),
            ..Default::default()
        };
        store.save_personalization(personal).unwrap();
        let (_, preset) = store.chat_preset(id).unwrap();
        let system = conversation_system(
            &preset,
            &store.personalization().unwrap(),
            Locale::En,
            &json!({}),
        )
        .unwrap();
        assert!(system.contains(marker));
        assert!(system.contains("overrides styles in remembered answers"));
        if marker.ends_with("TWO") {
            assert!(!system.contains("AUTHORED_STYLE_ONE"));
        }
    }
    assert!(store
        .save_personalization(Personalization {
            personality: Personality::Custom,
            ..Default::default()
        })
        .is_err());
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn thumbnail_render_parameters_are_canonicalized_without_auth_queries() {
    assert_eq!(
        public_thumbnail_url("https://booth.pximg.net/authored.png?w=300&h=300&q=80")
            .unwrap()
            .as_str(),
        "https://booth.pximg.net/authored.png"
    );
    for raw in [
        "https://booth.pximg.net/authored.png?token=hidden",
        "https://booth.pximg.net/authored.png?w=300&unknown=1",
        "https://evil.invalid/authored.png?w=300",
    ] {
        assert!(public_thumbnail_url(raw).is_none());
    }
}
#[test]
fn explicit_dropped_images_are_normalized_without_file_paths() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let dir = std::env::temp_dir().join(format!("nna-ai-drop-authored-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("authored.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([40, 80, 160, 255]))
        .save(&path)
        .unwrap();
    let images = read_images(vec![path.to_string_lossy().into()]).unwrap();
    assert_eq!(images.len(), 1);
    assert!(!serde_json::to_string(&images)
        .unwrap()
        .contains("authored.png"));
    let avatar = format!(
        "data:{};base64,{}",
        images[0].mime_type, images[0].data_base64
    );
    let normalized = normalize_avatar(Some(avatar)).unwrap().unwrap();
    let bytes = STANDARD
        .decode(normalized.split_once(",").unwrap().1)
        .unwrap();
    let image = image::load_from_memory(&bytes).unwrap();
    assert!(image.width() <= 256 && image.height() <= 256);
    assert!(normalize_avatar(Some("data:image/svg+xml;base64,PHN2Zz4=".into())).is_err());
    assert!(read_images(vec!["relative.png".into()]).is_err());
    assert!(read_images(vec![path.to_string_lossy().into(); 5]).is_err());
    let huge = dir.join("huge.png");
    std::fs::File::create(&huge)
        .unwrap()
        .set_len(17 * 1024 * 1024)
        .unwrap();
    assert_eq!(
        read_images(vec![huge.to_string_lossy().into()])
            .err()
            .unwrap()
            .to_string(),
        "ai_image_too_large"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
#[tokio::test]
async fn saved_library_previews_follow_moved_profile_and_current_visibility() {
    let fixture = std::env::temp_dir().join(format!(
        "nna-ai-chat-preview-authored-{}",
        uuid::Uuid::new_v4()
    ));
    let profile = fixture.join("original");
    let moved = fixture.join("moved");
    let product_id = "local:authored-preview";
    let thumbnail = "media/authored.png";
    let original_path = profile.join(thumbnail).to_string_lossy().into_owned();
    let store_thumbnail = "https://public-files.gumroad.com/authored.png";
    let chat_id = {
        let catalogue = Catalogue::open(&profile).unwrap();
        std::fs::create_dir_all(profile.join("media")).unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([40, 80, 160, 255]))
            .save(profile.join(thumbnail))
            .unwrap();
        catalogue
            .put(
                "product",
                product_id,
                json!({"id":product_id,"provider":"local","custom":true,"title":"Authored jacket"}),
            )
            .unwrap();
        catalogue
            .put(
                "overlay",
                product_id,
                json!({"id":product_id,"thumbnailPath":thumbnail}),
            )
            .unwrap();
        let ai = Arc::new(AiService::new(&profile).unwrap());
        let service = AiSuggestions::new(catalogue, ai).unwrap();
        let id = service.chats.create_chat("chat").unwrap().chat.id;
        let request_id = uuid::Uuid::new_v4().to_string();
        service
            .chats
            .append_user(&id, &request_id, "Authored request", vec![])
            .unwrap();
        service.chats.finish(&id, &request_id, &Ok(json!({"answer":"Authored answer","recommendations":[
                {"origin":"library","productId":product_id,"title":"Authored jacket","thumbnailPath":original_path,"thumbnailUrl":"https://i.pximg.net/obsolete.png"},
                {"origin":"store","title":"Authored public item","thumbnailUrl":store_thumbnail}
            ]}))).unwrap();
        id
    };
    // Close the authored databases before moving the complete portable profile.
    std::fs::rename(&profile, &moved).unwrap();
    let catalogue = Catalogue::open(&moved).unwrap();
    let ai = Arc::new(AiService::new(&moved).unwrap());
    let service = AiSuggestions::new(catalogue.clone(), ai).unwrap();
    let request = || AiSuggestionRequest::GetChat {
        chat_id: chat_id.clone(),
    };
    let chat = service.command(request()).await.unwrap();
    let rows = &chat["messages"][1]["response"]["recommendations"];
    assert_eq!(
        rows[0]["thumbnailPath"],
        moved.join(thumbnail).to_string_lossy().as_ref()
    );
    assert!(moved.join(thumbnail).is_file());
    assert!(rows[0].get("thumbnailUrl").is_none());
    assert_eq!(rows[1]["thumbnailUrl"], store_thumbnail);
    catalogue
        .put(
            "overlay",
            product_id,
            json!({"id":product_id,"thumbnailPath":thumbnail,"thumbnailHidden":true}),
        )
        .unwrap();
    let hidden = service.command(request()).await.unwrap();
    let row = &hidden["messages"][1]["response"]["recommendations"][0];
    assert!(row.get("thumbnailPath").is_none() && row.get("thumbnailUrl").is_none());
    // Rehydration is read-time presentation; the saved transcript remains untouched.
    let stored = service.chats.chat(&chat_id).unwrap();
    assert_eq!(
        stored.messages[1].response.as_ref().unwrap()["recommendations"][0]["thumbnailPath"],
        original_path
    );
    drop(service);
    drop(catalogue);
    std::fs::remove_dir_all(fixture).unwrap();
}
#[tokio::test]
async fn deleting_a_pending_chat_cancels_and_cannot_resurrect_it() {
    let dir = std::env::temp_dir().join(format!("nna-ai-delete-authored-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let catalogue = Catalogue::open(&dir).unwrap();
    let mut preferences = catalogue.get("preferences", "app").unwrap().unwrap();
    preferences["aiFeaturesEnabled"] = json!(true);
    catalogue.put("preferences", "app", preferences).unwrap();
    let ai = Arc::new(AiService::new(&dir).unwrap());
    let entry = crate::ai::AiConfig {
        entry_id: Some(uuid::Uuid::new_v4().to_string()),
        provider: "local".into(),
        endpoint: "http://localhost:1234/v1".into(),
        model: "authored-no-network".into(),
    }
    .validated()
    .unwrap();
    crate::ai::provider::save_entry(&dir.join("ai-settings.json"), entry).unwrap();
    let service = Arc::new(AiSuggestions::new(catalogue, ai).unwrap());
    let created = service
        .command(AiSuggestionRequest::CreateChat {
            preset_id: "chat".into(),
        })
        .await
        .unwrap();
    let id = created["chat"]["id"].as_str().unwrap().to_owned();
    let serial = service.serial.lock().await;
    let worker = service.clone();
    let request:AiSuggestionRequest=serde_json::from_value(json!({"action":"send","requestId":uuid::Uuid::new_v4().to_string(),"chatId":id,"appLanguage":"en","prompt":"Authored cancellation","preset":"chat","scope":"library","stores":[],"category":null,"avatarId":null,"history":[],"images":[]})).unwrap();
    let pending = tokio::spawn(async move { worker.command(request).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if service.chats.chat(&id).unwrap().messages.len() == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    service
        .command(AiSuggestionRequest::DeleteChat {
            chat_id: id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(
        pending.await.unwrap().err().unwrap().to_string(),
        "ai_cancelled"
    );
    assert!(service.chats.chat(&id).is_err());
    assert!(service.active.lock().await.is_empty());
    drop(serial);
    drop(service);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn recommendations_require_observed_candidates_and_preserve_library_identity() {
    let candidate = Candidate {
        candidate_id: "library:local:synthetic".into(),
        origin: Origin::Library,
        product_id: Some("local:synthetic".into()),
        title: "Authored jacket".into(),
        provider: "local".into(),
        url: None,
        thumbnail_url: Some("https://public-files.gumroad.com/authored.png".into()),
        facts: json!({}),
    };
    let good=json!({"answer":"A matching style; compatibility is unknown.","recommendations":[{"candidateId":candidate.candidate_id,"reason":"Matching style"}]}).to_string();
    let response = hydrate_reply(&good, &[candidate.clone()], vec![]).unwrap();
    let recommendation = &response.recommendations[0];
    assert_eq!(
        recommendation.product_id.as_deref(),
        Some("local:synthetic")
    );
    assert!(recommendation.url.is_none());
    assert_eq!(
        recommendation.thumbnail_url.clone().flatten().as_deref(),
        Some("https://public-files.gumroad.com/authored.png")
    );
    assert!(!serde_json::to_string(&candidate)
        .unwrap()
        .contains("authored.png"));
    assert!(hydrate_reply(
        &good.replace("local:synthetic", "invented"),
        &[candidate.clone()],
        vec![]
    )
    .is_err());
    assert!(hydrate_reply(
        &good.replace("Matching style", "https://maker.gumroad.com/l/invented"),
        &[candidate],
        vec![]
    )
    .is_err());
}
#[test]
fn public_store_parsers_accept_bound_listings_and_reject_capability_links() {
    let booth = r#"<li class="item-card" data-product-id="900001" data-product-name="Authored hair" data-product-price="0"><a class="item-card__category-anchor">3D Hair</a><a href="https://booth.pm/en/items/900001">Item</a><img data-original="https://booth.pximg.net/authored.png" src="data:image/png;base64,placeholder"></li>"#;
    let rows = parse_store(Store::Booth, booth).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].facts["free"], true);
    assert_eq!(
        rows[0].thumbnail_url.as_deref(),
        Some("https://booth.pximg.net/authored.png")
    );
    let page = json!({"props":{"search_results":{"products":[{"native_type":"digital","name":"Authored shader","url":"https://maker.gumroad.com/l/shader?layout=discover&recommended_by=search","price_cents":0,"thumbnail_url":"https://public-files.gumroad.com/authored.png"},{"native_type":"digital","name":"Unsafe","url":"https://maker.gumroad.com/l/secret?token=private","price_cents":0}]}}});
    let gum = format!("<div data-page='{}'></div>", page);
    let rows = parse_store(Store::Gumroad, &gum).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].url.as_deref(),
        Some("https://maker.gumroad.com/l/shader")
    );
    assert_eq!(
        rows[0].thumbnail_url.as_deref(),
        Some("https://public-files.gumroad.com/authored.png")
    );
    let transport = json!({"rehydrate":{"query":{"data":{"marketplace":{"listings":{"products":[{"service":"jinxxy","name":"Authored accessory","url":"asset","user":{"username":"maker"},"price_amount":100,"cover":{"url":"https://jinxxy-c.jinxxy-cdn.com/authored.png"}}]}}}}}});
    let jinx=format!("<script>(window[Symbol.for(\"ApolloSSRDataTransport\")] ??= []).push({transport});</script>");
    let rows = parse_store(Store::Jinxxy, &jinx).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].url.as_deref(),
        Some("https://jinxxy.com/maker/asset")
    );
    assert_eq!(rows[0].facts["free"], false);
    assert_eq!(
        rows[0].thumbnail_url.as_deref(),
        Some("https://jinxxy-c.jinxxy-cdn.com/authored.png")
    );
    assert!(parse_store(Store::Jinxxy, "<html>Sign in</html>").is_err());
}
