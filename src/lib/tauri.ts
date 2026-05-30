import { invoke } from "@tauri-apps/api/core";

export type NoteKind = "document" | "checklist" | "kanban" | "table";

export interface NoteMetadata {
  id: string;
  kind: NoteKind;
  title: string;
  tags: string[];
  created_at: number;
  updated_at: number;
  has_note_password: boolean;
  content_hint?: string;
  pinned: boolean;
  bg_color?: string;
  bg_image?: string;
  show_preview: boolean;
  preview_text?: string;
}

export interface NoteDetail {
  id: string;
  kind: NoteKind;
  title: string;
  content: unknown;
  tags: string[];
  created_at: number;
  updated_at: number;
  has_note_password: boolean;
  pinned: boolean;
  bg_color?: string;
  bg_image?: string;
  show_preview: boolean;
}

/// Sentinel error strings shared with the Rust backend (notes/commands.rs).
/// note_get / note_update return LOCKED when a protected note isn't unlocked.
export const LOCKED = "locked";
export const WRONG_PASSWORD = "wrong password";

export interface NoteInput {
  kind: NoteKind;
  title: string;
  content: unknown;
  tags: string[];
  content_hint?: string;
  pinned?: boolean;
  bg_color?: string;
  bg_image?: string;
  show_preview?: boolean;
}

export interface Peer {
  id: string;
  name: string;
  address: string;
  port: number;
  via: "lan" | "ble";
}

export interface PendingTransfer {
  transfer_id: string;
  from_peer: string;
  received_at: number;
}

export interface KnownPeer {
  peer_id: string;
  display_name: string | null;
  last_transfer_at: number | null;
}

export interface PendingOffer {
  offer_id: string;
  from_peer: string;
  note_count: number;
  received_at: number;
}

// Notes
export const noteCreate = (input: NoteInput) =>
  invoke<NoteMetadata>("note_create", { input });
export const noteUpdate = (id: string, input: NoteInput) =>
  invoke<NoteMetadata>("note_update", { id, input });
export const noteDelete = (id: string) => invoke<void>("note_delete", { id });
export const noteList = () => invoke<NoteMetadata[]>("note_list");
export const noteGet = (id: string) =>
  invoke<NoteDetail>("note_get", { id });
export const notePin = (id: string, pinned: boolean) =>
  invoke<void>("note_pin", { id, pinned });

// Per-note password
export const noteProtect = (id: string, password: string) =>
  invoke<void>("note_protect", { id, password });
export const noteUnprotect = (id: string, password: string) =>
  invoke<void>("note_unprotect", { id, password });
export const noteChangePassword = (id: string, oldPassword: string, newPassword: string) =>
  invoke<void>("note_change_password", { id, oldPassword, newPassword });
export const noteUnlock = (id: string, password: string) =>
  invoke<void>("note_unlock", { id, password });
export const noteLock = (id: string) => invoke<void>("note_lock", { id });
export const notesProtect = (ids: string[], password: string) =>
  invoke<void>("notes_protect", { ids, password });
export const notesUnprotect = (ids: string[], password: string) =>
  invoke<void>("notes_unprotect", { ids, password });

// Transfer
export const peersScan = () => invoke<Peer[]>("peers_scan");
export const peerAddManual = (address: string) =>
  invoke<Peer>("peer_add_manual", { address });
export const deviceIps = () => invoke<string[]>("device_ips");
export const noteSend = (
  noteId: string,
  peerId: string,
  passphrase: string,
  notePassword?: string,
) => invoke<void>("note_send", { noteId, peerId, passphrase, notePassword });
export const notesSend = (
  noteIds: string[],
  peerId: string,
  passphrase: string,
  notePassword?: string,
) => invoke<void>("notes_send", { noteIds, peerId, passphrase, notePassword });
export const pendingTransfersList = () =>
  invoke<PendingTransfer[]>("pending_transfers_list");
export const pendingOffersList = () =>
  invoke<PendingOffer[]>("pending_offers_list");
export const transferOfferRespond = (offerId: string, passphrase: string) =>
  invoke<void>("transfer_offer_respond", { offerId, passphrase });
export const noteReceiveAccept = (transferId: string, passphrase: string) =>
  invoke<string>("note_receive_accept", { transferId, passphrase });
export const noteReceiveReject = (transferId: string) =>
  invoke<void>("note_receive_reject", { transferId });
export const generatePairingCode = () => invoke<string>("generate_pairing_code");
export const knownPeersList = () => invoke<KnownPeer[]>("known_peers_list");
export const getDeviceName = () => invoke<string>("get_device_name");
export const setDeviceName = (name: string) =>
  invoke<void>("set_device_name", { name });
export const startReceiving = () => invoke<void>("start_receiving");
export const stopReceiving = () => invoke<void>("stop_receiving");
export const isReceiving = () => invoke<boolean>("is_receiving");

// Export / Import
export type ImportResolution = "overwrite" | "skip" | "keepboth";

export interface ImportSummary {
  imported: number;
  updated: number;
  skipped: number;
  errors: string[];
}

export const notesExport = (appVersion: string) =>
  invoke<string>("notes_export", { appVersion });
export const notesImport = (contents: string, resolution: ImportResolution) =>
  invoke<ImportSummary>("notes_import", { contents, resolution });
