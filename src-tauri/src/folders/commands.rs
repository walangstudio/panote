//! Folder commands. Names are encrypted with the device key, AAD-bound to the
//! folder id, framed exactly like tags (`notes::commands::encrypt_tags`).

use super::{queries, MAX_DEPTH};
use crate::crypto::note::{decrypt_with_vault, encrypt_with_vault};
use crate::db::queries::{note_get, note_insert};
use crate::notes::commands::{copy_row, CopyReport, LOCKED};
use crate::state::{now_secs, AppState};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::State;
use uuid::Uuid;

pub const NAME_EMPTY: &str = "FOLDER_NAME_EMPTY";
pub const CYCLE: &str = "FOLDER_CYCLE";
pub const TOO_DEEP: &str = "FOLDER_TOO_DEEP";

const MAX_NAME_LEN: usize = 200;

#[derive(Debug, Serialize, PartialEq)]
pub struct FolderJson {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub note_count: i64,
    /// Position under the Custom sort, as saved by folders_reorder.
    pub sort_order: i64,
}

fn encrypt_name(key: &[u8; 32], id: &str, name: &str) -> Result<String, String> {
    let (nonce, ct) =
        encrypt_with_vault(key, name.as_bytes(), id.as_bytes()).map_err(|e| e.to_string())?;
    let mut raw = Vec::with_capacity(nonce.len() + ct.len());
    raw.extend_from_slice(&nonce);
    raw.extend_from_slice(&ct);
    Ok(STANDARD.encode(raw))
}

fn decrypt_name(key: &[u8; 32], id: &str, stored: &str) -> anyhow::Result<String> {
    let raw = STANDARD.decode(stored)?;
    if raw.len() <= 12 {
        anyhow::bail!("folder name ciphertext too short to be a frame");
    }
    let (nonce, ct) = raw.split_at(12);
    let bytes = decrypt_with_vault(key, nonce, ct, id.as_bytes())?;
    Ok(String::from_utf8(bytes)?)
}

/// Trim, reject empty, and cap length so a pasted file can't become a folder name.
fn clean_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(NAME_EMPTY.into());
    }
    Ok(trimmed.chars().take(MAX_NAME_LEN).collect())
}

/// Root-to-folder chain, nearest ancestor last. Bounded by MAX_DEPTH so a cycle
/// that somehow reached the database still terminates.
async fn ancestors(pool: &SqlitePool, id: &str) -> anyhow::Result<Vec<String>> {
    let mut chain = Vec::new();
    let mut cursor = Some(id.to_string());
    while let Some(current) = cursor {
        if chain.len() > MAX_DEPTH {
            break;
        }
        let Some(row) = queries::get(pool, &current).await? else { break };
        chain.push(row.id);
        cursor = row.parent_id;
    }
    Ok(chain)
}

/// How deep the subtree under `id` runs, counting `id` itself as 1.
async fn subtree_height(pool: &SqlitePool, id: &str) -> anyhow::Result<usize> {
    let all = queries::list(pool).await?;
    fn walk(all: &[queries::FolderRow], id: &str, depth: usize) -> usize {
        if depth > MAX_DEPTH {
            return depth;
        }
        all.iter()
            .filter(|f| f.parent_id.as_deref() == Some(id))
            .map(|c| walk(all, &c.id, depth + 1))
            .max()
            .unwrap_or(depth)
    }
    Ok(walk(&all, id, 1))
}

async fn depth_of(pool: &SqlitePool, id: &str) -> anyhow::Result<usize> {
    Ok(ancestors(pool, id).await?.len())
}

pub(crate) async fn create_impl(
    state: &AppState,
    name: &str,
    parent_id: Option<&str>,
) -> Result<String, String> {
    let name = clean_name(name)?;
    if let Some(parent) = parent_id {
        if queries::get(&state.db, parent).await.map_err(|e| e.to_string())?.is_none() {
            return Err("parent folder not found".into());
        }
        let depth = depth_of(&state.db, parent).await.map_err(|e| e.to_string())?;
        if depth + 1 > MAX_DEPTH {
            return Err(TOO_DEEP.into());
        }
    }
    let id = Uuid::new_v4().to_string();
    let name_ct = encrypt_name(&state.device_key, &id, &name)?;
    queries::insert(&state.db, &id, parent_id, &name_ct, now_secs())
        .await
        .map_err(|e| e.to_string())?;
    Ok(id)
}

pub(crate) async fn rename_impl(
    state: &AppState,
    id: &str,
    name: &str,
) -> Result<(), String> {
    let name = clean_name(name)?;
    if queries::get(&state.db, id).await.map_err(|e| e.to_string())?.is_none() {
        return Err("folder not found".into());
    }
    let name_ct = encrypt_name(&state.device_key, id, &name)?;
    queries::rename(&state.db, id, &name_ct, now_secs())
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn move_impl(
    state: &AppState,
    id: &str,
    new_parent: Option<&str>,
) -> Result<(), String> {
    if queries::get(&state.db, id).await.map_err(|e| e.to_string())?.is_none() {
        return Err("folder not found".into());
    }
    if let Some(parent) = new_parent {
        if parent == id {
            return Err(CYCLE.into());
        }
        if queries::get(&state.db, parent).await.map_err(|e| e.to_string())?.is_none() {
            return Err("parent folder not found".into());
        }
        // Moving a folder under its own descendant would detach the subtree from
        // the root and make the tree walk loop forever.
        let chain = ancestors(&state.db, parent).await.map_err(|e| e.to_string())?;
        if chain.iter().any(|a| a == id) {
            return Err(CYCLE.into());
        }
        let parent_depth = chain.len();
        let height = subtree_height(&state.db, id).await.map_err(|e| e.to_string())?;
        if parent_depth + height > MAX_DEPTH {
            return Err(TOO_DEEP.into());
        }
    }
    queries::set_parent(&state.db, id, new_parent, now_secs())
        .await
        .map_err(|e| e.to_string())
}

/// Copies folder `id`, its subfolders and their notes under `new_parent`, in one
/// transaction. Trashed notes are left out, locked protected ones counted in
/// `skipped_locked`.
pub(crate) async fn copy_impl(
    state: &AppState,
    id: &str,
    new_parent: Option<&str>,
) -> Result<CopyReport, String> {
    let err = |e: anyhow::Error| e.to_string();
    // A snapshot, so copying a folder into its own subtree never walks its copies.
    let all = queries::list(&state.db).await.map_err(err)?;
    let source = all.iter().find(|f| f.id == id).ok_or("folder not found")?;
    let parent_depth = match new_parent {
        Some(p) if !all.iter().any(|f| f.id == p) => return Err("parent folder not found".into()),
        Some(p) => depth_of(&state.db, p).await.map_err(err)?,
        None => 0,
    };
    if parent_depth + subtree_height(&state.db, id).await.map_err(err)? > MAX_DEPTH {
        return Err(TOO_DEEP.into());
    }

    let key = &state.device_key;
    let mut report = CopyReport::default();
    let mut folders = Vec::new();
    let mut notes = Vec::new();
    let mut stack = vec![(source, new_parent.map(String::from), 1)];
    while let Some((folder, parent, depth)) = stack.pop() {
        let name = decrypt_name(key, &folder.id, &folder.name_ct)
            .unwrap_or_else(|_| "(unreadable)".into());
        let name = if depth == 1 && folder.parent_id.as_deref() == new_parent {
            format!("{name} (copy)")
        } else {
            name
        };
        let copy_id = Uuid::new_v4().to_string();
        for note_id in queries::live_note_ids_in(&state.db, &folder.id).await.map_err(err)? {
            let Some(row) = note_get(&state.db, &note_id).await.map_err(err)? else { continue };
            match copy_row(state, &row, Some(&copy_id), false) {
                Ok(copy) => notes.push(copy),
                Err(e) if e == LOCKED => report.skipped_locked += 1,
                Err(e) => return Err(e),
            }
        }
        if depth < MAX_DEPTH {
            for child in all.iter().filter(|f| f.parent_id.as_deref() == Some(folder.id.as_str())) {
                stack.push((child, Some(copy_id.clone()), depth + 1));
            }
        }
        folders.push((encrypt_name(key, &copy_id, &name)?, copy_id, parent));
    }

    let now = now_secs();
    let mut tx = state.db.begin().await.map_err(|e| e.to_string())?;
    // Parents were pushed before their children, which the foreign key needs.
    for (name_ct, copy_id, parent) in &folders {
        queries::insert(&mut *tx, copy_id, parent.as_deref(), name_ct, now).await.map_err(err)?;
    }
    for row in &notes {
        note_insert(&mut *tx, row).await.map_err(err)?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    report.copied = notes.into_iter().map(|r| r.id).collect();
    Ok(report)
}

pub(crate) async fn list_impl(state: &AppState) -> Result<Vec<FolderJson>, String> {
    let rows = queries::list(&state.db).await.map_err(|e| e.to_string())?;
    let counts = queries::note_counts(&state.db).await.map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|r| {
            // A name that will not decrypt must not hide the whole tree; show
            // the folder so it can still be renamed or deleted.
            let name = decrypt_name(&state.device_key, &r.id, &r.name_ct)
                .unwrap_or_else(|_| "(unreadable)".into());
            let note_count = counts
                .iter()
                .find(|(fid, _)| *fid == r.id)
                .map(|(_, n)| *n)
                .unwrap_or(0);
            FolderJson { id: r.id, parent_id: r.parent_id, name, note_count, sort_order: r.sort_order }
        })
        .collect())
}

/// Root-to-leaf names, for carrying a note's folder across a transfer. A name
/// that will not decrypt is skipped rather than failing the send.
pub(crate) async fn path_of(state: &AppState, folder_id: &str) -> Vec<String> {
    let Ok(chain) = ancestors(&state.db, folder_id).await else { return Vec::new() };
    let mut names = Vec::with_capacity(chain.len());
    // `ancestors` returns nearest-first; a path reads from the root.
    for id in chain.iter().rev() {
        if let Ok(Some(row)) = queries::get(&state.db, id).await {
            if let Ok(name) = decrypt_name(&state.device_key, id, &row.name_ct) {
                names.push(name);
            }
        }
    }
    names
}

/// Resolve a root-to-leaf path on the receiving device, creating whatever is
/// missing, and return the leaf's id. Matching is by name among siblings, so a
/// second send into the same folder files alongside the first instead of making
/// a duplicate.
pub(crate) async fn ensure_path(
    state: &AppState,
    path: &[String],
) -> Result<Option<String>, String> {
    let mut parent: Option<String> = None;
    for raw in path.iter().take(MAX_DEPTH) {
        let Ok(name) = clean_name(raw) else { continue };
        let existing = list_impl(state)
            .await?
            .into_iter()
            .find(|f| f.parent_id == parent && f.name == name)
            .map(|f| f.id);
        parent = Some(match existing {
            Some(id) => id,
            None => create_impl(state, &name, parent.as_deref()).await?,
        });
    }
    Ok(parent)
}

// ---- Tauri commands ----

#[tauri::command]
pub async fn folder_create(
    name: String,
    parent_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    create_impl(&state, &name, parent_id.as_deref()).await
}

#[tauri::command]
pub async fn folder_rename(
    id: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    rename_impl(&state, &id, &name).await
}

#[tauri::command]
pub async fn folder_move(
    id: String,
    parent_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    move_impl(&state, &id, parent_id.as_deref()).await
}

#[tauri::command]
pub async fn folder_copy(
    id: String,
    parent_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<CopyReport, String> {
    copy_impl(&state, &id, parent_id.as_deref()).await
}

#[tauri::command]
pub async fn folder_delete(id: String, state: State<'_, AppState>) -> Result<(), String> {
    queries::delete(&state.db, &id).await.map_err(|e| e.to_string())
}

/// Write an explicit order for one level. The caller sends the ids as arranged,
/// so a reorder is one call and cannot leave two rows claiming a position.
#[tauri::command]
pub async fn notes_reorder(ids: Vec<String>, state: State<'_, AppState>) -> Result<(), String> {
    crate::db::queries::notes_set_order(&state.db, &ids)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn folders_reorder(ids: Vec<String>, state: State<'_, AppState>) -> Result<(), String> {
    queries::set_order(&state.db, &ids).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn folder_list(state: State<'_, AppState>) -> Result<Vec<FolderJson>, String> {
    list_impl(&state).await
}

#[tauri::command]
pub async fn note_set_folder(
    note_id: String,
    folder_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(f) = folder_id.as_deref() {
        if queries::get(&state.db, f).await.map_err(|e| e.to_string())?.is_none() {
            return Err("folder not found".into());
        }
    }
    queries::set_note_folder(&state.db, &note_id, folder_id.as_deref(), now_secs())
        .await
        .map_err(|e| e.to_string())
}
