use crate::{
    crypto::note::{decrypt_with_vault, encrypt_with_vault},
    db::queries::{self, NoteRow},
    notes::types::{NoteDetail, NoteInput, NoteMetadata},
    state::AppState,
};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use uuid::Uuid;

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

// ----- Note commands -----

#[tauri::command]
pub async fn note_create(
    input: NoteInput,
    state: State<'_, AppState>,
) -> Result<NoteMetadata, String> {
    let key = &state.device_key;
    let id = Uuid::new_v4().to_string();
    let ts = now_secs();

    let (title_nonce, title_ct) =
        encrypt_with_vault(key, input.title.as_bytes()).map_err(|e| e.to_string())?;

    let content_json = serde_json::to_vec(&input.content).map_err(|e| e.to_string())?;
    let (content_nonce, content_ct) =
        encrypt_with_vault(key, &content_json).map_err(|e| e.to_string())?;

    let tags_json = serde_json::to_string(&input.tags).map_err(|e| e.to_string())?;

    let row = NoteRow {
        id: id.clone(),
        kind: input.kind.clone(),
        title_nonce: title_nonce.to_vec(),
        title_ct,
        nonce: content_nonce.to_vec(),
        content_ct,
        note_salt: None,
        note_nonce: None,
        created_at: ts,
        updated_at: ts,
        tags: tags_json,
    };

    queries::note_insert(&state.db, &row)
        .await
        .map_err(|e| e.to_string())?;

    Ok(NoteMetadata {
        id,
        kind: input.kind,
        title: input.title,
        tags: input.tags,
        created_at: ts,
        updated_at: ts,
        has_note_password: false,
    })
}

#[tauri::command]
pub async fn note_update(
    id: String,
    input: NoteInput,
    state: State<'_, AppState>,
) -> Result<NoteMetadata, String> {
    let key = &state.device_key;
    let ts = now_secs();

    let original = queries::note_get(&state.db, &id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("note not found")?;

    let (title_nonce, title_ct) =
        encrypt_with_vault(key, input.title.as_bytes()).map_err(|e| e.to_string())?;

    let content_json = serde_json::to_vec(&input.content).map_err(|e| e.to_string())?;
    let (content_nonce, content_ct) =
        encrypt_with_vault(key, &content_json).map_err(|e| e.to_string())?;

    let tags_json = serde_json::to_string(&input.tags).map_err(|e| e.to_string())?;

    let row = NoteRow {
        id: id.clone(),
        kind: input.kind.clone(),
        title_nonce: title_nonce.to_vec(),
        title_ct,
        nonce: content_nonce.to_vec(),
        content_ct,
        note_salt: None,
        note_nonce: None,
        created_at: original.created_at,
        updated_at: ts,
        tags: tags_json,
    };

    queries::note_update(&state.db, &row)
        .await
        .map_err(|e| e.to_string())?;

    Ok(NoteMetadata {
        id,
        kind: input.kind,
        title: input.title,
        tags: input.tags,
        created_at: original.created_at,
        updated_at: ts,
        has_note_password: false,
    })
}

#[tauri::command]
pub async fn note_delete(id: String, state: State<'_, AppState>) -> Result<(), String> {
    queries::note_delete(&state.db, &id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn note_list(state: State<'_, AppState>) -> Result<Vec<NoteMetadata>, String> {
    let key = &state.device_key;
    let rows = queries::note_list(&state.db)
        .await
        .map_err(|e| e.to_string())?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let title_bytes = decrypt_with_vault(key, &row.title_nonce, &row.title_ct)
            .map_err(|e| e.to_string())?;
        let title = String::from_utf8(title_bytes).map_err(|e| e.to_string())?;
        let tags: Vec<String> = serde_json::from_str(&row.tags).unwrap_or_default();
        result.push(NoteMetadata {
            id: row.id,
            kind: row.kind,
            title,
            tags,
            created_at: row.created_at,
            updated_at: row.updated_at,
            has_note_password: false,
        });
    }
    Ok(result)
}

#[tauri::command]
pub async fn note_get(
    id: String,
    state: State<'_, AppState>,
) -> Result<NoteDetail, String> {
    let key = &state.device_key;
    let row = queries::note_get(&state.db, &id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("note not found")?;

    let title_bytes = decrypt_with_vault(key, &row.title_nonce, &row.title_ct)
        .map_err(|e| e.to_string())?;
    let title = String::from_utf8(title_bytes).map_err(|e| e.to_string())?;

    let content_bytes = decrypt_with_vault(key, &row.nonce, &row.content_ct)
        .map_err(|e| e.to_string())?;
    let content: serde_json::Value =
        serde_json::from_slice(&content_bytes).map_err(|e| e.to_string())?;

    let tags: Vec<String> = serde_json::from_str(&row.tags).unwrap_or_default();

    Ok(NoteDetail {
        id: row.id,
        kind: row.kind,
        title,
        content,
        tags,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
