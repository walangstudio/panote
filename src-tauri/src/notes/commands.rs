use crate::{
    crypto::note::{
        apply_note_password, decrypt_with_vault, encrypt_with_vault, open_note,
        remove_note_password, seal_note, SealedNote,
    },
    db::queries::{self, NoteRow},
    notes::types::{NoteDetail, NoteInput, NoteMetadata},
    state::{now_secs, AppState},
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;
// Passwords arrive as owned Strings from the IPC layer. Wrapping them at the
// command boundary means the copy we hold is scrubbed on drop rather than left
// sitting in the heap for a core dump or memory scrape to find.
use zeroize::Zeroizing;

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
pub(crate) fn validate_bg_image(bg_image: &Option<String>) -> Result<(), String> {
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
        return Err(format!(
            "bg_image exceeds the {MAX_BG_IMAGE_BYTES}-byte limit"
        ));
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
pub(crate) fn decrypt_tags(
    key: &[u8; 32],
    note_id: &str,
    stored: &str,
) -> anyhow::Result<Vec<String>> {
    let raw = match STANDARD.decode(stored) {
        Ok(raw) if raw.len() > 12 => raw,
        _ => return Ok(serde_json::from_str(stored).unwrap_or_default()),
    };
    let (nonce, ct) = raw.split_at(12);
    let bytes = decrypt_with_vault(key, nonce, ct, note_id.as_bytes())?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Encrypt a note's list preview, same shape as tags: `base64(nonce ‖ ct)` in
/// the existing TEXT column, AAD-bound to the note id.
///
/// This column used to hold up to 150 characters of the note body in the CLEAR,
/// so anyone with read access to panote.db (or its -wal) could read real note
/// content with one SQL query — bypassing the whole at-rest encryption design.
pub(crate) fn encrypt_preview(
    key: &[u8; 32],
    note_id: &str,
    preview: &str,
) -> Result<String, String> {
    let (nonce, ct) = encrypt_with_vault(key, preview.as_bytes(), note_id.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut raw = Vec::with_capacity(nonce.len() + ct.len());
    raw.extend_from_slice(&nonce);
    raw.extend_from_slice(&ct);
    Ok(STANDARD.encode(raw))
}

/// Reverse of [`encrypt_preview`]. Rows written before previews were encrypted
/// hold plaintext; those decode-fail and are returned as-is so existing notes
/// keep rendering. They are re-encrypted the next time the note is saved.
pub(crate) fn decrypt_preview(key: &[u8; 32], note_id: &str, stored: &str) -> Option<String> {
    let raw = match STANDARD.decode(stored) {
        Ok(raw) if raw.len() > 12 => raw,
        _ => return Some(stored.to_string()), // legacy plaintext
    };
    let (nonce, ct) = raw.split_at(12);
    match decrypt_with_vault(key, nonce, ct, note_id.as_bytes()) {
        Ok(bytes) => String::from_utf8(bytes).ok(),
        // Not our ciphertext — treat as legacy plaintext rather than losing it.
        Err(_) => Some(stored.to_string()),
    }
}

// Stable error sentinels shared with the frontend (mirror in src/lib/tauri.ts).
// Control flow (unlock gate, batch skip-vs-fail) matches on these, so they must
// not be reworded casually.

/// Returned by `note_get` / `note_update` when a protected note isn't unlocked
/// this session. The frontend shows the unlock prompt on this.
pub const LOCKED: &str = "locked";
/// Listed in place of a protected note's title until it is unlocked this
/// session. The real title is sealed under the note password.
pub const LOCKED_TITLE: &str = "Locked note";
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
    create_impl(&state, input).await
}

pub(crate) async fn create_impl(
    state: &AppState,
    input: NoteInput,
) -> Result<NoteMetadata, String> {
    validate_bg_image(&input.bg_image)?;
    // A folder deleted since the list was drawn must not lose the note.
    let folder_id = match input.folder_id.as_deref() {
        Some(f) => crate::folders::queries::get(&state.db, f)
            .await
            .map_err(|e| e.to_string())?
            .map(|_| f.to_string()),
        None => None,
    };
    let key = &state.device_key;
    let id = Uuid::new_v4().to_string();
    let ts = now_secs();

    let (title_nonce, title_ct) = encrypt_with_vault(key, input.title.as_bytes(), id.as_bytes())
        .map_err(|e| e.to_string())?;

    let content_json = serde_json::to_vec(&input.content).map_err(|e| e.to_string())?;
    let (content_nonce, content_ct) =
        encrypt_with_vault(key, &content_json, id.as_bytes()).map_err(|e| e.to_string())?;

    let tags_stored = encrypt_tags(key, &id, &input.tags)?;
    let preview_text = extract_preview(&input.kind, &input.content)
        .map(|p| encrypt_preview(key, &id, &p))
        .transpose()?;

    let row = NoteRow {
        folder_id: folder_id.clone(),
        sort_order: 0, // owned by notes_set_order; note_insert/update never write it
        id: id.clone(),
        kind: input.kind.clone(),
        title_nonce: title_nonce.to_vec(),
        title_ct,
        title_note_nonce: None,
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
        folder_id,
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
        sort_order: row.sort_order,
    })
}

#[tauri::command]
pub async fn note_update(
    id: String,
    input: NoteInput,
    state: State<'_, AppState>,
) -> Result<NoteMetadata, String> {
    update_impl(&state, id, input).await
}

pub(crate) async fn update_impl(
    state: &AppState,
    id: String,
    input: NoteInput,
) -> Result<NoteMetadata, String> {
    validate_bg_image(&input.bg_image)?;
    let key = &state.device_key;
    let ts = now_secs();

    let original = queries::note_get(&state.db, &id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("note not found")?;

    let (title_nonce, title_ct) = encrypt_with_vault(key, input.title.as_bytes(), id.as_bytes())
        .map_err(|e| e.to_string())?;

    let content_json = serde_json::to_vec(&input.content).map_err(|e| e.to_string())?;
    let (content_nonce, vault_ct) =
        encrypt_with_vault(key, &content_json, id.as_bytes()).map_err(|e| e.to_string())?;

    // Preserve password protection across saves. A protected note can only be
    // edited after it was unlocked this session, so the password is cached.
    let protected = original.note_salt.is_some();
    let password = if protected {
        Some(state.note_password(&id).ok_or(LOCKED)?)
    } else {
        None
    };

    let tags_stored = encrypt_tags(key, &id, &input.tags)?;
    let preview_text = if protected {
        None
    } else {
        extract_preview(&input.kind, &input.content)
            .map(|p| encrypt_preview(key, &id, &p))
            .transpose()?
    };

    let mut row = NoteRow {
        folder_id: None, // owned by note_set_folder; note_update never writes it
        sort_order: 0,   // owned by notes_set_order, likewise
        id: id.clone(),
        kind: input.kind.clone(),
        title_nonce: title_nonce.to_vec(),
        title_ct,
        title_note_nonce: None,
        nonce: content_nonce.to_vec(),
        content_ct: vault_ct,
        note_salt: None,
        note_nonce: None,
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
    if let Some(pw) = &password {
        let sealed = seal_note(pw, &row.content_ct, &row.title_ct).map_err(|e| e.to_string())?;
        apply_sealed(&mut row, sealed);
    }

    queries::note_update(&state.db, &row)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(pw) = &password {
        state.unlock_note(&id, pw, &input.title);
    }

    // Saving IS the commit: the draft has served its purpose and must go, or the
    // editor would keep offering to restore edits the user already saved.
    queries::draft_delete(&state.db, &id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(NoteMetadata {
        // Unchanged by a save; report what the row already had.
        folder_id: original.folder_id.clone(),
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
        sort_order: original.sort_order,
    })
}

/// Moves the note to Trash; `trash_delete` is what removes it for good.
#[tauri::command]
pub async fn note_delete(id: String, state: State<'_, AppState>) -> Result<(), String> {
    crate::trash::queries::trash(&state.db, &[id], now_secs())
        .await
        .map_err(|e| e.to_string())
}

// ----- Drafts -----
//
// Editing must never overwrite a committed note. In-progress work is autosaved
// here instead, and `note_update` is what promotes it. Because the draft is
// durable, closing the window mid-edit stops being destructive.

/// What a draft carries. `content` is whatever shape the note's kind uses, so a
/// draft round-trips the same JSON `note_update` would have taken.
#[derive(Debug, Serialize, Deserialize)]
pub struct DraftPayload {
    pub title: String,
    pub content: serde_json::Value,
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DraftDetail {
    pub title: String,
    pub content: serde_json::Value,
    pub tags: Vec<String>,
    pub updated_at: i64,
}

pub(crate) async fn draft_save_impl(
    state: &AppState,
    id: &str,
    draft: &DraftPayload,
) -> Result<(), String> {
    // A draft of a protected note would sit outside that note's password layer,
    // so it is refused rather than quietly weakening the protection.
    let row = queries::note_get(&state.db, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "note not found".to_string())?;
    if row.note_salt.is_some() {
        return Err("password-protected notes do not keep drafts".into());
    }

    let json = serde_json::to_vec(draft).map_err(|e| e.to_string())?;
    let (nonce, ct) =
        encrypt_with_vault(&state.device_key, &json, id.as_bytes()).map_err(|e| e.to_string())?;
    queries::draft_upsert(&state.db, id, &nonce, &ct, now_secs())
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn draft_get_impl(
    state: &AppState,
    id: &str,
) -> Result<Option<DraftDetail>, String> {
    let Some(row) = queries::draft_get(&state.db, id)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let plain = decrypt_with_vault(&state.device_key, &row.nonce, &row.ct, id.as_bytes())
        .map_err(|e| e.to_string())?;
    let payload: DraftPayload = serde_json::from_slice(&plain).map_err(|e| e.to_string())?;
    Ok(Some(DraftDetail {
        title: payload.title,
        content: payload.content,
        tags: payload.tags,
        updated_at: row.updated_at,
    }))
}

/// Autosave in-progress edits. Does not touch the committed note.
#[tauri::command]
pub async fn note_draft_save(
    id: String,
    draft: DraftPayload,
    state: State<'_, AppState>,
) -> Result<(), String> {
    draft_save_impl(&state, &id, &draft).await
}

/// The draft for a note, if one is outstanding.
#[tauri::command]
pub async fn note_draft_get(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<DraftDetail>, String> {
    draft_get_impl(&state, &id).await
}

/// Throw away in-progress edits and keep the committed note.
#[tauri::command]
pub async fn note_draft_discard(id: String, state: State<'_, AppState>) -> Result<(), String> {
    queries::draft_delete(&state.db, &id)
        .await
        .map_err(|e| e.to_string())
}

pub(crate) fn migrate_kind(kind: &str) -> &str {
    match kind {
        "text" | "markdown" | "code" => "document",
        other => other,
    }
}

pub(crate) fn migrate_hint(kind: &str, hint: Option<String>) -> Option<String> {
    hint.or_else(|| match kind {
        "text" => Some("plain".to_string()),
        "markdown" => Some("markdown".to_string()),
        "code" => Some("code".to_string()),
        _ => None,
    })
}

/// Default page size for `note_list` (K14) — bounds the decrypt-all cost.
const DEFAULT_NOTE_LIST_LIMIT: i64 = 500;

/// How many notes exist, regardless of the list page size.
///
/// The list is capped, and without this the UI had no way to know it was showing
/// a partial view — notes past the cap simply ceased to exist, with nothing said.
#[tauri::command]
pub async fn note_count(state: State<'_, AppState>) -> Result<i64, String> {
    queries::note_count(&state.db)
        .await
        .map_err(|e| e.to_string())
}

/// Background images for every note that has one, keyed by note id.
///
/// Kept out of `note_list` because a background is a base64 data URI that dwarfs
/// the rest of the row, and the list refreshes on every save, pin and delete.
/// Backgrounds change rarely, so the frontend fetches this once and caches it.
#[tauri::command]
pub async fn note_bg_images(
    state: State<'_, AppState>,
) -> Result<std::collections::HashMap<String, String>, String> {
    Ok(queries::note_bg_images(&state.db)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .collect())
}

#[tauri::command]
pub async fn note_list(
    state: State<'_, AppState>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<NoteMetadata>, String> {
    list_impl(&state, limit, offset).await
}

pub(crate) async fn list_impl(
    state: &AppState,
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
        // A protected title is sealed at rest; only the unlock cache has it.
        let title = if row.note_salt.is_some() {
            state
                .note_title(&row.id)
                .unwrap_or_else(|| LOCKED_TITLE.to_string())
        } else {
            vault_title(key, &row, &row.title_ct)?
        };
        let tags = decrypt_tags(key, &row.id, &row.tags).map_err(|e| e.to_string())?;
        let content_hint = migrate_hint(&row.kind, row.content_hint);
        let preview_text = row
            .preview_text
            .as_deref()
            .and_then(|p| decrypt_preview(key, &row.id, p));
        result.push(NoteMetadata {
            folder_id: row.folder_id,
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
            preview_text,
            sort_order: row.sort_order,
        });
    }
    Ok(result)
}

fn migrate_code_content(content: serde_json::Value) -> serde_json::Value {
    let lang = content
        .get("lang")
        .and_then(|v| v.as_str())
        .unwrap_or("text");
    let body = content.get("body").and_then(|v| v.as_str()).unwrap_or("");
    serde_json::json!({ "body": format!("```{lang}\n{body}\n```") })
}

#[tauri::command]
pub async fn note_get(id: String, state: State<'_, AppState>) -> Result<NoteDetail, String> {
    let key = &state.device_key;
    let row = queries::note_get(&state.db, &id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("note not found")?;

    let (title, content_bytes) = open_row(&state, &row)?;
    let content: serde_json::Value =
        serde_json::from_slice(&content_bytes).map_err(|e| e.to_string())?;

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

/// Drop HTML tags from a preview line.
///
/// Note bodies can legitimately contain inline HTML — colour and highlight have
/// no markdown syntax, so the editor serialises them as spans. Without this a
/// highlighted first line showed up in the note list as raw tag soup:
/// `mark data color="#ffe58f" style="background color: ..." Hello mark`.
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn extract_preview(kind: &str, content: &serde_json::Value) -> Option<String> {
    let text = match kind {
        "document" => {
            let body = content.get("body").and_then(|v| v.as_str()).unwrap_or("");
            if body.is_empty() {
                return None;
            }
            // HTML first: the markdown pass below turns '>' into a space, which
            // would break tag boundaries and leave attribute soup behind.
            let no_html = strip_html(body);
            // Strip markdown markers, collapse whitespace into a single line.
            let stripped: String = no_html
                .chars()
                .map(|c| {
                    if matches!(c, '#' | '*' | '`' | '>' | '-') {
                        ' '
                    } else {
                        c
                    }
                })
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
                .map(|a| {
                    a.iter()
                        .filter_map(|c| c.get("name").and_then(|n| n.as_str()))
                        .collect()
                })
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

// ----- Copy -----

#[derive(Debug, Default, Serialize)]
pub struct CopyReport {
    /// Ids of the new notes.
    pub copied: Vec<String>,
    /// Protected notes left out because they are locked.
    pub skipped_locked: usize,
}

/// `row`'s note as a new row under a fresh id in `folder_id`. Title, body and
/// tags are AAD-bound to the note id, so they are decrypted and re-encrypted
/// rather than copied. A protected note is re-sealed under its own password,
/// which is why it has to be unlocked (LOCKED otherwise). Recovery codes stay
/// with the original.
pub(crate) fn copy_row(
    state: &AppState,
    row: &NoteRow,
    folder_id: Option<&str>,
    mark_copy: bool,
) -> Result<NoteRow, String> {
    let password = match row.note_salt {
        Some(_) => Some(Zeroizing::new(state.note_password(&row.id).ok_or(LOCKED)?)),
        None => None,
    };
    let (title, content) = open_row(state, row)?;
    let title = if mark_copy {
        format!("{title} (copy)")
    } else {
        title
    };
    let key = &state.device_key;
    let id = Uuid::new_v4().to_string();
    let ts = now_secs();

    let (title_nonce, title_ct) =
        encrypt_with_vault(key, title.as_bytes(), id.as_bytes()).map_err(|e| e.to_string())?;
    let (nonce, content_ct) =
        encrypt_with_vault(key, &content, id.as_bytes()).map_err(|e| e.to_string())?;
    let tags = decrypt_tags(key, &row.id, &row.tags).map_err(|e| e.to_string())?;
    let preview_text = match (&password, row.preview_text.as_deref()) {
        (None, Some(p)) => decrypt_preview(key, &row.id, p)
            .map(|p| encrypt_preview(key, &id, &p))
            .transpose()?,
        _ => None,
    };

    let mut copy = NoteRow {
        id: id.clone(),
        kind: row.kind.clone(),
        title_nonce: title_nonce.to_vec(),
        title_ct,
        title_note_nonce: None,
        nonce: nonce.to_vec(),
        content_ct,
        note_salt: None,
        note_nonce: None,
        created_at: ts,
        updated_at: ts,
        tags: encrypt_tags(key, &id, &tags)?,
        content_hint: row.content_hint.clone(),
        pinned: false,
        bg_color: row.bg_color.clone(),
        bg_image: row.bg_image.clone(),
        show_preview: row.show_preview,
        preview_text,
        // A new note, not another copy of the original: transfers dedup on this.
        origin_device_id: state.device_uuid.clone(),
        origin_note_id: id.clone(),
        folder_id: folder_id.map(String::from),
        sort_order: 0,
        rc_salt: None,
        rc_nonce: None,
        rc_ct: None,
    };
    if let Some(pw) = &password {
        let sealed = seal_note(pw, &copy.content_ct, &copy.title_ct).map_err(|e| e.to_string())?;
        apply_sealed(&mut copy, sealed);
        state.unlock_note(&id, pw, &title);
    }
    Ok(copy)
}

/// Copies `ids` into `folder_id`. Trashed notes are left out; locked protected
/// ones are counted in `skipped_locked`. All rows land in one transaction.
pub(crate) async fn copy_impl(
    state: &AppState,
    ids: &[String],
    folder_id: Option<&str>,
) -> Result<CopyReport, String> {
    if let Some(f) = folder_id {
        if crate::folders::queries::get(&state.db, f)
            .await
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Err("folder not found".into());
        }
    }
    let mut report = CopyReport::default();
    let mut rows = Vec::with_capacity(ids.len());
    for id in ids {
        if crate::folders::queries::is_trashed(&state.db, id)
            .await
            .map_err(|e| e.to_string())?
        {
            continue;
        }
        let Some(row) = queries::note_get(&state.db, id)
            .await
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        match copy_row(
            state,
            &row,
            folder_id,
            row.folder_id.as_deref() == folder_id,
        ) {
            Ok(copy) => rows.push(copy),
            Err(e) if e == LOCKED => report.skipped_locked += 1,
            Err(e) => return Err(e),
        }
    }
    let mut tx = state.db.begin().await.map_err(|e| e.to_string())?;
    for row in &rows {
        queries::note_insert(&mut *tx, row)
            .await
            .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    report.copied = rows.into_iter().map(|r| r.id).collect();
    Ok(report)
}

#[tauri::command]
pub async fn notes_copy(
    ids: Vec<String>,
    folder_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<CopyReport, String> {
    copy_impl(&state, &ids, folder_id.as_deref()).await
}

// ----- Per-note password protection -----

/// Open a row to (title, content JSON bytes), peeling a protected note's
/// password layer with the session-cached password. Returns the LOCKED sentinel
/// if it hasn't been unlocked this session. Shared by note_get, transfer send,
/// and export so the peel logic lives once.
pub(crate) fn open_row(state: &AppState, row: &NoteRow) -> Result<(String, Vec<u8>), String> {
    let (vault_ct, vault_title_ct) = match (&row.note_salt, &row.note_nonce) {
        (Some(salt), Some(nonce)) => {
            let password = state.note_password(&row.id).ok_or(LOCKED)?;
            let (vault_ct, title, _) =
                open_note(&password, salt, nonce, &row.content_ct, sealed_title(row))
                    .map_err(|_| LOCKED.to_string())?;
            (vault_ct, title.unwrap_or_else(|| row.title_ct.clone()))
        }
        _ => (row.content_ct.clone(), row.title_ct.clone()),
    };
    let title = vault_title(&state.device_key, row, &vault_title_ct)?;
    let content = decrypt_with_vault(&state.device_key, &row.nonce, &vault_ct, row.id.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok((title, content))
}

/// The title's password layer, if sealed. `None` for unprotected notes and for
/// notes protected before titles were sealed (migrated on their next unlock).
fn sealed_title(row: &NoteRow) -> Option<(&[u8], &[u8])> {
    row.title_note_nonce
        .as_deref()
        .map(|n| (n, row.title_ct.as_slice()))
}

/// Decrypt a title's device-key layer.
fn vault_title(key: &[u8; 32], row: &NoteRow, vault_title_ct: &[u8]) -> Result<String, String> {
    let bytes = decrypt_with_vault(key, &row.title_nonce, vault_title_ct, row.id.as_bytes())
        .map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// Put a note's sealed body and title on the row, replacing the vault-only form.
pub(crate) fn apply_sealed(row: &mut NoteRow, sealed: SealedNote) {
    row.content_ct = sealed.content_ct;
    row.note_salt = Some(sealed.salt.to_vec());
    row.note_nonce = Some(sealed.nonce.to_vec());
    row.title_ct = sealed.title_ct;
    row.title_note_nonce = Some(sealed.title_nonce.to_vec());
}

/// Persist a row re-sealed under a note password (protect, re-key, upgrade).
/// Drops the list preview, which would sit outside the password layer. Leaves
/// updated_at untouched so protecting/unlocking a note doesn't reorder a list
/// sorted by "date modified".
async fn persist_sealed(
    state: &AppState,
    mut row: NoteRow,
    sealed: SealedNote,
) -> Result<(), String> {
    apply_sealed(&mut row, sealed);
    row.preview_text = None;
    queries::note_update(&state.db, &row)
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn protect_impl(state: &AppState, id: &str, password: &str) -> Result<(), String> {
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
    let title = vault_title(&state.device_key, &row, &row.title_ct)?;
    let sealed = seal_note(password, &row.content_ct, &row.title_ct).map_err(|e| e.to_string())?;
    persist_sealed(state, row, sealed).await?;
    state.unlock_note(id, password, &title);
    Ok(())
}

/// Guard every password comparison on a note with the same lockout the LAN
/// pairing and recovery-code paths already use, keyed by note id.
///
/// Argon2id makes each guess expensive, but nothing capped the number of
/// attempts — so a foothold in the webview could have driven `note_unlock` in a
/// loop against other notes with no throttle at all.
fn check_note_lockout(state: &AppState, id: &str) -> Result<(), String> {
    if state.passphrase_locked_out(id) {
        return Err("too many wrong passwords — try again later".into());
    }
    Ok(())
}

/// Map a wrong-password result onto the failure counter. Success clears it, so
/// ordinary mistyping never accumulates toward a lockout.
fn record_password_attempt<T>(
    state: &AppState,
    id: &str,
    result: Result<T, String>,
) -> Result<T, String> {
    match result {
        Ok(v) => {
            state.reset_passphrase_failures(id);
            Ok(v)
        }
        Err(e) if e == WRONG_PASSWORD => {
            state.record_passphrase_failure(id);
            Err(e)
        }
        Err(e) => Err(e),
    }
}

async fn unprotect_impl(state: &AppState, id: &str, password: &str) -> Result<(), String> {
    check_note_lockout(state, id)?;
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
    let (vault_ct, vault_title_ct, _) = record_password_attempt(
        state,
        id,
        open_note(password, &salt, &nonce, &row.content_ct, sealed_title(&row))
            .map_err(|_| WRONG_PASSWORD.to_string()),
    )?;

    // Regenerate the list preview now that the content is no longer gated —
    // encrypted, like every other write of this column.
    let preview_text =
        decrypt_with_vault(&state.device_key, &row.nonce, &vault_ct, row.id.as_bytes())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|content| extract_preview(migrate_kind(&row.kind), &content))
            .map(|p| encrypt_preview(&state.device_key, id, &p))
            .transpose()?;

    row.content_ct = vault_ct;
    row.note_salt = None;
    row.note_nonce = None;
    // Back under the device key alone, like any unprotected title.
    if let Some(t) = vault_title_ct {
        row.title_ct = t;
    }
    row.title_note_nonce = None;
    row.preview_text = preview_text;
    queries::note_update(&state.db, &row)
        .await
        .map_err(|e| e.to_string())?;
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
    check_note_lockout(state, id)?;
    let mut row = queries::note_get(&state.db, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("note not found")?;
    let (salt, nonce) = match (&row.note_salt, &row.note_nonce) {
        (Some(s), Some(n)) => (s.clone(), n.clone()),
        _ => return Err(NOT_PROTECTED.into()),
    };
    let (vault_ct, vault_title_ct, _) = record_password_attempt(
        state,
        id,
        open_note(
            old_password,
            &salt,
            &nonce,
            &row.content_ct,
            sealed_title(&row),
        )
        .map_err(|_| WRONG_PASSWORD.to_string()),
    )?;
    let vault_title_ct = vault_title_ct.unwrap_or_else(|| row.title_ct.clone());
    let title = vault_title(&state.device_key, &row, &vault_title_ct)?;
    // Recovery wraps the old password, so a password change resets it. The user
    // can re-add a recovery code afterward.
    row.rc_salt = None;
    row.rc_nonce = None;
    row.rc_ct = None;
    let sealed = seal_note(new_password, &vault_ct, &vault_title_ct).map_err(|e| e.to_string())?;
    persist_sealed(state, row, sealed).await?;
    state.unlock_note(id, new_password, &title);
    Ok(())
}

async fn unlock_impl(state: &AppState, id: &str, password: &str) -> Result<(), String> {
    check_note_lockout(state, id)?;
    let row = queries::note_get(&state.db, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("note not found")?;
    match (row.note_salt.clone(), row.note_nonce.clone()) {
        (Some(salt), Some(nonce)) => {
            let (vault_ct, vault_title_ct, was_legacy) = record_password_attempt(
                state,
                id,
                open_note(password, &salt, &nonce, &row.content_ct, sealed_title(&row))
                    .map_err(|_| WRONG_PASSWORD.to_string()),
            )?;
            let title_sealed = vault_title_ct.is_some();
            let vault_title_ct = vault_title_ct.unwrap_or_else(|| row.title_ct.clone());
            let title = vault_title(&state.device_key, &row, &vault_title_ct)?;
            state.unlock_note(id, password, &title);
            // K2: transparently re-wrap a pre-bump (p=1) note at the current
            // Argon2 params on unlock, so it's hardened after first open. The
            // same pass seals the title of a note protected before titles were,
            // dropping its device-key-only copy.
            if was_legacy || !title_sealed {
                let sealed =
                    seal_note(password, &vault_ct, &vault_title_ct).map_err(|e| e.to_string())?;
                persist_sealed(state, row, sealed).await?;
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
    let password = Zeroizing::new(password);
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
    let password = Zeroizing::new(password);
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
    let (old_password, new_password) = (Zeroizing::new(old_password), Zeroizing::new(new_password));
    change_password_impl(&state, &id, &old_password, &new_password).await
}

/// Verify a password and cache it for the session. Returns Err on wrong password.
#[tauri::command]
pub async fn note_unlock(
    id: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let password = Zeroizing::new(password);
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
    let (vault_ct, vault_title_ct, _) = open_note(
        &old_password,
        &note_salt,
        &note_nonce,
        &row.content_ct,
        sealed_title(&row),
    )
    .map_err(|_| WRONG_RECOVERY.to_string())?;
    let vault_title_ct = vault_title_ct.unwrap_or_else(|| row.title_ct.clone());
    let title = vault_title(&state.device_key, &row, &vault_title_ct)?;
    let sealed = seal_note(new_password, &vault_ct, &vault_title_ct).map_err(|e| e.to_string())?;
    // Keep the same recovery code valid by re-wrapping it around the new password.
    let (rs, rn, rc) =
        apply_note_password(&norm, new_password.as_bytes()).map_err(|e| e.to_string())?;
    apply_sealed(&mut row, sealed);
    row.rc_salt = Some(rs.to_vec());
    row.rc_nonce = Some(rn.to_vec());
    row.rc_ct = Some(rc);
    queries::note_update(&state.db, &row)
        .await
        .map_err(|e| e.to_string())?;
    state.unlock_note(id, new_password, &title);
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
            folder_path: Vec::new(),
        };
        crate::transfer::commands::import_blob(state, &state.device_key, blob)
            .await
            .unwrap()
    }

    async fn fetch(state: &AppState, id: &str) -> NoteRow {
        queries::note_get(&state.db, id).await.unwrap().unwrap()
    }

    fn decrypt_content(state: &AppState, row: &NoteRow) -> Result<serde_json::Value, String> {
        open_row(state, row).map(|(_, bytes)| serde_json::from_slice(&bytes).unwrap())
    }

    /// What the list shows for a note right now.
    async fn listed_title(state: &AppState, id: &str) -> String {
        list_impl(state, None, None)
            .await
            .unwrap()
            .into_iter()
            .find(|m| m.id == id)
            .unwrap()
            .title
    }

    /// Custom sort is computed in the frontend from the listed sort_order. Without
    /// it every note ties and the list falls back to id order after each refresh.
    #[tokio::test]
    async fn the_list_reports_the_order_a_reorder_saved() {
        let state = test_state().await;
        let a = seed_note(&state, "a").await;
        let b = seed_note(&state, "b").await;
        let c = seed_note(&state, "c").await;
        queries::notes_set_order(&state.db, &[c.clone(), a.clone(), b.clone()])
            .await
            .unwrap();
        let listed = list_impl(&state, None, None).await.unwrap();
        let order = |id: &str| listed.iter().find(|m| m.id == id).unwrap().sort_order;
        assert_eq!((order(&c), order(&a), order(&b)), (0, 1, 2));
    }

    /// Whether the stored title opens with the device key alone, which is what
    /// anyone holding the DB plus the OS keychain has.
    fn title_readable_with_device_key(state: &AppState, row: &NoteRow) -> bool {
        decrypt_with_vault(
            &state.device_key,
            &row.title_nonce,
            &row.title_ct,
            row.id.as_bytes(),
        )
        .is_ok()
    }

    /// A note protected before titles were sealed: body under the password, title
    /// still under the device key only.
    async fn protect_legacy(state: &AppState, id: &str, password: &str) {
        let mut row = fetch(state, id).await;
        let (salt, nonce, ct) = apply_note_password(password, &row.content_ct).unwrap();
        row.content_ct = ct;
        row.note_salt = Some(salt.to_vec());
        row.note_nonce = Some(nonce.to_vec());
        queries::note_update(&state.db, &row).await.unwrap();
    }

    async fn seed_tagged(state: &AppState, title: &str, tags: &[&str]) -> String {
        let blob = TransferBlob {
            id: title.into(),
            kind: "document".into(),
            title: title.into(),
            content: json!({ "body": "x".repeat(4096) }),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            created_at: 1,
            updated_at: 1,
            origin_device_id: String::new(),
            origin_note_id: String::new(),
            folder_path: Vec::new(),
        };
        crate::transfer::commands::import_blob(state, &state.device_key, blob)
            .await
            .unwrap()
    }

    // ---- List cap (B2) ----

    /// Pinning is the user saying "keep this one". Before the ordering change, a
    /// pinned note whose `updated_at` fell outside the newest page vanished from
    /// the list — including from the Pinned section — with nothing said.
    #[tokio::test]
    async fn a_pinned_note_survives_the_page_cap() {
        let state = test_state().await;
        let old = seed_tagged(&state, "old-but-pinned", &[]).await;
        queries::note_pin(&state.db, &old, true).await.unwrap();
        // seed_tagged stamps every note with the same updated_at, so the ages
        // have to be set explicitly or this test proves nothing.
        sqlx::query("UPDATE notes SET updated_at = 1 WHERE id = ?")
            .bind(&old)
            .execute(&state.db)
            .await
            .unwrap();
        for n in 0..3 {
            let id = seed_tagged(&state, &format!("newer-{n}"), &[]).await;
            sqlx::query("UPDATE notes SET updated_at = ? WHERE id = ?")
                .bind(100 + n as i64)
                .bind(&id)
                .execute(&state.db)
                .await
                .unwrap();
        }

        // A one-row page: only the top-ordered note survives it.
        let page = queries::note_list_page(&state.db, 1, 0).await.unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(
            page[0].id, old,
            "the pinned note must lead the page even though it is the oldest"
        );
    }

    #[tokio::test]
    async fn note_count_reports_everything_not_just_the_page() {
        let state = test_state().await;
        for n in 0..5 {
            seed_tagged(&state, &format!("note-{n}"), &[]).await;
        }
        assert_eq!(
            queries::note_list_page(&state.db, 2, 0)
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(queries::note_count(&state.db).await.unwrap(), 5);
    }

    // ---- Password guess rate limiting (S5) ----

    /// Argon2id makes each guess expensive, but nothing capped the number of
    /// attempts, so a foothold in the webview could drive `note_unlock` in a
    /// loop against every note with no throttle.
    #[tokio::test]
    async fn repeated_wrong_passwords_lock_the_note_out() {
        let state = test_state().await;
        let id = seed_note(&state, "body").await;
        protect_impl(&state, &id, "correct").await.unwrap();

        for _ in 0..5 {
            assert!(unlock_impl(&state, &id, "wrong").await.is_err());
        }

        // Even the CORRECT password is refused once locked out — otherwise the
        // limit would not bound an attacker who happens to guess on attempt 6.
        let err = unlock_impl(&state, &id, "correct").await.unwrap_err();
        assert!(err.contains("too many"), "got: {err}");
    }

    /// Ordinary mistyping must not accumulate toward a lockout.
    #[tokio::test]
    async fn a_correct_password_clears_the_failure_count() {
        let state = test_state().await;
        let id = seed_note(&state, "body").await;
        protect_impl(&state, &id, "correct").await.unwrap();

        for _ in 0..4 {
            assert!(unlock_impl(&state, &id, "wrong").await.is_err());
        }
        unlock_impl(&state, &id, "correct").await.unwrap();

        // Counter reset, so a fresh run of wrong guesses is needed to lock out.
        for _ in 0..4 {
            assert!(unlock_impl(&state, &id, "wrong").await.is_err());
        }
        assert!(unlock_impl(&state, &id, "correct").await.is_ok());
    }

    /// The lockout is per note, so one note under attack cannot deny access to
    /// the rest.
    #[tokio::test]
    async fn a_lockout_does_not_spread_to_other_notes() {
        let state = test_state().await;
        let a = seed_note(&state, "a").await;
        let b = seed_note(&state, "b").await;
        protect_impl(&state, &a, "pw-a").await.unwrap();
        protect_impl(&state, &b, "pw-b").await.unwrap();

        for _ in 0..5 {
            assert!(unlock_impl(&state, &a, "wrong").await.is_err());
        }
        assert!(unlock_impl(&state, &b, "pw-b").await.is_ok());
    }

    #[tokio::test]
    async fn unprotect_and_change_password_are_also_rate_limited() {
        let state = test_state().await;
        let id = seed_note(&state, "body").await;
        protect_impl(&state, &id, "correct").await.unwrap();

        for _ in 0..5 {
            assert!(unprotect_impl(&state, &id, "wrong").await.is_err());
        }
        assert!(unprotect_impl(&state, &id, "correct")
            .await
            .unwrap_err()
            .contains("too many"));
        assert!(change_password_impl(&state, &id, "correct", "new")
            .await
            .unwrap_err()
            .contains("too many"));
    }

    // ---- Preview encryption ----

    /// The finding this fixes: `preview_text` held up to 150 chars of the note
    /// body as PLAIN TEXT in the database, so `sqlite3 panote.db "select
    /// preview_text from notes"` read real content and the at-rest encryption
    /// counted for nothing.
    #[tokio::test]
    async fn the_stored_preview_is_not_readable_from_the_database() {
        let state = test_state().await;
        // unprotect regenerates the preview, which is the reachable write path
        // from a test (note_create/note_update need a Tauri State).
        // No markdown markers in the body: '-' and friends are stripped when the
        // preview is generated, which would confuse the comparison below.
        let id = seed_note(&state, "SENSITIVE BODY TEXT").await;
        protect_impl(&state, &id, "pw").await.unwrap();
        unprotect_impl(&state, &id, "pw").await.unwrap();

        let row = fetch(&state, &id).await;
        let stored = row.preview_text.clone().expect("a preview should exist");
        assert!(
            !stored.contains("SENSITIVE BODY TEXT"),
            "preview stored in the clear: {stored}"
        );
        // …and it must still be readable through the proper path.
        assert_eq!(
            decrypt_preview(&state.device_key, &id, &stored).as_deref(),
            Some("SENSITIVE BODY TEXT")
        );
    }

    #[tokio::test]
    async fn a_preview_round_trips_through_encryption() {
        let state = test_state().await;
        let enc = encrypt_preview(&state.device_key, "n1", "hello preview").unwrap();
        assert_ne!(enc, "hello preview");
        assert_eq!(
            decrypt_preview(&state.device_key, "n1", &enc).as_deref(),
            Some("hello preview")
        );
    }

    /// Rows written before previews were encrypted hold plaintext. They must
    /// keep rendering rather than turning into blanks or errors.
    #[tokio::test]
    async fn legacy_plaintext_previews_still_render() {
        let state = test_state().await;
        assert_eq!(
            decrypt_preview(&state.device_key, "n1", "an old plaintext preview").as_deref(),
            Some("an old plaintext preview")
        );
    }

    /// AAD binds the preview to its note, so a blob cannot be moved between rows.
    #[tokio::test]
    async fn a_preview_does_not_decrypt_under_another_note_id() {
        let state = test_state().await;
        let enc = encrypt_preview(&state.device_key, "n1", "secret preview").unwrap();
        // Wrong id: falls back to returning the stored blob rather than the text.
        assert_ne!(
            decrypt_preview(&state.device_key, "other", &enc).as_deref(),
            Some("secret preview")
        );
    }

    #[test]
    fn previews_drop_html_so_the_list_shows_text_not_tag_soup() {
        let content = json!({
            "body": "<mark data-color=\"#ffe58f\" style=\"background-color:#ffe58f\">Hello</mark> world"
        });
        let preview = extract_preview("document", &content).expect("preview");
        assert!(!preview.contains("mark"), "got: {preview}");
        assert!(!preview.contains("background-color"), "got: {preview}");
        assert!(preview.contains("Hello"));
        assert!(preview.contains("world"));
    }

    // ---- Drafts ----

    fn draft(title: &str, body: &str) -> DraftPayload {
        DraftPayload {
            title: title.into(),
            content: json!({ "body": body }),
            tags: vec!["wip".into()],
        }
    }

    /// The whole point: autosaving must not touch the committed note. Compared
    /// at the ciphertext level — byte-identical means genuinely untouched.
    #[tokio::test]
    async fn saving_a_draft_leaves_the_note_untouched() {
        let state = test_state().await;
        let id = seed_note(&state, "committed body").await;
        let before = fetch(&state, &id).await;

        draft_save_impl(&state, &id, &draft("new title", "half-written"))
            .await
            .unwrap();

        let after = fetch(&state, &id).await;
        assert_eq!(
            before.content_ct, after.content_ct,
            "note body must not change"
        );
        assert_eq!(
            before.title_ct, after.title_ct,
            "note title must not change"
        );
        assert_eq!(
            before.updated_at, after.updated_at,
            "note must not look edited"
        );
    }

    #[tokio::test]
    async fn a_draft_round_trips() {
        let state = test_state().await;
        let id = seed_note(&state, "body").await;
        draft_save_impl(&state, &id, &draft("T", "in progress"))
            .await
            .unwrap();

        let got = draft_get_impl(&state, &id).await.unwrap().expect("draft");
        assert_eq!(got.title, "T");
        assert_eq!(got.content["body"], "in progress");
        assert_eq!(got.tags, vec!["wip".to_string()]);
    }

    #[tokio::test]
    async fn no_draft_means_none() {
        let state = test_state().await;
        let id = seed_note(&state, "body").await;
        assert!(draft_get_impl(&state, &id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn saving_a_draft_twice_overwrites_rather_than_duplicating() {
        let state = test_state().await;
        let id = seed_note(&state, "body").await;
        draft_save_impl(&state, &id, &draft("A", "first"))
            .await
            .unwrap();
        draft_save_impl(&state, &id, &draft("B", "second"))
            .await
            .unwrap();

        let got = draft_get_impl(&state, &id).await.unwrap().expect("draft");
        assert_eq!(got.content["body"], "second");
    }

    #[tokio::test]
    async fn discarding_a_draft_keeps_the_committed_note() {
        let state = test_state().await;
        let id = seed_note(&state, "committed body").await;
        draft_save_impl(&state, &id, &draft("x", "scratch"))
            .await
            .unwrap();

        let before = fetch(&state, &id).await;
        queries::draft_delete(&state.db, &id).await.unwrap();

        assert!(draft_get_impl(&state, &id).await.unwrap().is_none());
        assert_eq!(before.content_ct, fetch(&state, &id).await.content_ct);
    }

    /// Drafts are encrypted like note content — a draft in the clear would defeat
    /// at-rest encryption exactly the way the plaintext preview column does.
    #[tokio::test]
    async fn a_draft_is_not_stored_in_the_clear() {
        let state = test_state().await;
        let id = seed_note(&state, "body").await;
        draft_save_impl(&state, &id, &draft("t", "SENSITIVE-DRAFT-TEXT"))
            .await
            .unwrap();

        let row = queries::draft_get(&state.db, &id)
            .await
            .unwrap()
            .expect("row");
        let raw = String::from_utf8_lossy(&row.ct);
        assert!(!raw.contains("SENSITIVE-DRAFT-TEXT"));
    }

    /// The draft is bound to its note id, so a stored blob cannot be replayed
    /// onto a different note.
    #[tokio::test]
    async fn a_draft_will_not_decrypt_under_another_note_id() {
        let state = test_state().await;
        let a = seed_note(&state, "a").await;
        let b = seed_note(&state, "b").await;
        draft_save_impl(&state, &a, &draft("t", "secret"))
            .await
            .unwrap();

        let row = queries::draft_get(&state.db, &a).await.unwrap().unwrap();
        queries::draft_upsert(&state.db, &b, &row.nonce, &row.ct, row.updated_at)
            .await
            .unwrap();

        assert!(draft_get_impl(&state, &b).await.is_err());
    }

    /// A protected note's draft would sit outside its password layer.
    #[tokio::test]
    async fn protected_notes_refuse_drafts() {
        let state = test_state().await;
        let id = seed_note(&state, "body").await;
        protect_impl(&state, &id, "pw").await.unwrap();

        assert!(draft_save_impl(&state, &id, &draft("t", "x"))
            .await
            .is_err());
    }

    /// The list query selects a reduced column set; everything the list view
    /// renders must still decrypt, and note bodies must not be fetched at all.
    #[tokio::test]
    async fn list_page_returns_renderable_metadata_without_bodies() {
        let state = test_state().await;
        let id = seed_tagged(&state, "Groceries", &["home", "urgent"]).await;

        let rows = queries::note_list_page(&state.db, 500, 0).await.unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.id, id);
        assert_eq!(row.kind, "document");

        let title = decrypt_with_vault(
            &state.device_key,
            &row.title_nonce,
            &row.title_ct,
            row.id.as_bytes(),
        )
        .unwrap();
        assert_eq!(String::from_utf8(title).unwrap(), "Groceries");
        assert_eq!(
            decrypt_tags(&state.device_key, &row.id, &row.tags).unwrap(),
            vec!["home".to_string(), "urgent".to_string()]
        );
        assert!(
            row.note_salt.is_none(),
            "unprotected note reports no password"
        );

        // The whole point of the lean SELECT: no body ciphertext on this path.
        assert!(row.content_ct.is_empty());
    }

    /// A protected note must still be listable, with a lock derived from
    /// note_salt, but its title is sealed and the list shows a placeholder.
    #[tokio::test]
    async fn list_page_reports_protected_notes() {
        let state = test_state().await;
        let id = seed_tagged(&state, "Diary", &[]).await;
        protect_impl(&state, &id, "pw").await.unwrap();
        state.lock_note(&id);

        let listed = list_impl(&state, None, None).await.unwrap();
        assert!(listed[0].has_note_password);
        assert_eq!(listed[0].title, LOCKED_TITLE);
    }

    // ---- Sealed titles ----

    #[tokio::test]
    async fn protecting_seals_the_title_away_from_the_device_key() {
        let state = test_state().await;
        let id = seed_tagged(&state, "Swiss bank account", &[]).await;
        protect_impl(&state, &id, "pw").await.unwrap();

        let row = fetch(&state, &id).await;
        assert!(
            row.title_note_nonce.is_some(),
            "title should carry a password layer"
        );
        assert!(
            !title_readable_with_device_key(&state, &row),
            "the device key alone must not open a protected title"
        );
        assert_eq!(open_row(&state, &row).unwrap().0, "Swiss bank account");
    }

    #[tokio::test]
    async fn the_list_shows_the_real_title_only_while_unlocked() {
        let state = test_state().await;
        let id = seed_tagged(&state, "Diary", &[]).await;
        protect_impl(&state, &id, "pw").await.unwrap();
        assert_eq!(
            listed_title(&state, &id).await,
            "Diary",
            "protect leaves it unlocked"
        );

        state.lock_note(&id);
        assert_eq!(listed_title(&state, &id).await, LOCKED_TITLE);
        unlock_impl(&state, &id, "pw").await.unwrap();
        assert_eq!(listed_title(&state, &id).await, "Diary");
        state.lock_note(&id);
        assert_eq!(listed_title(&state, &id).await, LOCKED_TITLE);
    }

    #[tokio::test]
    async fn a_locked_note_get_leaks_no_title() {
        let state = test_state().await;
        let id = seed_note(&state, "body").await;
        protect_impl(&state, &id, "pw").await.unwrap();
        state.lock_note(&id);
        assert_eq!(
            open_row(&state, &fetch(&state, &id).await).unwrap_err(),
            LOCKED
        );
    }

    #[tokio::test]
    async fn unprotect_restores_a_device_key_title() {
        let state = test_state().await;
        let id = seed_tagged(&state, "Diary", &[]).await;
        protect_impl(&state, &id, "pw").await.unwrap();
        unprotect_impl(&state, &id, "pw").await.unwrap();

        let row = fetch(&state, &id).await;
        assert!(row.title_note_nonce.is_none());
        assert!(title_readable_with_device_key(&state, &row));
        assert_eq!(listed_title(&state, &id).await, "Diary");
    }

    #[tokio::test]
    async fn a_legacy_protected_note_seals_its_title_on_unlock() {
        let state = test_state().await;
        let id = seed_note(&state, "old secret").await;
        protect_legacy(&state, &id, "pw").await;
        let before = fetch(&state, &id).await;
        assert!(
            title_readable_with_device_key(&state, &before),
            "precondition: legacy row"
        );
        assert_eq!(
            listed_title(&state, &id).await,
            LOCKED_TITLE,
            "locked shows the placeholder"
        );

        unlock_impl(&state, &id, "pw").await.unwrap();
        let after = fetch(&state, &id).await;
        assert!(after.title_note_nonce.is_some());
        assert!(
            !title_readable_with_device_key(&state, &after),
            "the device-key copy must be gone after the first unlock"
        );
        assert_eq!(listed_title(&state, &id).await, "Secret");
        assert_eq!(
            decrypt_content(&state, &after).unwrap()["body"],
            "old secret"
        );

        state.lock_note(&id);
        unlock_impl(&state, &id, "pw").await.unwrap();
        assert_eq!(
            open_row(&state, &fetch(&state, &id).await).unwrap().0,
            "Secret"
        );
    }

    /// Both need the title before they re-seal it, including a legacy row whose
    /// title was never sealed.
    #[tokio::test]
    async fn change_password_and_recovery_keep_the_title() {
        let state = test_state().await;
        let id = seed_note(&state, "x").await;
        protect_legacy(&state, &id, "pw").await;
        change_password_impl(&state, &id, "pw", "pw2")
            .await
            .unwrap();
        let row = fetch(&state, &id).await;
        assert!(!title_readable_with_device_key(&state, &row));
        state.lock_note(&id);
        unlock_impl(&state, &id, "pw2").await.unwrap();
        assert_eq!(listed_title(&state, &id).await, "Secret");

        let code = add_recovery_impl(&state, &id, "pw2").await.unwrap();
        state.lock_note(&id);
        recover_impl(&state, &id, &code, "pw3").await.unwrap();
        assert_eq!(listed_title(&state, &id).await, "Secret");
        state.lock_note(&id);
        unlock_impl(&state, &id, "pw3").await.unwrap();
        let row = fetch(&state, &id).await;
        assert!(!title_readable_with_device_key(&state, &row));
        assert_eq!(open_row(&state, &row).unwrap().0, "Secret");
    }

    /// Saving a protected note writes a fresh title, which must go straight
    /// under the password too.
    #[tokio::test]
    async fn editing_a_protected_note_keeps_the_new_title_sealed() {
        let state = test_state().await;
        let id = seed_note(&state, "x").await;
        protect_impl(&state, &id, "pw").await.unwrap();
        let input = NoteInput {
            kind: "document".into(),
            title: "Renamed".into(),
            content: json!({ "body": "y" }),
            tags: vec![],
            content_hint: None,
            pinned: None,
            bg_color: None,
            bg_image: None,
            show_preview: None,
            folder_id: None,
        };
        update_impl(&state, id.clone(), input).await.unwrap();

        let row = fetch(&state, &id).await;
        assert!(!title_readable_with_device_key(&state, &row));
        assert_eq!(listed_title(&state, &id).await, "Renamed");
        state.lock_note(&id);
        assert_eq!(listed_title(&state, &id).await, LOCKED_TITLE);
    }

    fn new_note_in(folder_id: Option<String>) -> NoteInput {
        NoteInput {
            kind: "document".into(),
            title: "New".into(),
            content: json!({ "body": "b" }),
            tags: vec![],
            content_hint: None,
            pinned: None,
            bg_color: None,
            bg_image: None,
            show_preview: None,
            folder_id,
        }
    }

    #[tokio::test]
    async fn a_new_note_lands_in_the_folder_it_was_created_in() {
        let state = test_state().await;
        let folder = crate::folders::commands::create_impl(&state, "Work", None)
            .await
            .unwrap();
        let meta = create_impl(&state, new_note_in(Some(folder.clone())))
            .await
            .unwrap();
        assert_eq!(meta.folder_id.as_deref(), Some(folder.as_str()));
        assert_eq!(
            fetch(&state, &meta.id).await.folder_id.as_deref(),
            Some(folder.as_str())
        );
    }

    #[tokio::test]
    async fn a_new_note_for_a_missing_folder_lands_at_the_root() {
        let state = test_state().await;
        let meta = create_impl(&state, new_note_in(Some("gone".into())))
            .await
            .unwrap();
        assert_eq!(meta.folder_id, None);
        assert_eq!(fetch(&state, &meta.id).await.folder_id, None);
    }

    #[tokio::test]
    async fn list_page_honours_limit_and_offset() {
        let state = test_state().await;
        for n in 0..3 {
            seed_tagged(&state, &format!("note-{n}"), &[]).await;
        }
        assert_eq!(
            queries::note_list_page(&state.db, 2, 0)
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            queries::note_list_page(&state.db, 2, 2)
                .await
                .unwrap()
                .len(),
            1
        );
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
        assert!(recover_impl(&state, &id, "AAAA-BBBB-CCCC", "new")
            .await
            .is_err());
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
        change_password_impl(&state, &id, "pw", "pw2")
            .await
            .unwrap();
        assert!(
            fetch(&state, &id).await.rc_salt.is_none(),
            "recovery reset on pw change"
        );
    }

    #[tokio::test]
    async fn protect_then_locked_until_unlocked() {
        let state = test_state().await;
        let id = seed_note(&state, "top secret").await;

        protect_impl(&state, &id, "hunter2").await.unwrap();
        let row = fetch(&state, &id).await;
        assert!(row.note_salt.is_some(), "note should be protected");
        assert!(
            row.preview_text.is_none(),
            "preview must be cleared when protected"
        );

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

        assert!(change_password_impl(&state, &id, "wrong", "new")
            .await
            .is_err());
        change_password_impl(&state, &id, "old", "new")
            .await
            .unwrap();

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
        assert_eq!(
            decrypt_content(&state, &row).unwrap()["body"],
            "visible again"
        );
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

    // ---- Copy ----

    async fn copy(state: &AppState, ids: &[&String], folder: Option<&str>) -> CopyReport {
        let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
        copy_impl(state, &ids, folder).await.unwrap()
    }

    fn listed<'a>(list: &'a [NoteMetadata], id: &str) -> &'a NoteMetadata {
        list.iter().find(|m| m.id == id).unwrap()
    }

    #[tokio::test]
    async fn a_copied_note_carries_everything_but_its_id() {
        let state = test_state().await;
        let id = seed_tagged(&state, "Groceries", &["home"]).await;
        let mut row = fetch(&state, &id).await;
        row.bg_color = Some("#ffe3f1".into());
        row.show_preview = false;
        queries::note_update(&state.db, &row).await.unwrap();
        let dest = crate::folders::commands::create_impl(&state, "Dest", None)
            .await
            .unwrap();

        let report = copy(&state, &[&id], Some(&dest)).await;
        assert_eq!(report.skipped_locked, 0);
        let new_id = &report.copied[0];
        assert_ne!(new_id, &id);

        let list = list_impl(&state, None, None).await.unwrap();
        let (orig, dup) = (listed(&list, &id), listed(&list, new_id));
        assert_eq!(dup.title, "Groceries", "another folder needs no suffix");
        assert_eq!(dup.kind, orig.kind);
        assert_eq!(dup.tags, vec!["home"]);
        assert_eq!(dup.bg_color.as_deref(), Some("#ffe3f1"));
        assert!(!dup.show_preview);
        assert_eq!(dup.folder_id.as_deref(), Some(dest.as_str()));
        assert_eq!(orig.folder_id, None, "the original stays where it was");
        let copied = fetch(&state, new_id).await;
        assert_eq!(
            decrypt_content(&state, &copied),
            decrypt_content(&state, &row)
        );
        // A transfer dedups on origin, so a copy must not claim the original's.
        assert_eq!(copied.origin_note_id, *new_id);
    }

    #[tokio::test]
    async fn a_copy_beside_the_original_is_marked_as_a_copy() {
        let state = test_state().await;
        let id = seed_tagged(&state, "Groceries", &[]).await;
        let report = copy(&state, &[&id], None).await;
        assert_eq!(
            listed_title(&state, &report.copied[0]).await,
            "Groceries (copy)"
        );
    }

    #[tokio::test]
    async fn an_unlocked_protected_note_copies_under_the_same_password() {
        let state = test_state().await;
        let id = seed_note(&state, "secret body").await;
        protect_impl(&state, &id, "pw").await.unwrap();
        add_recovery_impl(&state, &id, "pw").await.unwrap();

        let report = copy(&state, &[&id], None).await;
        let new_id = report.copied[0].clone();
        let copied = fetch(&state, &new_id).await;
        assert!(copied.note_salt.is_some());
        assert!(
            copied.rc_salt.is_none(),
            "a recovery code belongs to the original only"
        );
        assert!(!title_readable_with_device_key(&state, &copied));

        state.lock_note(&new_id);
        assert_eq!(decrypt_content(&state, &copied).unwrap_err(), LOCKED);
        assert!(unlock_impl(&state, &new_id, "other").await.is_err());
        unlock_impl(&state, &new_id, "pw").await.unwrap();
        assert_eq!(
            decrypt_content(&state, &copied).unwrap()["body"],
            "secret body"
        );
        assert_eq!(listed_title(&state, &new_id).await, "Secret (copy)");
    }

    #[tokio::test]
    async fn a_locked_protected_note_is_skipped_and_the_rest_copied() {
        let state = test_state().await;
        let open = seed_note(&state, "a").await;
        let shut = seed_note(&state, "b").await;
        protect_impl(&state, &shut, "pw").await.unwrap();
        state.lock_note(&shut);

        let report = copy(&state, &[&open, &shut], None).await;
        assert_eq!(report.copied.len(), 1);
        assert_eq!(report.skipped_locked, 1);
        assert_eq!(list_impl(&state, None, None).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn trashed_notes_are_not_copied() {
        let state = test_state().await;
        let id = seed_note(&state, "a").await;
        crate::trash::queries::trash(&state.db, &[id.clone()], now_secs())
            .await
            .unwrap();
        assert!(copy(&state, &[&id], None).await.copied.is_empty());
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
        let uri = format!(
            "data:image/png;base64,{}",
            STANDARD.encode(b"tiny png bytes")
        );
        assert!(validate_bg_image(&Some(uri)).is_ok());
    }

    #[test]
    fn valid_webp_data_uri_is_accepted() {
        let uri = format!(
            "data:image/webp;base64,{}",
            STANDARD.encode(b"tiny webp bytes")
        );
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
        assert_eq!(
            decrypt_tags(&key, "note-1", "").unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn legacy_empty_json_array_is_empty_tags() {
        let key = key();
        assert_eq!(
            decrypt_tags(&key, "note-1", "[]").unwrap(),
            Vec::<String>::new()
        );
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
