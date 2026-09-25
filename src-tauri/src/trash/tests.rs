//! Trash invariants. A trashed note must vanish from everything that lists,
//! counts or ships notes, and must come back intact, protection included.

use super::commands::list_impl;
use super::{queries, RETENTION_SECS};
use crate::crypto::vault::derive_key;
use crate::db::{init_pool, queries as notes};
use crate::folders::{commands as folders, queries as folder_queries};
use crate::state::{now_secs, AppState};
use crate::transfer::blob::TransferBlob;
use serde_json::json;

async fn state() -> AppState {
    let pool = init_pool(":memory:").await.unwrap();
    let key = derive_key("trash-tests", &[0u8; 16]).unwrap();
    AppState::new(pool, key, "test-device".into())
}

fn blob(title: &str, origin: &str) -> TransferBlob {
    TransferBlob {
        id: title.into(),
        kind: "document".into(),
        title: title.into(),
        content: json!({ "body": "b" }),
        tags: vec![],
        created_at: 1,
        updated_at: 1,
        origin_device_id: if origin.is_empty() { String::new() } else { "peer".into() },
        origin_note_id: origin.into(),
        folder_path: Vec::new(),
    }
}

async fn note_in(state: &AppState, folder: Option<&str>, title: &str) -> String {
    let id = crate::transfer::commands::import_blob(state, &state.device_key, blob(title, ""))
        .await
        .unwrap();
    folder_queries::set_note_folder(&state.db, &id, folder, now_secs()).await.unwrap();
    id
}

async fn trash(state: &AppState, id: &str) {
    queries::trash(&state.db, &[id.to_string()], now_secs()).await.unwrap();
}

#[tokio::test]
async fn deleting_a_note_keeps_the_row_and_lists_it_in_trash() {
    let s = state().await;
    let id = note_in(&s, None, "Groceries").await;
    trash(&s, &id).await;

    assert!(notes::note_get(&s.db, &id).await.unwrap().is_some(), "trash must not destroy the row");
    let listed = list_impl(&s).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
    assert_eq!(listed[0].title, "Groceries");
    assert!(listed[0].deleted_at > 0);
}

#[tokio::test]
async fn a_trashed_note_is_excluded_from_list_counts_and_export() {
    let s = state().await;
    let f = folders::create_impl(&s, "Work", None).await.unwrap();
    let gone = note_in(&s, Some(&f), "gone").await;
    let kept = note_in(&s, Some(&f), "kept").await;
    sqlx::query("UPDATE notes SET bg_image = 'data:image/png;base64,AA==' WHERE id = ?")
        .bind(&gone)
        .execute(&s.db)
        .await
        .unwrap();
    trash(&s, &gone).await;

    let page: Vec<_> = notes::note_list_page(&s.db, 500, 0).await.unwrap().into_iter().map(|r| r.id).collect();
    assert_eq!(page, vec![kept.clone()], "list");
    assert_eq!(notes::note_count(&s.db).await.unwrap(), 1, "count");
    // `notes_export` reads exactly this.
    let exported: Vec<_> = notes::note_list(&s.db).await.unwrap().into_iter().map(|r| r.id).collect();
    assert_eq!(exported, vec![kept.clone()], "export");
    assert!(notes::note_bg_images(&s.db).await.unwrap().is_empty(), "backgrounds");

    let counted = folders::list_impl(&s).await.unwrap();
    assert_eq!(counted[0].note_count, 1, "folder count");
    assert_eq!(folder_queries::note_ids_in_subtree(&s.db, &f).await.unwrap(), vec![kept], "folder send");
}

#[tokio::test]
async fn restore_puts_the_note_back_in_its_folder() {
    let s = state().await;
    let f = folders::create_impl(&s, "Work", None).await.unwrap();
    let id = note_in(&s, Some(&f), "n").await;
    trash(&s, &id).await;

    queries::restore(&s.db, std::slice::from_ref(&id)).await.unwrap();

    assert!(list_impl(&s).await.unwrap().is_empty());
    assert_eq!(notes::note_count(&s.db).await.unwrap(), 1);
    assert_eq!(folder_queries::note_folder(&s.db, &id).await.unwrap().as_deref(), Some(f.as_str()));
}

#[tokio::test]
async fn restore_falls_back_to_the_root_when_the_folder_is_gone() {
    let s = state().await;
    let f = folders::create_impl(&s, "Work", None).await.unwrap();
    let id = note_in(&s, Some(&f), "n").await;
    trash(&s, &id).await;
    folder_queries::delete(&s.db, &f).await.unwrap();

    queries::restore(&s.db, std::slice::from_ref(&id)).await.unwrap();

    assert_eq!(notes::note_count(&s.db).await.unwrap(), 1);
    assert_eq!(folder_queries::note_folder(&s.db, &id).await.unwrap(), None);
}

#[tokio::test]
async fn restore_and_delete_forever_take_many_ids() {
    let s = state().await;
    let a = note_in(&s, None, "a").await;
    let b = note_in(&s, None, "b").await;
    let c = note_in(&s, None, "c").await;
    let d = note_in(&s, None, "d").await;
    queries::trash(&s.db, &[a.clone(), b.clone(), c.clone(), d.clone()], now_secs()).await.unwrap();

    queries::restore(&s.db, &[a.clone(), b.clone()]).await.unwrap();
    queries::purge(&s.db, &[c.clone(), d.clone()]).await.unwrap();

    assert_eq!(notes::note_count(&s.db).await.unwrap(), 2);
    assert!(list_impl(&s).await.unwrap().is_empty());
    assert!(notes::note_get(&s.db, &c).await.unwrap().is_none());
    assert!(notes::note_get(&s.db, &d).await.unwrap().is_none());
}

#[tokio::test]
async fn delete_forever_refuses_a_live_note() {
    let s = state().await;
    let live = note_in(&s, None, "live").await;
    queries::purge(&s.db, std::slice::from_ref(&live)).await.unwrap();
    assert!(notes::note_get(&s.db, &live).await.unwrap().is_some());
}

#[tokio::test]
async fn empty_trash_removes_only_trashed_notes() {
    let s = state().await;
    let live = note_in(&s, None, "live").await;
    let a = note_in(&s, None, "a").await;
    let b = note_in(&s, None, "b").await;
    trash(&s, &a).await;
    trash(&s, &b).await;

    queries::empty(&s.db).await.unwrap();

    assert!(list_impl(&s).await.unwrap().is_empty());
    assert!(notes::note_get(&s.db, &a).await.unwrap().is_none());
    assert!(notes::note_get(&s.db, &live).await.unwrap().is_some());
}

#[tokio::test]
async fn startup_purge_drops_notes_trashed_over_30_days_ago() {
    let s = state().await;
    let now = now_secs();
    let old = note_in(&s, None, "old").await;
    let recent = note_in(&s, None, "recent").await;
    let live = note_in(&s, None, "live").await;
    queries::trash(&s.db, std::slice::from_ref(&old), now - RETENTION_SECS - 1).await.unwrap();
    queries::trash(&s.db, std::slice::from_ref(&recent), now - RETENTION_SECS + 60).await.unwrap();

    let purged = queries::purge_before(&s.db, now - RETENTION_SECS).await.unwrap();

    assert_eq!(purged, 1);
    assert!(notes::note_get(&s.db, &old).await.unwrap().is_none());
    assert_eq!(list_impl(&s).await.unwrap()[0].id, recent);
    assert!(notes::note_get(&s.db, &live).await.unwrap().is_some());
}

#[tokio::test]
async fn a_protected_note_stays_protected_through_trash_and_restore() {
    let s = state().await;
    let id = note_in(&s, None, "Vault").await;
    crate::notes::commands::protect_impl(&s, &id, "hunter22").await.unwrap();
    let before = notes::note_get(&s.db, &id).await.unwrap().unwrap();

    trash(&s, &id).await;
    let listed = list_impl(&s).await.unwrap();
    assert!(listed[0].has_note_password);

    queries::restore(&s.db, std::slice::from_ref(&id)).await.unwrap();
    let after = notes::note_get(&s.db, &id).await.unwrap().unwrap();
    assert_eq!(after.note_salt, before.note_salt);
    assert_eq!(after.content_ct, before.content_ct, "ciphertext must be untouched");
}

#[tokio::test]
async fn a_re_sent_note_that_sits_in_trash_comes_back() {
    let s = state().await;
    let id = crate::transfer::commands::import_blob(&s, &s.device_key, blob("shared", "o1"))
        .await
        .unwrap();
    trash(&s, &id).await;

    let again = crate::transfer::commands::import_blob(&s, &s.device_key, blob("shared", "o1"))
        .await
        .unwrap();

    assert_eq!(again, id, "same origin updates the existing row");
    assert!(list_impl(&s).await.unwrap().is_empty());
    assert_eq!(notes::note_count(&s.db).await.unwrap(), 1);
}
