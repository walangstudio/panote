import { Page } from "playwright/test";

export const MOCK_NOTES = [
  {
    id: "note-1",
    kind: "markdown",
    title: "Meeting notes",
    tags: ["work", "q1"],
    created_at: 1700000000,
    updated_at: 1700010000,
    has_note_password: false,
  },
  {
    id: "note-2",
    kind: "checklist",
    title: "Shopping list",
    tags: [],
    created_at: 1700000000,
    updated_at: 1700005000,
    has_note_password: false,
  },
  {
    id: "note-3",
    kind: "text",
    title: "Draft",
    tags: ["personal"],
    created_at: 1700000000,
    updated_at: 1700001000,
    has_note_password: false,
  },
];

export const MOCK_NOTE_DETAIL = {
  id: "note-1",
  kind: "markdown" as const,
  title: "Meeting notes",
  content: { body: "## Agenda\n\n- Item 1\n- Item 2" },
  tags: ["work", "q1"],
  created_at: 1700000000,
  updated_at: 1700010000,
};

type HandlerMap = Record<string, unknown>;

// Anything not listed here rejects, which leaves the page half-rendered and the
// failure looks like a missing element rather than a missing mock. Every command
// a page touches on load has to be here - drafts and folders both landed after
// this file was written and took the whole note editor down with them.
const DEFAULT_HANDLERS: HandlerMap = {
  note_list: MOCK_NOTES,
  note_get: MOCK_NOTE_DETAIL,
  note_create: MOCK_NOTES[0],
  note_update: {},
  note_delete: null,
  notes_delete: null,
  note_count: MOCK_NOTES.length,
  note_pin: null,
  notes_reorder: null,
  note_set_folder: null,
  // Drafts: the editor asks for one on open. `null` means "no draft waiting",
  // which is the state the specs assume - a draft would raise a restore banner.
  note_draft_get: null,
  note_draft_save: null,
  note_draft_discard: null,
  // Folders: the layout refreshes these on mount.
  folder_list: [],
  folder_create: null,
  folder_rename: null,
  folder_delete: null,
  folder_move: null,
  folders_reorder: null,
  notes_copy: { copied: [], skipped_locked: 0 },
  folder_copy: { copied: [], skipped_locked: 0 },
  trash_list: [],
  trash_restore: null,
  trash_delete: null,
  trash_empty: null,
  get_theme: "candy-light",
  set_theme: null,
  get_autosave: false,
  set_autosave: null,
  get_device_name: "Test Device",
  set_device_name: null,
  device_ips: [],
  is_receiving: false,
  start_receiving: null,
  stop_receiving: null,
  pending_offers_list: [],
  transfer_offer_respond: null,
  peers_scan: [],
  known_peers_list: [],
  peer_add_manual: null,
  generate_pairing_code: "ABC123",
  pending_transfers_list: [],
  note_send: null,
  notes_send: null,
  note_receive_accept: "imported-id",
  note_receive_reject: null,
  "plugin:app|version": "0.2.0",
  // The layout awaits three listen() calls before it checks whether the device
  // is receiving. Unmocked, the first rejects and onMount dies there - polling
  // never starts and the incoming toast never appears, with nothing in the test
  // output to say why. The number is the event id listen() hands back.
  "plugin:event|listen": 1,
  "plugin:event|unlisten": null,
};

/// Returned by a function handler to reject the invoke with this exact value, the
/// way the backend rejects with a bare error string (e.g. "locked"). A thrown
/// error cannot do this: it crosses exposeFunction as an Error object.
export const reject = (error: string) => ({ __reject: error });

// Counter ensures unique function names across calls on the same page.
let _fnSeq = 0;

export async function setupTauriMock(page: Page, overrides: HandlerMap = {}) {
  const handlers: HandlerMap = { ...DEFAULT_HANDLERS };

  for (const [cmd, handler] of Object.entries(overrides)) {
    if (typeof handler === "function") {
      const fnName = `__tauri_mock_${_fnSeq++}_${cmd.replace(/[^a-zA-Z0-9]/g, "_")}`;
      // exposeFunction bridges Node.js closures into the browser context.
      await page.exposeFunction(fnName, handler as (...args: unknown[]) => unknown);
      // Sentinel tells the in-browser invoke shim to call the exposed function.
      handlers[cmd] = `__fn__${fnName}`;
    } else {
      handlers[cmd] = handler;
    }
  }

  await page.addInitScript((h: HandlerMap) => {
    (window as any).__TAURI_INTERNALS__ = {
      invoke: (cmd: string, _args: unknown) => {
        const handler = (h as any)[cmd];
        if (handler === undefined) {
          return Promise.reject(new Error(`Unmocked Tauri command: ${cmd}`));
        }
        if (typeof handler === "string" && handler.startsWith("__fn__")) {
          const fnName = handler.slice("__fn__".length);
          return (window as any)[fnName](_args).then((r: any) =>
            r && typeof r === "object" && "__reject" in r ? Promise.reject(r.__reject) : r,
          );
        }
        return Promise.resolve(handler);
      },
      transformCallback: (fn: (v: unknown) => void, once: boolean) => {
        const id = Math.floor(Math.random() * 2147483647);
        const name = `_${id}`;
        Object.defineProperty(window, name, {
          value: (e: unknown) => {
            if (once) delete (window as any)[name];
            fn(e);
          },
          configurable: true,
        });
        return id;
      },
      metadata: { currentWindow: { label: "main" } },
    };
  }, handlers);
}
