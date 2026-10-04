import { readable } from "svelte/store";

// Desktop = wide enough for the list + editor split. Width, not platform:
// a narrow desktop window gets the touch layout, same as a phone.
const QUERY = "(min-width: 900px)";

const match = () => typeof window !== "undefined" && window.matchMedia(QUERY).matches;

export const isDesktop = readable(match(), set => {
  if (typeof window === "undefined") return;
  const mq = window.matchMedia(QUERY);
  const update = () => set(mq.matches);
  mq.addEventListener("change", update);
  return () => mq.removeEventListener("change", update);
});
