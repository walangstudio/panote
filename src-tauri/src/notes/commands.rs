use crate::{
    crypto::note::{
        apply_note_password, decrypt_with_vault, encrypt_with_vault, peel_vault_ct,
        remove_note_password,
    },
    db::queries::{self, NoteRow},
    notes::types::{NoteDetail, NoteInput, NoteMetadata},
    state::{now_secs, AppState},
};
use tauri::State;
use uuid::Uuid;

// Stable error sentinels shared with the frontend (mirror in src/lib/tauri.ts).
// Control flow (unlock gate, batch skip-vs-fail) matches on these, so they must
// not be reworded casually.

/// Returned by `note_get` / `note_update` when a protected note isn't unlocked
/// this session. The frontend shows the unlock prompt on this.
pub const LOCKED: &str = "locked";
const WRONG_PASSWORD: &str = "wrong password";
const ALREADY_PROTECTED: &str = "note is already password-protected";
const NOT_PROTECTED: &str = "note is not password-protected";
const EMPTY_PASSWORD: &str = "password must not be empty";

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
    let (content_nonce, vault_ct) =
        encrypt_with_vault(key, &content_json).map_err(|e| e.to_string())?;

    // Preserve password protection across saves. A protected note can only be
    // edited after it was unlocked this session, so the password is cached.
    let protected = original.note_salt.is_some();
    let (content_ct, note_salt, note_nonce) = if protected {
        let password = state.note_password(&id).ok_or(LOCKED)?;
        let (salt, nonce, double_ct) =
            apply_note_password(&password, &vault_ct).map_err(|e| e.to_string())?;
        (double_ct, Some(salt.to_vec()), Some(nonce.to_vec()))
    } else {
        (vault_ct, None, None)
    };

    let tags_json = serde_json::to_string(&input.tags).map_err(|e| e.to_string())?;
    let preview_text = if protected {
        None
    } else {
        extract_preview(&input.kind, &input.content)
    };

    let row = NoteRow {
        id: id.clone(),
        kind: input.kind.clone(),
        title_nonce: title_nonce.to_vec(),
        title_ct,
        nonce: content_nonce.to_vec(),
        content_ct,
        note_salt,
        note_nonce,
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
        has_note_password: protected,
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
            has_note_password: row.note_salt.is_some(),
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

    let content = decrypt_content(&state, &row)?;

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
        has_note_password: row.note_salt.is_some(),
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

// ----- Per-note password protection -----

/// Decrypt a row's content, peeling the per-note password layer for protected
/// notes using the session-cached password. Returns the LOCKED sentinel if a
/// protected note hasn't been unlocked this session.
fn decrypt_content(state: &AppState, row: &NoteRow) -> Result<serde_json::Value, String> {
    let password = state.note_password(&row.id);
    let vault_ct = peel_vault_ct(
        row.note_salt.as_deref(),
        row.note_nonce.as_deref(),
        &row.content_ct,
        password.as_deref(),
    )
    .map_err(|_| LOCKED.to_string())?;
    let content_bytes =
        decrypt_with_vault(&state.device_key, &row.nonce, &vault_ct).map_err(|e| e.to_string())?;
    serde_json::from_slice(&content_bytes).map_err(|e| e.to_string())
}

/// Persist a row whose content protection (content_ct + salt + nonce + preview)
/// has been changed in place. Leaves updated_at untouched so protecting/unlocking
/// a note doesn't reorder a list sorted by "date modified".
async fn persist_protection(
    state: &AppState,
    mut row: NoteRow,
    content_ct: Vec<u8>,
    note_salt: Option<Vec<u8>>,
    note_nonce: Option<Vec<u8>>,
    preview_text: Option<String>,
) -> Result<(), String> {
    row.content_ct = content_ct;
    row.note_salt = note_salt;
    row.note_nonce = note_nonce;
    row.preview_text = preview_text;
    queries::note_update(&state.db, &row)
        .await
        .map_err(|e| e.to_string())
}

async fn protect_impl(state: &AppState, id: &str, password: &str) -> Result<(), String> {
    if password.is_empty() {
        return Err(EMPTY_PASSWORD.into());
    }
    let row = queries::note_get(&state.db, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("note not found")?;
    if row.note_salt.is_some() {
        return Err(ALREADY_PROTECTED.into());
    }
    let (salt, nonce, double_ct) =
        apply_note_password(password, &row.content_ct).map_err(|e| e.to_string())?;
    persist_protection(state, row, double_ct, Some(salt.to_vec()), Some(nonce.to_vec()), None)
        .await?;
    state.unlock_note(id, password);
    Ok(())
}

async fn unprotect_impl(state: &AppState, id: &str, password: &str) -> Result<(), String> {
    let row = queries::note_get(&state.db, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("note not found")?;
    let (salt, nonce) = match (&row.note_salt, &row.note_nonce) {
        (Some(s), Some(n)) => (s.clone(), n.clone()),
        _ => return Err(NOT_PROTECTED.into()),
    };
    let vault_ct = remove_note_password(password, &salt, &nonce, &row.content_ct)
        .map_err(|_| WRONG_PASSWORD.to_string())?;

    // Regenerate the list preview now that the content is no longer gated.
    let preview_text = decrypt_with_vault(&state.device_key, &row.nonce, &vault_ct)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|content| extract_preview(migrate_kind(&row.kind), &content));

    persist_protection(state, row, vault_ct, None, None, preview_text).await?;
    state.lock_note(id);
    Ok(())
}

async fn change_password_impl(
    state: &AppState,
    id: &str,
    old_password: &str,
    new_password: &str,
) -> Result<(), String> {
    if new_password.is_empty() {
        return Err(EMPTY_PASSWORD.into());
    }
    let row = queries::note_get(&state.db, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("note not found")?;
    let (salt, nonce) = match (&row.note_salt, &row.note_nonce) {
        (Some(s), Some(n)) => (s.clone(), n.clone()),
        _ => return Err(NOT_PROTECTED.into()),
    };
    let vault_ct = remove_note_password(old_password, &salt, &nonce, &row.content_ct)
        .map_err(|_| WRONG_PASSWORD.to_string())?;
    let (new_salt, new_nonce, double_ct) =
        apply_note_password(new_password, &vault_ct).map_err(|e| e.to_string())?;
    persist_protection(
        state,
        row,
        double_ct,
        Some(new_salt.to_vec()),
        Some(new_nonce.to_vec()),
        None,
    )
    .await?;
    state.unlock_note(id, new_password);
    Ok(())
}

async fn unlock_impl(state: &AppState, id: &str, password: &str) -> Result<(), String> {
    let row = queries::note_get(&state.db, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("note not found")?;
    match (&row.note_salt, &row.note_nonce) {
        (Some(salt), Some(nonce)) => {
            remove_note_password(password, salt, nonce, &row.content_ct)
                .map_err(|_| WRONG_PASSWORD.to_string())?;
            state.unlock_note(id, password);
            Ok(())
        }
        _ => Err(NOT_PROTECTED.into()),
    }
}

/// Protect a note with a password. Wraps the existing vault ciphertext in a
/// second password-derived layer. Caches the password so the note stays usable
/// this session. Errors if already protected.
#[tauri::command]
pub async fn note_protect(
    id: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    protect_impl(&state, &id, &password).await
}

/// Remove password protection from a note. Verifies the password by unwrapping
/// the layer, then stores the bare vault ciphertext and regenerates the preview.
#[tauri::command]
pub async fn note_unprotect(
    id: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    unprotect_impl(&state, &id, &password).await
}

/// Change a note's password. Requires the current password.
#[tauri::command]
pub async fn note_change_password(
    id: String,
    old_password: String,
    new_password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    change_password_impl(&state, &id, &old_password, &new_password).await
}

/// Verify a password and cache it for the session. Returns Err on wrong password.
#[tauri::command]
pub async fn note_unlock(
    id: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    unlock_impl(&state, &id, &password).await
}

/// Forget a note's cached password, re-locking it for this session.
#[tauri::command]
pub fn note_lock(id: String, state: State<'_, AppState>) {
    state.lock_note(&id);
}

/// Protect several notes with the same password in one call (batch / "group").
/// Already-protected notes are skipped.
#[tauri::command]
pub async fn notes_protect(
    ids: Vec<String>,
    password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if password.is_empty() {
        return Err(EMPTY_PASSWORD.into());
    }
    for id in &ids {
        match protect_impl(&state, id, &password).await {
            Ok(()) => {}
            // Already-protected notes are a no-op for batch protect.
            Err(e) if e == ALREADY_PROTECTED => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Remove protection from several notes that share the given password.
/// Notes that aren't protected are skipped; notes whose password doesn't match
/// are counted and reported so the caller doesn't see a false success.
#[tauri::command]
pub async fn notes_unprotect(
    ids: Vec<String>,
    password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut wrong = 0usize;
    for id in &ids {
        match unprotect_impl(&state, id, &password).await {
            Ok(()) => {}
            // Not protected → nothing to do.
            Err(e) if e == NOT_PROTECTED => {}
            // Different password → leave protected, but tell the user.
            Err(e) if e == WRONG_PASSWORD => wrong += 1,
            Err(e) => return Err(e),
        }
    }
    if wrong > 0 {
        return Err(format!(
            "{wrong} note(s) had a different password and stayed protected"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{crypto::vault::derive_key, db::init_pool, transfer::blob::TransferBlob};
    use serde_json::json;

    async fn test_state() -> AppState {
        let pool = init_pool(":memory:").await.unwrap();
        let key = derive_key("device", &[0u8; 16]).unwrap();
        AppState::new(pool, key, "test-device".into())
    }

    async fn seed_note(state: &AppState, body: &str) -> String {
        let blob = TransferBlob {
            id: "seed".into(),
            kind: "document".into(),
            title: "Secret".into(),
            content: json!({ "body": body }),
            tags: vec![],
            created_at: 1,
            updated_at: 1,
            origin_device_id: String::new(),
            origin_note_id: String::new(),
            note_password: None,
        };
        crate::transfer::commands::import_blob(state, &state.device_key, blob)
            .await
            .unwrap()
    }

    async fn fetch(state: &AppState, id: &str) -> NoteRow {
        queries::note_get(&state.db, id).await.unwrap().unwrap()
    }

    #[tokio::test]
    async fn protect_then_locked_until_unlocked() {
        let state = test_state().await;
        let id = seed_note(&state, "top secret").await;

        protect_impl(&state, &id, "hunter2").await.unwrap();
        let row = fetch(&state, &id).await;
        assert!(row.note_salt.is_some(), "note should be protected");
        assert!(row.preview_text.is_none(), "preview must be cleared when protected");

        // Cached from protect → readable.
        assert_eq!(decrypt_content(&state, &row).unwrap()["body"], "top secret");

        // Simulate a fresh session: drop the cache.
        state.lock_note(&id);
        assert_eq!(decrypt_content(&state, &row).unwrap_err(), LOCKED);

        // Wrong password stays locked, correct unlocks.
        assert!(unlock_impl(&state, &id, "wrong").await.is_err());
        assert_eq!(decrypt_content(&state, &row).unwrap_err(), LOCKED);
        unlock_impl(&state, &id, "hunter2").await.unwrap();
        assert_eq!(decrypt_content(&state, &row).unwrap()["body"], "top secret");
    }

    #[tokio::test]
    async fn change_password_invalidates_old() {
        let state = test_state().await;
        let id = seed_note(&state, "data").await;
        protect_impl(&state, &id, "old").await.unwrap();

        assert!(change_password_impl(&state, &id, "wrong", "new").await.is_err());
        change_password_impl(&state, &id, "old", "new").await.unwrap();

        state.lock_note(&id);
        assert!(unlock_impl(&state, &id, "old").await.is_err());
        unlock_impl(&state, &id, "new").await.unwrap();
        let row = fetch(&state, &id).await;
        assert_eq!(decrypt_content(&state, &row).unwrap()["body"], "data");
    }

    #[tokio::test]
    async fn unprotect_restores_plaintext_and_preview() {
        let state = test_state().await;
        let id = seed_note(&state, "visible again").await;
        protect_impl(&state, &id, "pw").await.unwrap();

        assert!(unprotect_impl(&state, &id, "wrong").await.is_err());
        unprotect_impl(&state, &id, "pw").await.unwrap();

        let row = fetch(&state, &id).await;
        assert!(row.note_salt.is_none(), "protection removed");
        assert!(row.preview_text.is_some(), "preview regenerated");
        assert_eq!(decrypt_content(&state, &row).unwrap()["body"], "visible again");
    }

    #[tokio::test]
    async fn double_protect_errors() {
        let state = test_state().await;
        let id = seed_note(&state, "x").await;
        protect_impl(&state, &id, "pw").await.unwrap();
        assert!(protect_impl(&state, &id, "pw2").await.is_err());
    }

    #[tokio::test]
    async fn batch_protect_then_unprotect_matching_password() {
        let state = test_state().await;
        let a = seed_note(&state, "a").await;
        let b = seed_note(&state, "b").await;

        notes_protect_inner(&state, &[a.clone(), b.clone()], "group").await;
        assert!(fetch(&state, &a).await.note_salt.is_some());
        assert!(fetch(&state, &b).await.note_salt.is_some());

        // unprotect with the shared password clears both.
        for id in [&a, &b] {
            unprotect_impl(&state, id, "group").await.unwrap();
            assert!(fetch(&state, id).await.note_salt.is_none());
        }
    }

    // Helper mirroring notes_protect without the State wrapper.
    async fn notes_protect_inner(state: &AppState, ids: &[String], password: &str) {
        for id in ids {
            protect_impl(state, id, password).await.unwrap();
        }
    }
}
