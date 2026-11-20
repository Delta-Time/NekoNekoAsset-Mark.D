use super::*;
use serde_json::{json, Value};
use tokio::net::TcpListener;

pub(super) fn registry(port: u16) -> Registry {
    serde_json::from_value(json!({"version":1,"instanceId":"a".repeat(64),"sessionId":"11111111-1111-4111-8111-111111111111","processId":42,"projectName":"Authored","projectRoot":"F:/AuthoredUnity","unityVersion":"2022.3.62f1","host":"127.0.0.1","port":port,"capability":"b".repeat(64),"heartbeatUtc":chrono::Utc::now().to_rfc3339(),"enabled":true,"busy":false})).unwrap()
}

fn response(registry: &Registry, request: &RequestIdentity, code: &str) -> BridgeResponse {
    serde_json::from_value(json!({"version":1,"ok":true,"code":code,"instanceId":registry.instance_id,"sessionId":registry.session_id,"requestId":request.request_id,"receipt":{"requestId":request.request_id,"fingerprint":request.fingerprint,"instanceId":request.instance_id,"sessionId":request.session_id,"stage":if code=="accepted" {"staging"} else {"waitingForUser"},"staged":code!="accepted","writesStarted":false,"updatedUtc":chrono::Utc::now().to_rfc3339(),"error":null,"compileErrors":null,"results":null},"projectName":null,"projectRoot":null,"unityVersion":null})).unwrap()
}

fn manifest(registry: &Registry, bytes: &[u8]) -> (RequestIdentity, String, String) {
    let entry_id = "33333333-3333-4333-8333-333333333333".to_string();
    let text = serde_json::to_string(&Manifest {
        version: 1,
        locale: None,
        instance_id: registry.instance_id.clone(),
        session_id: registry.session_id.clone(),
        entries: vec![ManifestEntry {
            entry_id: entry_id.clone(),
            kind: "file".into(),
            display_name: "Authored.txt".into(),
            relative_path: "Authored.txt".into(),
            bytes: bytes.len() as i64,
            sha256: hex::encode(Sha256::digest(bytes)),
            product_id: "local:authored".into(),
            file_id: "authored-file".into(),
            revision: "r1".into(),
            member_path: String::new(),
        }],
    })
    .unwrap();
    let request = RequestIdentity {
        request_id: "22222222-2222-4222-8222-222222222222".into(),
        instance_id: registry.instance_id.clone(),
        session_id: registry.session_id.clone(),
        fingerprint: hex::encode(Sha256::digest(text.as_bytes())),
    };
    (request, text, entry_id)
}

async fn read_command(stream: &mut TcpStream) -> Value {
    let mut prefix = [0; 4];
    stream.read_exact(&mut prefix).await.unwrap();
    let mut bytes = vec![0; u32::from_le_bytes(prefix) as usize];
    stream.read_exact(&mut bytes).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn framing_rejects_zero_oversize_invalid_utf8_and_truncated_json() {
    for bytes in [
        0u32.to_le_bytes().to_vec(),
        ((MAX_CONTROL_BYTES + 1) as u32).to_le_bytes().to_vec(),
        vec![1, 0, 0, 0, 255],
        vec![2, 0, 0, 0, b'{'],
    ] {
        let (mut writer, mut reader) = tokio::io::duplex(64);
        writer.write_all(&bytes).await.unwrap();
        writer.shutdown().await.unwrap();
        assert!(read_frame(&mut reader).await.is_err());
    }
}

#[test]
fn encoded_response_preserves_absent_preparation_counters() {
    let registry = registry(9000);
    let (request, _, _) = manifest(&registry, b"authored");
    for code in ["accepted", "staged", "status"] {
        let original = response(&registry, &request, code);
        let encoded = serde_json::to_vec(&original).unwrap();
        let decoded: BridgeResponse = serde_json::from_slice(&encoded).unwrap();
        validate_response(&decoded, &registry, Some(&request)).unwrap();
    }
}

#[test]
fn preparing_receipt_accepts_progress_without_changing_request_identity() {
    let registry = registry(9000);
    let (request, _, _) = manifest(&registry, b"authored");
    let mut preparing = response(&registry, &request, "staged");
    let receipt = preparing.receipt.as_mut().unwrap();
    receipt.stage = "preparing".into();
    receipt.preparation_phase = Some("readPackage".into());
    receipt.preparation_bytes = Some(50);
    receipt.preparation_total_bytes = Some(100);
    assert!(validate_transfer_response(&preparing, &registry, &request).is_ok());
    preparing.receipt.as_mut().unwrap().preparation_phase = None;
    preparing
        .receipt
        .as_mut()
        .unwrap()
        .preparation_waiting_for_editor = true;
    assert!(validate_transfer_response(&preparing, &registry, &request).is_ok());
    preparing.receipt.as_mut().unwrap().preparation_phase = Some("inventedPhase".into());
    assert!(validate_transfer_response(&preparing, &registry, &request).is_err());
    preparing.receipt.as_mut().unwrap().preparation_phase = Some("readPackage".into());
    preparing
        .receipt
        .as_mut()
        .unwrap()
        .preparation_waiting_for_editor = false;
    preparing.receipt.as_mut().unwrap().preparation_bytes = Some(101);
    assert!(validate_transfer_response(&preparing, &registry, &request).is_err());
}

#[test]
fn response_binds_current_listener_and_original_receipt_identity() {
    let mut registry = registry(9000);
    let (request, _, _) = manifest(&registry, b"authored");
    registry.session_id = "44444444-4444-4444-8444-444444444444".into();
    let original = response(&registry, &request, "status");
    assert!(validate_response(&original, &registry, Some(&request)).is_ok());
    for field in ["request", "instance", "session", "fingerprint", "envelope"] {
        let mut altered = original.clone();
        let receipt = altered.receipt.as_mut().unwrap();
        match field {
            "request" => receipt.request_id = "bad".into(),
            "instance" => receipt.instance_id = "c".repeat(64),
            "session" => receipt.session_id = registry.session_id.clone(),
            "fingerprint" => receipt.fingerprint = "c".repeat(64),
            _ => altered.session_id = request.session_id.clone(),
        }
        assert!(validate_response(&altered, &registry, Some(&request)).is_err());
    }
    let mut failed_without_receipt = original.clone();
    failed_without_receipt.code = "failed".into();
    failed_without_receipt.ok = false;
    failed_without_receipt.receipt = None;
    assert!(validate_transfer_response(&failed_without_receipt, &registry, &request).is_err());
    let staged = response(&registry, &request, "staged");
    assert!(validate_transfer_response(&staged, &registry, &request).is_ok());
}

#[test]
fn manifest_uses_exact_json_fingerprint_and_ordered_unique_safe_entries() {
    let registry = registry(9000);
    let (request, text, id) = manifest(&registry, b"authored");
    let files = vec![PayloadFile {
        entry_id: id,
        path: PathBuf::from("unused-authored"),
    }];
    assert!(validate_manifest(&registry, &request, &text, &files).is_ok());
    assert!(validate_manifest(&registry, &request, &format!("{text} "), &files).is_err());
    assert!(!safe_relative("../escape"));
    assert!(!safe_relative("Assets/CON.txt"));
    assert!(!safe_relative("F:/absolute"));
    assert!(!safe_relative("Assets\\file"));
}

#[test]
fn preflight_counts_outer_command_escaping_before_any_payload_or_connection() {
    let registry = registry(9000);
    let (mut request, text, _) = manifest(&registry, b"");
    let mut manifest: Manifest = serde_json::from_str(&text).unwrap();
    let mut entry = manifest.entries.remove(0);
    entry.display_name = "\"".repeat(1024);
    let files: Vec<_> = (1..=4096)
        .map(|index| {
            let mut current = entry.clone();
            current.entry_id = uuid::Uuid::from_u128(index).to_string();
            let file = PayloadFile {
                entry_id: current.entry_id.clone(),
                path: PathBuf::from("not-read-by-preflight"),
            };
            manifest.entries.push(current);
            file
        })
        .collect();
    let text = serde_json::to_string(&manifest).unwrap();
    assert!(text.len() < MAX_CONTROL_BYTES);
    request.fingerprint = hex::encode(Sha256::digest(text.as_bytes()));
    assert_eq!(
        preflight_submit(&registry, &request, &text, &files)
            .err()
            .unwrap()
            .to_string(),
        "UNITY_SELECTION_TOO_LARGE"
    );
}

#[tokio::test]
async fn existing_submit_status_sends_no_payload_even_after_listener_restart() {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let mut registry = registry(listener.local_addr().unwrap().port());
    let (request, text, id) = manifest(&registry, b"must not send");
    registry.session_id = "44444444-4444-4444-8444-444444444444".into();
    let reply = response(&registry, &request, "status");
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        assert_eq!(read_command(&mut socket).await["command"], "submit");
        write_frame(&mut socket, &reply).await.unwrap();
        let mut byte = [0];
        assert_eq!(socket.read(&mut byte).await.unwrap(), 0);
    });
    let files = [PayloadFile {
        entry_id: id,
        path: PathBuf::from("does-not-exist-no-read"),
    }];
    assert_eq!(
        submit(&registry, &request, &text, &files)
            .await
            .unwrap()
            .code,
        "status"
    );
    task.await.unwrap();
}

#[tokio::test]
async fn fresh_submit_streams_large_payload_then_eof_before_durable_ack() {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let registry = registry(listener.local_addr().unwrap().port());
    let bytes = vec![0x5a; 3 * 1024 * 1024 + 7];
    let (request, text, id) = manifest(&registry, &bytes);
    let path = std::env::temp_dir().join(format!("nna-unity-authored-{}", uuid::Uuid::new_v4()));
    tokio::fs::write(&path, &bytes).await.unwrap();
    let accepted = response(&registry, &request, "accepted");
    let staged = response(&registry, &request, "staged");
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let command = read_command(&mut socket).await;
        assert_eq!(command["command"], "submit");
        write_frame(&mut socket, &accepted).await.unwrap();
        let mut hash = Sha256::new();
        let mut total = 0;
        let mut buffer = [0; 65536];
        loop {
            let count = socket.read(&mut buffer).await.unwrap();
            if count == 0 {
                break;
            }
            total += count;
            hash.update(&buffer[..count]);
        }
        assert_eq!(total, bytes.len());
        assert_eq!(
            hex::encode(hash.finalize()),
            hex::encode(Sha256::digest(&bytes))
        );
        write_frame(&mut socket, &staged).await.unwrap();
        socket.shutdown().await.unwrap();
    });
    let result = submit(
        &registry,
        &request,
        &text,
        &[PayloadFile {
            entry_id: id,
            path: path.clone(),
        }],
    )
    .await;
    tokio::fs::remove_file(path).await.unwrap();
    assert!(result.unwrap().receipt.unwrap().staged);
    task.await.unwrap();
}

#[tokio::test]
async fn native_import_names_one_path_and_returns_the_bridge_answer() {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let registry = registry(listener.local_addr().unwrap().port());
    let path = r"E:\Authored Library\Outfit.unitypackage";
    let reply = |code: &str| -> BridgeResponse {
        serde_json::from_value(json!({"version":1,"ok":code=="accepted","code":code,"instanceId":registry.instance_id,"sessionId":registry.session_id,"requestId":"","enabled":true,"busy":false})).unwrap()
    };
    let replies = [reply("accepted"), reply("invalidRequest")];
    let task = tokio::spawn(async move {
        for reply in replies {
            let (mut socket, _) = listener.accept().await.unwrap();
            let command = read_command(&mut socket).await;
            assert_eq!(command["command"], "nativeImport");
            assert_eq!(command["path"], path);
            for absent in ["requestId", "fingerprint", "manifestJson"] {
                assert!(command.get(absent).is_none(), "{absent}");
            }
            write_frame(&mut socket, &reply).await.unwrap();
        }
    });
    // An older Bridge's refusal is an answer, not a failure, so the App can fall back.
    for code in ["accepted", "invalidRequest"] {
        assert_eq!(native_import(&registry, path).await.unwrap().code, code);
    }
    task.await.unwrap();
}

#[tokio::test]
async fn changed_snapshot_hash_never_returns_a_staged_ack() {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let registry = registry(listener.local_addr().unwrap().port());
    let (request, text, id) = manifest(&registry, b"authored-original");
    let path = std::env::temp_dir().join(format!("nna-unity-authored-{}", uuid::Uuid::new_v4()));
    tokio::fs::write(&path, b"authored-modified").await.unwrap();
    let accepted = response(&registry, &request, "accepted");
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_command(&mut socket).await;
        write_frame(&mut socket, &accepted).await.unwrap();
        let mut received = Vec::new();
        socket.read_to_end(&mut received).await.unwrap();
        assert_eq!(received, b"authored-modified");
    });
    let result = submit(
        &registry,
        &request,
        &text,
        &[PayloadFile {
            entry_id: id,
            path: path.clone(),
        }],
    )
    .await;
    tokio::fs::remove_file(path).await.unwrap();
    assert_eq!(result.err().unwrap().to_string(), "UNITY_SOURCE_CHANGED");
    task.await.unwrap();
}
