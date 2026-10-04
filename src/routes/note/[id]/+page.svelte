<script lang="ts">
  import { page } from "$app/state";
  import { untrack } from "svelte";
  import NoteEditor, { adoption } from "./NoteEditor.svelte";

  // One editor per note. Switching notes builds a fresh one, so nothing from the
  // note being left (unsaved edits, timers, writes still in flight) can reach the
  // next. Autosave giving a new note its real id is the same note: it keeps the editor.
  const target = $derived(
    page.params.id === "new"
      ? `new?${page.url.searchParams.get("kind")}&${page.url.searchParams.get("folder")}`
      : (page.params.id ?? ""),
  );
  let key = $state(0);
  let shown = untrack(() => target);

  $effect(() => {
    const next = target;
    untrack(() => {
      if (next === shown) return;
      if (next === adoption.id) adoption.id = null;
      else key++;
      shown = next;
    });
  });
</script>

{#key key}
  <NoteEditor />
{/key}
