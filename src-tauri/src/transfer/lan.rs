//! LAN transfer: mDNS discovery + TLS 1.3 TCP transport.
//!
//! Each app instance:
//! - Advertises `_panote._tcp.local.` via mDNS
//! - Browses for other instances, adding them to AppState.peers
//! - Runs a TLS TCP server on TRANSFER_PORT
//!
//! Security:
//! - TLS 1.3 with self-signed certs + TOFU fingerprint verification (transport layer)
//! - Transfer payloads are additionally encrypted with a shared passphrase (payload layer)

use crate::{
    crypto::{
        tls,
        vault::{derive_key, encrypt, decrypt, random_salt},
    },
    db::queries,
    state::{AppState, Peer, PendingTransfer, TransportKind},
    transfer::{
        blob::TransferBlob,
        frame::{read_frame, write_frame},
        message::Message,
    },
};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use serde_json;
use std::{sync::Arc, time::{SystemTime, UNIX_EPOCH}};
use tauri;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use uuid::Uuid;

pub const SERVICE_TYPE: &str = "_panote._tcp.local.";
pub const TRANSFER_PORT: u16 = 47291;

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

// ---- mDNS ----

/// Start advertising this instance and browsing for peers.
/// Returns a `ServiceDaemon` handle; drop it to stop.
pub fn start_mdns(
    device_name: &str,
    state: Arc<AppState>,
) -> anyhow::Result<ServiceDaemon> {
    let daemon = ServiceDaemon::new()?;

    let host = hostname();
    let info = ServiceInfo::new(
        SERVICE_TYPE,
        device_name,
        &host,
        (),
        TRANSFER_PORT,
        None,
    )?;
    daemon.register(info)?;

    let receiver = daemon.browse(SERVICE_TYPE)?;
    let own_name = device_name.to_string();
    tauri::async_runtime::spawn(async move {
        while let Ok(event) = receiver.recv_async().await {
            use mdns_sd::ServiceEvent;
            match event {
                ServiceEvent::ServiceResolved(info) => {
                    let name = info.get_fullname().to_string();
                    if name.starts_with(&own_name) {
                        continue;
                    }
                    let addresses: Vec<_> = info.get_addresses().iter().collect();
                    if let Some(addr) = addresses.first() {
                        let peer = Peer {
                            id: name.clone(),
                            name: info.get_hostname().to_string(),
                            address: addr.to_string(),
                            port: info.get_port(),
                            via: TransportKind::Lan,
                        };
                        let mut peers = state.peers.lock().unwrap();
                        peers.retain(|p| p.id != peer.id);
                        peers.push(peer);
                    }
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    state.peers.lock().unwrap().retain(|p| p.id != fullname);
                }
                _ => {}
            }
        }
    });

    Ok(daemon)
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "panote-device".to_string())
        + ".local."
}

// ---- TLS TCP server ----

pub async fn start_listener(state: Arc<AppState>) -> anyhow::Result<()> {
    let (cert_der, key_der) = device_identity(&state.db).await?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let server_cfg = tls::server_config(cert_der, key_der, provider)?;
    let acceptor = TlsAcceptor::from(Arc::new(server_cfg));

    let listener = TcpListener::bind(format!("0.0.0.0:{TRANSFER_PORT}")).await?;

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_incoming(stream, peer_addr.to_string(), acceptor, state).await {
                eprintln!("[lan] incoming connection error from {peer_addr}: {e}");
            }
        });
    }
}

async fn handle_incoming(
    stream: TcpStream,
    _peer_addr: String,
    acceptor: TlsAcceptor,
    state: Arc<AppState>,
) -> anyhow::Result<()> {
    let mut tls = acceptor.accept(stream).await?;
    let payload = read_frame(&mut tls).await?;
    let msg: Message = serde_json::from_slice(&payload)?;

    match msg {
        Message::SendNote { from_peer, transfer_salt, transfer_nonce, transfer_ct } => {
            let transfer = PendingTransfer {
                transfer_id: Uuid::new_v4().to_string(),
                from_peer,
                transfer_salt,
                transfer_nonce,
                transfer_ct,
                received_at: now_secs(),
            };
            let transfer_id = transfer.transfer_id.clone();
            state.add_pending(transfer);

            let ack = serde_json::to_vec(&Message::Ack { transfer_id })?;
            write_frame(&mut tls, &ack).await?;
        }
        Message::Hello { device_name: _ } => {
            let reply = serde_json::to_vec(&Message::Ack { transfer_id: String::new() })?;
            write_frame(&mut tls, &reply).await?;
        }
        _ => {
            let reject = serde_json::to_vec(&Message::Reject {
                reason: "unexpected message type".into(),
            })?;
            write_frame(&mut tls, &reject).await?;
        }
    }
    Ok(())
}

// ---- TLS TCP client (send) ----

/// Send a note to a LAN peer over TLS 1.3.
/// The note payload is additionally encrypted with the shared passphrase.
pub async fn send_note(
    state: &AppState,
    note_id: &str,
    address: &str,
    port: u16,
    passphrase: &str,
) -> Result<(), String> {
    let blob_bytes = build_blob(state, note_id)
        .await
        .map_err(|e| e.to_string())?;

    // Encrypt blob with transfer key derived from passphrase.
    let transfer_salt = random_salt();
    let transfer_key = derive_key(passphrase, &transfer_salt).map_err(|e| e.to_string())?;
    let (transfer_nonce, transfer_ct) =
        encrypt(&transfer_key, &blob_bytes).map_err(|e| e.to_string())?;

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let client_cfg = tls::client_config(state.tofu.clone(), provider)
        .map_err(|e| e.to_string())?;
    let connector = TlsConnector::from(Arc::new(client_cfg));

    let stream = TcpStream::connect(format!("{address}:{port}"))
        .await
        .map_err(|e| e.to_string())?;

    let domain = rustls::pki_types::ServerName::try_from(address.to_string())
        .unwrap_or_else(|_| rustls::pki_types::ServerName::try_from("panote.local").unwrap());
    let tofu_key = address; // key by peer IP, not TLS SNI (which collapses to "panote.local" for IPs)

    let mut tls = connector
        .connect(domain, stream)
        .await
        .map_err(|e| e.to_string())?;

    let msg = Message::SendNote {
        from_peer: hostname(),
        transfer_salt: transfer_salt.to_vec(),
        transfer_nonce: transfer_nonce.to_vec(),
        transfer_ct,
    };
    let payload = serde_json::to_vec(&msg).map_err(|e| e.to_string())?;
    write_frame(&mut tls, &payload)
        .await
        .map_err(|e| e.to_string())?;

    let reply_bytes = read_frame(&mut tls).await.map_err(|e| e.to_string())?;
    let reply: Message = serde_json::from_slice(&reply_bytes).map_err(|e| e.to_string())?;

    // Persist peer cert fingerprint so TOFU survives restarts.
    if let Some(certs) = tls.get_ref().1.peer_certificates() {
        if let Some(cert) = certs.first() {
            let fp = tls::cert_fingerprint(cert.as_ref());
            let _ = queries::known_peer_upsert(&state.db, &tofu_key, &fp, now_secs()).await;
        }
    }

    match reply {
        Message::Ack { .. } => Ok(()),
        Message::Reject { reason } => Err(format!("peer rejected: {reason}")),
        _ => Err("unexpected reply from peer".into()),
    }
}

/// Decrypt the note with the device key and return its plaintext bytes.
async fn build_blob(state: &AppState, note_id: &str) -> anyhow::Result<Vec<u8>> {
    use crate::crypto::note::decrypt_with_vault;

    let row = queries::note_get(&state.db, note_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("note not found"))?;

    let title_bytes = decrypt_with_vault(&state.device_key, &row.title_nonce, &row.title_ct)?;
    let title = String::from_utf8(title_bytes)?;

    let content_bytes = decrypt_with_vault(&state.device_key, &row.nonce, &row.content_ct)?;
    let content: serde_json::Value = serde_json::from_slice(&content_bytes)?;

    let tags: Vec<String> = serde_json::from_str(&row.tags).unwrap_or_default();

    let blob = TransferBlob {
        id: row.id,
        kind: row.kind,
        title,
        content,
        tags,
        created_at: row.created_at,
        updated_at: row.updated_at,
    };
    blob.encode()
}

/// Decrypt a pending transfer using the shared passphrase.
pub fn decrypt_transfer(
    transfer_salt: &[u8],
    transfer_nonce: &[u8],
    transfer_ct: &[u8],
    passphrase: &str,
) -> anyhow::Result<TransferBlob> {
    let transfer_key = derive_key(passphrase, transfer_salt)?;
    let blob_bytes = decrypt(&transfer_key, transfer_nonce, transfer_ct)?;
    TransferBlob::decode(&blob_bytes)
}

// ---- Device TLS identity ----

pub async fn device_identity(db: &sqlx::SqlitePool) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    if let Some(row) = queries::device_identity_get(db).await? {
        return Ok((row.cert_der, row.key_der));
    }
    let (cert_der, key_der) = tls::generate_self_signed()?;
    queries::device_identity_insert(db, &cert_der, &key_der).await?;
    Ok((cert_der, key_der))
}
