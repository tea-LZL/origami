<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import Titlebar from "./lib/Titlebar.svelte";
  import OrigamiBird from "./lib/OrigamiBird.svelte";

  interface AppInfo {
    name: string;
    coreVersion: string;
    shellVersion: string;
  }

  let info: AppInfo | null = $state(null);

  $effect(() => {
    invoke<AppInfo>("app_info").then((i) => (info = i));
  });
</script>

<div class="shell">
  <Titlebar />

  <main class="hero">
    <OrigamiBird size={120} />
    <h1>Origami</h1>
    <p class="tagline">Folded paper, delivered fast.</p>

    <div class="status">
      {#if info}
        <div class="row"><span>Shell</span><code>v{info.shellVersion}</code></div>
        <div class="row"><span>Core</span><code>v{info.coreVersion}</code></div>
        <div class="row ok"><span>IPC</span><code>connected</code></div>
      {:else}
        <div class="row"><span>IPC</span><code>connecting…</code></div>
      {/if}
      <div class="row"><span>Milestone</span><code>M0 — scaffold</code></div>
    </div>
  </main>
</div>

<style>
  .shell {
    height: 100%;
    display: flex;
    flex-direction: column;
  }

  .hero {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }

  h1 {
    margin: 8px 0 0;
    font-size: 28px;
    font-weight: 800;
    letter-spacing: 0.02em;
  }

  .tagline {
    margin: 0;
    color: var(--fg-muted);
  }

  .status {
    margin-top: 28px;
    min-width: 260px;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .row {
    display: flex;
    justify-content: space-between;
    gap: 24px;
    color: var(--fg-muted);
  }

  .row code {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--fg);
  }

  .row.ok code {
    color: #3fb950;
  }
</style>
