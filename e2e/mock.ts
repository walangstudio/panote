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

const DEFAULT_HANDLERS: HandlerMap = {
  note_list: MOCK_NOTES,
  note_get: MOCK_NOTE_DETAIL,
  note_create: MOCK_NOTES[0],
  note_update: {},
  note_delete: null,
  peers_scan: [],
  known_peers_list: [],
  generate_pairing_code: "ABC123",
  pending_transfers_list: [],
  note_send: null,
  note_receive_accept: "imported-id",
  note_receive_reject: null,
  "plugin:app|version": "0.2.0",
};

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
          return (window as any)[fnName](_args);
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
