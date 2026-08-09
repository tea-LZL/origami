<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { api } from "./api";
  import { app } from "./stores.svelte";
  import Select from "./Select.svelte";

  let { open, onclose, accountId }: { open: boolean; onclose: () => void; accountId: string } = $props();
  let loading = $state(false);
  let error = $state("");
  let name = $state("");
  let email = $state("");

  let imapHost = $state("");
  let imapPort = $state(993);
  let smtpHost = $state("");
  let smtpPort = $state(465);
  let username = $state("");
  let password = $state("");
  let auth = $state("login");
  let oauthProvider = $state<"google" | "microsoft" | null>(null);
  let reconnecting = $state(false);
  let loadGeneration = 0;

  $effect(() => {
    if (!open || !accountId) return;
    const generation = ++loadGeneration;
    loading = true;
    error = "";
    invoke<{
      name: string; email: string; imapHost: string | null; imapPort: number | null;
      smtpHost: string | null; smtpPort: number | null; username: string | null;
      auth: string | null; oauthProvider: "google" | "microsoft" | null;
    }>("get_account_settings", { accountId }).then((settings) => {
      if (generation !== loadGeneration) return;
      name = settings.name;
      email = settings.email;
      imapHost = settings.imapHost ?? "";
      imapPort = settings.imapPort ?? 993;
      smtpHost = settings.smtpHost ?? "";
      smtpPort = settings.smtpPort ?? 465;
      username = settings.username ?? "";
      auth = settings.auth ?? "login";
      oauthProvider = settings.oauthProvider;
      password = "";
    }).catch((reason) => {
      if (generation === loadGeneration) error = String(reason);
    }).finally(() => {
      if (generation === loadGeneration) loading = false;
    });
  });

  function close() { onclose(); }

  async function save() {
    loading = true; error = "";
    try {
      await invoke("update_account", {
        accountId,
        name: name || undefined,
        email: email || undefined,
        imapHost: imapHost || undefined,
        imapPort,
        smtpHost: smtpHost || undefined,
        smtpPort,
        auth: auth || undefined,
        username: username || undefined,
        password: password || undefined,
      });
      app.value.accounts = await api.listAccounts();
      onclose();
    } catch (e) { error = String(e); }
    finally { loading = false; }
  }

  async function reconnectOAuth() {
    if (!oauthProvider) return;
    reconnecting = true;
    error = "";
    try {
      const oauthConfig: { clientId: string; clientSecret: string | null } =
        await invoke("oauth_client_id", { provider: oauthProvider });
      const tokens = await invoke("oauth_sign_in", {
        provider: oauthProvider,
        clientId: oauthConfig.clientId,
        clientSecret: oauthConfig.clientSecret,
      }) as Record<string, unknown>;
      const accessToken = (tokens.access_token ?? tokens.accessToken) as string | undefined;
      const refreshToken = (tokens.refresh_token ?? tokens.refreshToken) as string | undefined;
      if (!accessToken || !refreshToken) {
        throw new Error("OAuth reauthentication returned incomplete credentials");
      }
      await api.updateAccount({
        accountId,
        oauthAccessToken: accessToken,
        oauthRefreshToken: refreshToken,
      });
      app.value.accountErrors = Object.fromEntries(
        Object.entries(app.value.accountErrors).filter(([id]) => id !== accountId),
      );
      onclose();
    } catch (cause) {
      error = String(cause);
    } finally {
      reconnecting = false;
    }
  }
</script>

{#if open}
  <div class="overlay" role="dialog" aria-modal="true">
    <div class="dialog">
      <h2>Account settings</h2>
      <form onsubmit={(e) => { e.preventDefault(); save(); }}>
        <label>Account name <input type="text" bind:value={name} /></label>
        <label>Email address <input type="email" bind:value={email} /></label>
        <fieldset>
          <legend>IMAP</legend>
          <label>Host <input type="text" bind:value={imapHost} /></label>
          <label>Port <input type="number" bind:value={imapPort} /></label>
        </fieldset>
        <fieldset>
          <legend>SMTP</legend>
          <label>Host <input type="text" bind:value={smtpHost} /></label>
          <label>Port <input type="number" bind:value={smtpPort} /></label>
        </fieldset>
        <label>Username <input type="text" bind:value={username} /></label>
        {#if oauthProvider}
          <div class="oauth-reconnect">
            <div>
              <strong>{oauthProvider === "google" ? "Google" : "Microsoft"} OAuth</strong>
              <span>Renew revoked or expired account access.</span>
            </div>
            <button type="button" disabled={reconnecting || loading} onclick={reconnectOAuth}>
              {reconnecting ? "Connecting…" : `Reconnect ${oauthProvider === "google" ? "Google" : "Microsoft"}`}
            </button>
          </div>
        {:else}
          <label>New password (leave blank to keep current)
            <input type="password" bind:value={password} />
          </label>
        {/if}
        <label>Auth
          <Select
            bind:value={auth}
            ariaLabel="Authentication method"
            options={[
              { value: "login", label: "Login" },
              { value: "plain", label: "Plain" },
              { value: "xoauth2", label: "XOAUTH2" },
              { value: "oauthbearer", label: "OAuth Bearer" },
            ]}
          />
        </label>
        {#if error}<p class="err">{error}</p>{/if}
        <div class="actions">
          <button type="button" onclick={close}>Cancel</button>
          <button type="submit" disabled={loading}>
            {loading ? "Saving…" : "Save & reconnect"}
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0,0,0,0.4);
    display: grid; place-items: center; z-index: 80;
  }
  .dialog {
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 24px;
    max-width: 440px;
    width: 90vw;
    box-shadow: 0 12px 48px rgba(0,0,0,0.35);
  }
  h2 { margin: 0 0 16px; font-size: 16px; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--fg-muted); margin-bottom: 12px; }
  label input {
    background: var(--bg-sunken);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 8px 10px;
    color: var(--fg);
    font: inherit;
  }
  fieldset { border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 12px; margin-bottom: 12px; }
  fieldset legend { font-size: 12px; color: var(--fg-subtle); }
  .oauth-reconnect {
    display: flex; align-items: center; justify-content: space-between; gap: 16px;
    margin-bottom: 12px; padding: 12px; border: 1px solid var(--border);
    border-radius: var(--radius-sm); background: var(--bg-sunken);
  }
  .oauth-reconnect div { display: grid; gap: 3px; }
  .oauth-reconnect strong { font-size: 12px; color: var(--fg); }
  .oauth-reconnect span { font-size: 10px; color: var(--fg-subtle); }
  .oauth-reconnect button { flex: none; padding: 7px 10px; border-radius: var(--radius-sm); background: var(--accent); color: var(--accent-fg); font-weight: 650; }
  .oauth-reconnect button:disabled { opacity: 0.6; }
  .err { color: var(--danger); font-size: 12px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 12px; }
  .actions button { padding: 6px 16px; border-radius: var(--radius-sm); font-size: 13px; }
  .actions button:first-child { color: var(--fg-muted); }
  .actions button:last-child { background: var(--accent); color: var(--accent-fg); font-weight: 600; }
  .actions button:last-child:disabled { opacity: 0.6; }
</style>
