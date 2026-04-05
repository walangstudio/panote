<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { pendingOffersList, type PendingOffer } from "$lib/tauri";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { refreshNotes } from "$lib/stores/notes";
  import IncomingTransferToast from "$lib/components/IncomingTransferToast.svelte";

  let { children } = $props();
  const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  let offers = $state<PendingOffer[]>([]);
  let pollTimer: ReturnType<typeof setInterval> | null = null;
  let unlistenOffer: UnlistenFn | null = null;
  let unlistenReceived: UnlistenFn | null = null;

  async function pollOffers() {
    offers = await pendingOffersList().catch(() => []);
  }

  onMount(async () => {
    if (!isTauri) return;
    pollOffers();
    pollTimer = setInterval(pollOffers, 3000);
    unlistenOffer = await listen("transfer-offer", () => pollOffers());
    unlistenReceived = await listen("notes-received", () => {
      pollOffers();
      refreshNotes();
    });
  });

  onDestroy(() => {
    if (pollTimer) clearInterval(pollTimer);
    if (unlistenOffer) unlistenOffer();
    if (unlistenReceived) unlistenReceived();
  });
</script>

{#if isTauri}
  {@render children()}
  <IncomingTransferToast {offers} onupdate={pollOffers} />
{:else}
  <div class="not-tauri">This app must be opened through the Panote desktop or mobile app.</div>
{/if}

<style>
  .not-tauri {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    color: var(--muted);
    font-size: 0.95rem;
  }
</style>
