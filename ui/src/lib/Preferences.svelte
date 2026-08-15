<script lang="ts">
  import { api, type NotificationSettings } from "./api";
  import {
    loadRemoteContentAllowlist,
    revokeRemoteOrigin,
    revokeRemoteSender,
    type RemoteContentAllowlist,
  } from "./remoteContent";
  import { app, setLayout, setPreferences } from "./stores.svelte";
  import Select from "./Select.svelte";

  let loaded = $state(false);
  let loading = $state(false);
  let saving = $state(false);
  let saved = $state(false);
  let error = $state("");
  let quietHoursEnabled = $state(false);
  let notifications = $state<NotificationSettings>({
    preview: "full",
    folderScope: "all",
    quietHours: null,
  });
  let remoteAllowlist = $state<RemoteContentAllowlist>(loadRemoteContentAllowlist());

  $effect(() => {
    if (app.value.preferencesOpen && !loaded) {
      loaded = true;
      void loadNotifications();
    } else if (!app.value.preferencesOpen) {
      loaded = false;
    }
  });

  async function loadNotifications() {
    loading = true;
    error = "";
    saved = false;
    remoteAllowlist = loadRemoteContentAllowlist();
    try {
      notifications = await api.getNotificationSettings();
      quietHoursEnabled = notifications.quietHours !== null;
    } catch (cause) {
      error = String(cause);
    } finally {
      loading = false;
    }
  }

  async function saveNotifications() {
    saving = true;
    saved = false;
    error = "";
    try {
      await api.updateNotificationSettings({
        ...notifications,
        quietHours: quietHoursEnabled
          ? (notifications.quietHours ?? { start: "22:00", end: "07:00" })
          : null,
      });
      saved = true;
    } catch (cause) {
      error = String(cause);
    } finally {
      saving = false;
    }
  }

  function toggleQuietHours(enabled: boolean) {
    quietHoursEnabled = enabled;
    if (enabled && !notifications.quietHours) {
      notifications.quietHours = { start: "22:00", end: "07:00" };
    }
    saved = false;
  }

  function removeAllowedOrigin(origin: string) {
    remoteAllowlist = revokeRemoteOrigin(remoteAllowlist, origin);
  }

  function removeAllowedSender(sender: string) {
    remoteAllowlist = revokeRemoteSender(remoteAllowlist, sender);
  }
</script>

{#if app.value.preferencesOpen}
  <div class="overlay" role="presentation">
    <dialog
      open
      class="preferences"
      aria-labelledby="preferences-title"
    >
      <header>
        <div>
          <span>Origami</span>
          <h2 id="preferences-title">Preferences</h2>
        </div>
        <button type="button" onclick={() => app.value.preferencesOpen = false} aria-label="Close preferences">×</button>
      </header>

      <section aria-labelledby="appearance-title">
        <h3 id="appearance-title">Appearance</h3>
        <label>
          Theme
          <Select
            value={app.value.theme}
            ariaLabel="Theme"
            options={[
              { value: "system", label: "Follow system" },
              { value: "light", label: "Paper light" },
              { value: "dark", label: "Midnight blue" },
            ]}
            onValueChange={(theme) => setPreferences({ theme: theme as "system" | "light" | "dark" })}
          />
        </label>

        <label>
          Message density
          <Select
            value={app.value.density}
            ariaLabel="Message density"
            options={[
              { value: "comfortable", label: "Comfortable" },
              { value: "compact", label: "Compact" },
            ]}
            onValueChange={(density) => setPreferences({ density: density as "comfortable" | "compact" })}
          />
        </label>

        <label>
          Motion
          <Select
            value={app.value.motion}
            ariaLabel="Motion"
            options={[
              { value: "system", label: "Follow system" },
              { value: "full", label: "Full motion" },
              { value: "reduced", label: "Reduced motion" },
            ]}
            onValueChange={(motion) => setPreferences({ motion: motion as "system" | "full" | "reduced" })}
          />
        </label>

        <label>
          Workspace layout
          <Select
            value={app.value.layout}
            ariaLabel="Workspace layout"
            options={[
              { value: "three-pane", label: "Three-pane" },
              { value: "two-pane", label: "Two-pane" },
              { value: "reading", label: "Reading focus" },
            ]}
            onValueChange={(layout) => setLayout(layout as "three-pane" | "two-pane" | "reading")}
          />
        </label>
      </section>

      <section aria-labelledby="remote-content-title">
        <h3 id="remote-content-title">Remote content</h3>
        <p class="help">Remote images are blocked by default. Saved permissions apply to future messages.</p>
        {#if remoteAllowlist.origins.length === 0 && remoteAllowlist.senders.length === 0}
          <p class="muted">No saved remote-content permissions.</p>
        {:else}
          {#if remoteAllowlist.origins.length > 0}
            <div class="allowlist-group">
              <strong>Allowed sources</strong>
              {#each remoteAllowlist.origins as origin}
                <div class="allowlist-row">
                  <span>{origin}</span>
                  <button type="button" onclick={() => removeAllowedOrigin(origin)}>Remove</button>
                </div>
              {/each}
            </div>
          {/if}
          {#if remoteAllowlist.senders.length > 0}
            <div class="allowlist-group">
              <strong>Allowed senders</strong>
              {#each remoteAllowlist.senders as sender}
                <div class="allowlist-row">
                  <span>{sender}</span>
                  <button type="button" onclick={() => removeAllowedSender(sender)}>Remove</button>
                </div>
              {/each}
            </div>
          {/if}
        {/if}
      </section>

      <section aria-labelledby="notifications-title">
        <h3 id="notifications-title">Notifications</h3>
        {#if loading}
          <p>Loading notification settings…</p>
        {:else}
          <label>
            Preview
            <Select
              bind:value={notifications.preview}
              ariaLabel="Notification preview"
              options={[
                { value: "full", label: "Sender and subject" },
                { value: "sender_only", label: "Sender only" },
                { value: "hidden", label: "Hide message details" },
              ]}
              onValueChange={() => saved = false}
            />
          </label>

          <label>
            Notify for
            <Select
              bind:value={notifications.folderScope}
              ariaLabel="Notification folder scope"
              options={[
                { value: "all", label: "All folders" },
                { value: "inbox", label: "Inbox only" },
              ]}
              onValueChange={() => saved = false}
            />
          </label>

          <label>
            Quiet hours
            <input
              type="checkbox"
              checked={quietHoursEnabled}
              onchange={(event) => toggleQuietHours(event.currentTarget.checked)}
            />
          </label>

          {#if quietHoursEnabled && notifications.quietHours}
            <div class="time-row">
              <label>
                From
                <input type="time" bind:value={notifications.quietHours.start} onchange={() => saved = false} />
              </label>
              <label>
                Until
                <input type="time" bind:value={notifications.quietHours.end} onchange={() => saved = false} />
              </label>
            </div>
          {/if}

          <div class="actions">
            {#if saved}<span role="status">Saved</span>{/if}
            <button type="button" class="primary" disabled={saving} onclick={saveNotifications}>
              {saving ? "Saving…" : "Save notifications"}
            </button>
          </div>
        {/if}
        {#if error}<p class="error" role="alert">{error}</p>{/if}
      </section>
    </dialog>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: grid;
    place-items: center;
    background: var(--overlay);
  }
  .preferences {
    position: static;
    width: min(440px, calc(100vw - 32px));
    max-height: min(720px, calc(100vh - 32px));
    overflow-y: auto;
    margin: 0;
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    display: grid;
    gap: 14px;
    background: var(--bg-raised);
    box-shadow: var(--shadow-panel);
    animation: surface-in var(--transition-med) both;
    color: var(--fg);
  }
  header { display: flex; align-items: flex-start; justify-content: space-between; }
  header span {
    color: var(--accent);
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }
  h2 { margin: 1px 0 0; font-size: 20px; }
  h3 {
    margin: 0;
    color: var(--fg-subtle);
    font-size: 10px;
    font-weight: 800;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }
  header button { width: 28px; height: 28px; border-radius: var(--radius-sm); font-size: 20px; }
  header button:hover { background: var(--bg-sunken); }
  section { display: grid; gap: 12px; padding-top: 4px; }
  section + section { padding-top: 16px; border-top: 1px solid var(--border); }
  label { display: grid; grid-template-columns: 1fr 180px; align-items: center; gap: 16px; font-weight: 650; }
  .help,
  .muted { margin: -4px 0 0; color: var(--fg-muted); font-size: 11px; line-height: 1.45; }
  .allowlist-group { display: grid; gap: 5px; }
  .allowlist-group strong { color: var(--fg-muted); font-size: 11px; }
  .allowlist-row {
    min-width: 0;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--bg-sunken);
    font-size: 11px;
  }
  .allowlist-row span { min-width: 0; flex: 1; overflow-wrap: anywhere; color: var(--fg); }
  .allowlist-row button { flex-shrink: 0; color: var(--danger); font-size: 10px; }
  input[type="time"] {
    width: 100%;
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-sunken);
    color: var(--fg);
    font: inherit;
  }
  input[type="checkbox"] { justify-self: end; width: 18px; height: 18px; accent-color: var(--accent); }
  .time-row { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .time-row label { grid-template-columns: auto 1fr; gap: 8px; font-size: 11px; }
  .actions { display: flex; min-height: 32px; align-items: center; justify-content: flex-end; gap: 12px; }
  .actions span { color: var(--success); font-size: 11px; font-weight: 700; }
  .primary { min-height: 36px; padding: 7px 11px; border-radius: var(--radius-sm); background: var(--accent); color: var(--accent-fg); font-weight: 750; }
  .primary:disabled { opacity: 0.55; }
  p { margin: 2px 0 0; color: var(--fg-subtle); font-size: 11px; }
  .error { color: var(--danger); }
  @media (max-width: 520px) {
    label { grid-template-columns: 1fr; gap: 6px; }
    input[type="checkbox"] { justify-self: start; }
    .time-row { grid-template-columns: 1fr; }
    .time-row label { grid-template-columns: auto 1fr; }
  }
</style>
