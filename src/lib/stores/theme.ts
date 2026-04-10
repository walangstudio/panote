import { writable } from "svelte/store";

const STORAGE_KEY = "panote-theme";
const DEFAULT_THEME = "candy-light";

function getInitial(): string {
  if (typeof window === "undefined") return DEFAULT_THEME;
  return localStorage.getItem(STORAGE_KEY) || DEFAULT_THEME;
}

export const theme = writable<string>(getInitial());

export function initTheme(): () => void {
  const saved = getInitial();
  document.documentElement.dataset.theme = saved;
  theme.set(saved);
  const unsub = theme.subscribe((value) => {
    document.documentElement.dataset.theme = value;
    localStorage.setItem(STORAGE_KEY, value);
  });
  return unsub;
}

export function toggleDarkMode() {
  theme.update((current) => {
    if (current === "candy-light") return "candy-dark";
    if (current === "candy-dark") return "candy-light";
    return current;
  });
}
