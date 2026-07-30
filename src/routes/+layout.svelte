<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    pendingOffersList, type PendingOffer,
    pendingTransfersList, type PendingTransfer,
    isReceiving as checkReceiving, startReceiving, stopReceiving,
  } from "$lib/tauri";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { refreshNotes } from "$lib/stores/notes";
import { refreshFolders } from "$lib/stores/folders";
  import { initTheme } from "$lib/stores/theme";
  import IncomingTransferToast from "$lib/components/IncomingTransferToast.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import NewNoteModal from "$lib/components/NewNoteModal.svelte";
  import NoteListPane from "$lib/components/NoteListPane.svelte";
  import { isDesktop } from "$lib/stores/layout";

  let { children } = $props();
  const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  let offers = $state<PendingOffer[]>([]);
  let transfers = $state<PendingTransfer[]>([]);
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let unlistenOffer: UnlistenFn | null = null;
  let unlistenReceived: UnlistenFn | null = null;
  let unlistenTransfer: UnlistenFn | null = null;
  let unsubTheme: (() => void) | null = null;
  let receiving = $state(false);
  let showNewNote = $state(false);

  function startPoll() {
    if (pollTimer) return;
    pollOffers();
    pollTimer = setInterval(pollOffers, 3000);
  }
  function stopPoll() {
    if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
    offers = [];
    transfers = [];
  }

  async function toggleReceive() {
    try {
      if (receiving) {
        await stopReceiving();
        receiving = false;
        stopPoll();
      } else {
        await startReceiving();
        receiving = true;
        startPoll();
      }
    } catch {}
  }

  /// Accepting a delivered note imports it right there, with no event to say
  /// so — the batch path gets `notes-received`, this one does not. Without the
  /// refresh the note is in the database but absent from the list until the
  /// next restart.
  async function incomingChanged() {
    await pollOffers();
    await refreshNotes({ withBackgrounds: true });
    // An arriving note can bring a folder with it, creating one this device did
    // not have. Without this the note appears but the sidebar still says "No
    // folders yet" until the next launch.
    await refreshFolders();
  }

  /// Both queues, together — an arrival the user has to act on is an arrival
  /// whichever protocol carried it.
  async function pollOffers() {
    const [o, t] = await Promise.all([
      pendingOffersList().catch(() => []),
      pendingTransfersList().catch(() => []),
    ]);
    offers = o;
    transfers = t;
  }

  onMount(async () => {
    unsubTheme = initTheme();
    if (!isTauri) return;
    refreshFolders();
    unlistenOffer = await listen("transfer-offer", () => pollOffers());
    // Nothing listened for this, so a note delivered by the single-note
    // protocol — what older senders still use — sat unreachable in memory and
    // was lost when the app closed.
    unlistenTransfer = await listen("transfer-received", () => pollOffers());
    unlistenReceived = await listen("notes-received", () => {
      pollOffers();
      refreshNotes({ withBackgrounds: true });
      refreshFolders();
    });
    try {
      receiving = await checkReceiving();
      if (receiving) startPoll();
    } catch {}
  });

  onDestroy(() => {
    if (unsubTheme) unsubTheme();
    if (pollTimer) clearInterval(pollTimer);
    if (unlistenOffer) unlistenOffer();
    if (unlistenTransfer) unlistenTransfer();
    if (unlistenReceived) unlistenReceived();
  });
</script>

{#if isTauri}
  <Sidebar {receiving} ontogglereceive={toggleReceive} onnewnote={() => showNewNote = true} />
  {#if $isDesktop}
    <div class="split">
      <aside class="list-pane"><NoteListPane desktop /></aside>
      <main class="detail-pane">{@render children()}</main>
    </div>
  {:else}
    <div class="app-content">
      {@render children()}
    </div>
  {/if}
  <IncomingTransferToast {offers} {transfers} onupdate={incomingChanged} />
  {#if showNewNote}
    <NewNoteModal onclose={() => showNewNote = false} />
  {/if}
{:else}
  <div class="not-tauri">This app must be opened through the Panote desktop or mobile app.</div>
{/if}

<style>
  /* Grid with one row, so the routed page is stretched to the full height rather
     than having to resolve `height: 100%` up through this chain itself. It did
     not: the note editor collapsed to its content and left the bottom half of a
     phone screen as dead space below the Save bar. */
  .app-content {
    height: 100%; overflow-y: auto;
    display: grid; grid-template-rows: 1fr; align-items: stretch;
  }
  /* A grid item defaults to min-height:auto, which lets tall content push the row
     open instead of scrolling inside it. */
  .app-content > :global(*) { min-height: 0; }
  /* Two independent scrollers — the list keeps its place while the note scrolls. */
  .split {
    height: 100%;
    /* Grows with the window instead of staying a thin strip on a 4K display. */
    display: grid; grid-template-columns: clamp(300px, 22%, 400px) minmax(0, 1fr);
  }
  .list-pane {
    min-height: 0; overflow-y: auto;
    border-right: 1px solid var(--border);
    background: var(--surface-glass);
    /* No backdrop-filter here: it would establish a containing block and trap
       every position:fixed modal, popover and backdrop inside this column. */
  }
  .detail-pane { min-width: 0; min-height: 0; overflow-y: auto; }
  .not-tauri {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    color: var(--muted);
    font-size: 0.95rem;
  }
</style>
