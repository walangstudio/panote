//! Optical transfer: notes sealed for an animated QR stream (screen to camera).
//!
//! Anyone who can film the sender's screen sees every frame, so the payload is
//! sealed under a passphrase typed on both devices. Layout:
//!
//!   MAGIC (8) | salt (32) | nonce (12) | ChaCha20-Poly1305(deflate(records))
//!
//! `records` is a run of `u32 LE length | TransferBlob JSON`, so each blob goes
//! through `TransferBlob::decode` and its nesting guard on the way back in.

use super::blob::TransferBlob;
use super::commands::{import_blob_detailed, ImportOutcome};
use crate::crypto::vault::{decrypt, derive_key, encrypt, random_salt};
use crate::state::AppState;
use flate2::{read::DeflateDecoder, write::DeflateEncoder, Compression};
use std::io::{Read, Write};

const MAGIC: &[u8; 8] = b"PANOPT01";
const SALT_LEN: usize = 32;
const NONCE_LEN: usize = 12;
const HEADER_LEN: usize = MAGIC.len() + SALT_LEN + NONCE_LEN;
/// Inflated size cap: notes are kilobytes, so this only stops a crafted
/// stream from inflating without bound.
const MAX_PLAINTEXT: u64 = 64 * 1024 * 1024;
const MAX_NOTES: usize = 10_000;
/// Sealed size cap, on both ends. Far above any real batch of notes; the
/// receiver's stream decoder refuses to size itself past this too.
pub const MAX_SEALED: usize = 16 * 1024 * 1024;
/// The stream is public to anyone filming it, so the passphrase is all that
/// stands between a recording and an offline guessing attack.
pub const MIN_PASSPHRASE_CHARS: usize = 10;

#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct OpticalImportSummary {
    pub inserted: u32,
    pub updated: u32,
}

pub fn seal(blobs: &[Vec<u8>], passphrase: &str) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(!blobs.is_empty(), "nothing to send");
    anyhow::ensure!(
        passphrase.chars().count() >= MIN_PASSPHRASE_CHARS,
        "the passphrase needs at least {MIN_PASSPHRASE_CHARS} characters"
    );
    anyhow::ensure!(blobs.len() <= MAX_NOTES, "too many notes for one transfer");
    let mut deflate = DeflateEncoder::new(Vec::new(), Compression::best());
    for blob in blobs {
        deflate.write_all(&u32::try_from(blob.len())?.to_le_bytes())?;
        deflate.write_all(blob)?;
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

pub fn open(payload: &[u8], passphrase: &str) -> anyhow::Result<Vec<TransferBlob>> {
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

    let mut blobs = Vec::new();
    let mut rest = records.as_slice();
    while !rest.is_empty() {
        anyhow::ensure!(blobs.len() < MAX_NOTES, "too many notes in transfer");
        anyhow::ensure!(rest.len() >= 4, "truncated transfer");
        let len = u32::from_le_bytes(rest[..4].try_into()?) as usize;
        rest = &rest[4..];
        anyhow::ensure!(len <= rest.len(), "truncated transfer");
        blobs.push(TransferBlob::decode(&rest[..len])?);
        rest = &rest[len..];
    }
    anyhow::ensure!(!blobs.is_empty(), "empty transfer");
    Ok(blobs)
}

pub async fn pack(
    state: &AppState,
    note_ids: &[String],
    passphrase: &str,
) -> anyhow::Result<Vec<u8>> {
    let mut blobs = Vec::with_capacity(note_ids.len());
    for id in note_ids {
        blobs.push(super::lan::build_blob(state, id).await?);
    }
    seal(&blobs, passphrase)
}

/// Opens the whole payload before writing anything, so a wrong passphrase or a
/// corrupt record leaves the database untouched. The imports themselves are one
/// note at a time: if one fails, the summary still counts the notes already in,
/// and importing the same stream again updates them instead of duplicating.
///
/// With `into_folder`, notes land under that folder, the sender's own folders
/// nested beneath it; without, they keep the sender's path from the root.
pub async fn import(
    state: &AppState,
    payload: &[u8],
    passphrase: &str,
    into_folder: Option<&str>,
) -> (OpticalImportSummary, anyhow::Result<()>) {
    let mut summary = OpticalImportSummary::default();
    let blobs = match open(payload, passphrase) {
        Ok(blobs) => blobs,
        Err(e) => return (summary, Err(e)),
    };
    let base = match into_folder {
        Some(id) => crate::folders::commands::path_of(state, id).await,
        None => Vec::new(),
    };
    for mut blob in blobs {
        if !base.is_empty() {
            blob.folder_path = base.iter().cloned().chain(blob.folder_path).collect();
        }
        match import_blob_detailed(state, &state.device_key, blob).await {
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
    use serde_json::json;

    const PASS: &str = "correct horse";

    fn blob(title: &str) -> TransferBlob {
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
            folder_path: vec!["Work".into()],
        }
    }

    fn sealed(titles: &[&str], pass: &str) -> Vec<u8> {
        let blobs: Vec<Vec<u8>> = titles.iter().map(|t| blob(t).encode().unwrap()).collect();
        seal(&blobs, pass).unwrap()
    }

    #[test]
    fn round_trips_several_notes_in_order() {
        let opened = open(&sealed(&["a", "b", "c"], PASS), PASS).unwrap();
        assert_eq!(opened, vec![blob("a"), blob("b"), blob("c")]);
    }

    #[test]
    fn the_stream_does_not_carry_the_note_in_the_clear() {
        let payload = sealed(&["secret-title"], PASS);
        let needle = b"secret-title";
        assert!(!payload.windows(needle.len()).any(|w| w == needle));
    }

    #[test]
    fn a_wrong_passphrase_opens_nothing() {
        let err = open(&sealed(&["a"], PASS), "wrong passphrase!").unwrap_err();
        assert_eq!(err.to_string(), "wrong passphrase");
    }

    #[test]
    fn a_tampered_byte_is_rejected() {
        let mut payload = sealed(&["a"], PASS);
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
        let blobs = vec![blob("a").encode().unwrap()];
        let err = seal(&blobs, "123456789").unwrap_err().to_string();
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
        assert_ne!(sealed(&["a"], PASS), sealed(&["a"], PASS));
    }

    #[test]
    fn a_truncated_record_is_rejected_even_under_the_right_key() {
        let mut deflate = DeflateEncoder::new(Vec::new(), Compression::default());
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
        use crate::db::init_pool;
        let device = |name: &'static str| async move {
            let pool = init_pool(":memory:").await.unwrap();
            let key = derive_key(name, &[0u8; 16]).unwrap();
            AppState::new(pool, key, name.into())
        };
        let sender = device("sender").await;
        let receiver = device("receiver").await;
        let id = crate::transfer::commands::import_blob(&sender, &sender.device_key, blob("hello"))
            .await
            .unwrap();

        let payload = pack(&sender, &[id], PASS).await.unwrap();
        let (none, wrong) = import(&receiver, &payload, "wrong passphrase!", None).await;
        assert!(wrong.is_err());
        assert_eq!(none, OpticalImportSummary::default());
        let (first, ok) = import(&receiver, &payload, PASS, None).await;
        ok.unwrap();
        assert_eq!(
            first,
            OpticalImportSummary {
                inserted: 1,
                updated: 0
            }
        );
        // The same stream caught twice updates the note instead of duplicating it.
        let (again, ok) = import(&receiver, &payload, PASS, None).await;
        ok.unwrap();
        assert_eq!(
            again,
            OpticalImportSummary {
                inserted: 0,
                updated: 1
            }
        );
    }

    #[tokio::test]
    async fn receiving_into_a_folder_nests_the_senders_folders_under_it() {
        use crate::db::init_pool;
        let pool = init_pool(":memory:").await.unwrap();
        let receiver = AppState::new(pool, derive_key("r", &[0u8; 16]).unwrap(), "r".into());
        let inbox = crate::folders::commands::create_impl(&receiver, "Inbox", None)
            .await
            .unwrap();
        // blob() files the note under "Work" on the sending side.
        let payload = seal(&[blob("filed").encode().unwrap()], PASS).unwrap();

        let (summary, ok) = import(&receiver, &payload, PASS, Some(&inbox)).await;
        ok.unwrap();
        assert_eq!(summary.inserted, 1);
        let folders = crate::folders::commands::list_impl(&receiver)
            .await
            .unwrap();
        let work = folders
            .iter()
            .find(|f| f.name == "Work")
            .expect("Work folder created");
        assert_eq!(work.parent_id.as_deref(), Some(inbox.as_str()));
        let row = crate::db::queries::note_find_by_origin(&receiver.db, "dev", "id-filed")
            .await
            .unwrap()
            .unwrap();
        let folder = crate::folders::queries::note_folder(&receiver.db, &row.id)
            .await
            .unwrap();
        assert_eq!(folder.as_deref(), Some(work.id.as_str()));
    }
}
