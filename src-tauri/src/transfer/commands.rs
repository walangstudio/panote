use crate::{
    crypto::note::encrypt_with_vault,
    db::queries::{self, NoteRow},
    state::{now_secs, AppState, TransportKind},
    transfer::{blob::TransferBlob, lan::decrypt_transfer},
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;
use if_addrs;

// ---- Receiving toggle ----

#[tauri::command]
pub async fn start_receiving(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;

    if state.receiving.load(Ordering::Relaxed) {
        return Ok(());
    }
    state.receiving.store(true, Ordering::Relaxed);

    let s = std::sync::Arc::new(state.inner().clone());
    let handle = app_handle.clone();
    let task = tokio::spawn(async move {
        if let Err(e) = super::lan::start_listener(s, handle).await {
            eprintln!("[lan] listener error: {e}");
        }
    });

    *state.listener_task.lock().unwrap() = Some(task);
    Ok(())
}

#[tauri::command]
pub fn stop_receiving(state: State<'_, AppState>) {
    use std::sync::atomic::Ordering;

    state.receiving.store(false, Ordering::Relaxed);
    if let Some(task) = state.listener_task.lock().unwrap().take() {
        task.abort();
    }
}

#[tauri::command]
pub fn is_receiving(state: State<'_, AppState>) -> bool {
    state.is_receiving()
}

// ---- Pairing code ----

/// Generate a random 6-character uppercase alphanumeric pairing code.
/// Excludes visually ambiguous characters (0, O, I, 1).
#[tauri::command]
pub fn generate_pairing_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..6)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

// ---- Manual peer add ----

/// Connect to a device by IP, send Hello, and add it to the peer list.
#[tauri::command]
pub async fn peer_add_manual(
    address: String,
    state: State<'_, AppState>,
) -> Result<PeerJson, String> {
    let port = super::lan::TRANSFER_PORT;
    let name = resolve_device_name(&state.db).await.unwrap_or_else(|_| "panote-device".into());
    let peer = super::lan::hello_probe(&state, &address, port, &name)
        .await
        .map_err(|e| e.to_string())?;
    let json = PeerJson {
        id: peer.id.clone(),
        name: peer.name.clone(),
        address: peer.address.clone(),
        port: peer.port,
        via: "lan".into(),
    };
    let mut peers = state.peers.lock().unwrap();
    peers.retain(|p| p.address != address);
    peers.push(peer);
    Ok(json)
}

// ---- Device name ----

#[tauri::command]
pub async fn get_device_name(state: State<'_, AppState>) -> Result<String, String> {
    resolve_device_name(&state.db)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_device_name(name: String, state: State<'_, AppState>) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() || name.len() > 64 {
        return Err("Name must be 1-64 characters".into());
    }
    queries::device_setting_set(&state.db, "device_name", &name)
        .await
        .map_err(|e| e.to_string())
}

/// Resolve the device name: user-set > env var > cert-hash fallback.
pub async fn resolve_device_name(pool: &sqlx::SqlitePool) -> anyhow::Result<String> {
    if let Some(name) = queries::device_setting_get(pool, "device_name").await? {
        return Ok(name);
    }
    if let Ok(name) = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")) {
        return Ok(format!("panote-{name}"));
    }
    // Generate from cert fingerprint for uniqueness on Android.
    if let Some(row) = queries::device_identity_get(pool).await? {
        let fp = crate::crypto::tls::cert_fingerprint(&row.cert_der);
        let short: String = fp[..3].iter().map(|b| format!("{b:02x}")).collect();
        return Ok(format!("panote-{short}"));
    }
    Ok("panote-device".into())
}

/// Return this device's local IPv4 addresses, filtering out VPN/virtual adapters.
#[tauri::command]
pub fn device_ips() -> Vec<String> {
    const VPN_PREFIXES: &[&str] = &[
        "tun", "utun", "tap", "wg", "proton", "vpn", "docker", "veth", "br-",
        "virbr", "vmnet", "vbox", "tailscale",
    ];
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|iface| {
            if let if_addrs::IfAddr::V4(v4) = iface.addr {
                if v4.ip.is_loopback() {
                    return None;
                }
                let name = iface.name.to_lowercase();
                if VPN_PREFIXES.iter().any(|pat| name.starts_with(pat)) {
                    return None;
                }
                // Filter Tailscale/CGNAT range 100.64.0.0/10
                let octets = v4.ip.octets();
                if octets[0] == 100 && (octets[1] & 0xC0) == 64 {
                    return None;
                }
                return Some(v4.ip.to_string());
            }
            None
        })
        .collect()
}

// ---- Peer discovery ----

#[derive(Debug, Serialize, Deserialize)]
pub struct PeerJson {
    pub id: String,
    pub name: String,
    pub address: String,
    pub port: u16,
    pub via: String,
}

#[tauri::command]
pub async fn peers_scan(state: State<'_, AppState>) -> Result<Vec<PeerJson>, String> {
    let peers = state.peers.lock().unwrap();
    Ok(peers
        .iter()
        .map(|p| PeerJson {
            id: p.id.clone(),
            name: p.name.clone(),
            address: p.address.clone(),
            port: p.port,
            via: match p.via {
                TransportKind::Lan => "lan".into(),
                TransportKind::Ble => "ble".into(),
            },
        })
        .collect())
}

// ---- Note send ----

#[tauri::command]
pub async fn note_send(
    note_id: String,
    peer_id: String,
    passphrase: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let peer = {
        let peers = state.peers.lock().unwrap();
        peers
            .iter()
            .find(|p| p.id == peer_id)
            .map(|p| (p.address.clone(), p.port, p.via.clone(), p.name.clone()))
            .ok_or("peer not found")?
    };

    let (address, port, via, peer_name) = peer;
    let device_name = resolve_device_name(&state.db).await.unwrap_or_else(|_| "panote-device".into());
    let result = match via {
        TransportKind::Lan => {
            super::lan::send_note(&state, &note_id, &address, port, &passphrase, &device_name).await
        }
        TransportKind::Ble => {
            super::ble::send_note(&state, &note_id, &peer_id).await
        }
    };
    if result.is_ok() {
        let _ = queries::known_peer_record_transfer(&state.db, &address, &peer_name, now_secs()).await;
    }
    result
}

// ---- Batch send (new protocol) ----

#[tauri::command]
pub async fn notes_send(
    note_ids: Vec<String>,
    peer_id: String,
    passphrase: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let peer = {
        let peers = state.peers.lock().unwrap();
        peers
            .iter()
            .find(|p| p.id == peer_id)
            .map(|p| (p.address.clone(), p.port, p.via.clone(), p.name.clone()))
            .ok_or("peer not found")?
    };

    let (address, port, via, peer_name) = peer;
    let device_name = resolve_device_name(&state.db).await.unwrap_or_else(|_| "panote-device".into());
    let result = match via {
        TransportKind::Lan => {
            super::lan::send_notes(&state, &note_ids, &address, port, &passphrase, &device_name).await
        }
        TransportKind::Ble => Err("BLE batch send not supported".into()),
    };
    if result.is_ok() {
        let _ = queries::known_peer_record_transfer(&state.db, &address, &peer_name, now_secs()).await;
    }
    result
}

// ---- Transfer offer respond ----

#[tauri::command]
pub fn transfer_offer_respond(
    offer_id: String,
    passphrase: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let tx = state
        .offer_responses
        .lock()
        .unwrap()
        .remove(&offer_id)
        .ok_or("no pending offer with this ID")?;
    tx.send(passphrase).map_err(|_| "offer connection already closed".to_string())
}

// ---- Pending offers ----

#[derive(Debug, Serialize)]
pub struct PendingOfferJson {
    pub offer_id: String,
    pub from_peer: String,
    pub note_count: u32,
    pub received_at: i64,
}

#[tauri::command]
pub fn pending_offers_list(state: State<'_, AppState>) -> Vec<PendingOfferJson> {
    state
        .pending_offers
        .lock()
        .unwrap()
        .values()
        .map(|o| PendingOfferJson {
            offer_id: o.offer_id.clone(),
            from_peer: o.from_peer.clone(),
            note_count: o.note_count,
            received_at: o.received_at,
        })
        .collect()
}

// ---- Pending transfers ----

/// Pending transfer metadata — content is unknown until passphrase is entered.
#[derive(Debug, Serialize)]
pub struct PendingTransferJson {
    pub transfer_id: String,
    pub from_peer: String,
    pub received_at: i64,
}

#[tauri::command]
pub fn pending_transfers_list(state: State<'_, AppState>) -> Vec<PendingTransferJson> {
    state
        .list_pending()
        .into_iter()
        .map(|t| PendingTransferJson {
            transfer_id: t.transfer_id,
            from_peer: t.from_peer,
            received_at: t.received_at,
        })
        .collect()
}

/// Accept a pending transfer: decrypt with passphrase, re-encrypt with device key, store.
#[tauri::command]
pub async fn note_receive_accept(
    transfer_id: String,
    passphrase: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Peek without removing so the user can retry if they enter the wrong code.
    let transfer = state
        .peek_pending(&transfer_id)
        .ok_or("transfer not found or already processed")?;

    let blob = decrypt_transfer(
        &transfer.transfer_salt,
        &transfer.transfer_nonce,
        &transfer.transfer_ct,
        &passphrase,
    )
    .map_err(|_| "wrong passphrase".to_string())?;

    // Decryption succeeded — remove from pending before import so concurrent
    // accept calls can't import the same transfer twice.
    state.take_pending(&transfer_id);

    let from_peer = transfer.from_peer.clone();
    let note_id = import_blob(&state, &state.device_key, blob)
        .await
        .map_err(|e| e.to_string())?;
    let _ = queries::known_peer_record_transfer(&state.db, &from_peer, &from_peer, now_secs()).await;
    Ok(note_id)
}

/// Reject and discard a pending transfer.
#[tauri::command]
pub fn note_receive_reject(transfer_id: String, state: State<'_, AppState>) {
    state.take_pending(&transfer_id);
}

// ---- Transfer history ----

#[derive(Debug, Serialize)]
pub struct KnownPeerJson {
    pub peer_id: String,
    pub display_name: Option<String>,
    pub last_transfer_at: Option<i64>,
}

#[tauri::command]
pub async fn known_peers_list(state: State<'_, AppState>) -> Result<Vec<KnownPeerJson>, String> {
    queries::known_peers_list_history(&state.db)
        .await
        .map_err(|e| e.to_string())
        .map(|rows| {
            rows.into_iter()
                .map(|r| KnownPeerJson {
                    peer_id: r.peer_id,
                    display_name: r.display_name,
                    last_transfer_at: r.last_transfer_at,
                })
                .collect()
        })
}

// ---- Import ----

pub async fn import_blob(
    state: &AppState,
    device_key: &[u8; 32],
    blob: TransferBlob,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let ts = now_secs();

    let (title_nonce, title_ct) = encrypt_with_vault(device_key, blob.title.as_bytes())?;
    let content_json = serde_json::to_vec(&blob.content)?;
    let (content_nonce, content_ct) = encrypt_with_vault(device_key, &content_json)?;
    let tags_json = serde_json::to_string(&blob.tags)?;

    let row = NoteRow {
        id: id.clone(),
        kind: blob.kind,
        title_nonce: title_nonce.to_vec(),
        title_ct,
        nonce: content_nonce.to_vec(),
        content_ct,
        note_salt: None,
        note_nonce: None,
        created_at: blob.created_at,
        updated_at: ts,
        tags: tags_json,
    };

    queries::note_insert(&state.db, &row).await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::{note::decrypt_with_vault, vault::derive_key},
        db::{init_pool, queries},
        state::AppState,
        transfer::blob::TransferBlob,
    };
    use serde_json::json;

    async fn test_state() -> AppState {
        let pool = init_pool(":memory:").await.unwrap();
        let key = derive_key("test-device-key", &[0u8; 16]).unwrap();
        AppState::new(pool, key)
    }

    fn sample_blob() -> TransferBlob {
        TransferBlob {
            id: "orig-id-from-sender".into(),
            kind: "markdown".into(),
            title: "Received note".into(),
            content: json!({ "body": "Note from Alice" }),
            tags: vec!["shared".into()],
            created_at: 1700000000,
            updated_at: 1700000001,
        }
    }

    #[tokio::test]
    async fn import_blob_creates_note_in_db() {
        let state = test_state().await;
        let blob = sample_blob();
        let note_id = import_blob(&state, &state.device_key, blob).await.unwrap();
        let row = queries::note_get(&state.db, &note_id).await.unwrap().unwrap();
        assert_eq!(row.kind, "markdown");
        assert!(row.note_salt.is_none());
    }

    #[tokio::test]
    async fn import_blob_encrypts_title_correctly() {
        let state = test_state().await;
        let blob = sample_blob();
        let note_id = import_blob(&state, &state.device_key, blob.clone()).await.unwrap();
        let row = queries::note_get(&state.db, &note_id).await.unwrap().unwrap();
        let title_bytes =
            decrypt_with_vault(&state.device_key, &row.title_nonce, &row.title_ct).unwrap();
        assert_eq!(String::from_utf8(title_bytes).unwrap(), blob.title);
    }

    #[tokio::test]
    async fn import_blob_encrypts_content_correctly() {
        let state = test_state().await;
        let blob = sample_blob();
        let note_id = import_blob(&state, &state.device_key, blob.clone()).await.unwrap();
        let row = queries::note_get(&state.db, &note_id).await.unwrap().unwrap();
        let content_bytes =
            decrypt_with_vault(&state.device_key, &row.nonce, &row.content_ct).unwrap();
        let content: serde_json::Value = serde_json::from_slice(&content_bytes).unwrap();
        assert_eq!(content, blob.content);
    }

    #[tokio::test]
    async fn import_blob_preserves_tags() {
        let state = test_state().await;
        let blob = TransferBlob { tags: vec!["rust".into(), "shared".into()], ..sample_blob() };
        let note_id = import_blob(&state, &state.device_key, blob).await.unwrap();
        let row = queries::note_get(&state.db, &note_id).await.unwrap().unwrap();
        let tags: Vec<String> = serde_json::from_str(&row.tags).unwrap();
        assert_eq!(tags, vec!["rust", "shared"]);
    }

    #[tokio::test]
    async fn import_blob_assigns_new_id() {
        let state = test_state().await;
        let blob = sample_blob();
        let original_id = blob.id.clone();
        let note_id = import_blob(&state, &state.device_key, blob).await.unwrap();
        assert_ne!(note_id, original_id);
    }

    #[tokio::test]
    async fn import_blob_preserves_original_created_at() {
        let state = test_state().await;
        let blob = sample_blob();
        let note_id = import_blob(&state, &state.device_key, blob.clone()).await.unwrap();
        let row = queries::note_get(&state.db, &note_id).await.unwrap().unwrap();
        assert_eq!(row.created_at, blob.created_at);
    }

    #[tokio::test]
    async fn import_blob_wrong_key_cannot_decrypt() {
        let state = test_state().await;
        let blob = sample_blob();
        let note_id = import_blob(&state, &state.device_key, blob).await.unwrap();
        let row = queries::note_get(&state.db, &note_id).await.unwrap().unwrap();
        let wrong_key = derive_key("different-key", &[0u8; 16]).unwrap();
        assert!(decrypt_with_vault(&wrong_key, &row.title_nonce, &row.title_ct).is_err());
    }

    #[tokio::test]
    async fn pending_transfer_lifecycle() {
        let state = test_state().await;
        let transfer = crate::state::PendingTransfer {
            transfer_id: "tid-abc".into(),
            from_peer: "alice.local".into(),
            transfer_salt: vec![1, 2, 3],
            transfer_nonce: vec![4, 5, 6],
            transfer_ct: vec![7, 8, 9],
            received_at: 1700000000,
        };
        state.add_pending(transfer);
        assert_eq!(state.list_pending().len(), 1);
        let taken = state.take_pending("tid-abc").unwrap();
        assert_eq!(taken.from_peer, "alice.local");
        assert_eq!(state.list_pending().len(), 0);
    }

    #[test]
    fn pairing_code_is_6_chars() {
        let code = generate_pairing_code();
        assert_eq!(code.len(), 6);
    }

    #[test]
    fn pairing_code_uses_valid_charset() {
        const VALID: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        for _ in 0..20 {
            let code = generate_pairing_code();
            assert!(code.chars().all(|c| VALID.contains(c)), "unexpected char in code: {code}");
        }
    }

    #[tokio::test]
    async fn take_nonexistent_pending_returns_none() {
        let state = test_state().await;
        assert!(state.take_pending("no-such-id").is_none());
    }

    #[tokio::test]
    async fn wrong_passphrase_leaves_transfer_pending() {
        use crate::crypto::vault::{derive_key, encrypt, random_salt};

        let state = test_state().await;
        let passphrase = "correct";
        let salt = random_salt();
        let key = derive_key(passphrase, &salt).unwrap();
        let blob_bytes = sample_blob().encode().unwrap();
        let (nonce, ct) = encrypt(&key, &blob_bytes).unwrap();

        let transfer = crate::state::PendingTransfer {
            transfer_id: "t-retry".into(),
            from_peer: "bob.local".into(),
            transfer_salt: salt.to_vec(),
            transfer_nonce: nonce.to_vec(),
            transfer_ct: ct,
            received_at: 1000,
        };
        state.add_pending(transfer);

        // Simulate what note_receive_accept does: peek then decrypt
        let t = state.peek_pending("t-retry").unwrap();
        let result = crate::transfer::lan::decrypt_transfer(
            &t.transfer_salt, &t.transfer_nonce, &t.transfer_ct, "wrong",
        );
        assert!(result.is_err(), "wrong passphrase must fail");
        assert_eq!(state.list_pending().len(), 1, "transfer must remain pending after wrong code");
    }

    #[tokio::test]
    async fn correct_passphrase_after_wrong_succeeds() {
        use crate::crypto::vault::{derive_key, encrypt, random_salt};

        let state = test_state().await;
        let passphrase = "correct";
        let salt = random_salt();
        let key = derive_key(passphrase, &salt).unwrap();
        let blob_bytes = sample_blob().encode().unwrap();
        let (nonce, ct) = encrypt(&key, &blob_bytes).unwrap();

        let transfer = crate::state::PendingTransfer {
            transfer_id: "t-retry2".into(),
            from_peer: "bob.local".into(),
            transfer_salt: salt.to_vec(),
            transfer_nonce: nonce.to_vec(),
            transfer_ct: ct,
            received_at: 1000,
        };
        state.add_pending(transfer);

        // First attempt: wrong code
        let t = state.peek_pending("t-retry2").unwrap();
        assert!(crate::transfer::lan::decrypt_transfer(
            &t.transfer_salt, &t.transfer_nonce, &t.transfer_ct, "wrong"
        ).is_err());
        assert_eq!(state.list_pending().len(), 1);

        // Second attempt: correct code
        let t = state.peek_pending("t-retry2").unwrap();
        let blob = crate::transfer::lan::decrypt_transfer(
            &t.transfer_salt, &t.transfer_nonce, &t.transfer_ct, passphrase,
        ).unwrap();
        state.take_pending("t-retry2");
        let note_id = import_blob(&state, &state.device_key, blob).await.unwrap();
        assert!(!note_id.is_empty());
        assert_eq!(state.list_pending().len(), 0);
    }

    #[test]
    fn decrypt_transfer_wrong_passphrase_fails() {
        use crate::crypto::vault::{derive_key, encrypt, random_salt};
        use crate::transfer::lan::decrypt_transfer;

        let passphrase = "correct-pass";
        let salt = random_salt();
        let key = derive_key(passphrase, &salt).unwrap();
        let blob = sample_blob();
        let blob_bytes = blob.encode().unwrap();
        let (nonce, ct) = encrypt(&key, &blob_bytes).unwrap();

        // Wrong passphrase should fail
        assert!(decrypt_transfer(&salt, &nonce, &ct, "wrong-pass").is_err());
    }

    #[test]
    fn decrypt_transfer_correct_passphrase_succeeds() {
        use crate::crypto::vault::{derive_key, encrypt, random_salt};
        use crate::transfer::lan::decrypt_transfer;

        let passphrase = "shared-secret-42";
        let salt = random_salt();
        let key = derive_key(passphrase, &salt).unwrap();
        let blob = sample_blob();
        let blob_bytes = blob.encode().unwrap();
        let (nonce, ct) = encrypt(&key, &blob_bytes).unwrap();

        let decoded = decrypt_transfer(&salt, &nonce, &ct, passphrase).unwrap();
        assert_eq!(decoded.title, blob.title);
        assert_eq!(decoded.kind, blob.kind);
    }
}
