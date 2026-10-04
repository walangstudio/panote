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
  /// The editor asked to be replaced on the next navigation, same URL or not.
  let renew = false;

  // SvelteKit also calls this for the navigation that mounted the route; that
  // editor is already the right one.
  let mounted = false;
  afterNavigate(({ from, to }) => {
    if (!mounted) { mounted = true; return; }
    const adopted = adopting !== null && to?.url.pathname === `/note/${adopting}` && !to.url.search;
    adopting = null;
    // Re-clicking the note already open keeps its editor, unless it asked otherwise.
    const same = from?.url.pathname === to?.url.pathname && from?.url.search === to?.url.search;
    if (!adopted && (!same || renew)) key++;
    renew = false;
  });
</script>

{#key key}
  <NoteEditor onadopt={(id) => (adopting = id)} onrenew={() => (renew = true)} />
{/key}
