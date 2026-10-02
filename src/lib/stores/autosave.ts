import { writable } from "svelte/store";
import { getAutosave, setAutosave as persistAutosave } from "$lib/tauri";

/// Whether the editor saves a note by itself. Off unless turned on in Settings;
/// the DB is the source of truth.
export const autosave = writable(false);

export async function loadAutosave() {
  try { autosave.set(await getAutosave()); } catch {}
}

export async function setAutosave(on: boolean) {
  autosave.set(on);
  try { await persistAutosave(on); } catch { autosave.set(!on); }
}
