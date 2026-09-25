import { writable, derived } from "svelte/store";
import { getTheme, setTheme as persistTheme } from "$lib/tauri";

export type ThemePreference = "candy-light" | "candy-dark" | "system";

const STORAGE_KEY = "panote-theme";
const DEFAULT_THEME: ThemePreference = "system";
const DARK_QUERY = "(prefers-color-scheme: dark)";

function isPreference(v: unknown): v is ThemePreference {
  return v === "candy-light" || v === "candy-dark" || v === "system";
}

function localInitial(): ThemePreference {
  if (typeof window === "undefined") return DEFAULT_THEME;
  const stored = localStorage.getItem(STORAGE_KEY);
  return isPreference(stored) ? stored : DEFAULT_THEME;
}

export const theme = writable<ThemePreference>(localInitial());

const systemDark = writable(typeof window !== "undefined" && window.matchMedia(DARK_QUERY).matches);

/// What is painted: the preference with "system" resolved against the OS.
export const resolvedTheme = derived([theme, systemDark], ([$theme, $dark]) =>
  $theme === "system" ? ($dark ? "candy-dark" : "candy-light") : $theme,
);

export function initTheme(): () => void {
  // localStorage paints instantly; the DB is the durable source of truth that
  // survives even if the webview drops localStorage between launches.
  let ready = false;
  theme.set(localInitial());

  const query = window.matchMedia(DARK_QUERY);
  systemDark.set(query.matches);
  const onChange = (e: MediaQueryListEvent) => systemDark.set(e.matches);
  query.addEventListener("change", onChange);

  (async () => {
    try {
      const saved = await getTheme();
      if (isPreference(saved)) theme.set(saved);
    } catch {}
    ready = true;
    let current: ThemePreference = DEFAULT_THEME;
    theme.subscribe((v) => (current = v))();
    persistTheme(current).catch(() => {});
  })();

  const unsubPaint = resolvedTheme.subscribe((value) => {
    document.documentElement.dataset.theme = value;
  });
  const unsubPersist = theme.subscribe((value) => {
    try { localStorage.setItem(STORAGE_KEY, value); } catch {}
    if (ready) persistTheme(value).catch(() => {});
  });
  return () => {
    unsubPaint();
    unsubPersist();
    query.removeEventListener("change", onChange);
  };
}
