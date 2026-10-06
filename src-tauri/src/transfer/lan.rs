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
        vault::{decrypt, derive_key, encrypt, random_salt},
    },
    db::queries,
    state::{now_secs, AppState, Peer, PendingTransfer, TransportKind},
    transfer::{
        blob::TransferBlob,
        frame::{read_frame, write_frame},
        message::Message,
        pake,
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
/// Max notes accepted per transfer offer — bounds the receive read-loop (K1).
pub const MAX_NOTES_PER_TRANSFER: u32 = 1000;
/// Max transfer offers awaiting recipient response at once (N2).
const MAX_PENDING_OFFERS: usize = 64;
/// Max concurrent inbound TLS connections being handled (K10).
const MAX_INBOUND_CONNECTIONS: usize = 16;
/// Fixed AAD context binding transfer-blob ciphertext to this protocol (N3).
pub(crate) const TRANSFER_AAD: &[u8] = b"panote-transfer-v1";

// ---- mDNS ----

/// Start advertising this instance and browsing for peers.
/// Returns a `ServiceDaemon` handle; drop it to stop.
pub fn start_mdns(device_name: &str, state: Arc<AppState>) -> anyhow::Result<ServiceDaemon> {
    let daemon = ServiceDaemon::new()?;

    let host = format!("{device_name}.local.");
    let info = ServiceInfo::new(SERVICE_TYPE, device_name, &host, (), TRANSFER_PORT, None)?;
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
    let announcement = format!(r#"{{"name":"{device_name}","port":{TRANSFER_PORT},"v":1}}"#);

    // Listener
    let state_l = state.clone();
    let own_name_l = own_name.clone();
    tauri::async_runtime::spawn(async move {
        let sock = match UdpSocket::bind(format!("0.0.0.0:{BEACON_PORT}")).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[beacon] bind error: {e}");
                return;
            }
        };
        sock.set_broadcast(true).ok();
        let mut buf = [0u8; 512];
        loop {
            match sock.recv_from(&mut buf).await {
                Ok((n, from)) => {
                    let from_ip = from.ip().to_string();
                    if let Ok(s) = std::str::from_utf8(&buf[..n]) {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                            // K9: validate untrusted beacon fields before use.
                            let name: String = v["name"]
                                .as_str()
                                .unwrap_or("unknown")
                                .chars()
                                .take(128)
                                .collect();
                            if name == own_name_l {
                                continue;
                            }
                            let port = match v["port"].as_u64() {
                                Some(p) if (1..=65535).contains(&p) => p as u16,
                                _ => continue,
                            };
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
                    if let Ok(sock) = std::net::UdpSocket::bind((v4.ip, 0)) {
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

/// What the listener needs from the UI: two notifications. Behind a trait so the
/// transport can be driven end to end without a window - `AppHandle` is the only
/// real implementation.
pub trait TransferEvents: Send + Sync + 'static {
    fn offer_received(&self, offer: &crate::state::PendingOffer);
    fn transfer_received(&self, transfer_id: &str);
    fn transfer_rejected(&self, reason: &str);
    fn notes_received(&self, from_peer: &str, inserted: u32, updated: u32);
}

#[derive(serde::Serialize, Clone)]
struct ReceiveSummary<'a> {
    from_peer: &'a str,
    inserted: u32,
    updated: u32,
}

impl<R: tauri::Runtime> TransferEvents for tauri::AppHandle<R> {
    fn offer_received(&self, offer: &crate::state::PendingOffer) {
        self.emit("transfer-offer", offer).ok();
    }
    fn transfer_received(&self, transfer_id: &str) {
        self.emit("transfer-received", transfer_id).ok();
    }
    fn transfer_rejected(&self, reason: &str) {
        self.emit("transfer-rejected", reason).ok();
    }
    fn notes_received(&self, from_peer: &str, inserted: u32, updated: u32) {
        self.emit(
            "notes-received",
            ReceiveSummary {
                from_peer,
                inserted,
                updated,
            },
        )
        .ok();
    }
}

pub async fn start_listener(
    state: Arc<AppState>,
    events: Arc<dyn TransferEvents>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(format!("0.0.0.0:{TRANSFER_PORT}")).await?;
    serve(listener, state, events).await
}

/// The listener loop, split from the bind so a test can hand in a socket on an
/// ephemeral port and drive a real client against it.
pub async fn serve(
    listener: TcpListener,
    state: Arc<AppState>,
    events: Arc<dyn TransferEvents>,
) -> anyhow::Result<()> {
    let (cert_der, key_der) = device_identity(&state.db, &state.device_key).await?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());

    let server_cfg = tls::server_config(cert_der, key_der, provider)?;
    let acceptor = TlsAcceptor::from(Arc::new(server_cfg));

    // K10: cap concurrent inbound TLS connections so a flood can't spawn unbounded tasks.
    let inbound_limit = Arc::new(tokio::sync::Semaphore::new(MAX_INBOUND_CONNECTIONS));

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let Ok(permit) = inbound_limit.clone().try_acquire_owned() else {
            eprintln!("[lan] inbound connection limit reached, dropping {peer_addr}");
            continue;
        };
        let acceptor = acceptor.clone();
        let state = state.clone();
        let handle = events.clone();
        tokio::spawn(async move {
            let _permit = permit;
            if let Err(e) = handle_incoming(stream, peer_addr, acceptor, state, handle).await {
                eprintln!("[lan] incoming connection error from {peer_addr}: {e}");
            }
        });
    }
}

async fn handle_incoming(
    stream: TcpStream,
    peer_addr: std::net::SocketAddr,
    acceptor: TlsAcceptor,
    state: Arc<AppState>,
    events: Arc<dyn TransferEvents>,
) -> anyhow::Result<()> {
    let mut tls = acceptor.accept(stream).await?;
    let payload = read_frame(&mut tls).await?;
    let msg: Message = serde_json::from_slice(&payload)?;

    // N2: rate-limit by IP, not the full ip:ephemeral_port socket address —
    // the port changes every connection so keying by the full address would
    // never actually throttle a repeat offender. `.ip()` also handles IPv6
    // correctly, unlike splitting the string on ':'.
    let peer_ip = peer_addr.ip().to_string();

    match msg {
        Message::TransferOffer {
            from_peer,
            offer_id,
            note_count,
            pake_msg,
        } => {
            handle_transfer_offer(
                &mut tls, &state, &events, &peer_ip, from_peer, offer_id, note_count, pake_msg,
            )
            .await?;
        }
        // Keep backward-compat: old senders may still blast SendNote directly.
        Message::SendNote {
            from_peer,
            transfer_salt,
            transfer_nonce,
            transfer_ct,
        } => {
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
            events.transfer_received(&transfer_id);

            let ack = serde_json::to_vec(&Message::Ack { transfer_id })?;
            write_frame(&mut tls, &ack).await?;
        }
        Message::Hello { device_name: _ } => {
            let reply = serde_json::to_vec(&Message::Ack {
                transfer_id: String::new(),
            })?;
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
// The arguments are the offer's own fields plus the connection; a struct would
// only exist to be unpacked again on the next line.
#[allow(clippy::too_many_arguments)]
async fn handle_transfer_offer(
    tls: &mut tokio_rustls::server::TlsStream<TcpStream>,
    state: &Arc<AppState>,
    events: &Arc<dyn TransferEvents>,
    peer_addr: &str,
    from_peer: String,
    offer_id: String,
    note_count: u32,
    pake_msg: Vec<u8>,
) -> anyhow::Result<()> {
    // K1: cap notes per offer — bounds the receive read-loop below.
    if note_count > MAX_NOTES_PER_TRANSFER {
        let reject = serde_json::to_vec(&Message::Reject {
            reason: format!("too many notes in one transfer (max {MAX_NOTES_PER_TRANSFER})"),
        })?;
        write_frame(tls, &reject).await?;
        anyhow::bail!("rejected offer from {peer_addr}: note_count {note_count} exceeds cap");
    }

    // N2: per-peer rate limit on offer creation.
    if !state.allow_offer_attempt(peer_addr) {
        let reject = serde_json::to_vec(&Message::Reject {
            reason: "too many transfer offers, try again later".into(),
        })?;
        write_frame(tls, &reject).await?;
        anyhow::bail!("rejected offer from {peer_addr}: rate limit exceeded");
    }

    // N2: cap total pending offers awaiting a response.
    if state.pending_offers.lock().unwrap().len() >= MAX_PENDING_OFFERS {
        let reject = serde_json::to_vec(&Message::Reject {
            reason: "too many pending transfer offers, try again later".into(),
        })?;
        write_frame(tls, &reject).await?;
        anyhow::bail!("rejected offer from {peer_addr}: pending offer cap reached");
    }

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

    events.offer_received(&offer);

    // Wait up to 5 minutes for the recipient to enter the code.
    let passphrase = tokio::time::timeout(std::time::Duration::from_secs(300), rx)
        .await
        .map_err(|_| anyhow::anyhow!("offer timed out"))?
        .map_err(|_| anyhow::anyhow!("offer cancelled"))?;

    // Clean up the offer from pending.
    state.pending_offers.lock().unwrap().remove(&offer_id);

    // Run the SPAKE2 handshake with the entered code — the code itself never
    // goes on the wire. Both sides derive the same session key iff the codes match.
    let (pake_state, pake_msg_r) = pake::start(&passphrase);
    let spake_key = pake::finish(pake_state, &pake_msg)
        .map_err(|e| anyhow::anyhow!("pairing handshake failed: {e}"))?;
    let keys = pake::derive_keys(&spake_key, &pake_msg, &pake_msg_r);

    // Reply with our SPAKE2 message + our key-confirmation MAC.
    let accept = Message::TransferAccept {
        offer_id: offer_id.clone(),
        pake_msg: pake_msg_r,
        confirm: pake::confirm_mac(&keys.confirm_responder),
    };
    write_frame(tls, &serde_json::to_vec(&accept)?).await?;

    // Require the sender's key-confirmation MAC before accepting any note. A
    // wrong pairing code makes this mismatch, and we abort (mutual auth).
    match serde_json::from_slice::<Message>(&read_frame(tls).await?)? {
        Message::PakeConfirm { confirm } => {
            if !pake::verify_mac(&keys.confirm_initiator, &confirm) {
                let reject = serde_json::to_vec(&Message::Reject {
                    reason: "wrong pairing code".into(),
                })?;
                write_frame(tls, &reject).await?;
                events.transfer_rejected("wrong pairing code");
                anyhow::bail!("sender failed key confirmation (wrong code)");
            }
        }
        Message::Reject { reason } => {
            events.transfer_rejected(&reason);
            return Err(anyhow::anyhow!("sender rejected: {reason}"));
        }
        _ => anyhow::bail!("expected key confirmation from sender"),
    }

    // Receive each note, decrypting under the PAKE session key.
    use crate::transfer::commands::{import_blob_detailed, ImportOutcome};
    let (mut inserted, mut updated) = (0u32, 0u32);
    for _ in 0..note_count {
        let (nonce, ct) = match serde_json::from_slice::<Message>(&read_frame(tls).await?)? {
            Message::SessionNote { nonce, ct } => (nonce, ct),
            _ => anyhow::bail!("expected an encrypted note"),
        };
        let blob_bytes = decrypt(&keys.session, &nonce, &ct, TRANSFER_AAD)?;
        let blob = TransferBlob::decode(&blob_bytes)?;
        match import_blob_detailed(state.as_ref(), &state.device_key, blob)
            .await?
            .1
        {
            ImportOutcome::Inserted => inserted += 1,
            ImportOutcome::Updated => updated += 1,
        }
    }
    let _ =
        queries::known_peer_record_transfer(&state.db, &from_peer, &from_peer, now_secs()).await;

    let ack = serde_json::to_vec(&Message::Ack {
        transfer_id: offer_id,
    })?;
    write_frame(tls, &ack).await?;

    events.notes_received(&from_peer, inserted, updated);
    Ok(())
}

// ---- TOFU fingerprint pinning (K3) ----

/// Defense-in-depth alongside `TofuVerifier`: hard-reject if `peer_id` has a
/// previously-persisted fingerprint that differs from the one just presented
/// (e.g. covers the case where the in-memory TOFU store's DB preload at
/// startup silently failed). Persists first-seen fingerprints.
// ponytail: still trust-on-FIRST-sight — an attacker present for the very
// first connection to a peer_id is accepted as that peer. Out-of-band
// fingerprint confirmation during pairing is the upgrade path.
async fn verify_and_persist_fingerprint(
    db: &sqlx::SqlitePool,
    peer_id: &str,
    cert_der: &[u8],
) -> anyhow::Result<()> {
    let fp = tls::cert_fingerprint(cert_der);
    if let Some(existing) = queries::known_peer_get(db, peer_id).await? {
        if existing.fingerprint.len() == 32 && existing.fingerprint != fp {
            anyhow::bail!("TOFU fingerprint mismatch for {peer_id} — possible MITM, rejecting");
        }
    }
    queries::known_peer_upsert(db, peer_id, &fp, now_secs()).await?;
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

    // K3: hard-reject and persist before sending anything sensitive.
    if let Some(certs) = tls.get_ref().1.peer_certificates() {
        if let Some(cert) = certs.first() {
            verify_and_persist_fingerprint(&state.db, address, cert.as_ref()).await?;
        }
    }

    let msg = Message::Hello {
        device_name: device_name.to_string(),
    };
    let payload = serde_json::to_vec(&msg)?;
    write_frame(&mut tls, &payload).await?;

    let reply_bytes = read_frame(&mut tls).await?;
    let _reply: Message = serde_json::from_slice(&reply_bytes)?;

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
        encrypt(&transfer_key, &blob_bytes, TRANSFER_AAD).map_err(|e| e.to_string())?;

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let client_cfg = tls::client_config(state.tofu.clone(), provider).map_err(|e| e.to_string())?;
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
        connector
            .connect(domain, stream)
            .await
            .map_err(|e| e.to_string())?
    };

    // K3: hard-reject and persist before sending anything sensitive.
    if let Some(certs) = tls.get_ref().1.peer_certificates() {
        if let Some(cert) = certs.first() {
            verify_and_persist_fingerprint(&state.db, tofu_key, cert.as_ref())
                .await
                .map_err(|e| e.to_string())?;
        }
    }

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
    // Build every blob up front so a locked or missing note fails before we
    // connect or transmit anything — otherwise earlier notes would already be
    // delivered when a later one errors (partial, non-atomic send).
    let mut blobs = Vec::with_capacity(note_ids.len());
    for note_id in note_ids {
        blobs.push(
            build_blob(state, note_id)
                .await
                .map_err(|e| e.to_string())?,
        );
    }

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let client_cfg = tls::client_config(state.tofu.clone(), provider).map_err(|e| e.to_string())?;
    let connector = TlsConnector::from(Arc::new(client_cfg));

    let stream = TcpStream::connect(format!("{address}:{port}"))
        .await
        .map_err(|e| format!("could not reach {address}:{port} — {e}"))?;

    let domain = rustls::pki_types::ServerName::try_from(address.to_string())
        .unwrap_or_else(|_| rustls::pki_types::ServerName::try_from("panote.local").unwrap());

    let mut tls = {
        let _guard = state.outbound_lock.lock().await;
        state.tofu.set_peer_address(address);
        connector
            .connect(domain, stream)
            .await
            .map_err(|e| e.to_string())?
    };

    // K3: hard-reject and persist before sending anything sensitive.
    if let Some(certs) = tls.get_ref().1.peer_certificates() {
        if let Some(cert) = certs.first() {
            verify_and_persist_fingerprint(&state.db, address, cert.as_ref())
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    // 1. Start SPAKE2 with the pairing code and open the transfer with our msg.
    let (pake_state, pake_msg_i) = pake::start(passphrase);
    let offer_id = Uuid::new_v4().to_string();
    let offer = Message::TransferOffer {
        from_peer: device_name.to_string(),
        offer_id: offer_id.clone(),
        note_count: note_ids.len() as u32,
        pake_msg: pake_msg_i.clone(),
    };
    write_frame(
        &mut tls,
        &serde_json::to_vec(&offer).map_err(|e| e.to_string())?,
    )
    .await
    .map_err(|e| e.to_string())?;

    // 2. Read the recipient's SPAKE2 message + key-confirmation MAC.
    let reply: Message =
        serde_json::from_slice(&read_frame(&mut tls).await.map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let (pake_msg_r, confirm_r) = match reply {
        Message::TransferAccept {
            pake_msg, confirm, ..
        } => (pake_msg, confirm),
        Message::Reject { reason } => return Err(format!("recipient rejected: {reason}")),
        _ => return Err("unexpected reply from recipient".into()),
    };

    // K5: lock out repeated wrong-code attempts against this address.
    if state.passphrase_locked_out(address) {
        return Err("too many failed pairing attempts for this device — try again later".into());
    }

    // 3. Finish SPAKE2 and verify the recipient proved knowledge of the code.
    let spake_key = pake::finish(pake_state, &pake_msg_r).map_err(|e| e.to_string())?;
    let keys = pake::derive_keys(&spake_key, &pake_msg_i, &pake_msg_r);
    if !pake::verify_mac(&keys.confirm_responder, &confirm_r) {
        state.record_passphrase_failure(address);
        let reject = serde_json::to_vec(&Message::Reject {
            reason: "wrong code".into(),
        })
        .map_err(|e| e.to_string())?;
        write_frame(&mut tls, &reject)
            .await
            .map_err(|e| e.to_string())?;
        return Err("wrong pairing code".into());
    }
    state.reset_passphrase_failures(address);

    // 4. Prove we also know the code (mutual auth), then send the notes.
    let confirm = Message::PakeConfirm {
        confirm: pake::confirm_mac(&keys.confirm_initiator),
    };
    write_frame(
        &mut tls,
        &serde_json::to_vec(&confirm).map_err(|e| e.to_string())?,
    )
    .await
    .map_err(|e| e.to_string())?;

    for blob_bytes in &blobs {
        let (nonce, ct) =
            encrypt(&keys.session, blob_bytes, TRANSFER_AAD).map_err(|e| e.to_string())?;
        let msg = Message::SessionNote {
            nonce: nonce.to_vec(),
            ct,
        };
        write_frame(
            &mut tls,
            &serde_json::to_vec(&msg).map_err(|e| e.to_string())?,
        )
        .await
        .map_err(|e| e.to_string())?;
    }

    // 5. Read final Ack
    let ack_bytes = read_frame(&mut tls).await.map_err(|e| e.to_string())?;
    let ack: Message = serde_json::from_slice(&ack_bytes).map_err(|e| e.to_string())?;

    match ack {
        Message::Ack { .. } => Ok(()),
        Message::Reject { reason } => Err(format!("peer rejected: {reason}")),
        _ => Err("unexpected reply from peer".into()),
    }
}

/// Decrypt the note with the device key and return its plaintext bytes.
/// For a protected note the session-cached password is used to peel the
/// password layer; sending a locked note errors until it's unlocked. The blob
/// carries plaintext (Model B) — the receiver chooses whether to protect it.
pub(crate) async fn build_blob(state: &AppState, note_id: &str) -> anyhow::Result<Vec<u8>> {
    let row = queries::note_get(&state.db, note_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("note not found"))?;

    // A protected note must be unlocked this session to be sent.
    let (title, content_bytes) =
        crate::notes::commands::open_row(state, &row).map_err(|e| match e.as_str() {
            crate::notes::commands::LOCKED => anyhow::anyhow!("unlock the note before sending"),
            _ => anyhow::anyhow!(e),
        })?;
    let content: serde_json::Value = serde_json::from_slice(&content_bytes)?;

    let tags = crate::notes::commands::decrypt_tags(&state.device_key, &row.id, &row.tags)?;

    // Empty for a note that is not in a folder, which is the ordinary case.
    let folder_path = match row.folder_id.as_deref() {
        Some(fid) => crate::folders::commands::path_of(state, fid).await,
        None => Vec::new(),
    };

    let blob = TransferBlob {
        id: row.id.clone(),
        kind: row.kind,
        title,
        content,
        tags,
        created_at: row.created_at,
        updated_at: row.updated_at,
        origin_device_id: row.origin_device_id,
        origin_note_id: row.origin_note_id,
        folder_path,
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
    let blob_bytes = decrypt(&transfer_key, transfer_nonce, transfer_ct, TRANSFER_AAD)?;
    TransferBlob::decode(&blob_bytes)
}

// ---- Device TLS identity ----

/// Binds the encrypted TLS private key to this specific use (K7).
const DEVICE_KEY_AAD: &[u8] = b"panote-device-identity-key";

/// Load (or generate) this device's TLS identity. The private key is stored
/// encrypted under the device key — never in plaintext (K7).
pub async fn device_identity(
    db: &sqlx::SqlitePool,
    device_key: &[u8; 32],
) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    if let Some(row) = queries::device_identity_get(db).await? {
        let key_der = decrypt_device_identity_key(db, device_key, &row.key_der).await?;
        return Ok((row.cert_der, key_der));
    }
    let (cert_der, key_der) = tls::generate_self_signed()?;
    let stored_key = encrypt_device_identity_key(device_key, &key_der)?;
    queries::device_identity_insert(db, &cert_der, &stored_key).await?;
    Ok((cert_der, key_der))
}

/// Encrypt the TLS private key before persisting. Stored layout:
/// `nonce (12 bytes) || ciphertext`.
fn encrypt_device_identity_key(device_key: &[u8; 32], key_der: &[u8]) -> anyhow::Result<Vec<u8>> {
    use crate::crypto::note::encrypt_with_vault;
    let (nonce, ct) = encrypt_with_vault(device_key, key_der, DEVICE_KEY_AAD)?;
    let mut out = Vec::with_capacity(nonce.len() + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt a stored TLS private key.
///
/// Rows written before this fix hold a raw (unencrypted) PKCS8 DER key.
/// `decrypt_with_vault` already retries with an empty AAD for rows encrypted
/// before AAD binding; if that also fails, this is only treated as a legacy
/// plaintext key when it plausibly IS one, and is re-encrypted in place so
/// it's read as plaintext at most once. Anything else is a hard error rather
/// than being handed to the TLS layer as a "key".
// ponytail: `stored.first() == Some(&0x30)` (DER SEQUENCE tag) is a
// heuristic, not a real ASN.1 parse — good enough to distinguish a genuine
// legacy key from corrupted/garbage ciphertext without pulling in a DER
// parser for a one-time migration path.
async fn decrypt_device_identity_key(
    db: &sqlx::SqlitePool,
    device_key: &[u8; 32],
    stored: &[u8],
) -> anyhow::Result<Vec<u8>> {
    use crate::crypto::note::decrypt_with_vault;
    if stored.len() > 12 {
        let (nonce, ct) = stored.split_at(12);
        if let Ok(key) = decrypt_with_vault(device_key, nonce, ct, DEVICE_KEY_AAD) {
            return Ok(key);
        }
    }
    if stored.first() == Some(&0x30) {
        let reencrypted = encrypt_device_identity_key(device_key, stored)?;
        queries::device_identity_update_key(db, &reencrypted).await?;
        return Ok(stored.to_vec());
    }
    anyhow::bail!("device identity key is neither valid ciphertext nor a legacy DER key")
}

#[cfg(test)]
mod device_identity_key_tests {
    use super::*;
    use crate::db::init_pool;

    async fn pool() -> sqlx::SqlitePool {
        init_pool(":memory:").await.unwrap()
    }

    #[tokio::test]
    async fn roundtrip() {
        let pool = pool().await;
        let device_key = [7u8; 32];
        let key_der = b"fake pkcs8 der bytes".to_vec();
        let stored = encrypt_device_identity_key(&device_key, &key_der).unwrap();
        assert_ne!(stored, key_der, "must not store the key in plaintext");
        let decrypted = decrypt_device_identity_key(&pool, &device_key, &stored)
            .await
            .unwrap();
        assert_eq!(decrypted, key_der);
    }

    #[tokio::test]
    async fn legacy_der_row_falls_back_and_is_reencrypted() {
        // A row written before this fix: raw DER bytes (starts with the
        // ASN.1 SEQUENCE tag 0x30), no nonce/ct framing.
        let pool = pool().await;
        let device_key = [7u8; 32];
        let mut legacy_key_der = vec![0x30u8];
        legacy_key_der.extend_from_slice(b"legacy raw pkcs8 der bytes, longer than 12");
        queries::device_identity_insert(&pool, b"dummy cert", &legacy_key_der)
            .await
            .unwrap();

        let decrypted = decrypt_device_identity_key(&pool, &device_key, &legacy_key_der)
            .await
            .unwrap();
        assert_eq!(decrypted, legacy_key_der);

        // Must be re-encrypted in the DB so it's read as plaintext at most once.
        let row = queries::device_identity_get(&pool).await.unwrap().unwrap();
        assert_ne!(
            row.key_der, legacy_key_der,
            "must be re-encrypted after first read"
        );
    }

    #[tokio::test]
    async fn non_der_garbage_is_rejected() {
        // #3: garbage that fails decryption AND doesn't look like a DER key
        // must error, not be handed to the TLS layer as if it were a key.
        let pool = pool().await;
        let device_key = [7u8; 32];
        let garbage = b"not ciphertext and not a DER key at all, long enough".to_vec();
        assert!(decrypt_device_identity_key(&pool, &device_key, &garbage)
            .await
            .is_err());
    }
}
