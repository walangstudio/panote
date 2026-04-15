// Portable backup format for panote notes.
//
// # Versioning rules (forward compatibility)
//
// Two distinct version fields:
//
// * `format_version` (integer): schema of the file itself. Bump ONLY when a
//   change is non-backward-compatible within a single struct — field removed,
//   field renamed, semantics changed, nested shape reshaped. Adding a new
//   optional field does NOT warrant a bump; use `#[serde(default)]` on the
//   new field and it will deserialize cleanly from older files.
//
// * `app_version` (string): informational only. The panote semver that produced
//   the file. Never branch logic on it.
//
// # Adding a new format version
//
// When v1 can no longer represent a change, add `ExportFileV2` as a frozen copy
// of `ExportFileV1`, apply the change to V2, bump `CURRENT_FORMAT_VERSION`, and
// add a `v1_to_v2` upgrade function. Never mutate `ExportFileV1` after this
// module has shipped — old backups in the wild still parse against it. Each
// past version gets a permanent fixture test in the test module below.

use crate::{
    crypto::note::decrypt_with_vault,
    db::queries::{self, NoteRow},
    state::{now_secs, AppState},
    transfer::commands::{import_blob_detailed, ImportOutcome},
    transfer::blob::TransferBlob,
};
use serde::{Deserialize, Serialize};
use tauri::State;

pub const CURRENT_FORMAT_VERSION: u32 = 1;
pub const FORMAT_TAG: &str = "panote-export";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NoteExportEntryV1 {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub content: serde_json::Value,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub origin_device_id: String,
    pub origin_note_id: String,
    #[serde(default)]
    pub content_hint: Option<String>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub bg_color: Option<String>,
    #[serde(default)]
    pub bg_image: Option<String>,
    #[serde(default = "default_show_preview")]
    pub show_preview: bool,
}

fn default_show_preview() -> bool { true }

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExportFileV1 {
    pub format: String,
    pub format_version: u32,
    pub app_version: String,
    pub exported_at: i64,
    pub device_uuid: String,
    #[serde(default)]
    pub device_name: Option<String>,
    pub notes: Vec<NoteExportEntryV1>,
}

/// Internal unified representation after any version upgrades.
type ExportFile = ExportFileV1;

/// Parse export bytes, detect the version, and upgrade to the current schema.
/// Rejects unknown formats and versions newer than this build.
pub fn parse_export(bytes: &[u8]) -> anyhow::Result<ExportFile> {
    let raw: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| anyhow::anyhow!("not a valid JSON file: {e}"))?;

    if raw.get("format").and_then(|v| v.as_str()) != Some(FORMAT_TAG) {
        anyhow::bail!("not a panote export file (missing or wrong 'format' tag)");
    }

    let version = raw
        .get("format_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;

    match version {
        1 => {
            let v1: ExportFileV1 = serde_json::from_value(raw)
                .map_err(|e| anyhow::anyhow!("malformed v1 export: {e}"))?;
            Ok(v1)
        }
        n if n > CURRENT_FORMAT_VERSION => anyhow::bail!(
            "this backup was made by a newer version of panote (export format v{n}). \
             Update panote to at least the version that wrote this file before importing."
        ),
        n => anyhow::bail!("unknown export format version: {n}"),
    }
}

/// How to handle notes that already exist locally (matched by origin).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportResolution {
    /// Replace local copy when origin matches.
    Overwrite,
    /// Leave local copy untouched when origin matches.
    Skip,
    /// Always create a new local row, even when origin matches (keeps both).
    KeepBoth,
}

#[derive(Debug, Serialize)]
pub struct ImportSummary {
    pub imported: u32,
    pub updated: u32,
    pub skipped: u32,
    pub errors: Vec<String>,
}

// ----- Commands -----

/// Decrypt all notes and return the export file as a UTF-8 JSON string.
/// The frontend is responsible for writing this to disk (via a Blob download
/// or similar) so we don't need a native file-dialog plugin.
#[tauri::command]
pub async fn notes_export(
    app_version: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let key = &state.device_key;
    let rows = queries::note_list(&state.db)
        .await
        .map_err(|e| e.to_string())?;

    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        let entry = row_to_entry(key, &row).map_err(|e| e.to_string())?;
        entries.push(entry);
    }

    let device_name = crate::transfer::commands::resolve_device_name(&state.db)
        .await
        .ok();

    let file = ExportFileV1 {
        format: FORMAT_TAG.into(),
        format_version: CURRENT_FORMAT_VERSION,
        app_version,
        exported_at: now_secs(),
        device_uuid: state.device_uuid.clone(),
        device_name,
        notes: entries,
    };

    serde_json::to_string_pretty(&file).map_err(|e| e.to_string())
}

/// Accept an export file's contents as a string and import all notes.
/// Resolution controls how origin-duplicates are handled.
#[tauri::command]
pub async fn notes_import(
    contents: String,
    resolution: ImportResolution,
    state: State<'_, AppState>,
) -> Result<ImportSummary, String> {
    let file = parse_export(contents.as_bytes()).map_err(|e| e.to_string())?;

    let mut summary = ImportSummary {
        imported: 0,
        updated: 0,
        skipped: 0,
        errors: Vec::new(),
    };

    for entry in file.notes {
        match import_entry(&state, entry, resolution).await {
            Ok(ImportEntryResult::Inserted) => summary.imported += 1,
            Ok(ImportEntryResult::Updated) => summary.updated += 1,
            Ok(ImportEntryResult::Skipped) => summary.skipped += 1,
            Err(e) => summary.errors.push(e.to_string()),
        }
    }

    Ok(summary)
}

// ----- Helpers -----

fn row_to_entry(key: &[u8; 32], row: &NoteRow) -> anyhow::Result<NoteExportEntryV1> {
    let title_bytes = decrypt_with_vault(key, &row.title_nonce, &row.title_ct)?;
    let title = String::from_utf8(title_bytes)?;
    let content_bytes = decrypt_with_vault(key, &row.nonce, &row.content_ct)?;
    let content: serde_json::Value = serde_json::from_slice(&content_bytes)?;
    let tags: Vec<String> = serde_json::from_str(&row.tags).unwrap_or_default();

    Ok(NoteExportEntryV1 {
        id: row.id.clone(),
        kind: row.kind.clone(),
        title,
        content,
        tags,
        created_at: row.created_at,
        updated_at: row.updated_at,
        origin_device_id: row.origin_device_id.clone(),
        origin_note_id: row.origin_note_id.clone(),
        content_hint: row.content_hint.clone(),
        pinned: row.pinned,
        bg_color: row.bg_color.clone(),
        bg_image: row.bg_image.clone(),
        show_preview: row.show_preview,
    })
}

enum ImportEntryResult {
    Inserted,
    Updated,
    Skipped,
}

async fn import_entry(
    state: &AppState,
    entry: NoteExportEntryV1,
    resolution: ImportResolution,
) -> anyhow::Result<ImportEntryResult> {
    // Skip/KeepBoth need a pre-check; Overwrite can go straight through import_blob_detailed.
    if resolution != ImportResolution::Overwrite && !entry.origin_device_id.is_empty() {
        let existing = queries::note_find_by_origin(
            &state.db,
            &entry.origin_device_id,
            &entry.origin_note_id,
        )
        .await?;

        if existing.is_some() {
            match resolution {
                ImportResolution::Skip => return Ok(ImportEntryResult::Skipped),
                ImportResolution::KeepBoth => {
                    // Strip origin so import_blob_detailed treats it as a fresh note
                    // with a newly-minted local origin (attributed to this device).
                    let mut stripped = entry;
                    stripped.origin_device_id = String::new();
                    stripped.origin_note_id = String::new();
                    return insert_as_blob(state, stripped).await;
                }
                ImportResolution::Overwrite => unreachable!(),
            }
        }
    }

    insert_as_blob(state, entry).await
}

async fn insert_as_blob(
    state: &AppState,
    entry: NoteExportEntryV1,
) -> anyhow::Result<ImportEntryResult> {
    // Re-use import_blob_detailed for the encrypt + insert/update plumbing,
    // then (if it was an insert) apply the extras — pinned/bg/etc — that the
    // transfer blob doesn't carry. For updates we intentionally preserve the
    // existing row's extras to match transfer semantics.
    let blob = TransferBlob {
        id: entry.id.clone(),
        kind: entry.kind.clone(),
        title: entry.title.clone(),
        content: entry.content.clone(),
        tags: entry.tags.clone(),
        created_at: entry.created_at,
        updated_at: entry.updated_at,
        origin_device_id: entry.origin_device_id.clone(),
        origin_note_id: entry.origin_note_id.clone(),
    };

    let (local_id, outcome) = import_blob_detailed(state, &state.device_key, blob).await?;

    if matches!(outcome, ImportOutcome::Inserted) {
        // Apply inserted-only extras (pinned, bg_color, bg_image, show_preview,
        // content_hint). Use a targeted UPDATE to avoid rewriting ciphertext.
        sqlx::query(
            "UPDATE notes SET pinned = ?, bg_color = ?, bg_image = ?, show_preview = ?, content_hint = ? WHERE id = ?",
        )
        .bind(entry.pinned as i32)
        .bind(&entry.bg_color)
        .bind(&entry.bg_image)
        .bind(entry.show_preview as i32)
        .bind(&entry.content_hint)
        .bind(&local_id)
        .execute(&state.db)
        .await?;
    }

    Ok(match outcome {
        ImportOutcome::Inserted => ImportEntryResult::Inserted,
        ImportOutcome::Updated => ImportEntryResult::Updated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::vault::derive_key,
        db::init_pool,
        state::AppState,
    };
    use serde_json::json;

    async fn test_state() -> AppState {
        let pool = init_pool(":memory:").await.unwrap();
        let key = derive_key("export-test-key", &[0u8; 16]).unwrap();
        AppState::new(pool, key, "device-a".into())
    }

    fn sample_v1() -> ExportFileV1 {
        ExportFileV1 {
            format: FORMAT_TAG.into(),
            format_version: 1,
            app_version: "0.4.0".into(),
            exported_at: 1700000000,
            device_uuid: "device-a".into(),
            device_name: Some("Alice".into()),
            notes: vec![NoteExportEntryV1 {
                id: "n1".into(),
                kind: "document".into(),
                title: "Hello".into(),
                content: json!({ "body": "world" }),
                tags: vec!["greetings".into()],
                created_at: 1699999000,
                updated_at: 1700000000,
                origin_device_id: "device-a".into(),
                origin_note_id: "n1".into(),
                content_hint: Some("plain".into()),
                pinned: false,
                bg_color: None,
                bg_image: None,
                show_preview: true,
            }],
        }
    }

    #[test]
    fn parse_rejects_non_panote_file() {
        let bytes = br#"{"format":"some-other-app","format_version":1}"#;
        assert!(parse_export(bytes).is_err());
    }

    #[test]
    fn parse_rejects_newer_format_version() {
        let raw = serde_json::json!({
            "format": FORMAT_TAG,
            "format_version": 999,
            "app_version": "99.0.0",
            "exported_at": 0,
            "device_uuid": "x",
            "notes": [],
        });
        let bytes = serde_json::to_vec(&raw).unwrap();
        let err = parse_export(&bytes).unwrap_err().to_string();
        assert!(err.contains("newer version"), "got: {err}");
    }

    #[test]
    fn parse_accepts_current_v1() {
        let bytes = serde_json::to_vec(&sample_v1()).unwrap();
        let parsed = parse_export(&bytes).unwrap();
        assert_eq!(parsed.format_version, 1);
        assert_eq!(parsed.notes.len(), 1);
        assert_eq!(parsed.notes[0].title, "Hello");
    }

    #[test]
    fn parse_v1_tolerates_missing_optional_fields() {
        // Minimal v1: only the required fields.
        let raw = serde_json::json!({
            "format": FORMAT_TAG,
            "format_version": 1,
            "app_version": "0.4.0",
            "exported_at": 1,
            "device_uuid": "device-a",
            "notes": [{
                "id": "n1",
                "kind": "document",
                "title": "t",
                "content": { "body": "" },
                "tags": [],
                "created_at": 0,
                "updated_at": 0,
                "origin_device_id": "device-a",
                "origin_note_id": "n1"
            }]
        });
        let bytes = serde_json::to_vec(&raw).unwrap();
        let parsed = parse_export(&bytes).unwrap();
        assert!(!parsed.notes[0].pinned);
        assert!(parsed.notes[0].show_preview);
    }

    #[tokio::test]
    async fn import_then_export_roundtrips_count() {
        let state = test_state().await;
        let file = sample_v1();
        let bytes = serde_json::to_vec(&file).unwrap();

        let contents = String::from_utf8(bytes).unwrap();
        let parsed = parse_export(contents.as_bytes()).unwrap();
        for entry in parsed.notes {
            insert_as_blob(&state, entry).await.unwrap();
        }

        let rows = queries::note_list(&state.db).await.unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[tokio::test]
    async fn import_twice_overwrite_is_idempotent() {
        let state = test_state().await;
        let bytes = serde_json::to_vec(&sample_v1()).unwrap();
        let contents = String::from_utf8(bytes).unwrap();

        for entry in parse_export(contents.as_bytes()).unwrap().notes {
            import_entry(&state, entry, ImportResolution::Overwrite).await.unwrap();
        }
        // Second run: should update, not insert.
        for entry in parse_export(contents.as_bytes()).unwrap().notes {
            let res = import_entry(&state, entry, ImportResolution::Overwrite).await.unwrap();
            assert!(matches!(res, ImportEntryResult::Updated));
        }
        assert_eq!(queries::note_list(&state.db).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn import_twice_skip_leaves_existing() {
        let state = test_state().await;
        let bytes = serde_json::to_vec(&sample_v1()).unwrap();
        let contents = String::from_utf8(bytes).unwrap();

        for entry in parse_export(contents.as_bytes()).unwrap().notes {
            import_entry(&state, entry, ImportResolution::Overwrite).await.unwrap();
        }
        for entry in parse_export(contents.as_bytes()).unwrap().notes {
            let res = import_entry(&state, entry, ImportResolution::Skip).await.unwrap();
            assert!(matches!(res, ImportEntryResult::Skipped));
        }
        assert_eq!(queries::note_list(&state.db).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn import_keep_both_creates_duplicate() {
        let state = test_state().await;
        let bytes = serde_json::to_vec(&sample_v1()).unwrap();
        let contents = String::from_utf8(bytes).unwrap();

        for entry in parse_export(contents.as_bytes()).unwrap().notes {
            import_entry(&state, entry, ImportResolution::Overwrite).await.unwrap();
        }
        for entry in parse_export(contents.as_bytes()).unwrap().notes {
            let res = import_entry(&state, entry, ImportResolution::KeepBoth).await.unwrap();
            assert!(matches!(res, ImportEntryResult::Inserted));
        }
        assert_eq!(queries::note_list(&state.db).await.unwrap().len(), 2);
    }
}
