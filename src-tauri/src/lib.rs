mod crypto;
mod db;
mod notes;
mod state;
mod transfer;

use notes::commands::*;
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

            let state = AppState::new(pool, device_key);

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

            // Start TCP listener in background.
            let listener_state = Arc::new(state.clone());
            tauri::async_runtime::spawn(async move {
                if let Err(e) = transfer::lan::start_listener(listener_state).await {
                    eprintln!("[lan] listener error: {e}");
                }
            });

            // Start mDNS — store the daemon handle to keep it alive.
            let device_name = format!(
                "panote-{}",
                std::env::var("COMPUTERNAME")
                    .or_else(|_| std::env::var("HOSTNAME"))
                    .unwrap_or_else(|_| "device".to_string())
            );
            match transfer::lan::start_mdns(&device_name, Arc::new(state.clone())) {
                Ok(daemon) => {
                    app.manage(MdnsHandle(Mutex::new(daemon)));
                }
                Err(e) => eprintln!("[mdns] start error: {e}"),
            }

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
            // Transfer
            peers_scan,
            note_send,
            pending_transfers_list,
            note_receive_accept,
            note_receive_reject,
        ])
        .run(tauri::generate_context!())
        .expect("error while running panote");
}
