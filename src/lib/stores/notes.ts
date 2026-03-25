import { writable } from "svelte/store";
import type { NoteMetadata } from "$lib/tauri";
import { noteList } from "$lib/tauri";

export const notes = writable<NoteMetadata[]>([]);

export async function refreshNotes() {
  notes.set(await noteList());
}
