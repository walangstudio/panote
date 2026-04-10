<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    pendingOffersList, type PendingOffer,
    isReceiving as checkReceiving, startReceiving, stopReceiving,
  } from "$lib/tauri";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { refreshNotes } from "$lib/stores/notes";
  import { initTheme } from "$lib/stores/theme";
  import IncomingTransferToast from "$lib/components/IncomingTransferToast.svelte";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import NewNoteModal from "$lib/components/NewNoteModal.svelte";

  let { children } = $props();
  const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  let offers = $state<PendingOffer[]>([]);
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let unlistenOffer: UnlistenFn | null = null;
  let unlistenReceived: UnlistenFn | null = null;
  let unsubTheme: (() => void) | null = null;
  let receiving = $state(false);
  let showNewNote = $state(false);

  async function toggleReceive() {
    try {
      if (receiving) {
        await stopReceiving();
        receiving = false;
      } else {
        await startReceiving();
        receiving = true;
      }
    } catch {}
  }

  async function pollOffers() {
    offers = await pendingOffersList().catch(() => []);
  }

  onMount(async () => {
    unsubTheme = initTheme();
    if (!isTauri) return;
    pollOffers();
    pollTimer = setInterval(pollOffers, 3000);
    unlistenOffer = await listen("transfer-offer", () => pollOffers());
    unlistenReceived = await listen("notes-received", () => {
      pollOffers();
      refreshNotes();
    });
    try { receiving = await checkReceiving(); } catch {}
  });

  onDestroy(() => {
    if (unsubTheme) unsubTheme();
    if (pollTimer) clearInterval(pollTimer);
    if (unlistenOffer) unlistenOffer();
    if (unlistenReceived) unlistenReceived();
  });
</script>

{#if isTauri}
  <Sidebar {receiving} ontogglereceive={toggleReceive} onnewnote={() => showNewNote = true} />
  <div class="app-content">
    {@render children()}
  </div>
  <IncomingTransferToast {offers} onupdate={pollOffers} />
  {#if showNewNote}
    <NewNoteModal onclose={() => showNewNote = false} />
  {/if}
{:else}
  <div class="not-tauri">This app must be opened through the Panote desktop or mobile app.</div>
{/if}

<style>
  .app-content {
    height: 100%; overflow-y: auto;
  }
  .not-tauri {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    color: var(--muted);
    font-size: 0.95rem;
  }
</style>
