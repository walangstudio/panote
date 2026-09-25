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
  /// Which folder the note is in; null at the root.
  folder_id?: string | null;
  /// Position within its level, used only by the Manual sort.
  sort_order?: number;
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
  has_recovery: boolean;
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
/// Moves the note to Trash. Only `trashDelete` / `trashEmpty` remove it for good.
export const noteDelete = (id: string) => invoke<void>("note_delete", { id });

/// A note in Trash: what the list shows, plus when it was deleted (unix secs).
export interface TrashedNote {
  id: string;
  kind: NoteKind;
  title: string;
  has_note_password: boolean;
  content_hint?: string;
  deleted_at: number;
}
export const trashList = () => invoke<TrashedNote[]>("trash_list");
export const trashRestore = (ids: string[]) => invoke<void>("trash_restore", { ids });
export const trashDelete = (ids: string[]) => invoke<void>("trash_delete", { ids });
export const trashEmpty = () => invoke<void>("trash_empty");
/// A folder as the backend returns it: flat, with its parent. `note_count` is
/// this folder only; the tree rolls subfolder counts up (see stores/folders.ts).
export interface Folder {
  id: string;
  parent_id: string | null;
  name: string;
  note_count: number;
  sort_order?: number;
}

export const folderList = () => invoke<Folder[]>("folder_list");
export const folderCreate = (name: string, parentId?: string | null) =>
  invoke<string>("folder_create", { name, parentId: parentId ?? null });
export const folderRename = (id: string, name: string) =>
  invoke<void>("folder_rename", { id, name });
export const folderMove = (id: string, parentId: string | null) =>
  invoke<void>("folder_move", { id, parentId });
export const folderDelete = (id: string) => invoke<void>("folder_delete", { id });
export const noteSetFolder = (noteId: string, folderId: string | null) =>
  invoke<void>("note_set_folder", { noteId, folderId });
/// Ids in the order the user arranged them; positions are derived from the order.
export const notesReorder = (ids: string[]) => invoke<void>("notes_reorder", { ids });
export const foldersReorder = (ids: string[]) => invoke<void>("folders_reorder", { ids });

export const noteList = () => invoke<NoteMetadata[]>("note_list");
/// Total notes in the database. The list is capped (K14), so this is how the UI
/// knows when it is showing a partial view instead of silently omitting notes.
export const noteCount = () => invoke<number>("note_count");
/// Background images keyed by note id. Kept out of the list payload because a
/// background is a base64 data URI far larger than the rest of the row, and the
/// list refreshes on every save, pin and delete.
export const noteBgImages = () => invoke<Record<string, string>>("note_bg_images");
export const noteGet = (id: string) =>
  invoke<NoteDetail>("note_get", { id });
export const notePin = (id: string, pinned: boolean) =>
  invoke<void>("note_pin", { id, pinned });

// Drafts — unsaved edits, held apart from the committed note so autosaving can
// never overwrite it. Saving the note is what commits and clears the draft.
export interface DraftPayload {
  title: string;
  content: unknown;
  tags: string[];
}
export interface DraftDetail extends DraftPayload {
  updated_at: number;
}
export const noteDraftSave = (id: string, draft: DraftPayload) =>
  invoke<void>("note_draft_save", { id, draft });
export const noteDraftGet = (id: string) =>
  invoke<DraftDetail | null>("note_draft_get", { id });
export const noteDraftDiscard = (id: string) =>
  invoke<void>("note_draft_discard", { id });

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
// Recovery code: add returns the one-time code to show; recover sets a new password.
export const noteAddRecovery = (id: string, password: string) =>
  invoke<string>("note_add_recovery", { id, password });
export const noteRecover = (id: string, recoveryCode: string, newPassword: string) =>
  invoke<void>("note_recover", { id, recoveryCode, newPassword });
export const notesProtect = (ids: string[], password: string) =>
  invoke<void>("notes_protect", { ids, password });
export const notesUnprotect = (ids: string[], password: string) =>
  invoke<void>("notes_unprotect", { ids, password });

// Transfer
export const peersScan = () => invoke<Peer[]>("peers_scan");
export const peerAddManual = (address: string) =>
  invoke<Peer>("peer_add_manual", { address });
export const deviceIps = () => invoke<string[]>("device_ips");
export const noteSend = (noteId: string, peerId: string, passphrase: string) =>
  invoke<void>("note_send", { noteId, peerId, passphrase });
export const notesSend = (noteIds: string[], peerId: string, passphrase: string) =>
  invoke<void>("notes_send", { noteIds, peerId, passphrase });
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
export const getTheme = () => invoke<string | null>("get_theme");
export const setTheme = (theme: string) => invoke<void>("set_theme", { theme });
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
/// `secretPassword` unseals password-protected notes in the backup. Notes that
/// can't be opened are reported in `errors`, never silently dropped.
export const notesImport = (
  contents: string,
  resolution: ImportResolution,
  secretPassword?: string,
) =>
  invoke<ImportSummary>("notes_import", {
    contents,
    resolution,
    secretPassword: secretPassword ?? null,
  });
