mod crypto;
mod db;
mod notes;
mod state;
mod transfer;

use notes::commands::*;
use notes::export::{notes_export, notes_import};
use state::AppState;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use transfer::commands::*;

/// Keeps the mDNS ServiceDaemon alive for the lifetime of the app.
#[allow(dead_code)]
struct MdnsHandle(Mutex<mdns_sd::ServiceDaemon>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data dir");
            std::fs::create_dir_all(&app_dir)?;
            let db_path = app_dir.join("panote.db");
            let db_path_str = db_path.to_string_lossy().into_owned();

            let pool = tauri::async_runtime::block_on(db::init_pool(&db_path_str))
                .expect("db init failed");

            // Load or generate the device key (random, stored in DB on first launch).
            let device_key =
                tauri::async_runtime::block_on(db::queries::get_or_create_device_key(&pool))
                    .expect("device key init failed");

            // Stable device UUID for note origin tracking (separate from device_key).
            let device_uuid =
                tauri::async_runtime::block_on(db::queries::get_or_create_device_uuid(&pool))
                    .expect("device uuid init failed");

            // Backfill origin fields on any notes that predate migration 0010.
            tauri::async_runtime::block_on(db::queries::backfill_note_origins(&pool, &device_uuid))
                .expect("note origin backfill failed");

            let state = AppState::new(pool, device_key, device_uuid);

            // Restore TOFU fingerprints from DB so they survive restarts.
            if let Ok(known) = tauri::async_runtime::block_on(db::queries::known_peers_list(&state.db)) {
                for peer in known {
                    if peer.fingerprint.len() == 32 {
                        let mut fp = [0u8; 32];
                        fp.copy_from_slice(&peer.fingerprint);
                        state.tofu.preload(&peer.peer_id, fp);
                    }
                }
            }

            // TCP listener is NOT started here — user must toggle "Receive" on.
            // This keeps port 47291 closed until explicitly enabled.

            // Start mDNS — store the daemon handle to keep it alive.
            let device_name = tauri::async_runtime::block_on(
                transfer::commands::resolve_device_name(&state.db),
            )
            .unwrap_or_else(|_| "panote-device".into());
            match transfer::lan::start_mdns(&device_name, Arc::new(state.clone())) {
                Ok(daemon) => {
                    app.manage(MdnsHandle(Mutex::new(daemon)));
                }
                Err(e) => eprintln!("[mdns] start error: {e}"),
            }

            transfer::lan::start_beacon(&device_name, Arc::new(state.clone()));

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Notes
            note_create,
            note_update,
            note_delete,
            note_list,
            note_get,
            note_pin,
            // Transfer
            start_receiving,
            stop_receiving,
            is_receiving,
            peers_scan,
            peer_add_manual,
            device_ips,
            note_send,
            notes_send,
            transfer_offer_respond,
            pending_offers_list,
            pending_transfers_list,
            note_receive_accept,
            note_receive_reject,
            generate_pairing_code,
            known_peers_list,
            get_device_name,
            set_device_name,
            // Export / Import
            notes_export,
            notes_import,
        ])
        .run(tauri::generate_context!())
        .expect("error while running panote");
}
