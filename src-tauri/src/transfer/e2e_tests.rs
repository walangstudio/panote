//! Two real devices, one real socket.
//!
//! Everything below the UI runs for real here: TCP, TLS 1.3 with TOFU, the
//! SPAKE2 exchange, framing, chunking and the blob crypto. The two ends have
//! separate databases and separate device keys, exactly as two machines would.
//!
//! This is the only place the transfer protocol is exercised end to end - every
//! other transfer test stops at a function boundary, so a break in the wiring
//! between them showed up on a phone rather than in CI.

use super::lan::{send_note, send_notes, serve, TransferEvents};
use crate::crypto::{note::decrypt_with_vault, vault::derive_key};
use crate::db::{init_pool, queries};
use crate::state::AppState;
use crate::transfer::blob::TransferBlob;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;

const CODE: &str = "K4X7P2";

async fn device(name: &str) -> Arc<AppState> {
    let pool = init_pool(":memory:").await.unwrap();
    let key = derive_key(name, &[0u8; 16]).unwrap();
    Arc::new(AppState::new(pool, key, name.into()))
}

async fn seed(state: &AppState, title: &str, body: &str, tags: &[&str]) -> String {
    let blob = TransferBlob {
        id: format!("src-{title}"),
        kind: "document".into(),
        title: title.into(),
        content: json!({ "body": body }),
        tags: tags.iter().map(|t| t.to_string()).collect(),
        created_at: 1_700_000_000,
        updated_at: 1_700_000_001,
        origin_device_id: String::new(),
        origin_note_id: String::new(),
        folder_path: Vec::new(),
    };
    crate::transfer::commands::import_blob(state, &state.device_key, blob)
        .await
        .unwrap()
}

/// The UI notifications, dropped on the floor. The test observes the same state
/// the UI renders from (`pending_offers`, the database) rather than the events.
struct Silent;
impl TransferEvents for Silent {
    fn offer_received(&self, _: &crate::state::PendingOffer) {}
    fn transfer_received(&self, _: &str) {}
    fn transfer_rejected(&self, _: &str) {}
    fn notes_received(&self, _: &str, _: u32, _: u32) {}
}

/// Bind an ephemeral port and serve on it, so these can run in parallel and
/// never collide with a real app holding TRANSFER_PORT.
async fn listen(state: Arc<AppState>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let _ = serve(listener, state, Arc::new(Silent)).await;
    });
    port
}

/// Stands in for the recipient typing the code the sender read out, polling for
/// the offer the way the pending-offer list does.
fn answer_with(state: Arc<AppState>, code: &str) -> tokio::task::JoinHandle<bool> {
    answer_into(state, code, None)
}

/// As `answer_with`, filing what arrives under `folder`.
fn answer_into(
    state: Arc<AppState>,
    code: &str,
    folder: Option<String>,
) -> tokio::task::JoinHandle<bool> {
    let code = code.to_string();
    tokio::spawn(async move {
        for _ in 0..400 {
            let entry = {
                let mut responses = state.offer_responses.lock().unwrap();
                let id = responses.keys().next().cloned();
                id.and_then(|id| responses.remove(&id))
            };
            if let Some(tx) = entry {
                return tx
                    .send(crate::state::OfferAnswer {
                        code,
                        into_folder: folder,
                    })
                    .is_ok();
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        false
    })
}

async fn received(state: &AppState) -> Vec<queries::NoteRow> {
    queries::note_list_page(&state.db, 100, 0).await.unwrap()
}

/// `send_note` parks the note as a pending transfer and the recipient types the
/// passphrase to open it. This is what `note_receive_accept` does, minus the
/// Tauri `State` wrapper a test cannot construct.
async fn accept_pending(state: &AppState, passphrase: &str) -> Result<String, String> {
    let pending = state.list_pending();
    let t = pending.first().ok_or("no pending transfer arrived")?;
    let blob = super::lan::decrypt_transfer(
        &t.transfer_salt,
        &t.transfer_nonce,
        &t.transfer_ct,
        passphrase,
    )
    .map_err(|_| "wrong passphrase".to_string())?;
    state.take_pending(&t.transfer_id);
    crate::transfer::commands::import_blob(state, &state.device_key, blob)
        .await
        .map_err(|e| e.to_string())
}

/// Wait for the inbound connection to have parked its pending transfer - the
/// sender's Ack returns before the receiver has finished storing it.
async fn await_pending(state: &AppState) {
    for _ in 0..400 {
        if !state.list_pending().is_empty() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Read a landed note back the way the app does: decrypt with the receiver's
/// own device key, AAD-bound to the new note id.
///
/// Fetched with `note_get`, not from the list rows - the list query drops
/// `content_ct` and `nonce` on purpose (they are dead weight on every card), so
/// a list row cannot be decrypted.
async fn open(state: &AppState, id: &str) -> (String, String, Vec<String>) {
    let row = queries::note_get(&state.db, id).await.unwrap().unwrap();
    let title = String::from_utf8(
        decrypt_with_vault(
            &state.device_key,
            &row.title_nonce,
            &row.title_ct,
            row.id.as_bytes(),
        )
        .expect("receiver must be able to decrypt the title it stored"),
    )
    .unwrap();
    let content_bytes = decrypt_with_vault(
        &state.device_key,
        &row.nonce,
        &row.content_ct,
        row.id.as_bytes(),
    )
    .expect("receiver must be able to decrypt the content it stored");
    let content: serde_json::Value = serde_json::from_slice(&content_bytes).unwrap();
    // Tags are encrypted alongside the rest, not stored as readable JSON.
    let tags = crate::notes::commands::decrypt_tags(&state.device_key, &row.id, &row.tags)
        .expect("receiver must be able to decrypt the tags it stored");
    (
        title,
        content["body"].as_str().unwrap_or_default().to_string(),
        tags,
    )
}

/// Full rows for everything the receiver holds.
async fn opened_all(state: &AppState) -> Vec<(String, String, Vec<String>)> {
    let mut out = Vec::new();
    for row in received(state).await {
        out.push(open(state, &row.id).await);
    }
    out
}

/// Manual harness for a real second device - not run by CI.
///
/// Point it at a listening peer and it performs a genuine cross-device send:
///
/// ```text
/// adb forward tcp:47391 tcp:47291
/// PANOTE_PEER=127.0.0.1:47391 cargo test --lib send_to_a_real_device -- --ignored --nocapture
/// ```
///
/// Used to verify Windows -> Android against the emulator, whose NAT means the
/// host can only reach it through a forwarded port.
#[tokio::test]
#[ignore = "needs a real device listening; set PANOTE_PEER"]
async fn send_to_a_real_device() {
    let peer = std::env::var("PANOTE_PEER").expect("set PANOTE_PEER=host:port");
    let (host, port) = crate::transfer::commands::split_host_port(&peer).unwrap();

    let alice = device("harness").await;

    let probed = super::lan::hello_probe(&alice, &host, port, "WindowsHarness")
        .await
        .expect("TLS handshake with the real device should succeed");
    println!(
        "handshake ok: {} at {}:{}",
        probed.name, probed.address, probed.port
    );

    // Both cases in one run: a note in no folder must keep working exactly as
    // before, and a nested one should rebuild its path on the far device.
    let loose = seed(
        &alice,
        "No folder",
        "should land at the root",
        &["cross-device"],
    )
    .await;
    send_note(&alice, &loose, &host, port, CODE, "WindowsHarness")
        .await
        .expect("a note with no folder should send");
    println!("sent 'No folder' (root)");

    let work = crate::folders::commands::create_impl(&alice, "Work", None)
        .await
        .unwrap();
    let clients = crate::folders::commands::create_impl(&alice, "Clients", Some(&work))
        .await
        .unwrap();
    let filed = seed(
        &alice,
        "In a folder",
        "should land in Work/Clients",
        &["cross-device"],
    )
    .await;
    crate::folders::queries::set_note_folder(&alice.db, &filed, Some(&clients), 1)
        .await
        .unwrap();
    send_note(&alice, &filed, &host, port, CODE, "WindowsHarness")
        .await
        .expect("a note in a folder should send");
    println!("sent 'In a folder' (Work/Clients)");

    println!("enter {CODE} on the device twice to open both");
}

// ---- Single-note send: passphrase-wrapped, parked until the recipient opens it ----

#[tokio::test]
async fn a_note_sent_from_one_device_arrives_on_the_other() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let note_id = seed(&alice, "Shopping", "milk and eggs", &["errands"]).await;

    let port = listen(bob.clone()).await;
    send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice")
        .await
        .expect("the send should be acknowledged");
    await_pending(&bob).await;

    // It waits for the recipient rather than landing unannounced.
    assert_eq!(bob.list_pending().len(), 1, "a transfer should be waiting");
    assert!(
        received(&bob).await.is_empty(),
        "nothing lands before the code is entered"
    );

    accept_pending(&bob, CODE)
        .await
        .expect("the right code should open it");

    let rows = received(&bob).await;
    assert_eq!(rows.len(), 1);
    let (title, body, tags) = open(&bob, &rows[0].id).await;
    assert_eq!(title, "Shopping");
    assert_eq!(body, "milk and eggs");
    assert_eq!(tags, vec!["errands"]);
}

/// The point of re-encrypting on arrival: the receiver holds it under its own
/// key, so the sender's key is never needed to read it and never travels.
#[tokio::test]
async fn the_note_is_stored_under_the_receivers_own_key() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    assert_ne!(
        alice.device_key, bob.device_key,
        "the two devices must differ"
    );
    let note_id = seed(&alice, "Recipe", "sourdough starter", &[]).await;

    let port = listen(bob.clone()).await;
    send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap();
    await_pending(&bob).await;
    accept_pending(&bob, CODE).await.unwrap();

    let listed = received(&bob).await;
    let row = queries::note_get(&bob.db, &listed[0].id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(open(&bob, &row.id).await.1, "sourdough starter");
    assert!(
        decrypt_with_vault(
            &alice.device_key,
            &row.nonce,
            &row.content_ct,
            row.id.as_bytes()
        )
        .is_err(),
        "the sender's key must not open the receiver's copy",
    );
}

#[tokio::test]
async fn the_wrong_code_opens_nothing_and_allows_a_retry() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let note_id = seed(&alice, "Secret", "do not leak", &[]).await;

    let port = listen(bob.clone()).await;
    send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap();
    await_pending(&bob).await;

    assert!(
        accept_pending(&bob, "WRONGC").await.is_err(),
        "a wrong code must not decrypt"
    );
    assert!(
        received(&bob).await.is_empty(),
        "nothing may land on a wrong code"
    );
    assert_eq!(
        bob.list_pending().len(),
        1,
        "the transfer stays pending so it can be retried"
    );

    accept_pending(&bob, CODE)
        .await
        .expect("the correct code should still work");
    let rows = received(&bob).await;
    assert_eq!(open(&bob, &rows[0].id).await.1, "do not leak");
}

#[tokio::test]
async fn unicode_and_tags_survive_the_wire() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let title = "Cafe - naive \u{65e5}\u{672c}\u{8a9e} \u{1F389}";
    let body = "line one\nline two\ttabbed \u{2014} em dash \u{1F389}";
    let note_id = seed(&alice, title, body, &["a", "b", "c"]).await;

    let port = listen(bob.clone()).await;
    send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap();
    await_pending(&bob).await;
    accept_pending(&bob, CODE).await.unwrap();

    let rows = received(&bob).await;
    let (got_title, got_body, got_tags) = open(&bob, &rows[0].id).await;
    assert_eq!(got_title, title);
    assert_eq!(got_body, body);
    assert_eq!(got_tags, vec!["a", "b", "c"]);
}

/// Well past a single frame, so chunking and reassembly are genuinely exercised.
#[tokio::test]
async fn a_note_larger_than_one_frame_arrives_whole() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let big = "x".repeat(400_000);
    let note_id = seed(&alice, "Big", &big, &[]).await;

    let port = listen(bob.clone()).await;
    send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap();
    await_pending(&bob).await;
    accept_pending(&bob, CODE).await.unwrap();

    let rows = received(&bob).await;
    let (_, body, _) = open(&bob, &rows[0].id).await;
    assert_eq!(
        body.len(),
        big.len(),
        "large body must survive chunking intact"
    );
    assert_eq!(body, big);
}

#[tokio::test]
async fn an_unreachable_peer_fails_instead_of_hanging() {
    let alice = device("alice").await;
    let note_id = seed(&alice, "Nowhere", "body", &[]).await;
    // Nothing serves port 1.
    let result = send_note(&alice, &note_id, "127.0.0.1", 1, CODE, "Alice").await;
    assert!(result.is_err(), "an unreachable peer must surface an error");
}

/// Both directions across one pair of devices. The reply is a fresh connection
/// in the opposite direction, not a reuse of the inbound one.
#[tokio::test]
async fn devices_can_send_both_ways() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let from_alice = seed(&alice, "ToBob", "hello bob", &[]).await;
    let from_bob = seed(&bob, "ToAlice", "hello alice", &[]).await;

    let alice_port = listen(alice.clone()).await;
    let bob_port = listen(bob.clone()).await;

    send_note(&alice, &from_alice, "127.0.0.1", bob_port, CODE, "Alice")
        .await
        .unwrap();
    await_pending(&bob).await;
    accept_pending(&bob, CODE).await.unwrap();

    send_note(&bob, &from_bob, "127.0.0.1", alice_port, CODE, "Bob")
        .await
        .unwrap();
    await_pending(&alice).await;
    accept_pending(&alice, CODE).await.unwrap();

    let on_bob: Vec<String> = opened_all(&bob).await.into_iter().map(|t| t.0).collect();
    let on_alice: Vec<String> = opened_all(&alice).await.into_iter().map(|t| t.0).collect();
    assert!(
        on_bob.contains(&"ToBob".to_string()),
        "bob should hold alice's note"
    );
    assert!(
        on_alice.contains(&"ToAlice".to_string()),
        "alice should hold bob's note"
    );
}

// ---- Folders across the wire ----
//
// A note not in a folder is the ordinary case and must keep working exactly as
// before; a note in one should arrive filed the same way on the far device.

async fn folder(state: &AppState, name: &str, parent: Option<&str>) -> String {
    crate::folders::commands::create_impl(state, name, parent)
        .await
        .unwrap()
}

async fn folder_of(state: &AppState, note_id: &str) -> Option<String> {
    crate::folders::queries::note_folder(&state.db, note_id)
        .await
        .unwrap()
}

async fn folder_named(state: &AppState, name: &str) -> Option<String> {
    crate::folders::commands::list_impl(state)
        .await
        .unwrap()
        .into_iter()
        .find(|f| f.name == name)
        .map(|f| f.id)
}

#[tokio::test]
async fn a_note_with_no_folder_still_transfers_and_lands_at_the_root() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let note_id = seed(&alice, "Loose", "not in any folder", &[]).await;
    assert_eq!(folder_of(&alice, &note_id).await, None);

    let port = listen(bob.clone()).await;
    send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap();
    await_pending(&bob).await;
    let new_id = accept_pending(&bob, CODE).await.unwrap();

    assert_eq!(open(&bob, &new_id).await.1, "not in any folder");
    assert_eq!(
        folder_of(&bob, &new_id).await,
        None,
        "it should sit at the root"
    );
    assert!(
        crate::folders::commands::list_impl(&bob)
            .await
            .unwrap()
            .is_empty(),
        "no folder should be invented for a note that had none",
    );
}

#[tokio::test]
async fn a_note_in_a_folder_arrives_in_that_folder() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let f = folder(&alice, "Work", None).await;
    let note_id = seed(&alice, "Report", "quarterly numbers", &[]).await;
    crate::folders::queries::set_note_folder(&alice.db, &note_id, Some(&f), 1)
        .await
        .unwrap();

    let port = listen(bob.clone()).await;
    send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap();
    await_pending(&bob).await;
    let new_id = accept_pending(&bob, CODE).await.unwrap();

    let landed = folder_named(&bob, "Work")
        .await
        .expect("Work should have been created");
    assert_eq!(
        folder_of(&bob, &new_id).await.as_deref(),
        Some(landed.as_str())
    );
}

#[tokio::test]
async fn nesting_is_recreated_on_the_receiving_device() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let work = folder(&alice, "Work", None).await;
    let clients = folder(&alice, "Clients", Some(&work)).await;
    let note_id = seed(&alice, "Acme", "contract", &[]).await;
    crate::folders::queries::set_note_folder(&alice.db, &note_id, Some(&clients), 1)
        .await
        .unwrap();

    let port = listen(bob.clone()).await;
    send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap();
    await_pending(&bob).await;
    let new_id = accept_pending(&bob, CODE).await.unwrap();

    let listed = crate::folders::commands::list_impl(&bob).await.unwrap();
    let w = listed
        .iter()
        .find(|f| f.name == "Work")
        .expect("Work missing");
    let c = listed
        .iter()
        .find(|f| f.name == "Clients")
        .expect("Clients missing");
    assert_eq!(
        c.parent_id.as_deref(),
        Some(w.id.as_str()),
        "Clients must sit under Work"
    );
    assert_eq!(w.parent_id, None);
    assert_eq!(
        folder_of(&bob, &new_id).await.as_deref(),
        Some(c.id.as_str())
    );
}

/// Two notes from the same folder must share one folder on arrival, not make a
/// second copy of it.
#[tokio::test]
async fn a_second_note_files_into_the_folder_already_there() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let f = folder(&alice, "Work", None).await;
    let port = listen(bob.clone()).await;

    for title in ["One", "Two"] {
        let id = seed(&alice, title, "body", &[]).await;
        crate::folders::queries::set_note_folder(&alice.db, &id, Some(&f), 1)
            .await
            .unwrap();
        send_note(&alice, &id, "127.0.0.1", port, CODE, "Alice")
            .await
            .unwrap();
        await_pending(&bob).await;
        accept_pending(&bob, CODE).await.unwrap();
    }

    let listed = crate::folders::commands::list_impl(&bob).await.unwrap();
    assert_eq!(listed.len(), 1, "the folder must not be duplicated");
    assert_eq!(listed[0].note_count, 2);
}

/// An older sender emits no folder_path at all. That must import, not fail.
#[tokio::test]
async fn a_blob_from_an_older_sender_without_a_folder_path_still_imports() {
    let bob = device("bob").await;
    let legacy = serde_json::json!({
        "id": "old-1",
        "kind": "document",
        "title": "From an old build",
        "content": { "body": "still works" },
        "tags": ["legacy"],
        "created_at": 1_700_000_000,
        "updated_at": 1_700_000_001,
    });
    let blob = TransferBlob::decode(&serde_json::to_vec(&legacy).unwrap())
        .expect("a blob with no folder_path must still decode");
    assert!(blob.folder_path.is_empty());

    let id = crate::transfer::commands::import_blob(&bob, &bob.device_key, blob)
        .await
        .unwrap();
    assert_eq!(open(&bob, &id).await.1, "still works");
    assert_eq!(folder_of(&bob, &id).await, None);
}

/// Re-sending a note the recipient has already filed somewhere of their own must
/// not drag it back to the sender's folder.
#[tokio::test]
async fn a_resend_does_not_move_a_note_the_recipient_refiled() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let note_id = seed(&alice, "Shared", "body", &[]).await;
    let port = listen(bob.clone()).await;

    send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap();
    await_pending(&bob).await;
    let new_id = accept_pending(&bob, CODE).await.unwrap();

    let mine = folder(&bob, "Mine", None).await;
    crate::folders::queries::set_note_folder(&bob.db, &new_id, Some(&mine), 1)
        .await
        .unwrap();

    send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap();
    await_pending(&bob).await;
    accept_pending(&bob, CODE).await.unwrap();

    assert_eq!(
        folder_of(&bob, &new_id).await.as_deref(),
        Some(mine.as_str()),
        "the recipient's filing wins on a re-send",
    );
}

// ---- Password-protected notes ----
//
// The password layer is peeled off before the blob is built, so protection does
// NOT travel: the note arrives as an ordinary note and the recipient decides
// whether to protect it again. Deliberate ("Model B"), and worth pinning,
// because the alternative reading - that a protected note stays protected on the
// far side - is what a user would assume.

async fn protect(state: &AppState, id: &str, password: &str) {
    crate::notes::commands::protect_impl(state, id, password)
        .await
        .expect("protecting should succeed");
}

#[tokio::test]
async fn a_locked_note_refuses_to_send() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let note_id = seed(&alice, "Bank", "account 12345", &[]).await;
    protect(&alice, &note_id, "s3cret").await;
    // Setting a password leaves the note open for the session, so lock it -
    // this is the state after a restart, or after the inactivity timeout.
    alice.lock_note(&note_id);

    let port = listen(bob.clone()).await;
    let result = send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice").await;

    assert!(result.is_err(), "a locked note must not be sent");
    assert!(
        result.unwrap_err().to_lowercase().contains("unlock"),
        "the error should say the note needs unlocking",
    );
    assert!(bob.list_pending().is_empty(), "nothing may reach the peer");
}

#[tokio::test]
async fn an_unlocked_protected_note_sends_its_real_content() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let note_id = seed(&alice, "Bank", "account 12345", &["finance"]).await;
    protect(&alice, &note_id, "s3cret").await;
    alice.unlock_note(&note_id, "s3cret", "Bank");

    let port = listen(bob.clone()).await;
    send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice")
        .await
        .expect("an unlocked note should send");
    await_pending(&bob).await;
    accept_pending(&bob, CODE).await.unwrap();

    let rows = received(&bob).await;
    let (title, body, tags) = open(&bob, &rows[0].id).await;
    assert_eq!(title, "Bank");
    assert_eq!(
        body, "account 12345",
        "the password layer must be peeled, not shipped"
    );
    assert_eq!(tags, vec!["finance"]);
}

#[tokio::test]
async fn a_protected_note_arrives_unprotected() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let note_id = seed(&alice, "Bank", "account 12345", &[]).await;
    protect(&alice, &note_id, "s3cret").await;
    alice.unlock_note(&note_id, "s3cret", "Bank");

    let port = listen(bob.clone()).await;
    send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap();
    await_pending(&bob).await;
    let new_id = accept_pending(&bob, CODE).await.unwrap();

    let row = queries::note_get(&bob.db, &new_id).await.unwrap().unwrap();
    assert!(
        row.note_salt.is_none(),
        "protection does not travel with the note"
    );
    // The sender's sealed title travels in the clear blob, like the body.
    assert_eq!(open(&bob, &new_id).await.0, "Bank");
    // And the sender's copy keeps its protection.
    let src = queries::note_get(&alice.db, &note_id)
        .await
        .unwrap()
        .unwrap();
    assert!(
        src.note_salt.is_some(),
        "the sender's copy must stay protected"
    );
}

/// The receiving device can protect what arrived, with its own password - the
/// sender's password is neither needed nor transmitted.
#[tokio::test]
async fn the_receiver_can_protect_what_arrived_with_a_different_password() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let note_id = seed(&alice, "Bank", "account 12345", &[]).await;
    protect(&alice, &note_id, "alice-password").await;
    alice.unlock_note(&note_id, "alice-password", "Bank");

    let port = listen(bob.clone()).await;
    send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap();
    await_pending(&bob).await;
    let new_id = accept_pending(&bob, CODE).await.unwrap();

    protect(&bob, &new_id, "bob-password").await;
    let row = queries::note_get(&bob.db, &new_id).await.unwrap().unwrap();
    assert!(
        row.note_salt.is_some(),
        "the receiver's own protection should apply"
    );
}

// ---- Credential tables ----
//
// A masked column is `type: "masked"` inside the table content JSON, and content
// crosses the wire verbatim. If that were dropped, an imported password column
// would render in the clear on the far device.

#[tokio::test]
async fn a_credential_table_keeps_its_masked_columns() {
    let alice = device("alice").await;
    let bob = device("bob").await;

    let table = json!({
        "columns": [
            { "id": "c-site", "name": "Site" },
            { "id": "c-user", "name": "Username" },
            { "id": "c-pw", "name": "Password", "type": "masked" }
        ],
        "rows": [
            { "id": "r1", "cells": { "c-site": "github.com", "c-user": "me", "c-pw": "hunter2" } }
        ]
    });

    let blob = TransferBlob {
        id: "src-creds".into(),
        kind: "table".into(),
        title: "Credentials".into(),
        content: table.clone(),
        tags: vec!["secrets".into()],
        created_at: 1_700_000_000,
        updated_at: 1_700_000_001,
        origin_device_id: String::new(),
        origin_note_id: String::new(),
        folder_path: Vec::new(),
    };
    let note_id = crate::transfer::commands::import_blob(&alice, &alice.device_key, blob)
        .await
        .unwrap();

    let port = listen(bob.clone()).await;
    send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap();
    await_pending(&bob).await;
    let new_id = accept_pending(&bob, CODE).await.unwrap();

    let row = queries::note_get(&bob.db, &new_id).await.unwrap().unwrap();
    let bytes = decrypt_with_vault(
        &bob.device_key,
        &row.nonce,
        &row.content_ct,
        row.id.as_bytes(),
    )
    .unwrap();
    let got: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(row.kind, "table", "it must still be a table");
    assert_eq!(
        got, table,
        "the whole table, masking included, must round-trip"
    );
    assert_eq!(
        got["columns"][2]["type"], "masked",
        "the password column must arrive still masked, not in the clear",
    );
    assert_eq!(
        got["rows"][0]["cells"]["c-pw"], "hunter2",
        "and its value must survive"
    );
}

#[tokio::test]
async fn a_protected_credential_table_survives_the_whole_round_trip() {
    let alice = device("alice").await;
    let bob = device("bob").await;

    let table = json!({
        "columns": [
            { "id": "c-k", "name": "Key" },
            { "id": "c-v", "name": "Secret", "type": "masked" }
        ],
        "rows": [
            { "id": "r1", "cells": { "c-k": "API_TOKEN", "c-v": "sk-live-abc123" } },
            { "id": "r2", "cells": { "c-k": "DB_PASSWORD", "c-v": "p@ssw0rd" } }
        ]
    });

    let blob = TransferBlob {
        id: "src-env".into(),
        kind: "table".into(),
        title: "Production env".into(),
        content: table.clone(),
        tags: vec![],
        created_at: 1_700_000_000,
        updated_at: 1_700_000_001,
        origin_device_id: String::new(),
        origin_note_id: String::new(),
        folder_path: Vec::new(),
    };
    let note_id = crate::transfer::commands::import_blob(&alice, &alice.device_key, blob)
        .await
        .unwrap();
    protect(&alice, &note_id, "vault-password").await;
    alice.unlock_note(&note_id, "vault-password", "Production env");

    let port = listen(bob.clone()).await;
    send_note(&alice, &note_id, "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap();
    await_pending(&bob).await;
    let new_id = accept_pending(&bob, CODE).await.unwrap();

    let row = queries::note_get(&bob.db, &new_id).await.unwrap().unwrap();
    let bytes = decrypt_with_vault(
        &bob.device_key,
        &row.nonce,
        &row.content_ct,
        row.id.as_bytes(),
    )
    .unwrap();
    let got: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(got, table, "both secret rows must arrive intact");
    assert!(
        row.note_salt.is_none(),
        "and, as ever, unprotected on arrival"
    );
}

// ---- Batch send: SPAKE2 offer, code confirmed, notes land directly ----

#[tokio::test]
async fn several_notes_go_in_one_offer() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let ids = vec![
        seed(&alice, "One", "first", &[]).await,
        seed(&alice, "Two", "second", &[]).await,
        seed(&alice, "Three", "third", &[]).await,
    ];

    let port = listen(bob.clone()).await;
    let responder = answer_with(bob.clone(), CODE);
    send_notes(&alice, &ids, None, "127.0.0.1", port, CODE, "Alice")
        .await
        .expect("the batch transfer should succeed");
    assert!(responder.await.unwrap(), "recipient never saw the offer");

    let mut titles: Vec<String> = opened_all(&bob).await.into_iter().map(|t| t.0).collect();
    titles.sort();
    assert_eq!(titles, vec!["One", "Three", "Two"]);
}

/// SPAKE2 means a mismatched code fails key confirmation, so the notes are
/// never transmitted at all - not decrypted-and-discarded at the far end.
#[tokio::test]
async fn a_mismatched_code_aborts_the_batch_before_any_note_moves() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let ids = vec![seed(&alice, "Private", "do not send", &[]).await];

    let port = listen(bob.clone()).await;
    let responder = answer_with(bob.clone(), "WRONGC");

    let result = send_notes(&alice, &ids, None, "127.0.0.1", port, CODE, "Alice").await;
    responder.await.unwrap();

    assert!(
        result.is_err(),
        "a mismatched pairing code must fail the transfer"
    );
    assert!(
        received(&bob).await.is_empty(),
        "nothing may land on a failed pairing"
    );
}

/// A re-send onto a note the receiver protected re-seals the title with the
/// body, so the new title never lands under the receiver's device key alone.
#[tokio::test]
async fn a_resend_onto_a_protected_note_keeps_its_title_sealed() {
    use crate::notes::commands::{list_impl, open_row, update_impl, LOCKED_TITLE};
    let alice = device("alice").await;
    let bob = device("bob").await;
    let ids = vec![seed(&alice, "Bank", "account 12345", &[]).await];
    let port = listen(bob.clone()).await;

    let responder = answer_with(bob.clone(), CODE);
    send_notes(&alice, &ids, None, "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap();
    responder.await.unwrap();
    let bob_id = received(&bob).await[0].id.clone();
    protect(&bob, &bob_id, "bob-password").await;

    let renamed = crate::notes::types::NoteInput {
        kind: "document".into(),
        title: "Bank, renamed".into(),
        content: json!({ "body": "account 67890" }),
        tags: vec![],
        content_hint: None,
        pinned: None,
        bg_color: None,
        bg_image: None,
        show_preview: None,
        folder_id: None,
    };
    update_impl(&alice, ids[0].clone(), renamed).await.unwrap();
    let responder = answer_with(bob.clone(), CODE);
    send_notes(&alice, &ids, None, "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap();
    responder.await.unwrap();

    let row = queries::note_get(&bob.db, &bob_id).await.unwrap().unwrap();
    assert!(row.note_salt.is_some() && row.title_note_nonce.is_some());
    assert!(
        decrypt_with_vault(
            &bob.device_key,
            &row.title_nonce,
            &row.title_ct,
            row.id.as_bytes()
        )
        .is_err(),
        "the re-sent title must not be readable with the device key alone"
    );
    assert_eq!(
        list_impl(&bob, None, None).await.unwrap()[0].title,
        "Bank, renamed"
    );
    bob.lock_note(&bob_id);
    assert_eq!(
        list_impl(&bob, None, None).await.unwrap()[0].title,
        LOCKED_TITLE
    );
    bob.unlock_note(&bob_id, "bob-password", "");
    let (title, body) = open_row(&bob, &row).unwrap();
    assert_eq!(title, "Bank, renamed");
    assert!(String::from_utf8(body).unwrap().contains("account 67890"));
}

/// Re-sending after an edit is ordinary. It must update the copy already there
/// rather than pile up duplicates.
#[tokio::test]
async fn resending_the_same_note_does_not_duplicate_it() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let ids = vec![seed(&alice, "Once", "body", &[]).await];
    let port = listen(bob.clone()).await;

    for _ in 0..2 {
        let responder = answer_with(bob.clone(), CODE);
        send_notes(&alice, &ids, None, "127.0.0.1", port, CODE, "Alice")
            .await
            .unwrap();
        responder.await.unwrap();
    }

    assert_eq!(
        received(&bob).await.len(),
        1,
        "re-receiving the same origin note should update, not duplicate",
    );
}

/// The network follows the camera's rule: notes travel without their folders,
/// a sent folder arrives whole, and both land under the receiving folder.
#[tokio::test]
async fn a_batch_files_into_the_folder_the_receiver_chose() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let projects = folder(&alice, "Projects", None).await;
    let work = folder(&alice, "Work", Some(&projects)).await;
    let note_id = seed(&alice, "Filed", "in Projects/Work", &[]).await;
    crate::folders::queries::set_note_folder(&alice.db, &note_id, Some(&work), 1)
        .await
        .unwrap();
    let inbox = folder(&bob, "Inbox", None).await;
    let port = listen(bob.clone()).await;

    // A single note: no folders travel, it lands straight in Inbox.
    let responder = answer_into(bob.clone(), CODE, Some(inbox.clone()));
    send_notes(
        &alice,
        std::slice::from_ref(&note_id),
        None,
        "127.0.0.1",
        port,
        CODE,
        "Alice",
    )
    .await
    .unwrap();
    assert!(responder.await.unwrap());
    let first = received(&bob).await;
    assert_eq!(first.len(), 1);
    assert_eq!(
        folder_of(&bob, &first[0].id).await.as_deref(),
        Some(inbox.as_str())
    );

    // The folder itself: Work arrives under Inbox, Projects stays behind. A
    // different note, since a resend never refiles what the recipient filed.
    let in_work = seed(&alice, "In work", "also in Projects/Work", &[]).await;
    crate::folders::queries::set_note_folder(&alice.db, &in_work, Some(&work), 1)
        .await
        .unwrap();
    let responder = answer_into(bob.clone(), CODE, Some(inbox.clone()));
    send_notes(
        &alice,
        std::slice::from_ref(&in_work),
        Some(&work),
        "127.0.0.1",
        port,
        CODE,
        "Alice",
    )
    .await
    .unwrap();
    assert!(responder.await.unwrap());
    let bob_work = folder_named(&bob, "Work").await.expect("Work arrived");
    assert!(folder_named(&bob, "Projects").await.is_none());
    let note = received(&bob)
        .await
        .into_iter()
        .find(|n| n.origin_note_id == in_work)
        .unwrap();
    assert_eq!(
        folder_of(&bob, &note.id).await.as_deref(),
        Some(bob_work.as_str())
    );
}

/// The network protocol carries notes only, so an empty folder is refused up
/// front instead of "delivering" nothing.
#[tokio::test]
async fn an_empty_folder_is_refused_over_the_network() {
    let alice = device("alice").await;
    let bob = device("bob").await;
    let empty = folder(&alice, "Empty", None).await;
    let port = listen(bob.clone()).await;
    let err = send_notes(&alice, &[], Some(&empty), "127.0.0.1", port, CODE, "Alice")
        .await
        .unwrap_err();
    assert!(err.contains("by camera"), "{err}");
}
