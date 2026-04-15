#![allow(dead_code)]
//! BLE transfer — central (scanner/sender) role via btleplug.
//!
//! Peripheral (receiver/advertiser) mode requires platform-specific APIs
//! (Windows BLE peripheral role is not exposed by btleplug 0.11).
//! Peripheral mode is stubbed with a TODO.
//!
//! Packet format: [2-byte big-endian seq][1-byte is_last][payload bytes]

use crate::{
    state::{AppState, Peer, TransportKind},
    transfer::blob::TransferBlob,
};
use btleplug::{
    api::{
        Central, Manager as _, Peripheral as _, ScanFilter, WriteType,
    },
    platform::{Manager, Peripheral},
};
use std::time::Duration;
use uuid::Uuid;

// Panote GATT UUIDs (stable, randomly assigned)
pub const SERVICE_UUID: Uuid = Uuid::from_u128(0xa1b2c3d4_e5f6_7890_abcd_ef1234567890);
pub const NOTE_SEND_UUID: Uuid = Uuid::from_u128(0xa1b2c3d4_e5f6_7890_abcd_ef1234567891);
pub const STATUS_UUID: Uuid = Uuid::from_u128(0xa1b2c3d4_e5f6_7890_abcd_ef1234567892);

pub const CHUNK_PAYLOAD: usize = 509; // 512 - 3 header bytes

// ---- Chunking (pure, fully tested) ----

/// Split data into 512-byte packets with [seq_hi, seq_lo, is_last] header.
pub fn chunk_payload(data: &[u8]) -> Vec<Vec<u8>> {
    let raw_chunks: Vec<&[u8]> = data.chunks(CHUNK_PAYLOAD).collect();
    let total = raw_chunks.len();
    raw_chunks
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| {
            let seq = i as u16;
            let is_last: u8 = if i == total - 1 { 1 } else { 0 };
            let mut packet = Vec::with_capacity(3 + chunk.len());
            packet.extend_from_slice(&seq.to_be_bytes());
            packet.push(is_last);
            packet.extend_from_slice(chunk);
            packet
        })
        .collect()
}

/// Reassemble packets into the original payload.
/// Tolerates out-of-order delivery.
pub fn reassemble_chunks(packets: &[Vec<u8>]) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(!packets.is_empty(), "no packets to reassemble");
    let mut indexed: Vec<(u16, &[u8])> = packets
        .iter()
        .map(|p| {
            let seq = u16::from_be_bytes([p[0], p[1]]);
            (seq, &p[3..])
        })
        .collect();
    indexed.sort_by_key(|(seq, _)| *seq);
    let data: Vec<u8> = indexed.into_iter().flat_map(|(_, d)| d.iter().copied()).collect();
    Ok(data)
}

// ---- BLE peer discovery (central mode) ----

/// Scan for nearby BLE peripherals advertising the panote service UUID.
/// Returns a list of discovered peers to be added to `AppState.peers`.
pub async fn scan_peers(duration_secs: u64) -> anyhow::Result<Vec<Peer>> {
    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;
    let adapter = adapters
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no Bluetooth adapter found"))?;

    adapter
        .start_scan(ScanFilter {
            services: vec![SERVICE_UUID],
        })
        .await?;

    tokio::time::sleep(Duration::from_secs(duration_secs)).await;
    adapter.stop_scan().await?;

    let peripherals = adapter.peripherals().await?;
    let mut peers = Vec::new();
    for p in peripherals {
        if let Some(props) = p.properties().await? {
            if props.services.contains(&SERVICE_UUID) {
                let addr = props.address.to_string();
                let name = props.local_name.unwrap_or_else(|| addr.clone());
                peers.push(Peer {
                    id: addr.clone(),
                    name,
                    address: addr,
                    port: 0, // BLE doesn't use ports
                    via: TransportKind::Ble,
                });
            }
        }
    }
    Ok(peers)
}

// ---- BLE send (central mode) ----

/// Find a previously scanned peripheral by address and send chunked note data.
async fn find_peripheral(peer_id: &str) -> anyhow::Result<Peripheral> {
    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;
    let adapter = adapters
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no Bluetooth adapter found"))?;

    adapter.start_scan(ScanFilter::default()).await?;
    tokio::time::sleep(Duration::from_secs(2)).await;
    adapter.stop_scan().await?;

    let peripherals = adapter.peripherals().await?;
    for p in peripherals {
        if p.address().to_string() == peer_id {
            return Ok(p);
        }
    }
    anyhow::bail!("BLE peripheral {peer_id} not found")
}

/// Send a note to a BLE peripheral in 512-byte GATT chunks.
pub async fn send_note(
    state: &AppState,
    note_id: &str,
    peer_id: &str,
) -> Result<(), String> {
    let blob = build_blob(state, note_id)
        .await
        .map_err(|e| e.to_string())?;

    let chunks = chunk_payload(&blob);
    let peripheral = find_peripheral(peer_id)
        .await
        .map_err(|e| e.to_string())?;

    peripheral.connect().await.map_err(|e| e.to_string())?;
    peripheral.discover_services().await.map_err(|e| e.to_string())?;

    let chars = peripheral.characteristics();
    let note_send_char = chars
        .iter()
        .find(|c| c.uuid == NOTE_SEND_UUID)
        .ok_or("NOTE_SEND characteristic not found")?;

    for chunk in &chunks {
        peripheral
            .write(note_send_char, chunk, WriteType::WithResponse)
            .await
            .map_err(|e| e.to_string())?;
    }

    peripheral.disconnect().await.map_err(|e| e.to_string())?;
    Ok(())
}

async fn build_blob(state: &AppState, note_id: &str) -> anyhow::Result<Vec<u8>> {
    use crate::{crypto::note::decrypt_with_vault, db::queries};

    let vault_key = state.device_key;

    let row = queries::note_get(&state.db, note_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("note not found"))?;

    if row.note_salt.is_some() {
        anyhow::bail!("cannot transfer a per-note-password note over BLE without the per-note password");
    }

    let title = String::from_utf8(decrypt_with_vault(&vault_key, &row.title_nonce, &row.title_ct)?)?;
    let content: serde_json::Value =
        serde_json::from_slice(&decrypt_with_vault(&vault_key, &row.nonce, &row.content_ct)?)?;
    let tags: Vec<String> = serde_json::from_str(&row.tags).unwrap_or_default();

    TransferBlob {
        id: row.id.clone(),
        kind: row.kind,
        title,
        content,
        tags,
        created_at: row.created_at,
        updated_at: row.updated_at,
        origin_device_id: row.origin_device_id,
        origin_note_id: row.origin_note_id,
    }
    .encode()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_reassemble_small_payload() {
        let data = b"hello ble world";
        let chunks = chunk_payload(data);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0][2], 1); // is_last = true
        let recovered = reassemble_chunks(&chunks).unwrap();
        assert_eq!(recovered, data);
    }

    #[test]
    fn chunk_reassemble_large_payload() {
        let data: Vec<u8> = (0u8..=255).cycle().take(2000).collect();
        let chunks = chunk_payload(&data);
        assert!(chunks.len() > 1);
        for (i, chunk) in chunks.iter().enumerate() {
            let expected_last = if i == chunks.len() - 1 { 1 } else { 0 };
            assert_eq!(chunk[2], expected_last, "chunk {i} is_last flag wrong");
        }
        assert_eq!(reassemble_chunks(&chunks).unwrap(), data);
    }

    #[test]
    fn reassemble_out_of_order() {
        let data: Vec<u8> = (0u8..=255).cycle().take(1500).collect();
        let mut chunks = chunk_payload(&data);
        chunks.reverse();
        assert_eq!(reassemble_chunks(&chunks).unwrap(), data);
    }

    #[test]
    fn chunk_sequence_numbers_are_correct() {
        let data: Vec<u8> = vec![0u8; CHUNK_PAYLOAD * 3];
        let chunks = chunk_payload(&data);
        for (i, chunk) in chunks.iter().enumerate() {
            let seq = u16::from_be_bytes([chunk[0], chunk[1]]);
            assert_eq!(seq, i as u16);
        }
    }

    #[test]
    fn reassemble_empty_input_errors() {
        assert!(reassemble_chunks(&[]).is_err());
    }

    #[test]
    fn exact_chunk_boundary() {
        // Exactly 2 full chunks
        let data: Vec<u8> = vec![0xABu8; CHUNK_PAYLOAD * 2];
        let chunks = chunk_payload(&data);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0][2], 0); // not last
        assert_eq!(chunks[1][2], 1); // last
        assert_eq!(reassemble_chunks(&chunks).unwrap(), data);
    }

    #[test]
    fn single_byte_payload() {
        let data = b"X";
        let chunks = chunk_payload(data);
        assert_eq!(chunks.len(), 1);
        assert_eq!(reassemble_chunks(&chunks).unwrap(), data);
    }
}
