//! Optical transfer: notes sealed for an animated QR stream (screen to camera).
//!
//! Anyone who can film the sender's screen sees every frame, so the payload is
//! sealed under a passphrase typed on both devices. Layout:
//!
//!   MAGIC (8) | salt (32) | nonce (12) | ChaCha20-Poly1305(deflate(records))
//!
//! `records` is a run of `tag u8 | u32 LE length | bytes`: a note is its
//! TransferBlob JSON (through `TransferBlob::decode` and its nesting guard on
//! the way back in), a folder is its path as a JSON array of names.
//!
//! Paths are relative to wherever the receiver files the transfer. Sending
//! notes carries no folders at all; sending a folder carries that folder and
//! everything under it, empty subfolders included.

use super::blob::TransferBlob;
use super::commands::{import_blob_detailed, ImportOutcome};
use crate::crypto::vault::{decrypt, derive_key, encrypt, random_salt};
use crate::folders::commands::{ensure_path, list_impl, path_of};
use crate::state::AppState;
use flate2::{read::DeflateDecoder, write::DeflateEncoder, Compression};
use std::io::{Read, Write};

const MAGIC: &[u8; 8] = b"PANOPT02";
const SALT_LEN: usize = 32;
const NONCE_LEN: usize = 12;
const HEADER_LEN: usize = MAGIC.len() + SALT_LEN + NONCE_LEN;
const NOTE: u8 = 1;
const FOLDER: u8 = 2;
/// Inflated size cap: notes are kilobytes, so this only stops a crafted
/// stream from inflating without bound.
const MAX_PLAINTEXT: u64 = 64 * 1024 * 1024;
const MAX_RECORDS: usize = 10_000;
/// Sealed size cap, on both ends. Far above any real batch of notes; the
/// receiver's stream decoder refuses to size itself past this too.
pub const MAX_SEALED: usize = 16 * 1024 * 1024;
/// The stream is public to anyone filming it, so the passphrase is all that
/// stands between a recording and an offline guessing attack.
pub const MIN_PASSPHRASE_CHARS: usize = 10;

/// What one stream carries.
#[derive(Debug, Default, PartialEq)]
pub struct Contents {
    pub notes: Vec<TransferBlob>,
    /// Every folder sent, empty ones included, as paths relative to the
    /// receiving folder. Notes carry their own path in `folder_path`.
    pub folders: Vec<Vec<String>>,
}

#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct OpticalImportSummary {
    pub inserted: u32,
    pub updated: u32,
}

fn write_record(out: &mut impl Write, tag: u8, bytes: &[u8]) -> anyhow::Result<()> {
    out.write_all(&[tag])?;
    out.write_all(&u32::try_from(bytes.len())?.to_le_bytes())?;
    out.write_all(bytes)?;
    Ok(())
}

pub fn seal(contents: &Contents, passphrase: &str) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        !contents.notes.is_empty() || !contents.folders.is_empty(),
        "nothing to send"
    );
    anyhow::ensure!(
        passphrase.chars().count() >= MIN_PASSPHRASE_CHARS,
        "the passphrase needs at least {MIN_PASSPHRASE_CHARS} characters"
    );
    anyhow::ensure!(
        contents.notes.len() + contents.folders.len() <= MAX_RECORDS,
        "too much to send in one transfer"
    );
    let mut deflate = DeflateEncoder::new(Vec::new(), Compression::best());
    for folder in &contents.folders {
        write_record(&mut deflate, FOLDER, &serde_json::to_vec(folder)?)?;
    }
    for note in &contents.notes {
        write_record(&mut deflate, NOTE, &note.encode()?)?;
    }
    let plaintext = deflate.finish()?;

    let salt = random_salt();
    let key = derive_key(passphrase, &salt)?;
    let (nonce, ct) = encrypt(&key, &plaintext, MAGIC)?;

    let mut out = Vec::with_capacity(HEADER_LEN + ct.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&salt);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    anyhow::ensure!(out.len() <= MAX_SEALED, "too much to send in one transfer");
    Ok(out)
}

pub fn open(payload: &[u8], passphrase: &str) -> anyhow::Result<Contents> {
    anyhow::ensure!(
        payload.len() > HEADER_LEN && payload.starts_with(MAGIC),
        "not a panote transfer"
    );
    anyhow::ensure!(payload.len() <= MAX_SEALED, "transfer too large");
    let salt = &payload[MAGIC.len()..MAGIC.len() + SALT_LEN];
    let nonce = &payload[MAGIC.len() + SALT_LEN..HEADER_LEN];
    let key = derive_key(passphrase, salt)?;
    let compressed = decrypt(&key, nonce, &payload[HEADER_LEN..], MAGIC)
        .map_err(|_| anyhow::anyhow!("wrong passphrase"))?;

    let mut records = Vec::new();
    DeflateDecoder::new(compressed.as_slice())
        .take(MAX_PLAINTEXT + 1)
        .read_to_end(&mut records)?;
    anyhow::ensure!(records.len() as u64 <= MAX_PLAINTEXT, "transfer too large");

    let mut contents = Contents::default();
    let mut rest = records.as_slice();
    while !rest.is_empty() {
        anyhow::ensure!(
            contents.notes.len() + contents.folders.len() < MAX_RECORDS,
            "too much in one transfer"
        );
        anyhow::ensure!(rest.len() >= 5, "truncated transfer");
        let tag = rest[0];
        let len = u32::from_le_bytes(rest[1..5].try_into()?) as usize;
        rest = &rest[5..];
        anyhow::ensure!(len <= rest.len(), "truncated transfer");
        let body = &rest[..len];
        match tag {
            NOTE => contents.notes.push(TransferBlob::decode(body)?),
            FOLDER => contents.folders.push(serde_json::from_slice(body)?),
            other => anyhow::bail!("unknown record {other} in transfer"),
        }
        rest = &rest[len..];
    }
    anyhow::ensure!(
        !contents.notes.is_empty() || !contents.folders.is_empty(),
        "empty transfer"
    );
    Ok(contents)
}

/// Gather what to send. With `folder`, that folder is the thing being sent:
/// every path is cut to start at it, and each folder under it goes along, empty
/// or not. Without, the notes travel on their own, with no folders, and land
/// wherever the receiver files them.
pub async fn collect(
    state: &AppState,
    note_ids: &[String],
    folder: Option<&str>,
) -> anyhow::Result<Contents> {
    let mut contents = Contents::default();
    // Names above the sent folder, stripped so every path starts at it.
    let mut above: Option<Vec<String>> = None;
    if let Some(root) = folder {
        let base = path_of(state, root).await;
        anyhow::ensure!(!base.is_empty(), "that folder no longer exists");
        let cut = base.len() - 1;
        let all = list_impl(state).await.map_err(anyhow::Error::msg)?;
        let mut queue = vec![root.to_string()];
        let mut visited = 0;
        while let Some(id) = queue.pop() {
            visited += 1;
            anyhow::ensure!(visited <= MAX_RECORDS, "too many folders to send");
            let path = path_of(state, &id).await;
            if path.len() > cut {
                contents.folders.push(path[cut..].to_vec());
            }
            queue.extend(
                all.iter()
                    .filter(|f| f.parent_id.as_deref() == Some(id.as_str()))
                    .map(|f| f.id.clone()),
            );
        }
        contents.folders.sort_by_key(|p| p.len());
        above = Some(base[..cut].to_vec());
    }
    for id in note_ids {
        let mut note = TransferBlob::decode(&super::lan::build_blob(state, id).await?)?;
        note.folder_path = match &above {
            Some(prefix) if note.folder_path.starts_with(prefix) => {
                note.folder_path[prefix.len()..].to_vec()
            }
            _ => Vec::new(),
        };
        contents.notes.push(note);
    }
    Ok(contents)
}

pub async fn pack(
    state: &AppState,
    note_ids: &[String],
    folder: Option<&str>,
    passphrase: &str,
) -> anyhow::Result<Vec<u8>> {
    seal(&collect(state, note_ids, folder).await?, passphrase)
}

/// Opens the whole payload before writing anything, so a wrong passphrase or a
/// corrupt record leaves the database untouched. The imports themselves are one
/// at a time: if one fails, the summary still counts the notes already in, and
/// importing the same stream again updates them instead of duplicating.
///
/// Everything lands under `into_folder`, or at the root without one.
pub async fn import(
    state: &AppState,
    payload: &[u8],
    passphrase: &str,
    into_folder: Option<&str>,
) -> (OpticalImportSummary, anyhow::Result<()>) {
    let mut summary = OpticalImportSummary::default();
    let contents = match open(payload, passphrase) {
        Ok(contents) => contents,
        Err(e) => return (summary, Err(e)),
    };
    let base = match into_folder {
        Some(id) => path_of(state, id).await,
        None => Vec::new(),
    };
    let under = |path: Vec<String>| base.iter().cloned().chain(path).collect::<Vec<_>>();
    for folder in contents.folders {
        if let Err(e) = ensure_path(state, &under(folder)).await {
            return (summary, Err(anyhow::anyhow!(e)));
        }
    }
    for mut note in contents.notes {
        note.folder_path = under(note.folder_path);
        match import_blob_detailed(state, &state.device_key, note).await {
            Ok((_, ImportOutcome::Inserted)) => summary.inserted += 1,
            Ok((_, ImportOutcome::Updated)) => summary.updated += 1,
            Err(e) => return (summary, Err(e)),
        }
    }
    (summary, Ok(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;
    use crate::folders::commands::create_impl;
    use serde_json::json;

    const PASS: &str = "correct horse";

    fn blob(title: &str, path: &[&str]) -> TransferBlob {
        TransferBlob {
            id: format!("id-{title}"),
            kind: "document".into(),
            title: title.into(),
            content: json!({ "body": format!("body of {title}") }),
            tags: vec!["t".into()],
            created_at: 1,
            updated_at: 2,
            origin_device_id: "dev".into(),
            origin_note_id: format!("id-{title}"),
            folder_path: path.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn notes(titles: &[&str]) -> Contents {
        Contents {
            notes: titles.iter().map(|t| blob(t, &[])).collect(),
            folders: Vec::new(),
        }
    }

    fn sealed(titles: &[&str]) -> Vec<u8> {
        seal(&notes(titles), PASS).unwrap()
    }

    async fn device(name: &str) -> AppState {
        let pool = init_pool(":memory:").await.unwrap();
        AppState::new(pool, derive_key(name, &[0u8; 16]).unwrap(), name.into())
    }

    /// (path from the root, parent name) for every folder on a device.
    async fn tree(state: &AppState) -> Vec<Vec<String>> {
        let mut out = Vec::new();
        for f in list_impl(state).await.unwrap() {
            out.push(path_of(state, &f.id).await);
        }
        out.sort();
        out
    }

    async fn folder_path_of_note(state: &AppState, title: &str) -> Vec<String> {
        let row = crate::db::queries::note_find_by_origin(&state.db, "dev", &format!("id-{title}"))
            .await
            .unwrap()
            .unwrap();
        match crate::folders::queries::note_folder(&state.db, &row.id)
            .await
            .unwrap()
        {
            Some(fid) => path_of(state, &fid).await,
            None => Vec::new(),
        }
    }

    fn strs(path: &[&str]) -> Vec<String> {
        path.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn round_trips_notes_and_folders() {
        let contents = Contents {
            notes: vec![blob("a", &["Work"]), blob("b", &[])],
            folders: vec![strs(&["Work"]), strs(&["Work", "Empty"])],
        };
        assert_eq!(
            open(&seal(&contents, PASS).unwrap(), PASS).unwrap(),
            contents
        );
    }

    #[test]
    fn the_stream_does_not_carry_the_note_in_the_clear() {
        let payload = sealed(&["secret-title"]);
        let needle = b"secret-title";
        assert!(!payload.windows(needle.len()).any(|w| w == needle));
    }

    #[test]
    fn a_wrong_passphrase_opens_nothing() {
        let err = open(&sealed(&["a"]), "wrong passphrase!").unwrap_err();
        assert_eq!(err.to_string(), "wrong passphrase");
    }

    #[test]
    fn a_tampered_byte_is_rejected() {
        let mut payload = sealed(&["a"]);
        let last = payload.len() - 1;
        payload[last] ^= 1;
        assert!(open(&payload, PASS).is_err());
    }

    #[test]
    fn foreign_bytes_are_not_mistaken_for_a_transfer() {
        assert_eq!(
            open(b"not a transfer at all, just a file", PASS)
                .unwrap_err()
                .to_string(),
            "not a panote transfer"
        );
    }

    #[test]
    fn a_short_passphrase_is_refused_before_anything_is_sealed() {
        let err = seal(&notes(&["a"]), "123456789").unwrap_err().to_string();
        assert!(err.contains("at least 10"), "{err}");
    }

    #[test]
    fn an_oversized_payload_is_refused_before_the_key_is_derived() {
        let mut payload = MAGIC.to_vec();
        payload.resize(MAX_SEALED + 1, 0);
        assert_eq!(
            open(&payload, PASS).unwrap_err().to_string(),
            "transfer too large"
        );
    }

    #[test]
    fn sealing_twice_gives_different_bytes() {
        assert_ne!(sealed(&["a"]), sealed(&["a"]));
    }

    #[test]
    fn a_truncated_record_is_rejected_even_under_the_right_key() {
        let mut deflate = DeflateEncoder::new(Vec::new(), Compression::default());
        deflate.write_all(&[NOTE]).unwrap();
        deflate.write_all(&100u32.to_le_bytes()).unwrap();
        deflate.write_all(b"short").unwrap();
        let salt = random_salt();
        let key = derive_key(PASS, &salt).unwrap();
        let (nonce, ct) = encrypt(&key, &deflate.finish().unwrap(), MAGIC).unwrap();
        let payload = [MAGIC.as_slice(), &salt, &nonce, &ct].concat();
        assert_eq!(
            open(&payload, PASS).unwrap_err().to_string(),
            "truncated transfer"
        );
    }

    #[tokio::test]
    async fn notes_packed_on_one_device_import_on_another() {
        let sender = device("sender").await;
        let receiver = device("receiver").await;
        let id =
            crate::transfer::commands::import_blob(&sender, &sender.device_key, blob("hello", &[]))
                .await
                .unwrap();

        let payload = pack(&sender, &[id], None, PASS).await.unwrap();
        let (none, wrong) = import(&receiver, &payload, "wrong passphrase!", None).await;
        assert!(wrong.is_err());
        assert_eq!(none, OpticalImportSummary::default());
        let (first, ok) = import(&receiver, &payload, PASS, None).await;
        ok.unwrap();
        assert_eq!((first.inserted, first.updated), (1, 0));
        // The same stream caught twice updates the note instead of duplicating it.
        let (again, ok) = import(&receiver, &payload, PASS, None).await;
        ok.unwrap();
        assert_eq!((again.inserted, again.updated), (0, 1));
    }

    #[tokio::test]
    async fn sending_a_note_leaves_its_folders_behind() {
        let sender = device("sender").await;
        let receiver = device("receiver").await;
        let id = crate::transfer::commands::import_blob(
            &sender,
            &sender.device_key,
            blob("filed", &["Work", "Clients"]),
        )
        .await
        .unwrap();
        let inbox = create_impl(&receiver, "Inbox", None).await.unwrap();

        let payload = pack(&sender, &[id], None, PASS).await.unwrap();
        let (_, ok) = import(&receiver, &payload, PASS, Some(&inbox)).await;
        ok.unwrap();

        assert_eq!(
            folder_path_of_note(&receiver, "filed").await,
            strs(&["Inbox"])
        );
        assert_eq!(tree(&receiver).await, vec![strs(&["Inbox"])]);
    }

    #[tokio::test]
    async fn sending_a_folder_sends_it_whole_into_the_receiving_folder() {
        let sender = device("sender").await;
        let receiver = device("receiver").await;
        // Sender: Projects / Work / { Clients (note), Empty }, and a note in Work.
        let projects = create_impl(&sender, "Projects", None).await.unwrap();
        let work = create_impl(&sender, "Work", Some(&projects)).await.unwrap();
        let clients = create_impl(&sender, "Clients", Some(&work)).await.unwrap();
        create_impl(&sender, "Empty", Some(&work)).await.unwrap();
        let import_into = |title: &'static str, path: &'static [&'static str]| {
            let sender = &sender;
            async move {
                crate::transfer::commands::import_blob(
                    sender,
                    &sender.device_key,
                    blob(title, path),
                )
                .await
                .unwrap()
            }
        };
        let top = import_into("top", &["Projects", "Work"]).await;
        let deep = import_into("deep", &["Projects", "Work", "Clients"]).await;
        assert!(!clients.is_empty());
        let inbox = create_impl(&receiver, "Inbox", None).await.unwrap();

        let payload = pack(&sender, &[top, deep], Some(&work), PASS)
            .await
            .unwrap();
        let (summary, ok) = import(&receiver, &payload, PASS, Some(&inbox)).await;
        ok.unwrap();
        assert_eq!(summary.inserted, 2);

        // "Projects" stays behind; "Work" arrives whole under "Inbox".
        assert_eq!(
            tree(&receiver).await,
            vec![
                strs(&["Inbox"]),
                strs(&["Inbox", "Work"]),
                strs(&["Inbox", "Work", "Clients"]),
                strs(&["Inbox", "Work", "Empty"]),
            ]
        );
        assert_eq!(
            folder_path_of_note(&receiver, "top").await,
            strs(&["Inbox", "Work"])
        );
        assert_eq!(
            folder_path_of_note(&receiver, "deep").await,
            strs(&["Inbox", "Work", "Clients"])
        );
    }

    #[tokio::test]
    async fn received_at_the_root_without_a_target_folder() {
        let receiver = device("receiver").await;
        let contents = Contents {
            notes: vec![blob("loose", &[])],
            folders: Vec::new(),
        };
        let (_, ok) = import(&receiver, &seal(&contents, PASS).unwrap(), PASS, None).await;
        ok.unwrap();
        assert_eq!(
            folder_path_of_note(&receiver, "loose").await,
            Vec::<String>::new()
        );
    }
}
