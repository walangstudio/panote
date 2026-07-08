import { writable } from "svelte/store";
import { getTheme, setTheme as persistTheme } from "$lib/tauri";

const STORAGE_KEY = "panote-theme";
const DEFAULT_THEME = "candy-light";

function localInitial(): string {
  if (typeof window === "undefined") return DEFAULT_THEME;
  return localStorage.getItem(STORAGE_KEY) || DEFAULT_THEME;
}

export const theme = writable<string>(localInitial());

export function initTheme(): () => void {
  // localStorage paints instantly; the DB is the durable source of truth that
  // survives even if the webview drops localStorage between launches.
  let ready = false;
  const initial = localInitial();
  document.documentElement.dataset.theme = initial;
  theme.set(initial);

  (async () => {
    try {
      const saved = await getTheme();
      if (saved === "candy-light" || saved === "candy-dark") theme.set(saved);
    } catch {}
    ready = true;
    let current = DEFAULT_THEME;
    theme.subscribe((v) => (current = v))();
    persistTheme(current).catch(() => {});
  })();

  const unsub = theme.subscribe((value) => {
    document.documentElement.dataset.theme = value;
    try { localStorage.setItem(STORAGE_KEY, value); } catch {}
    if (ready) persistTheme(value).catch(() => {});
  });
  return unsub;
}

export function toggleDarkMode() {
  theme.update((current) => (current === "candy-dark" ? "candy-light" : "candy-dark"));
}
