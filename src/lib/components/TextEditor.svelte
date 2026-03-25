<script lang="ts">
  let { content = $bindable({ body: "" }) } = $props<{ content: { body: string } }>();
</script>

<textarea
  class="editor"
  placeholder="Start typing…"
  bind:value={content.body}
></textarea>

<style>
  .editor {
    display: block; width: 100%; height: 100%;
    padding: 1.5rem; font-size: 1rem;
    line-height: 1.7; color: var(--text);
    border: none; resize: none; outline: none;
    background: var(--bg);
    font-family: inherit;
    box-sizing: border-box;
  }
</style>
