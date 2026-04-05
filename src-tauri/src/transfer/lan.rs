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
    state::{now_secs, AppState, Peer, PendingTransfer, TransportKind},
    transfer::{
        blob::TransferBlob,
        frame::{read_frame, write_frame},
        message::Message,
    },
};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::sync::Arc;
use tauri::{self, Emitter};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio_rustls::{TlsAcceptor, TlsConnector};
use uuid::Uuid;

pub const SERVICE_TYPE: &str = "_panote._tcp.local.";
pub const TRANSFER_PORT: u16 = 47291;
pub const BEACON_PORT: u16 = 47292;

// ---- mDNS ----

/// Start advertising this instance and browsing for peers.
/// Returns a `ServiceDaemon` handle; drop it to stop.
pub fn start_mdns(
    device_name: &str,
    state: Arc<AppState>,
) -> anyhow::Result<ServiceDaemon> {
    let daemon = ServiceDaemon::new()?;

    let host = format!("{device_name}.local.");
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

// ---- UDP broadcast beacon (fallback for networks that filter mDNS multicast) ----

/// Sends periodic UDP broadcast announcements and listens for peers doing the same.
/// Works across WiFi/Ethernet boundaries where mDNS multicast is filtered.
pub fn start_beacon(device_name: &str, state: Arc<AppState>) {
    let own_name = device_name.to_string();
    let announcement = format!(
        r#"{{"name":"{device_name}","port":{TRANSFER_PORT},"v":1}}"#
    );

    // Listener
    let state_l = state.clone();
    let own_name_l = own_name.clone();
    tauri::async_runtime::spawn(async move {
        let sock = match UdpSocket::bind(format!("0.0.0.0:{BEACON_PORT}")).await {
            Ok(s) => s,
            Err(e) => { eprintln!("[beacon] bind error: {e}"); return; }
        };
        sock.set_broadcast(true).ok();
        let mut buf = [0u8; 512];
        loop {
            match sock.recv_from(&mut buf).await {
                Ok((n, from)) => {
                    let from_ip = from.ip().to_string();
                    if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                            let name = v["name"].as_str().unwrap_or("unknown").to_string();
                            if name == own_name_l { continue; }
                            let port = v["port"].as_u64().unwrap_or(TRANSFER_PORT as u64) as u16;
                            let peer = Peer {
                                id: format!("{from_ip}:{port}"),
                                name,
                                address: from_ip.clone(),
                                port,
                                via: TransportKind::Lan,
                            };
                            let mut peers = state_l.peers.lock().unwrap();
                            peers.retain(|p| p.address != from_ip);
                            peers.push(peer);
                        }
                    }
                }
                Err(e) => eprintln!("[beacon] recv error: {e}"),
            }
        }
    });

    // Sender — broadcasts on every IPv4 interface every 2 seconds.
    // Binding per-interface ensures packets egress the correct NIC rather than
    // letting the OS pick one (which silently drops coverage on multi-homed hosts).
    tauri::async_runtime::spawn(async move {
        let msg = announcement.into_bytes();
        loop {
            for iface in if_addrs::get_if_addrs().unwrap_or_default() {
                if let if_addrs::IfAddr::V4(v4) = iface.addr {
                    if v4.ip.is_loopback() {
                        continue;
                    }
                    let bcast = v4.broadcast.unwrap_or_else(|| {
                        let ip = u32::from(v4.ip);
                        let mask = u32::from(v4.netmask);
                        std::net::Ipv4Addr::from(ip | !mask)
                    });
                    if let Ok(sock) =
                        std::net::UdpSocket::bind((v4.ip, 0))
                    {
                        sock.set_broadcast(true).ok();
                        let _ = sock.send_to(&msg, (bcast, BEACON_PORT));
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    });
}

// hostname() removed — use resolve_device_name() from commands.rs instead

// ---- TLS TCP server ----

pub async fn start_listener(
    state: Arc<AppState>,
    app_handle: tauri::AppHandle,
) -> anyhow::Result<()> {
    let (cert_der, key_der) = device_identity(&state.db).await?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let server_cfg = tls::server_config(cert_der, key_der, provider)?;
    let acceptor = TlsAcceptor::from(Arc::new(server_cfg));

    let listener = TcpListener::bind(format!("0.0.0.0:{TRANSFER_PORT}")).await?;

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let state = state.clone();
        let handle = app_handle.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_incoming(stream, peer_addr.to_string(), acceptor, state, handle).await {
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
    app_handle: tauri::AppHandle,
) -> anyhow::Result<()> {
    let mut tls = acceptor.accept(stream).await?;
    let payload = read_frame(&mut tls).await?;
    let msg: Message = serde_json::from_slice(&payload)?;

    match msg {
        Message::TransferOffer { from_peer, offer_id, note_count } => {
            handle_transfer_offer(
                &mut tls, &state, &app_handle,
                from_peer, offer_id, note_count,
            ).await?;
        }
        // Keep backward-compat: old senders may still blast SendNote directly.
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
            app_handle.emit("transfer-received", &transfer_id).ok();

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

/// Handle the new transfer offer protocol:
/// 1. Store offer, emit event, wait for recipient to enter code
/// 2. Send code back to sender
/// 3. Read incoming SendNote messages and auto-import
async fn handle_transfer_offer(
    tls: &mut tokio_rustls::server::TlsStream<TcpStream>,
    state: &Arc<AppState>,
    app_handle: &tauri::AppHandle,
    from_peer: String,
    offer_id: String,
    note_count: u32,
) -> anyhow::Result<()> {
    use crate::transfer::commands::import_blob;

    let offer = crate::state::PendingOffer {
        offer_id: offer_id.clone(),
        from_peer: from_peer.clone(),
        note_count,
        received_at: now_secs(),
    };

    // Create oneshot channel for the UI to send back the passphrase.
    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
    {
        let mut offers = state.pending_offers.lock().unwrap();
        offers.insert(offer_id.clone(), offer.clone());
    }
    {
        let mut responses = state.offer_responses.lock().unwrap();
        responses.insert(offer_id.clone(), tx);
    }

    app_handle.emit("transfer-offer", &offer).ok();

    // Wait up to 5 minutes for the recipient to enter the code.
    let passphrase = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        rx,
    ).await
        .map_err(|_| anyhow::anyhow!("offer timed out"))?
        .map_err(|_| anyhow::anyhow!("offer cancelled"))?;

    // Clean up the offer from pending.
    state.pending_offers.lock().unwrap().remove(&offer_id);

    // Send the passphrase back to the sender for verification.
    let accept_msg = Message::TransferAccept {
        offer_id: offer_id.clone(),
        passphrase: passphrase.clone(),
    };
    let accept_bytes = serde_json::to_vec(&accept_msg)?;
    write_frame(tls, &accept_bytes).await?;

    // Now read the sender's response — either Reject (wrong code) or SendNote messages.
    let first_frame = read_frame(tls).await?;
    let first_msg: Message = serde_json::from_slice(&first_frame)?;

    match first_msg {
        Message::Reject { reason } => {
            app_handle.emit("transfer-rejected", &reason).ok();
            return Err(anyhow::anyhow!("sender rejected: {reason}"));
        }
        Message::SendNote { from_peer, transfer_salt, transfer_nonce, transfer_ct } => {
            // Decrypt and import the first note.
            let blob = decrypt_transfer(&transfer_salt, &transfer_nonce, &transfer_ct, &passphrase)?;
            import_blob(state.as_ref(), &state.device_key, blob).await?;
            let _ = queries::known_peer_record_transfer(&state.db, &from_peer, &from_peer, now_secs()).await;

            // Read remaining notes (note_count - 1).
            for _ in 1..note_count {
                let frame = read_frame(tls).await?;
                let msg: Message = serde_json::from_slice(&frame)?;
                if let Message::SendNote { transfer_salt, transfer_nonce, transfer_ct, .. } = msg {
                    let blob = decrypt_transfer(&transfer_salt, &transfer_nonce, &transfer_ct, &passphrase)?;
                    import_blob(state.as_ref(), &state.device_key, blob).await?;
                }
            }

            // Send final Ack.
            let ack = serde_json::to_vec(&Message::Ack { transfer_id: offer_id })?;
            write_frame(tls, &ack).await?;

            // Notify frontend to refresh notes.
            app_handle.emit("notes-received", ()).ok();
        }
        _ => {
            return Err(anyhow::anyhow!("unexpected message after TransferAccept"));
        }
    }
    Ok(())
}

// ---- TLS TCP probe (Hello handshake) ----

/// Send a Hello message to a peer by IP and return a Peer struct if it responds.
pub async fn hello_probe(
    state: &AppState,
    address: &str,
    port: u16,
    device_name: &str,
) -> anyhow::Result<Peer> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let client_cfg = tls::client_config(state.tofu.clone(), provider)?;
    let connector = TlsConnector::from(Arc::new(client_cfg));

    let stream = TcpStream::connect(format!("{address}:{port}"))
        .await
        .map_err(|e| anyhow::anyhow!("could not reach {address}:{port} — {e}"))?;

    let domain = rustls::pki_types::ServerName::try_from(address.to_string())
        .unwrap_or_else(|_| rustls::pki_types::ServerName::try_from("panote.local").unwrap());

    let mut tls = {
        let _guard = state.outbound_lock.lock().await;
        state.tofu.set_peer_address(address);
        connector.connect(domain, stream).await?
    };

    let msg = Message::Hello {
        device_name: device_name.to_string(),
    };
    let payload = serde_json::to_vec(&msg)?;
    write_frame(&mut tls, &payload).await?;

    let reply_bytes = read_frame(&mut tls).await?;
    let _reply: Message = serde_json::from_slice(&reply_bytes)?;

    // Persist TOFU fingerprint.
    if let Some(certs) = tls.get_ref().1.peer_certificates() {
        if let Some(cert) = certs.first() {
            let fp = tls::cert_fingerprint(cert.as_ref());
            let _ = queries::known_peer_upsert(&state.db, address, &fp, now_secs()).await;
        }
    }

    Ok(Peer {
        id: format!("{address}:{port}"),
        name: format!("Device at {address}"),
        address: address.to_string(),
        port,
        via: TransportKind::Lan,
    })
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
    device_name: &str,
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

    let mut tls = {
        let _guard = state.outbound_lock.lock().await;
        state.tofu.set_peer_address(address);
        connector.connect(domain, stream).await.map_err(|e| e.to_string())?
    };

    let msg = Message::SendNote {
        from_peer: device_name.to_string(),
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

/// Send multiple notes using the new offer/accept protocol.
/// 1. Connect TLS
/// 2. Send TransferOffer
/// 3. Read TransferAccept (recipient enters code)
/// 4. Verify passphrase, encrypt & send all notes, read final Ack
pub async fn send_notes(
    state: &AppState,
    note_ids: &[String],
    address: &str,
    port: u16,
    passphrase: &str,
    device_name: &str,
) -> Result<(), String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let client_cfg = tls::client_config(state.tofu.clone(), provider)
        .map_err(|e| e.to_string())?;
    let connector = TlsConnector::from(Arc::new(client_cfg));

    let stream = TcpStream::connect(format!("{address}:{port}"))
        .await
        .map_err(|e| format!("could not reach {address}:{port} — {e}"))?;

    let domain = rustls::pki_types::ServerName::try_from(address.to_string())
        .unwrap_or_else(|_| rustls::pki_types::ServerName::try_from("panote.local").unwrap());

    let mut tls = {
        let _guard = state.outbound_lock.lock().await;
        state.tofu.set_peer_address(address);
        connector.connect(domain, stream).await.map_err(|e| e.to_string())?
    };

    // 1. Send TransferOffer
    let offer_id = Uuid::new_v4().to_string();
    let offer = Message::TransferOffer {
        from_peer: device_name.to_string(),
        offer_id: offer_id.clone(),
        note_count: note_ids.len() as u32,
    };
    let offer_bytes = serde_json::to_vec(&offer).map_err(|e| e.to_string())?;
    write_frame(&mut tls, &offer_bytes).await.map_err(|e| e.to_string())?;

    // 2. Read TransferAccept (recipient sends back the code they entered)
    let reply_bytes = read_frame(&mut tls).await.map_err(|e| e.to_string())?;
    let reply: Message = serde_json::from_slice(&reply_bytes).map_err(|e| e.to_string())?;

    let recipient_code = match reply {
        Message::TransferAccept { passphrase: code, .. } => code,
        Message::Reject { reason } => return Err(format!("recipient rejected: {reason}")),
        _ => return Err("unexpected reply from recipient".into()),
    };

    // 3. Verify passphrase matches (case-insensitive, strip dashes)
    let normalize = |s: &str| s.replace('-', "").to_uppercase();
    if normalize(&recipient_code) != normalize(passphrase) {
        let reject = Message::Reject { reason: "wrong code".into() };
        let reject_bytes = serde_json::to_vec(&reject).map_err(|e| e.to_string())?;
        write_frame(&mut tls, &reject_bytes).await.map_err(|e| e.to_string())?;
        return Err("recipient entered the wrong code".into());
    }

    // 4. Send all notes
    for note_id in note_ids {
        let blob_bytes = build_blob(state, note_id)
            .await
            .map_err(|e| e.to_string())?;

        let transfer_salt = random_salt();
        let transfer_key = derive_key(passphrase, &transfer_salt).map_err(|e| e.to_string())?;
        let (transfer_nonce, transfer_ct) =
            encrypt(&transfer_key, &blob_bytes).map_err(|e| e.to_string())?;

        let msg = Message::SendNote {
            from_peer: device_name.to_string(),
            transfer_salt: transfer_salt.to_vec(),
            transfer_nonce: transfer_nonce.to_vec(),
            transfer_ct,
        };
        let payload = serde_json::to_vec(&msg).map_err(|e| e.to_string())?;
        write_frame(&mut tls, &payload).await.map_err(|e| e.to_string())?;
    }

    // 5. Read final Ack
    let ack_bytes = read_frame(&mut tls).await.map_err(|e| e.to_string())?;
    let ack: Message = serde_json::from_slice(&ack_bytes).map_err(|e| e.to_string())?;

    // Persist TOFU fingerprint
    if let Some(certs) = tls.get_ref().1.peer_certificates() {
        if let Some(cert) = certs.first() {
            let fp = tls::cert_fingerprint(cert.as_ref());
            let _ = queries::known_peer_upsert(&state.db, address, &fp, now_secs()).await;
        }
    }

    match ack {
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
