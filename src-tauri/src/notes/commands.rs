use base64::{engine::general_purpose::STANDARD, Engine as _};
use crate::{
    crypto::note::{
        apply_note_password, decrypt_with_vault, encrypt_with_vault, peel_vault_ct,
        remove_note_password, remove_note_password_detect,
    },
    db::queries::{self, NoteRow},
    notes::types::{NoteDetail, NoteInput, NoteMetadata},
    state::{now_secs, AppState},
};
use tauri::State;
use uuid::Uuid;

/// Max decoded `bg_image` size (K6).
const MAX_BG_IMAGE_BYTES: usize = 3 * 1024 * 1024;
const BG_IMAGE_PREFIXES: &[&str] = &[
    "data:image/png;base64,",
    "data:image/jpeg;base64,",
    "data:image/webp;base64,",
    "data:image/gif;base64,",
];

/// Validate `bg_image` at the command boundary (K6): must be empty/absent,
/// or a `data:image/(png|jpeg|webp|gif);base64,...` URI decoding to at most
/// `MAX_BG_IMAGE_BYTES`.
fn validate_bg_image(bg_image: &Option<String>) -> Result<(), String> {
    let Some(s) = bg_image else { return Ok(()) };
    if s.is_empty() {
        return Ok(());
    }
    let prefix = BG_IMAGE_PREFIXES
        .iter()
        .find(|p| s.starts_with(**p))
        .ok_or("bg_image must be a data:image/(png|jpeg|webp|gif);base64,... URI")?;
    let decoded = STANDARD
        .decode(&s[prefix.len()..])
        .map_err(|_| "bg_image is not valid base64".to_string())?;
    if decoded.len() > MAX_BG_IMAGE_BYTES {
        return Err(format!("bg_image exceeds the {MAX_BG_IMAGE_BYTES}-byte limit"));
    }
    Ok(())
}

/// Encrypt tags for storage in the existing `tags` TEXT column (K12), as
/// `base64(nonce || ciphertext)`. Smallest change that avoids a schema
/// migration; kept in one place so note commands and transfer import/export
/// (which also read/write `tags`) stay in sync.
pub(crate) fn encrypt_tags(
    key: &[u8; 32],
    note_id: &str,
    tags: &[String],
) -> Result<String, String> {
    let json = serde_json::to_vec(tags).map_err(|e| e.to_string())?;
    let (nonce, ct) =
        encrypt_with_vault(key, &json, note_id.as_bytes()).map_err(|e| e.to_string())?;
    let mut raw = Vec::with_capacity(nonce.len() + ct.len());
    raw.extend_from_slice(&nonce);
    raw.extend_from_slice(&ct);
    Ok(STANDARD.encode(raw))
}

/// Decrypt tags stored by `encrypt_tags`.
// ponytail: rows written before K12 hold plaintext JSON (e.g. `[]`, or even
// `""` for a legacy empty-tags row). `""` base64-decodes to an empty (valid)
// byte string, so "decodes as base64" alone can't distinguish a legacy row
// from a framed one — a framed row must be at least nonce(12) + 16-byte GCM
// tag. Only take the legacy plaintext path when base64 decoding fails OR
// decodes too short to be a real frame. A value that decodes long enough to
// be a genuine frame but then fails to DECRYPT is real corruption and must
// error rather than silently returning empty tags. Legacy rows aren't
// re-encrypted in place; a future migration should rewrite every row on next
// write and drop this fallback.
pub(crate) fn decrypt_tags(key: &[u8; 32], note_id: &str, stored: &str) -> anyhow::Result<Vec<String>> {
    let raw = match STANDARD.decode(stored) {
        Ok(raw) if raw.len() > 12 => raw,
        _ => return Ok(serde_json::from_str(stored).unwrap_or_default()),
    };
    let (nonce, ct) = raw.split_at(12);
    let bytes = decrypt_with_vault(key, nonce, ct, note_id.as_bytes())?;
    Ok(serde_json::from_slice(&bytes)?)
}

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
const NOT_RECOVERABLE: &str = "note has no recovery code";
const WRONG_RECOVERY: &str = "wrong recovery code";

// ----- Note commands -----

#[tauri::command]
pub async fn note_create(
    input: NoteInput,
    state: State<'_, AppState>,
) -> Result<NoteMetadata, String> {
    validate_bg_image(&input.bg_image)?;
    let key = &state.device_key;
    let id = Uuid::new_v4().to_string();
    let ts = now_secs();

    let (title_nonce, title_ct) =
        encrypt_with_vault(key, input.title.as_bytes(), id.as_bytes()).map_err(|e| e.to_string())?;

    let content_json = serde_json::to_vec(&input.content).map_err(|e| e.to_string())?;
    let (content_nonce, content_ct) =
        encrypt_with_vault(key, &content_json, id.as_bytes()).map_err(|e| e.to_string())?;

    let tags_stored = encrypt_tags(key, &id, &input.tags)?;
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
        tags: tags_stored,
        content_hint: input.content_hint.clone(),
        pinned: input.pinned.unwrap_or(false),
        bg_color: input.bg_color.clone(),
        bg_image: input.bg_image.clone(),
        show_preview: input.show_preview.unwrap_or(true),
        preview_text: preview_text.clone(),
        origin_device_id: state.device_uuid.clone(),
        origin_note_id: id.clone(),
        rc_salt: None,
        rc_nonce: None,
        rc_ct: None,
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
    validate_bg_image(&input.bg_image)?;
    let key = &state.device_key;
    let ts = now_secs();

    let original = queries::note_get(&state.db, &id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("note not found")?;

    let (title_nonce, title_ct) =
        encrypt_with_vault(key, input.title.as_bytes(), id.as_bytes()).map_err(|e| e.to_string())?;

    let content_json = serde_json::to_vec(&input.content).map_err(|e| e.to_string())?;
    let (content_nonce, vault_ct) =
        encrypt_with_vault(key, &content_json, id.as_bytes()).map_err(|e| e.to_string())?;

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

    let tags_stored = encrypt_tags(key, &id, &input.tags)?;
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
        tags: tags_stored,
        content_hint: input.content_hint.clone(),
        pinned: input.pinned.unwrap_or(original.pinned),
        bg_color: input.bg_color.clone(),
        bg_image: input.bg_image.clone(),
        show_preview: input.show_preview.unwrap_or(original.show_preview),
        preview_text: preview_text.clone(),
        origin_device_id: original.origin_device_id,
        origin_note_id: original.origin_note_id,
        // Recovery wraps the password (not the content), so an edit leaves it valid.
        rc_salt: original.rc_salt.clone(),
        rc_nonce: original.rc_nonce.clone(),
        rc_ct: original.rc_ct.clone(),
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

/// Default page size for `note_list` (K14) — bounds the decrypt-all cost.
const DEFAULT_NOTE_LIST_LIMIT: i64 = 500;

#[tauri::command]
pub async fn note_list(
    state: State<'_, AppState>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<NoteMetadata>, String> {
    let key = &state.device_key;
    let rows = queries::note_list_page(
        &state.db,
        limit.unwrap_or(DEFAULT_NOTE_LIST_LIMIT),
        offset.unwrap_or(0),
    )
    .await
    .map_err(|e| e.to_string())?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let title_bytes = decrypt_with_vault(key, &row.title_nonce, &row.title_ct, row.id.as_bytes())
            .map_err(|e| e.to_string())?;
        let title = String::from_utf8(title_bytes).map_err(|e| e.to_string())?;
        let tags = decrypt_tags(key, &row.id, &row.tags).map_err(|e| e.to_string())?;
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

    let title_bytes = decrypt_with_vault(key, &row.title_nonce, &row.title_ct, row.id.as_bytes())
        .map_err(|e| e.to_string())?;
    let title = String::from_utf8(title_bytes).map_err(|e| e.to_string())?;

    let content = decrypt_content(&state, &row)?;

    let tags = decrypt_tags(key, &row.id, &row.tags).map_err(|e| e.to_string())?;

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
        has_recovery: row.rc_salt.is_some(),
        pinned: row.pinned,
        bg_color: row.bg_color,
        bg_image: row.bg_image,
        show_preview: row.show_preview,
    })
}

fn extract_preview(kind: &str, content: &serde_json::Value) -> Option<String> {
    let text = match kind {
        "document" => {
            let body = content.get("body").and_then(|v| v.as_str()).unwrap_or("");
            if body.is_empty() {
                return None;
            }
            // Strip markdown markers, collapse whitespace into a single line.
            let stripped: String = body
                .chars()
                .map(|c| if matches!(c, '#' | '*' | '`' | '>' | '-') { ' ' } else { c })
                .collect();
            stripped.split_whitespace().collect::<Vec<_>>().join(" ")
        }
        "checklist" => {
            let items = content.get("items").and_then(|v| v.as_array())?;
            if items.is_empty() {
                return None;
            }
            let done = items
                .iter()
                .filter(|i| i.get("done").and_then(|d| d.as_bool()).unwrap_or(false))
                .count();
            let texts: Vec<&str> = items
                .iter()
                .filter_map(|i| i.get("text").and_then(|t| t.as_str()))
                .take(3)
                .collect();
            format!("{}/{} done · {}", done, items.len(), texts.join(", "))
        }
        "kanban" => {
            let cols = content.get("columns").and_then(|v| v.as_array())?;
            let cards: usize = cols
                .iter()
                .filter_map(|c| c.get("cards").and_then(|x| x.as_array()))
                .map(|a| a.len())
                .sum();
            format!("{} columns · {} cards", cols.len(), cards)
        }
        "table" => {
            let rows = content
                .get("rows")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let names: Vec<&str> = content
                .get("columns")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|c| c.get("name").and_then(|n| n.as_str())).collect())
                .unwrap_or_default();
            format!("{} rows · {}", rows, names.join(", "))
        }
        _ => return None,
    };
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(truncate_str(text, 150).to_string())
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
    let content_bytes = decrypt_with_vault(&state.device_key, &row.nonce, &vault_ct, row.id.as_bytes())
        .map_err(|e| e.to_string())?;
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
    let mut row = queries::note_get(&state.db, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("note not found")?;
    // An unprotected note has no recovery wrap.
    row.rc_salt = None;
    row.rc_nonce = None;
    row.rc_ct = None;
    let (salt, nonce) = match (&row.note_salt, &row.note_nonce) {
        (Some(s), Some(n)) => (s.clone(), n.clone()),
        _ => return Err(NOT_PROTECTED.into()),
    };
    let vault_ct = remove_note_password(password, &salt, &nonce, &row.content_ct)
        .map_err(|_| WRONG_PASSWORD.to_string())?;

    // Regenerate the list preview now that the content is no longer gated.
    let preview_text = decrypt_with_vault(&state.device_key, &row.nonce, &vault_ct, row.id.as_bytes())
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
    let mut row = queries::note_get(&state.db, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("note not found")?;
    let (salt, nonce) = match (&row.note_salt, &row.note_nonce) {
        (Some(s), Some(n)) => (s.clone(), n.clone()),
        _ => return Err(NOT_PROTECTED.into()),
    };
    let vault_ct = remove_note_password(old_password, &salt, &nonce, &row.content_ct)
        .map_err(|_| WRONG_PASSWORD.to_string())?;
    // Recovery wraps the old password, so a password change resets it. The user
    // can re-add a recovery code afterward.
    row.rc_salt = None;
    row.rc_nonce = None;
    row.rc_ct = None;
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
    match (row.note_salt.clone(), row.note_nonce.clone()) {
        (Some(salt), Some(nonce)) => {
            let (vault_ct, was_legacy) =
                remove_note_password_detect(password, &salt, &nonce, &row.content_ct)
                    .map_err(|_| WRONG_PASSWORD.to_string())?;
            state.unlock_note(id, password);
            // K2: transparently re-wrap a pre-bump (p=1) note at the current
            // Argon2 params on unlock, so it's hardened after first open.
            if was_legacy {
                let (new_salt, new_nonce, double_ct) =
                    apply_note_password(password, &vault_ct).map_err(|e| e.to_string())?;
                persist_protection(
                    state,
                    row,
                    double_ct,
                    Some(new_salt.to_vec()),
                    Some(new_nonce.to_vec()),
                    None,
                )
                .await?;
            }
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

/// Generate a 128-bit recovery code as grouped RFC4648 base32. High entropy so
/// it can't be brute-forced; shown once and never stored — only a wrap of the
/// note password under a key derived from it lives in the DB.
fn generate_recovery_code() -> String {
    use rand::{rngs::OsRng, RngCore};
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    let (mut bits, mut nbits) = (0u32, 0u32);
    let mut chars = String::new();
    for &b in &bytes {
        bits = (bits << 8) | b as u32;
        nbits += 8;
        while nbits >= 5 {
            nbits -= 5;
            chars.push(ALPHABET[((bits >> nbits) & 31) as usize] as char);
        }
    }
    if nbits > 0 {
        chars.push(ALPHABET[((bits << (5 - nbits)) & 31) as usize] as char);
    }
    chars
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join("-")
}

/// Normalize a recovery code for derivation: keep alphanumerics, uppercase.
fn normalize_code(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase()
}

async fn add_recovery_impl(state: &AppState, id: &str, password: &str) -> Result<String, String> {
    let mut row = queries::note_get(&state.db, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("note not found")?;
    let (salt, nonce) = match (&row.note_salt, &row.note_nonce) {
        (Some(s), Some(n)) => (s.clone(), n.clone()),
        _ => return Err(NOT_PROTECTED.into()),
    };
    remove_note_password(password, &salt, &nonce, &row.content_ct)
        .map_err(|_| WRONG_PASSWORD.to_string())?;
    let code = generate_recovery_code();
    let norm = normalize_code(&code);
    // Wrap the password under the recovery-code-derived key. Recovery decrypts
    // the password with the code, then unlocks normally — so it survives edits.
    let (rc_salt, rc_nonce, rc_ct) =
        apply_note_password(&norm, password.as_bytes()).map_err(|e| e.to_string())?;
    row.rc_salt = Some(rc_salt.to_vec());
    row.rc_nonce = Some(rc_nonce.to_vec());
    row.rc_ct = Some(rc_ct);
    queries::note_update(&state.db, &row)
        .await
        .map_err(|e| e.to_string())?;
    Ok(code)
}

async fn recover_impl(
    state: &AppState,
    id: &str,
    recovery_code: &str,
    new_password: &str,
) -> Result<(), String> {
    if new_password.is_empty() {
        return Err(EMPTY_PASSWORD.into());
    }
    if state.passphrase_locked_out(id) {
        return Err("too many recovery attempts — try again later".into());
    }
    let mut row = queries::note_get(&state.db, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("note not found")?;
    let (rc_salt, rc_nonce, rc_ct) = match (&row.rc_salt, &row.rc_nonce, &row.rc_ct) {
        (Some(s), Some(n), Some(c)) => (s.clone(), n.clone(), c.clone()),
        _ => return Err(NOT_RECOVERABLE.into()),
    };
    let (note_salt, note_nonce) = match (&row.note_salt, &row.note_nonce) {
        (Some(s), Some(n)) => (s.clone(), n.clone()),
        _ => return Err(NOT_PROTECTED.into()),
    };
    let norm = normalize_code(recovery_code);
    let password_bytes = match remove_note_password(&norm, &rc_salt, &rc_nonce, &rc_ct) {
        Ok(pw) => pw,
        Err(_) => {
            state.record_passphrase_failure(id);
            return Err(WRONG_RECOVERY.into());
        }
    };
    state.reset_passphrase_failures(id);
    let old_password = String::from_utf8(password_bytes).map_err(|_| WRONG_RECOVERY.to_string())?;
    let vault_ct = remove_note_password(&old_password, &note_salt, &note_nonce, &row.content_ct)
        .map_err(|_| WRONG_RECOVERY.to_string())?;
    let (new_salt, new_nonce, double_ct) =
        apply_note_password(new_password, &vault_ct).map_err(|e| e.to_string())?;
    // Keep the same recovery code valid by re-wrapping it around the new password.
    let (rs, rn, rc) =
        apply_note_password(&norm, new_password.as_bytes()).map_err(|e| e.to_string())?;
    row.content_ct = double_ct;
    row.note_salt = Some(new_salt.to_vec());
    row.note_nonce = Some(new_nonce.to_vec());
    row.rc_salt = Some(rs.to_vec());
    row.rc_nonce = Some(rn.to_vec());
    row.rc_ct = Some(rc);
    queries::note_update(&state.db, &row)
        .await
        .map_err(|e| e.to_string())?;
    state.unlock_note(id, new_password);
    Ok(())
}

/// Add a recovery code to an already-protected note. Verifies the password and
/// returns a freshly generated code to show ONCE (it is never stored).
#[tauri::command]
pub async fn note_add_recovery(
    id: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    add_recovery_impl(&state, &id, &password).await
}

/// Recover a note with its recovery code, setting a new password.
#[tauri::command]
pub async fn note_recover(
    id: String,
    recovery_code: String,
    new_password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    recover_impl(&state, &id, &recovery_code, &new_password).await
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
    async fn recovery_code_opens_note_and_sets_new_password() {
        let state = test_state().await;
        let id = seed_note(&state, "top secret").await;
        protect_impl(&state, &id, "orig-pw").await.unwrap();
        let code = add_recovery_impl(&state, &id, "orig-pw").await.unwrap();
        assert!(fetch(&state, &id).await.rc_salt.is_some());

        // Forgot the password: recover with the code, set a new one.
        recover_impl(&state, &id, &code, "new-pw").await.unwrap();
        state.lock_note(&id);
        assert!(unlock_impl(&state, &id, "orig-pw").await.is_err());
        assert!(unlock_impl(&state, &id, "new-pw").await.is_ok());

        // The same recovery code still works after recovery (re-wrapped).
        state.lock_note(&id);
        recover_impl(&state, &id, &code, "third-pw").await.unwrap();
        state.lock_note(&id);
        assert!(unlock_impl(&state, &id, "third-pw").await.is_ok());
    }

    #[tokio::test]
    async fn wrong_recovery_code_fails() {
        let state = test_state().await;
        let id = seed_note(&state, "x").await;
        protect_impl(&state, &id, "pw").await.unwrap();
        add_recovery_impl(&state, &id, "pw").await.unwrap();
        assert!(recover_impl(&state, &id, "AAAA-BBBB-CCCC", "new").await.is_err());
        // The real password still works — a failed recovery didn't corrupt it.
        assert!(unlock_impl(&state, &id, "pw").await.is_ok());
    }

    #[tokio::test]
    async fn add_recovery_requires_correct_password_and_protection() {
        let state = test_state().await;
        let id = seed_note(&state, "x").await;
        assert!(add_recovery_impl(&state, &id, "pw").await.is_err()); // not protected
        protect_impl(&state, &id, "pw").await.unwrap();
        assert!(add_recovery_impl(&state, &id, "wrong").await.is_err()); // wrong pw
        assert!(add_recovery_impl(&state, &id, "pw").await.is_ok());
    }

    #[tokio::test]
    async fn changing_password_clears_recovery() {
        let state = test_state().await;
        let id = seed_note(&state, "x").await;
        protect_impl(&state, &id, "pw").await.unwrap();
        add_recovery_impl(&state, &id, "pw").await.unwrap();
        change_password_impl(&state, &id, "pw", "pw2").await.unwrap();
        assert!(fetch(&state, &id).await.rc_salt.is_none(), "recovery reset on pw change");
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

#[cfg(test)]
mod bg_image_tests {
    use super::*;

    #[test]
    fn none_is_valid() {
        assert!(validate_bg_image(&None).is_ok());
    }

    #[test]
    fn empty_string_is_valid() {
        assert!(validate_bg_image(&Some(String::new())).is_ok());
    }

    #[test]
    fn valid_png_data_uri_is_accepted() {
        let uri = format!("data:image/png;base64,{}", STANDARD.encode(b"tiny png bytes"));
        assert!(validate_bg_image(&Some(uri)).is_ok());
    }

    #[test]
    fn valid_webp_data_uri_is_accepted() {
        let uri = format!("data:image/webp;base64,{}", STANDARD.encode(b"tiny webp bytes"));
        assert!(validate_bg_image(&Some(uri)).is_ok());
    }

    #[test]
    fn plain_url_is_rejected() {
        assert!(validate_bg_image(&Some("https://evil.example/x.png".into())).is_err());
    }

    #[test]
    fn wrong_mime_type_is_rejected() {
        let uri = format!("data:image/svg+xml;base64,{}", STANDARD.encode(b"<svg/>"));
        assert!(validate_bg_image(&Some(uri)).is_err());
    }

    #[test]
    fn invalid_base64_is_rejected() {
        let uri = "data:image/png;base64,not-valid-base64!!!".to_string();
        assert!(validate_bg_image(&Some(uri)).is_err());
    }

    #[test]
    fn oversized_image_is_rejected() {
        let big = vec![0u8; MAX_BG_IMAGE_BYTES + 1];
        let uri = format!("data:image/jpeg;base64,{}", STANDARD.encode(&big));
        assert!(validate_bg_image(&Some(uri)).is_err());
    }

    #[test]
    fn image_at_exact_limit_is_accepted() {
        let exact = vec![0u8; MAX_BG_IMAGE_BYTES];
        let uri = format!("data:image/gif;base64,{}", STANDARD.encode(&exact));
        assert!(validate_bg_image(&Some(uri)).is_ok());
    }
}

#[cfg(test)]
mod tags_tests {
    use super::*;
    use crate::crypto::vault::derive_key;

    fn key() -> [u8; 32] {
        derive_key("tags-test", &[0u8; 16]).unwrap()
    }

    #[test]
    fn roundtrip() {
        let key = key();
        let tags = vec!["a".to_string(), "b".to_string()];
        let stored = encrypt_tags(&key, "note-1", &tags).unwrap();
        let recovered = decrypt_tags(&key, "note-1", &stored).unwrap();
        assert_eq!(recovered, tags);
    }

    #[test]
    fn legacy_empty_string_is_empty_tags() {
        // A legacy row whose tags column was never written (empty string).
        // "" base64-decodes to a valid, but frame-too-short, empty byte
        // string — must fall back to the legacy plaintext path, not error.
        let key = key();
        assert_eq!(decrypt_tags(&key, "note-1", "").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn legacy_empty_json_array_is_empty_tags() {
        let key = key();
        assert_eq!(decrypt_tags(&key, "note-1", "[]").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn legacy_plaintext_json_roundtrips() {
        let key = key();
        let stored = r#"["rust","notes"]"#;
        assert_eq!(
            decrypt_tags(&key, "note-1", stored).unwrap(),
            vec!["rust".to_string(), "notes".to_string()]
        );
    }

    #[test]
    fn corrupted_framed_value_errors() {
        // Long enough to look like a real nonce||ct frame, but garbage —
        // must error rather than silently return empty tags.
        let key = key();
        let garbage = STANDARD.encode(vec![0xABu8; 40]);
        assert!(decrypt_tags(&key, "note-1", &garbage).is_err());
    }
}
