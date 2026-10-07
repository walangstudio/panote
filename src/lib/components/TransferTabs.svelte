<script lang="ts" module>
  export type TransferTab = "camera" | "network" | "bluetooth";
</script>

<script lang="ts">
  interface Props {
    tab: TransferTab;
    onselect: (tab: TransferTab) => void;
  }
  let { tab, onselect }: Props = $props();

  const tabs: { id: TransferTab; icon: string; label: string; soon?: boolean }[] = [
    { id: "camera", icon: "photo_camera", label: "Camera" },
    { id: "network", icon: "wifi_tethering", label: "Network" },
    { id: "bluetooth", icon: "bluetooth", label: "Bluetooth", soon: true },
  ];

  /// Arrow keys, Home and End move focus; Enter or Space picks the tab. Picking
  /// on focus alone would start the camera (and its permission prompt) just by
  /// arrowing past it.
  function onKey(e: KeyboardEvent) {
    const buttons = [...(e.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>("[role=tab]")];
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const to =
      e.key === "ArrowRight" ? (at + 1) % buttons.length
      : e.key === "ArrowLeft" ? (at - 1 + buttons.length) % buttons.length
      : e.key === "Home" ? 0
      : e.key === "End" ? buttons.length - 1
      : -1;
    if (to < 0) return;
    e.preventDefault();
    buttons[to]?.focus();
  }
</script>

<div class="tabs" role="tablist" aria-label="How to transfer" tabindex="-1" onkeydown={onKey}>
  {#each tabs as t (t.id)}
    <button
      type="button"
      role="tab"
      class="tab"
      class:active={tab === t.id}
      aria-selected={tab === t.id}
      tabindex={tab === t.id ? 0 : -1}
      onclick={() => onselect(t.id)}
    >
      <span class="material-symbols-outlined" aria-hidden="true">{t.icon}</span>
      {t.label}
      {#if t.soon}<span class="soon">Soon</span>{/if}
    </button>
  {/each}
</div>

<style>
  .tabs {
    display: flex; gap: 0.25rem; padding: 0.25rem; margin: 0 0 1rem;
    background: var(--surface-container); border-radius: var(--radius-full);
  }
  .tab {
    flex: 1; display: flex; align-items: center; justify-content: center; gap: 0.35rem;
    padding: 0.45rem 0.5rem; border: none; border-radius: var(--radius-full);
    background: transparent; color: var(--muted); cursor: pointer;
    font: inherit; font-size: 0.82rem; font-weight: 600;
    transition: background 0.15s ease, color 0.15s ease;
  }
  .tab:hover { color: var(--text); }
  .tab.active { background: var(--accent); color: var(--on-accent); }
  .tab .material-symbols-outlined { font-size: 18px; }
  .soon {
    font-size: 0.62rem; font-weight: 700; text-transform: uppercase; letter-spacing: 0.04em;
    padding: 0.05rem 0.35rem; border-radius: var(--radius-full);
    background: var(--accent-muted); color: var(--accent);
  }
  .tab.active .soon { background: rgba(255, 255, 255, 0.25); color: inherit; }
</style>
