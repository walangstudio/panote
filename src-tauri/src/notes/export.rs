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
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use tauri::State;

pub const CURRENT_FORMAT_VERSION: u32 = 2;
pub const FORMAT_TAG: &str = "panote-export";

/// A password-protected note's content, re-encrypted for the backup file.
///
/// The at-rest ciphertext can't simply be copied out: it is wrapped under this
/// device's key, which the importing device does not have. So the content is
/// decrypted and re-encrypted under a key derived from the note's own password
/// (Argon2id over a fresh salt), which travels with the file and nothing else.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SecretBlobV1 {
    /// base64 Argon2id salt.
    pub salt: String,
    /// base64 ChaCha20-Poly1305 nonce.
    pub nonce: String,
    /// base64 ciphertext of the note's content JSON.
    pub ct: String,
}

/// Bound as AAD so a blob cannot be swapped between entries in the file.
const SECRET_AAD_PREFIX: &[u8] = b"panote-export-secret-v1:";

fn secret_aad(note_id: &str) -> Vec<u8> {
    [SECRET_AAD_PREFIX, note_id.as_bytes()].concat()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NoteExportEntryV1 {
    pub id: String,
    pub kind: String,
    pub title: String,
    /// Null for protected notes — their content lives in `secret` instead.
    pub content: serde_json::Value,
    /// Present only for password-protected notes. See [`SecretBlobV1`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<SecretBlobV1>,
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

/// v2 shares v1's *shape* exactly — what changed is the semantics of a document
/// note's `content.body`, not the file schema — so there is no frozen struct copy
/// to make here. A future change that alters fields must follow the rules at the
/// top of this module and define a real `ExportFileV3`.
///
/// v1 → v2: coloured text moved off KaTeX inline math and onto an inline span.
/// v1 wrote `$\textcolor{#hex}{\text{...}}$`; v2 writes
/// `<span style="color:#hex">...</span>`, which is what the rich editor round-trips.
pub type ExportFileV2 = ExportFileV1;

/// Internal unified representation after any version upgrades.
type ExportFile = ExportFileV2;

/// Reverse `escapeLatexText` from the old markdown editor, so text that was
/// escaped to survive inside `\text{}` comes back as the user originally typed it.
fn unescape_latex_text(s: &str) -> String {
    s.replace("\\textbackslash{}", "\\")
        .replace("\\textasciitilde{}", "~")
        .replace("\\textasciicircum{}", "^")
        .replace("\\{", "{")
        .replace("\\}", "}")
        .replace("\\$", "$")
        .replace("\\&", "&")
        .replace("\\#", "#")
        .replace("\\%", "%")
        .replace("\\_", "_")
}

/// Rewrite every `$\textcolor{#hex}{\text{...}}$` run in `body` as an inline span.
/// Hand-rolled rather than regex-driven because the inner text can contain the
/// braces we are scanning for, so we have to track nesting depth.
fn migrate_color_syntax(body: &str) -> String {
    const OPEN: &str = "$\\textcolor{";
    let mut out = String::with_capacity(body.len());
    let mut rest = body;

    while let Some(at) = rest.find(OPEN) {
        let (before, tail) = rest.split_at(at);
        let after_open = &tail[OPEN.len()..];

        // colour, up to the closing brace
        let Some(close) = after_open.find('}') else { break };
        let color = &after_open[..close];
        let after_color = &after_open[close + 1..];

        // must be followed by the literal `{\text{`
        const TEXT_OPEN: &str = "{\\text{";
        if !after_color.starts_with(TEXT_OPEN) || !is_hex_color(color) {
            // Not a colour run we wrote; copy the marker through untouched.
            out.push_str(before);
            out.push_str(OPEN);
            rest = after_open;
            continue;
        }
        let inner_start = &after_color[TEXT_OPEN.len()..];

        // Walk to the brace that closes `\text{`, honouring nesting and escapes.
        let mut depth = 1usize;
        let mut end = None;
        let bytes = inner_start.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 1, // skip the escaped char
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 { end = Some(i); break; }
                }
                _ => {}
            }
            i += 1;
        }
        let Some(end) = end else { break };

        let inner = &inner_start[..end];
        // after the inner text we expect `}$` closing the group and the math
        let tail_after = &inner_start[end + 1..];
        let Some(stripped) = tail_after.strip_prefix("}$") else {
            out.push_str(before);
            out.push_str(OPEN);
            rest = after_open;
            continue;
        };

        out.push_str(before);
        out.push_str(&format!(
            "<span style=\"color:{}\">{}</span>",
            color,
            unescape_latex_text(inner)
        ));
        rest = stripped;
    }
    out.push_str(rest);
    out
}

fn is_hex_color(s: &str) -> bool {
    let h = s.strip_prefix('#').unwrap_or("");
    !h.is_empty() && h.len() <= 8 && h.chars().all(|c| c.is_ascii_hexdigit())
}

/// Upgrade a v1 file in place. Only `document` notes carry markdown bodies.
fn v1_to_v2(mut file: ExportFileV1) -> ExportFileV2 {
    for note in &mut file.notes {
        if note.kind != "document" {
            continue;
        }
        if let Some(body) = note.content.get("body").and_then(|b| b.as_str()) {
            let migrated = migrate_color_syntax(body);
            if migrated != body {
                note.content["body"] = serde_json::Value::String(migrated);
            }
        }
    }
    file.format_version = CURRENT_FORMAT_VERSION;
    file
}

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
            Ok(v1_to_v2(v1))
        }
        2 => {
            let v2: ExportFileV2 = serde_json::from_value(raw)
                .map_err(|e| anyhow::anyhow!("malformed v2 export: {e}"))?;
            Ok(v2)
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
    let mut locked = 0usize;
    for mut row in rows {
        // Protected notes carry a password layer over the vault ciphertext. Peel
        // it with the session-cached password so the content can be re-sealed
        // under that same password for the file — a backup must never contain a
        // protected note in the clear. A note that can't be peeled (not unlocked
        // this session) is counted, not silently dropped, and fails loudly below.
        let mut note_password: Option<String> = None;
        if row.note_salt.is_some() {
            let password = state.note_password(&row.id);
            match crate::crypto::note::peel_vault_ct(
                row.note_salt.as_deref(),
                row.note_nonce.as_deref(),
                &row.content_ct,
                password.as_deref(),
            ) {
                Ok(vault_ct) => {
                    row.content_ct = vault_ct;
                    row.note_salt = None;
                    row.note_nonce = None;
                    note_password = password;
                }
                Err(_) => {
                    locked += 1;
                    continue;
                }
            }
        }
        let entry =
            row_to_entry(key, &row, note_password.as_deref()).map_err(|e| e.to_string())?;
        entries.push(entry);
    }
    if locked > 0 {
        return Err(format!(
            "{locked} password-protected note(s) must be unlocked before export, \
             otherwise they would be left out of the backup. Open each protected \
             note to unlock it this session, then export again."
        ));
    }

    let device_name = crate::transfer::commands::resolve_device_name(&state.db)
        .await
        .ok();

    let file = ExportFileV2 {
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
    secret_password: Option<String>,
    state: State<'_, AppState>,
) -> Result<ImportSummary, String> {
    let file = parse_export(contents.as_bytes()).map_err(|e| e.to_string())?;

    let mut summary = ImportSummary {
        imported: 0,
        updated: 0,
        skipped: 0,
        errors: Vec::new(),
    };

    for mut entry in file.notes {
        // A sealed note needs its password before anything else can happen; a
        // failure here is reported, never swallowed into a half-imported note.
        if let Some(blob) = entry.secret.take() {
            let Some(pw) = secret_password.as_deref() else {
                summary.errors.push(format!(
                    "\"{}\" is password-protected — supply its password to import it",
                    entry.title
                ));
                continue;
            };
            match open_secret(&entry.id, &blob, pw) {
                Ok(content) => entry.content = content,
                Err(_) => {
                    summary.errors.push(format!(
                        "\"{}\" could not be decrypted — wrong password?",
                        entry.title
                    ));
                    continue;
                }
            }
        }
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

/// Re-encrypt a protected note's content under its own password so the backup
/// never carries it in the clear.
fn seal_secret(note_id: &str, content_json: &[u8], password: &str) -> anyhow::Result<SecretBlobV1> {
    let salt = crate::crypto::vault::random_salt();
    let key = crate::crypto::vault::derive_key(password, &salt)?;
    let (nonce, ct) = crate::crypto::vault::encrypt(&key, content_json, &secret_aad(note_id))?;
    Ok(SecretBlobV1 {
        salt: STANDARD.encode(salt),
        nonce: STANDARD.encode(nonce),
        ct: STANDARD.encode(ct),
    })
}

/// Reverse of [`seal_secret`]. Wrong password surfaces as an error, never as
/// silently dropped content.
fn open_secret(
    note_id: &str,
    blob: &SecretBlobV1,
    password: &str,
) -> anyhow::Result<serde_json::Value> {
    let salt = STANDARD.decode(&blob.salt)?;
    let nonce = STANDARD.decode(&blob.nonce)?;
    let ct = STANDARD.decode(&blob.ct)?;
    let key = crate::crypto::vault::derive_key(password, &salt)?;
    let plain = crate::crypto::vault::decrypt(&key, &nonce, &ct, &secret_aad(note_id))?;
    Ok(serde_json::from_slice(&plain)?)
}

/// `password` is `Some` for protected notes: their content is sealed under it
/// rather than written out in the clear.
fn row_to_entry(
    key: &[u8; 32],
    row: &NoteRow,
    password: Option<&str>,
) -> anyhow::Result<NoteExportEntryV1> {
    let title_bytes = decrypt_with_vault(key, &row.title_nonce, &row.title_ct, row.id.as_bytes())?;
    let title = String::from_utf8(title_bytes)?;
    let content_bytes = decrypt_with_vault(key, &row.nonce, &row.content_ct, row.id.as_bytes())?;
    let tags = crate::notes::commands::decrypt_tags(key, &row.id, &row.tags)?;

    let (content, secret) = match password {
        Some(pw) => (
            serde_json::Value::Null,
            Some(seal_secret(&row.id, &content_bytes, pw)?),
        ),
        None => (serde_json::from_slice(&content_bytes)?, None),
    };

    Ok(NoteExportEntryV1 {
        id: row.id.clone(),
        kind: row.kind.clone(),
        title,
        content,
        secret,
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
        // A backup file carries no folder; import lands notes at the root.
        folder_path: Vec::new(),
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
        // A backup file is untrusted input. note_create/note_update validate
        // bg_image, but this path bypassed them entirely, so a crafted file could
        // smuggle an oversized image or a disallowed MIME straight into the DB.
        // Drop an invalid image rather than failing the whole note — the rest of
        // the note is still worth importing.
        let bg_image = match crate::notes::commands::validate_bg_image(&entry.bg_image) {
            Ok(()) => entry.bg_image.clone(),
            Err(_) => None,
        };
        // Apply inserted-only extras (pinned, bg_color, bg_image, show_preview,
        // content_hint). Use a targeted UPDATE to avoid rewriting ciphertext.
        sqlx::query(
            "UPDATE notes SET pinned = ?, bg_color = ?, bg_image = ?, show_preview = ?, content_hint = ? WHERE id = ?",
        )
        .bind(entry.pinned as i32)
        .bind(&entry.bg_color)
        .bind(&bg_image)
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
        let key = derive_key("export-test-key", &crate::crypto::vault::random_salt()).unwrap();
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
                secret: None,
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

    fn body_of(f: &ExportFile) -> String {
        f.notes[0].content["body"].as_str().unwrap().to_string()
    }

    fn v1_with_body(body: &str) -> Vec<u8> {
        let mut f = sample_v1();
        f.notes[0].content = json!({ "body": body });
        serde_json::to_vec(&f).unwrap()
    }

    #[test]
    fn v1_colored_text_becomes_a_span() {
        let bytes = v1_with_body("hi $\\textcolor{#3182ce}{\\text{blue bit}}$ there");
        let parsed = parse_export(&bytes).unwrap();
        assert_eq!(
            body_of(&parsed),
            "hi <span style=\"color:#3182ce\">blue bit</span> there"
        );
    }

    #[test]
    fn v1_migration_handles_multiple_runs_and_unescapes() {
        // `_` and `$` were LaTeX-escaped on the way in; they must come back plain.
        let bytes = v1_with_body(
            "$\\textcolor{#e53e3e}{\\text{a\\_b}}$ and $\\textcolor{#27ae60}{\\text{5\\$}}$",
        );
        let parsed = parse_export(&bytes).unwrap();
        assert_eq!(
            body_of(&parsed),
            "<span style=\"color:#e53e3e\">a_b</span> and <span style=\"color:#27ae60\">5$</span>"
        );
    }

    /// Real maths must survive untouched — only our colour runs get rewritten.
    #[test]
    fn v1_migration_leaves_other_math_alone() {
        let src = "cost is $x^2 + y$ and $\\textcolor{notahex}{\\text{q}}$";
        let parsed = parse_export(&v1_with_body(src)).unwrap();
        assert_eq!(body_of(&parsed), src);
    }

    /// Non-document kinds have structured content, not a markdown body.
    #[test]
    fn v1_migration_skips_non_document_kinds() {
        let mut f = sample_v1();
        f.notes[0].kind = "checklist".into();
        f.notes[0].content = json!({ "body": "$\\textcolor{#e53e3e}{\\text{x}}$" });
        let parsed = parse_export(&serde_json::to_vec(&f).unwrap()).unwrap();
        assert_eq!(body_of(&parsed), "$\\textcolor{#e53e3e}{\\text{x}}$");
    }

    #[test]
    fn v2_parses_without_modification() {
        let mut f = sample_v1();
        f.format_version = 2;
        f.notes[0].content = json!({ "body": "<span style=\"color:#3182ce\">kept</span>" });
        let parsed = parse_export(&serde_json::to_vec(&f).unwrap()).unwrap();
        assert_eq!(body_of(&parsed), "<span style=\"color:#3182ce\">kept</span>");
    }

    #[test]
    fn export_writes_the_current_version() {
        assert_eq!(CURRENT_FORMAT_VERSION, 2);
    }

    // ---- Protected notes must never appear in the clear in a backup ----

    const SECRET_BODY: &str = "AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI";

    fn sealed_entry(password: &str) -> NoteExportEntryV1 {
        let content = json!({ "body": SECRET_BODY });
        let bytes = serde_json::to_vec(&content).unwrap();
        let mut e = sample_v1().notes.remove(0);
        e.id = "secret-note".into();
        e.title = "Credentials".into();
        e.content = serde_json::Value::Null;
        e.secret = Some(seal_secret(&e.id, &bytes, password).unwrap());
        e
    }

    #[test]
    fn sealed_content_is_not_recoverable_from_the_file() {
        let entry = sealed_entry("correct horse");
        let mut f = sample_v1();
        f.notes = vec![entry];
        let text = serde_json::to_string(&f).unwrap();
        assert!(
            !text.contains(SECRET_BODY),
            "the backup must not contain the note body in the clear"
        );
        assert!(!text.contains("correct horse"), "nor the password");
    }

    #[test]
    fn sealed_content_round_trips_with_the_right_password() {
        let entry = sealed_entry("correct horse");
        let opened = open_secret("secret-note", entry.secret.as_ref().unwrap(), "correct horse")
            .unwrap();
        assert_eq!(opened["body"], SECRET_BODY);
    }

    #[test]
    fn sealed_content_rejects_the_wrong_password() {
        let entry = sealed_entry("correct horse");
        assert!(
            open_secret("secret-note", entry.secret.as_ref().unwrap(), "wrong").is_err(),
            "a wrong password must fail, not return garbage"
        );
    }

    /// AAD binds each blob to its note id, so an attacker with write access to
    /// the file cannot move one note's secret onto another entry.
    #[test]
    fn sealed_content_is_bound_to_its_note_id() {
        let entry = sealed_entry("correct horse");
        assert!(
            open_secret("a-different-note", entry.secret.as_ref().unwrap(), "correct horse")
                .is_err(),
            "a blob must not decrypt under another note's id"
        );
    }

    /// Each seal draws a fresh salt and nonce, so identical content does not
    /// produce identical ciphertext.
    #[test]
    fn sealing_twice_produces_different_ciphertext() {
        let a = sealed_entry("pw").secret.unwrap();
        let b = sealed_entry("pw").secret.unwrap();
        assert_ne!(a.salt, b.salt);
        assert_ne!(a.ct, b.ct);
    }

    #[test]
    fn unprotected_notes_carry_no_secret_blob() {
        let f = sample_v1();
        assert!(f.notes[0].secret.is_none());
        let text = serde_json::to_string(&f).unwrap();
        assert!(!text.contains("\"secret\""), "field should be omitted, not null");
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

    /// A v1 backup — the format every note exported before the rich editor —
    /// must still import, and come back tagged as the current version.
    #[test]
    fn parse_upgrades_v1_to_current() {
        let bytes = serde_json::to_vec(&sample_v1()).unwrap();
        let parsed = parse_export(&bytes).unwrap();
        assert_eq!(parsed.format_version, CURRENT_FORMAT_VERSION);
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

    /// A backup file is untrusted. note_create/note_update validate bg_image,
    /// but this path bypassed them, so a crafted file could smuggle a disallowed
    /// MIME or an oversized image straight into the database.
    #[tokio::test]
    async fn import_drops_a_bg_image_that_fails_validation() {
        let state = test_state().await;
        let mut f = sample_v1();
        // Explicitly blocked elsewhere: SVG can carry script.
        f.notes[0].bg_image = Some("data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=".into());
        let contents = String::from_utf8(serde_json::to_vec(&f).unwrap()).unwrap();

        for entry in parse_export(contents.as_bytes()).unwrap().notes {
            import_entry(&state, entry, ImportResolution::Overwrite).await.unwrap();
        }

        let rows = queries::note_list(&state.db).await.unwrap();
        assert_eq!(rows.len(), 1, "the note itself should still import");
        assert!(
            rows[0].bg_image.is_none(),
            "an invalid bg_image must not reach the database: {:?}",
            rows[0].bg_image
        );
    }

    #[tokio::test]
    async fn import_keeps_a_valid_bg_image() {
        let state = test_state().await;
        let mut f = sample_v1();
        let png = "data:image/png;base64,iVBORw0KGgo=";
        f.notes[0].bg_image = Some(png.into());
        let contents = String::from_utf8(serde_json::to_vec(&f).unwrap()).unwrap();

        for entry in parse_export(contents.as_bytes()).unwrap().notes {
            import_entry(&state, entry, ImportResolution::Overwrite).await.unwrap();
        }

        let rows = queries::note_list(&state.db).await.unwrap();
        assert_eq!(rows[0].bg_image.as_deref(), Some(png));
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
