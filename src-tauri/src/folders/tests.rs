//! Folder invariants. The ones that matter most are destructive-adjacent:
//! deleting a folder must never take a note with it, and a cycle must be
//! impossible, because the tree is walked recursively when rendering.

use super::commands::{
    create_impl, list_impl, move_impl, rename_impl, CYCLE, NAME_EMPTY, TOO_DEEP,
};
use super::{queries, MAX_DEPTH};
use crate::crypto::vault::derive_key;
use crate::db::init_pool;
use crate::state::{now_secs, AppState};
use crate::transfer::blob::TransferBlob;
use serde_json::json;

async fn state() -> AppState {
    let pool = init_pool(":memory:").await.unwrap();
    let key = derive_key("folder-tests", &[0u8; 16]).unwrap();
    AppState::new(pool, key, "test-device".into())
}

async fn note_in(state: &AppState, folder: Option<&str>, title: &str) -> String {
    let blob = TransferBlob {
        id: title.into(),
        kind: "document".into(),
        title: title.into(),
        content: json!({ "body": "b" }),
        tags: vec![],
        created_at: 1,
        updated_at: 1,
        origin_device_id: String::new(),
        origin_note_id: String::new(),
        folder_path: Vec::new(),
    };
    let id = crate::transfer::commands::import_blob(state, &state.device_key, blob)
        .await
        .unwrap();
    queries::set_note_folder(&state.db, &id, folder, now_secs()).await.unwrap();
    id
}

async fn names(state: &AppState) -> Vec<String> {
    let mut n: Vec<String> = list_impl(state).await.unwrap().into_iter().map(|f| f.name).collect();
    n.sort();
    n
}

#[tokio::test]
async fn a_folder_can_be_created_and_listed() {
    let s = state().await;
    create_impl(&s, "Work", None).await.unwrap();
    assert_eq!(names(&s).await, vec!["Work"]);
}

#[tokio::test]
async fn the_name_is_encrypted_at_rest() {
    let s = state().await;
    let id = create_impl(&s, "Bank accounts", None).await.unwrap();
    let row = queries::get(&s.db, &id).await.unwrap().unwrap();
    assert!(
        !row.name_ct.contains("Bank"),
        "the folder name must not sit in the clear: {}",
        row.name_ct,
    );
    // and it still reads back
    assert_eq!(names(&s).await, vec!["Bank accounts"]);
}

#[tokio::test]
async fn names_are_trimmed_and_blank_ones_refused() {
    let s = state().await;
    create_impl(&s, "  Padded  ", None).await.unwrap();
    assert_eq!(names(&s).await, vec!["Padded"]);
    assert_eq!(create_impl(&s, "   ", None).await.unwrap_err(), NAME_EMPTY);
    assert_eq!(create_impl(&s, "", None).await.unwrap_err(), NAME_EMPTY);
}

#[tokio::test]
async fn renaming_changes_it_once_everywhere() {
    let s = state().await;
    let id = create_impl(&s, "Old", None).await.unwrap();
    rename_impl(&s, &id, "New").await.unwrap();
    assert_eq!(names(&s).await, vec!["New"]);
    assert_eq!(rename_impl(&s, &id, " ").await.unwrap_err(), NAME_EMPTY);
}

#[tokio::test]
async fn a_subfolder_records_its_parent() {
    let s = state().await;
    let parent = create_impl(&s, "Work", None).await.unwrap();
    let child = create_impl(&s, "Clients", Some(&parent)).await.unwrap();
    let listed = list_impl(&s).await.unwrap();
    let c = listed.iter().find(|f| f.id == child).unwrap();
    assert_eq!(c.parent_id.as_deref(), Some(parent.as_str()));
}

#[tokio::test]
async fn creating_under_a_missing_parent_fails() {
    let s = state().await;
    assert!(create_impl(&s, "Orphan", Some("nope")).await.is_err());
}

// ---- Cycles ----

#[tokio::test]
async fn a_folder_cannot_be_moved_into_itself() {
    let s = state().await;
    let a = create_impl(&s, "A", None).await.unwrap();
    assert_eq!(move_impl(&s, &a, Some(&a)).await.unwrap_err(), CYCLE);
}

#[tokio::test]
async fn a_folder_cannot_be_moved_into_its_own_descendant() {
    let s = state().await;
    let a = create_impl(&s, "A", None).await.unwrap();
    let b = create_impl(&s, "B", Some(&a)).await.unwrap();
    let c = create_impl(&s, "C", Some(&b)).await.unwrap();

    assert_eq!(move_impl(&s, &a, Some(&b)).await.unwrap_err(), CYCLE);
    assert_eq!(move_impl(&s, &a, Some(&c)).await.unwrap_err(), CYCLE);
    // and the tree is untouched
    let listed = list_impl(&s).await.unwrap();
    assert_eq!(listed.iter().find(|f| f.id == a).unwrap().parent_id, None);
}

#[tokio::test]
async fn a_legitimate_move_is_allowed() {
    let s = state().await;
    let a = create_impl(&s, "A", None).await.unwrap();
    let b = create_impl(&s, "B", None).await.unwrap();
    move_impl(&s, &b, Some(&a)).await.unwrap();
    let listed = list_impl(&s).await.unwrap();
    assert_eq!(listed.iter().find(|f| f.id == b).unwrap().parent_id.as_deref(), Some(a.as_str()));
}

#[tokio::test]
async fn moving_to_the_root_is_allowed() {
    let s = state().await;
    let a = create_impl(&s, "A", None).await.unwrap();
    let b = create_impl(&s, "B", Some(&a)).await.unwrap();
    move_impl(&s, &b, None).await.unwrap();
    let listed = list_impl(&s).await.unwrap();
    assert_eq!(listed.iter().find(|f| f.id == b).unwrap().parent_id, None);
}

// ---- Depth ----

#[tokio::test]
async fn nesting_stops_at_the_depth_cap() {
    let s = state().await;
    let mut parent = create_impl(&s, "d0", None).await.unwrap();
    for i in 1..MAX_DEPTH {
        parent = create_impl(&s, &format!("d{i}"), Some(&parent)).await.unwrap();
    }
    // MAX_DEPTH levels exist; one more must be refused.
    assert_eq!(create_impl(&s, "toodeep", Some(&parent)).await.unwrap_err(), TOO_DEEP);
}

#[tokio::test]
async fn a_move_that_would_exceed_the_cap_is_refused() {
    let s = state().await;
    // A chain one short of the cap...
    let mut deep = create_impl(&s, "d0", None).await.unwrap();
    for i in 1..(MAX_DEPTH - 1) {
        deep = create_impl(&s, &format!("d{i}"), Some(&deep)).await.unwrap();
    }
    // ...and a 3-deep subtree that will not fit under it.
    let x = create_impl(&s, "x", None).await.unwrap();
    let y = create_impl(&s, "y", Some(&x)).await.unwrap();
    create_impl(&s, "z", Some(&y)).await.unwrap();

    assert_eq!(move_impl(&s, &x, Some(&deep)).await.unwrap_err(), TOO_DEEP);
}

// ---- Deletion: the destructive path ----

#[tokio::test]
async fn deleting_a_folder_returns_its_notes_to_the_root_and_keeps_them() {
    let s = state().await;
    let f = create_impl(&s, "Work", None).await.unwrap();
    let note = note_in(&s, Some(&f), "Keep me").await;

    queries::delete(&s.db, &f).await.unwrap();

    let row = crate::db::queries::note_get(&s.db, &note).await.unwrap();
    assert!(row.is_some(), "deleting a folder must never delete a note");
    assert_eq!(
        queries::note_folder(&s.db, &note).await.unwrap(),
        None,
        "the note should fall back to the root",
    );
}

#[tokio::test]
async fn deleting_a_folder_removes_its_subfolders() {
    let s = state().await;
    let a = create_impl(&s, "A", None).await.unwrap();
    let b = create_impl(&s, "B", Some(&a)).await.unwrap();
    create_impl(&s, "C", Some(&b)).await.unwrap();

    queries::delete(&s.db, &a).await.unwrap();
    assert!(names(&s).await.is_empty(), "the whole subtree should be gone");
}

#[tokio::test]
async fn deleting_a_folder_keeps_notes_from_its_subfolders_too() {
    let s = state().await;
    let a = create_impl(&s, "A", None).await.unwrap();
    let b = create_impl(&s, "B", Some(&a)).await.unwrap();
    let note = note_in(&s, Some(&b), "Nested note").await;

    queries::delete(&s.db, &a).await.unwrap();

    assert!(
        crate::db::queries::note_get(&s.db, &note).await.unwrap().is_some(),
        "a note in a subfolder must survive its folder being deleted",
    );
    assert_eq!(queries::note_folder(&s.db, &note).await.unwrap(), None);
}

// ---- Counts and subtree collection ----

#[tokio::test]
async fn note_counts_are_per_folder() {
    let s = state().await;
    let a = create_impl(&s, "A", None).await.unwrap();
    let b = create_impl(&s, "B", None).await.unwrap();
    note_in(&s, Some(&a), "one").await;
    note_in(&s, Some(&a), "two").await;
    note_in(&s, Some(&b), "three").await;
    note_in(&s, None, "loose").await;

    let listed = list_impl(&s).await.unwrap();
    assert_eq!(listed.iter().find(|f| f.id == a).unwrap().note_count, 2);
    assert_eq!(listed.iter().find(|f| f.id == b).unwrap().note_count, 1);
}

#[tokio::test]
async fn a_folder_send_collects_the_whole_subtree() {
    let s = state().await;
    let a = create_impl(&s, "A", None).await.unwrap();
    let b = create_impl(&s, "B", Some(&a)).await.unwrap();
    let top = note_in(&s, Some(&a), "top").await;
    let nested = note_in(&s, Some(&b), "nested").await;
    let loose = note_in(&s, None, "loose").await;

    let ids = queries::note_ids_in_subtree(&s.db, &a).await.unwrap();
    assert!(ids.contains(&top));
    assert!(ids.contains(&nested), "subfolder notes must be included");
    assert!(!ids.contains(&loose), "notes outside the folder must not be");
    assert_eq!(ids.len(), 2);
}

#[tokio::test]
async fn moving_a_note_between_folders_works_and_can_clear_it() {
    let s = state().await;
    let a = create_impl(&s, "A", None).await.unwrap();
    let b = create_impl(&s, "B", None).await.unwrap();
    let note = note_in(&s, Some(&a), "n").await;

    queries::set_note_folder(&s.db, &note, Some(&b), now_secs()).await.unwrap();
    assert_eq!(queries::note_folder(&s.db, &note).await.unwrap().as_deref(), Some(b.as_str()));

    queries::set_note_folder(&s.db, &note, None, now_secs()).await.unwrap();
    assert_eq!(queries::note_folder(&s.db, &note).await.unwrap(), None);
}
