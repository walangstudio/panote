import { writable } from "svelte/store";

/// Where accepted transfers are filed while the Receive dialog is open: the
/// folder it was opened from (id null is the root). Null when it is closed, so
/// a transfer accepted from the notification alone lands at the root as before.
export const receiveTarget = writable<{ id: string | null; name?: string } | null>(null);
