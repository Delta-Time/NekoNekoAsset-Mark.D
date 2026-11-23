use super::*;
fn fixture() -> (std::path::PathBuf, ChatStore) {
    let path = std::env::temp_dir().join(format!("nna-ai-chat-authored-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&path).unwrap();
    let store = ChatStore::new(&path).unwrap();
    (path, store)
}
#[test]
fn optional_chat_storage_opens_on_demand_and_retries_without_replacing_data() {
    let (path, store) = fixture();
    let database = path.join("ai-chat.sqlite3");
    assert!(!database.exists());
    std::fs::write(&database, b"authored corrupt chat database").unwrap();
    assert!(store.workspace().is_err());
    assert_eq!(
        std::fs::read(&database).unwrap(),
        b"authored corrupt chat database"
    );
    std::fs::remove_file(&database).unwrap();
    assert!(store.workspace().unwrap().chats.is_empty());
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn durable_history_memory_and_delete_remove_images_and_logs() {
    let (path, store) = fixture();
    let created = store.create_chat("outfit").unwrap();
    let id = created.chat.id.as_str();
    let request = uuid::Uuid::new_v4().to_string();
    let image = AiImage {
        mime_type: "image/png".into(),
        data_base64: "AUTHORED_IMAGE_MARKER".into(),
    };
    store
        .append_user(id, &request, "Authored blue outfit", vec![image])
        .unwrap();
    let reply = json!({"answer":"Authored reply","recommendations":[{"title":"Authored jacket","reason":"Blue style","url":"https://maker.gumroad.com/l/jacket"}]});
    store.finish(id, &request, &Ok(reply)).unwrap();
    assert!(store
        .append_user(id, &request, "duplicate", vec![])
        .is_err());
    store.rename_chat(id, "Named chat").unwrap();
    let (memory, images) = store.context(id, "next", &[]).unwrap();
    assert_eq!(memory.len(), 2);
    assert_eq!(images.len(), 1);
    assert!(memory[1].content.contains("Authored jacket"));
    assert!(!memory[1].content.contains("https://"));
    drop(store);
    let store = ChatStore::new(&path).unwrap();
    assert_eq!(store.chat(id).unwrap().chat.title, "Named chat");
    store.delete_chat(id).unwrap();
    assert!(store.chat(id).is_err());
    assert!(store
        .finish(id, &request, &Ok(json!({"answer":"late"})))
        .is_err());
    let conn = store.lock().unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM request_logs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(conn);
    drop(store);
    assert!(!std::fs::read(path.join("ai-chat.sqlite3"))
        .unwrap()
        .windows("AUTHORED_IMAGE_MARKER".len())
        .any(|w| w == b"AUTHORED_IMAGE_MARKER"));
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn interrupted_messages_do_not_become_model_memory() {
    let (path, store) = fixture();
    let c = store.create_chat("chat").unwrap();
    let id = c.chat.id.as_str();
    let req = uuid::Uuid::new_v4().to_string();
    store.append_user(id, &req, "Interrupted", vec![]).unwrap();
    drop(store);
    let store = ChatStore::new(&path).unwrap();
    assert_eq!(
        store.chat(id).unwrap().messages[0].status,
        Some(TurnStatus::Cancelled)
    );
    assert!(store.context(id, "next", &[]).unwrap().0.is_empty());
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn presets_exchange_only_prompt_fields_and_have_new_identity() {
    let (path, store) = fixture();
    let p = store
        .save_preset(CustomPreset {
            id: None,
            title: "Authored preset".into(),
            description: "説明".into(),
            system_prompt: "Use authored style advice".into(),
        })
        .unwrap();
    let id = p.preset.id.as_str();
    let raw = store.export_preset(id).unwrap().json;
    assert!(!raw.contains(id));
    let imported = store.import_preset(&raw).unwrap();
    assert_ne!(imported.preset.id, id);
    assert_eq!(imported.preset.system_prompt, "Use authored style advice");
    assert!(store
        .import_preset(&raw.replace("\"version\": 1", "\"version\": 99"))
        .is_err());
    assert!(store
        .import_preset(&raw.replace("\"title\":", "\"apiKey\":\"secret\",\"title\":"))
        .is_err());
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn memory_is_bounded_and_current_photos_take_priority() {
    let (path, store) = fixture();
    let c = store.create_chat("chat").unwrap();
    let id = c.chat.id.as_str();
    for _ in 0..10 {
        let req = uuid::Uuid::new_v4().to_string();
        store
            .append_user(id, &req, &"x".repeat(4000), vec![])
            .unwrap();
        store
            .finish(
                id,
                &req,
                &Ok(json!({"answer":"y".repeat(4000),"recommendations":[]})),
            )
            .unwrap();
    }
    let current = vec![
        AiImage {
            mime_type: "image/png".into(),
            data_base64: "new".into()
        };
        4
    ];
    let (memory, images) = store.context(id, "next", &current).unwrap();
    assert!(memory.len() <= 12);
    assert!(memory.iter().map(|m| m.content.len()).sum::<usize>() <= 32000);
    assert_eq!(images.len(), 4);
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn prompt_snapshot_and_explicit_title_survive_preset_changes() {
    let (path, store) = fixture();
    let saved = store
        .save_preset(CustomPreset {
            id: None,
            title: "Authored".into(),
            description: "Original".into(),
            system_prompt: "Original prompt".into(),
        })
        .unwrap();
    let preset_id = saved.preset.id.as_str();
    let created = store.create_chat(preset_id).unwrap();
    let id = created.chat.id.as_str();
    store.rename_chat(id, "Explicit title").unwrap();
    store
        .save_preset(CustomPreset {
            id: Some(preset_id.into()),
            title: "Updated".into(),
            description: "Updated".into(),
            system_prompt: "Changed prompt".into(),
        })
        .unwrap();
    assert_eq!(store.chat_preset(id).unwrap().1, "Original prompt");
    let request = uuid::Uuid::new_v4().to_string();
    store
        .append_user(id, &request, "Automatic title candidate", vec![])
        .unwrap();
    store.finish(id,&request,&Ok(json!({"answer":"Reply","recommendations":[{"title":"Authored accessory","reason":"Style","productId":"local:authored"}]}))).unwrap();
    assert_eq!(store.chat(id).unwrap().chat.title, "Explicit title");
    assert_eq!(
        store.context_product_ids(id, "next").unwrap(),
        vec!["local:authored"]
    );
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}
