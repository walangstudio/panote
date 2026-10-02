<script lang="ts">
  import { page } from "$app/state";
  import { goto } from "$app/navigation";
  import { isDesktop } from "$lib/stores/layout";
  import { applyFolderParam } from "$lib/stores/listState";
  import NoteListPane from "$lib/components/NoteListPane.svelte";

  $effect(() => {
    if (applyFolderParam(page.url)) void goto("/", { replaceState: true, noScroll: true, keepFocus: true });
  });
</script>

{#if $isDesktop}
  <!-- The list lives in the layout's left pane; this route is the empty detail pane. -->
  <div class="placeholder">
    <span class="material-symbols-outlined">sticky_note_2</span>
    <p>Select a note to read it, or create a new one.</p>
  </div>
{:else}
  <NoteListPane />
{/if}

<style>
  .placeholder {
    height: 100%;
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 0.75rem; color: var(--muted); text-align: center; padding: 2rem;
  }
  .placeholder .material-symbols-outlined { font-size: 56px; opacity: 0.35; }
  .placeholder p { margin: 0; font-size: 0.9rem; font-weight: 500; }
</style>
