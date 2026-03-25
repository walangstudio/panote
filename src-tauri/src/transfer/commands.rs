use crate::{
    crypto::note::encrypt_with_vault,
    db::queries::{self, NoteRow},
    state::{AppState, TransportKind},
    transfer::{blob::TransferBlob, lan::decrypt_transfer},
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use uuid::Uuid;

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
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
            .map(|p| (p.address.clone(), p.port, p.via.clone()))
            .ok_or("peer not found")?
    };

    let (address, port, via) = peer;
    match via {
        TransportKind::Lan => {
            super::lan::send_note(&state, &note_id, &address, port, &passphrase).await
        }
        TransportKind::Ble => {
            super::ble::send_note(&state, &note_id, &peer_id).await
        }
    }
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
    let transfer = state
        .take_pending(&transfer_id)
        .ok_or("transfer not found or already processed")?;

    let blob = decrypt_transfer(
        &transfer.transfer_salt,
        &transfer.transfer_nonce,
        &transfer.transfer_ct,
        &passphrase,
    )
    .map_err(|_| "wrong passphrase".to_string())?;

    let note_id = import_blob(&state, &state.device_key, blob)
        .await
        .map_err(|e| e.to_string())?;
    Ok(note_id)
}

/// Reject and discard a pending transfer.
#[tauri::command]
pub fn note_receive_reject(transfer_id: String, state: State<'_, AppState>) {
    state.take_pending(&transfer_id);
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

    #[tokio::test]
    async fn take_nonexistent_pending_returns_none() {
        let state = test_state().await;
        assert!(state.take_pending("no-such-id").is_none());
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
