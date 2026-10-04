<script lang="ts">
  import { afterNavigate } from "$app/navigation";
  import NoteEditor from "./NoteEditor.svelte";

  // One editor per note. Every navigation builds a fresh one, so nothing from the
  // note being left (unsaved edits, timers, writes still in flight) can reach the
  // next. The one exception is autosave giving a new note its real id: that is
  // the same note, and rebuilding would throw away the cursor mid-sentence.
  let key = $state(0);
  /// The id an adoption is moving to. Spent by the next navigation either way, so
  /// an adoption cut short by a click cannot make a later visit reuse the editor.
  let adopting: string | null = null;

  afterNavigate(({ type, to }) => {
    if (type === "enter") return;
    const adopted = adopting !== null && to?.params?.id === adopting;
    adopting = null;
    if (!adopted) key++;
  });
</script>

{#key key}
  <NoteEditor onadopt={(id) => (adopting = id)} />
{/key}
