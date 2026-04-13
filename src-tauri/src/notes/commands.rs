use crate::{
    crypto::note::{decrypt_with_vault, encrypt_with_vault},
    db::queries::{self, NoteRow},
    notes::types::{NoteDetail, NoteInput, NoteMetadata},
    state::{now_secs, AppState},
};
use tauri::State;
use uuid::Uuid;

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
    let preview_text = extract_preview(&input.kind, &input.content);

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
        content_hint: input.content_hint.clone(),
        pinned: input.pinned.unwrap_or(false),
        bg_color: input.bg_color.clone(),
        bg_image: input.bg_image.clone(),
        show_preview: input.show_preview.unwrap_or(true),
        preview_text: preview_text.clone(),
        origin_device_id: state.device_uuid.clone(),
        origin_note_id: id.clone(),
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
        content_hint: input.content_hint,
        pinned: row.pinned,
        bg_color: input.bg_color,
        bg_image: input.bg_image,
        show_preview: row.show_preview,
        preview_text,
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
    let preview_text = extract_preview(&input.kind, &input.content);

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
        content_hint: input.content_hint.clone(),
        pinned: input.pinned.unwrap_or(original.pinned),
        bg_color: input.bg_color.clone(),
        bg_image: input.bg_image.clone(),
        show_preview: input.show_preview.unwrap_or(original.show_preview),
        preview_text: preview_text.clone(),
        origin_device_id: original.origin_device_id,
        origin_note_id: original.origin_note_id,
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
        content_hint: input.content_hint,
        pinned: row.pinned,
        bg_color: input.bg_color,
        bg_image: input.bg_image,
        show_preview: row.show_preview,
        preview_text,
    })
}

#[tauri::command]
pub async fn note_delete(id: String, state: State<'_, AppState>) -> Result<(), String> {
    queries::note_delete(&state.db, &id)
        .await
        .map_err(|e| e.to_string())
}

fn migrate_kind(kind: &str) -> &str {
    match kind {
        "text" | "markdown" | "code" => "document",
        other => other,
    }
}

fn migrate_hint(kind: &str, hint: Option<String>) -> Option<String> {
    hint.or_else(|| match kind {
        "text" => Some("plain".to_string()),
        "markdown" => Some("markdown".to_string()),
        "code" => Some("code".to_string()),
        _ => None,
    })
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
        let content_hint = migrate_hint(&row.kind, row.content_hint);
        result.push(NoteMetadata {
            id: row.id,
            kind: migrate_kind(&row.kind).to_string(),
            title,
            tags,
            created_at: row.created_at,
            updated_at: row.updated_at,
            has_note_password: false,
            content_hint,
            pinned: row.pinned,
            bg_color: row.bg_color,
            bg_image: row.bg_image,
            show_preview: row.show_preview,
            preview_text: row.preview_text,
        });
    }
    Ok(result)
}

fn migrate_code_content(content: serde_json::Value) -> serde_json::Value {
    let lang = content.get("lang").and_then(|v| v.as_str()).unwrap_or("text");
    let body = content.get("body").and_then(|v| v.as_str()).unwrap_or("");
    serde_json::json!({ "body": format!("```{lang}\n{body}\n```") })
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

    let content = if row.kind == "code" {
        migrate_code_content(content)
    } else {
        content
    };

    Ok(NoteDetail {
        id: row.id,
        kind: migrate_kind(&row.kind).to_string(),
        title,
        content,
        tags,
        created_at: row.created_at,
        updated_at: row.updated_at,
        pinned: row.pinned,
        bg_color: row.bg_color,
        bg_image: row.bg_image,
        show_preview: row.show_preview,
    })
}

fn extract_preview(kind: &str, content: &serde_json::Value) -> Option<String> {
    match kind {
        "document" => {
            let body = content.get("body").and_then(|v| v.as_str()).unwrap_or("");
            if body.is_empty() {
                return None;
            }
            let clean: String = body
                .lines()
                .filter(|l| !l.trim().is_empty())
                .take(3)
                .collect::<Vec<_>>()
                .join(" ");
            Some(truncate_str(&clean, 150).to_string())
        }
        "checklist" => {
            let items = content.get("items").and_then(|v| v.as_array())?;
            let texts: Vec<&str> = items
                .iter()
                .filter_map(|i| i.get("text").and_then(|t| t.as_str()))
                .take(3)
                .collect();
            if texts.is_empty() {
                return None;
            }
            Some(texts.join(", "))
        }
        _ => None,
    }
}

fn truncate_str(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

#[tauri::command]
pub async fn note_pin(id: String, pinned: bool, state: State<'_, AppState>) -> Result<(), String> {
    queries::note_pin(&state.db, &id, pinned)
        .await
        .map_err(|e| e.to_string())
}
