//! Trash commands. A trashed note keeps its ciphertext as-is, so password
//! protection survives a trip through Trash without any re-sealing.

use super::queries;
use crate::crypto::note::decrypt_with_vault;
use crate::notes::commands::{migrate_hint, migrate_kind, LOCKED_TITLE};
use crate::state::AppState;
use serde::Serialize;
use tauri::State;

/// The same title-level view the note list gives, plus when it was deleted.
#[derive(Debug, Serialize)]
pub struct TrashedNote {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub has_note_password: bool,
    pub content_hint: Option<String>,
    pub deleted_at: i64,
}

pub(crate) async fn list_impl(state: &AppState) -> Result<Vec<TrashedNote>, String> {
    let rows = queries::list(&state.db).await.map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|r| {
            // A protected title is sealed at rest; only the unlock cache has it.
            // One unreadable title must not make the rest of Trash unreachable,
            // or the note could never be deleted for good.
            let title = if r.note_salt.is_some() {
                state
                    .note_title(&r.id)
                    .unwrap_or_else(|| LOCKED_TITLE.to_string())
            } else {
                decrypt_with_vault(
                    &state.device_key,
                    &r.title_nonce,
                    &r.title_ct,
                    r.id.as_bytes(),
                )
                .ok()
                .and_then(|b| String::from_utf8(b).ok())
                .unwrap_or_else(|| "(unreadable)".into())
            };
            TrashedNote {
                content_hint: migrate_hint(&r.kind, r.content_hint),
                kind: migrate_kind(&r.kind).to_string(),
                title,
                has_note_password: r.note_salt.is_some(),
                deleted_at: r.deleted_at,
                id: r.id,
            }
        })
        .collect())
}

#[tauri::command]
pub async fn trash_list(state: State<'_, AppState>) -> Result<Vec<TrashedNote>, String> {
    list_impl(&state).await
}

#[tauri::command]
pub async fn trash_restore(ids: Vec<String>, state: State<'_, AppState>) -> Result<(), String> {
    queries::restore(&state.db, &ids)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn trash_delete(ids: Vec<String>, state: State<'_, AppState>) -> Result<(), String> {
    queries::purge(&state.db, &ids)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn trash_empty(state: State<'_, AppState>) -> Result<(), String> {
    queries::empty(&state.db).await.map_err(|e| e.to_string())
}
