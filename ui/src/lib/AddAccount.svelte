<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { app, pollAccountErrors, selectFolder } from "./stores.svelte";
  import { api } from "./api";
  import { trapFocus } from "./trapFocus";

  let open = $state(false);
  let step = $state(0); // 0=email, 1=config, 2=done
  let email = $state("");
  let name = $state("");
  let loading = $state(false);
  let error = $state("");

  // Detected provider hints
  let imapHost = $state("");
  let imapPort = $state(993);
  let smtpHost = $state("");
  let smtpPort = $state(465);
  let auth = $state("login");
  let oauthProvider: string | null = $state(null);
  let description = $state("");

  let username = $state("");
  let password = $state("");
  let showAdvanced = $state(false);
  let addedAccountId = $state<string | null>(null);

  const addedStatus = $derived(
    addedAccountId ? app.value.accountStatuses[addedAccountId] : undefined,
  );

  const isGmail = $derived(oauthProvider === "google");
  const isMicrosoft = $derived(oauthProvider === "microsoft");
  const knownProvider = $derived(imapHost.length > 0);
  const appPasswordKind = $derived.by(() => {
    if (isGmail) return "gmail";
    const haystack = `${description} ${email}`.toLowerCase();
    if (haystack.includes("icloud") || haystack.includes("apple")) return "icloud";
    if (haystack.includes("yahoo")) return "yahoo";
    if (haystack.includes("fastmail")) return "fastmail";
    return null;
  });

  async function detect() {
    if (!email.includes("@")) {
      error = "Enter a valid email address";
      return;
    }
    loading = true;
    error = "";
    try {
      const hints: {
        description: string | null;
        imapHost: string | null;
        imapPort: number | null;
        smtpHost: string | null;
        smtpPort: number | null;
        auth: string;
        oauthProvider: string | null;
      } = await invoke("provider_hints", { email });
      description = hints.description ?? "";
      imapHost = hints.imapHost ?? "";
      imapPort = hints.imapPort ?? 993;
      smtpHost = hints.smtpHost ?? "";
      smtpPort = hints.smtpPort ?? 465;
      auth = hints.auth;
      oauthProvider = hints.oauthProvider;
      name = email.split("@")[0] ?? "personal";
      username = email;
      showAdvanced = !imapHost;
      step = 1;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function oauthSignIn() {
    if (oauthProvider !== "microsoft") return;
    loading = true;
    error = "";
    try {
      const oauthConfig: { clientId: string; clientSecret: string | null } =
        await invoke("oauth_client_id", { provider: oauthProvider });
      const tokens = await invoke("oauth_sign_in", {
        provider: oauthProvider,
        clientId: oauthConfig.clientId,
        clientSecret: oauthConfig.clientSecret,
      });
      // Tauri may return snake_case or camelCase depending on features;
      // handle both so the wizard never silently passes undefined.
      const accessToken: string = (tokens as Record<string, unknown>).access_token as string
        ?? (tokens as Record<string, unknown>).accessToken as string;
      const refreshToken: string | null = ((tokens as Record<string, unknown>).refresh_token as string
        ?? (tokens as Record<string, unknown>).refreshToken as string) || null;
      if (!accessToken) {
        error = "OAuth flow returned no access token";
        loading = false;
        return;
      }
      // Save account with OAuth tokens.
      const account: { id: string; dbId: string; name: string; email: string } =
        await invoke("add_account", {
        accountId: name,
        name,
        email,
        imapHost: imapHost || undefined,
        imapPort: imapHost ? imapPort : null,
        smtpHost: smtpHost || undefined,
        smtpPort: smtpHost ? smtpPort : null,
        auth,
        username,
        password: null,
        oauthAccessToken: accessToken,
        oauthRefreshToken: refreshToken,
      });
      await finishAddedAccount(account);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function savePassword() {
    if (!password) {
      error = appPasswordKind === "gmail"
        ? "Enter a Gmail app password"
        : appPasswordKind
          ? "Enter an app-specific password"
          : "Enter a password or app-specific token";
      return;
    }
    loading = true;
    error = "";
    try {
      const passwordAuth = isGmail || appPasswordKind ? "login" : auth;
      const account: { id: string; dbId: string; name: string; email: string } =
        await invoke("add_account", {
          accountId: name,
          name,
          email,
          imapHost: imapHost || undefined,
          imapPort: imapHost ? imapPort : null,
          smtpHost: smtpHost || undefined,
          smtpPort: smtpHost ? smtpPort : null,
          auth: passwordAuth,
          username,
          password,
          oauthAccessToken: null,
          oauthRefreshToken: null,
        });
      await finishAddedAccount(account);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function finishAddedAccount(account: {
    id: string;
    dbId: string;
    name: string;
    email: string;
  }) {
    app.value.accounts = [...app.value.accounts, { ...account, hasImap: !!imapHost, hasSmtp: !!smtpHost }];
    addedAccountId = account.id;
    step = 2;
    try {
      const folders = await api.listFolders();
      app.value.folders = folders;
      const inbox = folders.find((folder) => folder.accountId === account.dbId && folder.role === "Inbox");
      if (inbox) await selectFolder(inbox.id);
      await pollAccountErrors();
    } catch {
      // Folders and status refresh after first sync; the account is already saved.
    }
  }

  function close() {
    open = false;
    step = 0;
    email = "";
    name = "";
    error = "";
    password = "";
    showAdvanced = false;
    addedAccountId = null;
  }

  export function show() {
    open = true;
    step = 0;
    error = "";
  }
</script>

{#if open}
  <div class="overlay" role="dialog" aria-modal="true" aria-labelledby="add-account-title" use:trapFocus={{ onEscape: close }}>
    <div class="wizard">
      {#if step === 0}
        <h2 id="add-account-title">Add an email account</h2>
        <p>Origami will look up the best configuration for your provider.</p>
        <form onsubmit={(e) => { e.preventDefault(); detect(); }}>
          <label>
            Email address
            <input
              type="email"
              bind:value={email}
              placeholder="you@example.com"
            />
          </label>
          {#if error}<p class="err">{error}</p>{/if}
          <div class="actions">
            <button type="button" class="cancel" onclick={close}>Cancel</button>
            <button type="submit" class="next" disabled={loading}>
              {loading ? "Looking up…" : "Next"}
            </button>
          </div>
        </form>
      {:else if step === 1}
        <h2 id="add-account-title">
          {#if isGmail}Connect Gmail{:else}Configure account{/if}
        </h2>
        {#if description}<p class="provider">{description}</p>{/if}

        <form onsubmit={(e) => e.preventDefault()}>
          <label>Display name <input type="text" bind:value={name} /></label>

          {#if isMicrosoft}
            <button
              type="button"
              class="oauth"
              onclick={oauthSignIn}
              disabled={loading}
            >
              {loading ? "Signing in…" : "Sign in with Microsoft"}
            </button>
            <p class="alt">or use a password</p>
          {/if}

          {#if appPasswordKind === "gmail"}
            <p class="hint">
              Gmail needs an app password, not your regular password. Turn on
              2-Step Verification, then create a 16-character app password at
              <button
                type="button"
                class="link"
                onclick={() => openUrl("https://myaccount.google.com/apppasswords")}
              >
                myaccount.google.com/apppasswords
              </button>.
            </p>
            <label>
              App password
              <input type="password" bind:value={password} autocomplete="off" />
            </label>
          {:else if appPasswordKind === "icloud"}
            <p class="hint">iCloud requires an app-specific password from appleid.apple.com.</p>
            <label>
              App-specific password
              <input type="password" bind:value={password} autocomplete="off" />
            </label>
          {:else if appPasswordKind === "yahoo"}
            <p class="hint">Yahoo requires an app password generated in account security settings.</p>
            <label>
              App password
              <input type="password" bind:value={password} autocomplete="off" />
            </label>
          {:else if appPasswordKind === "fastmail"}
            <p class="hint">Fastmail requires an app password from Privacy &amp; Security settings.</p>
            <label>
              App password
              <input type="password" bind:value={password} autocomplete="off" />
            </label>
          {:else}
            <label>
              Password / app token
              <input type="password" bind:value={password} />
            </label>
          {/if}

          {#if knownProvider}
            <button
              type="button"
              class="advanced"
              aria-expanded={showAdvanced}
              onclick={() => showAdvanced = !showAdvanced}
            >
              Advanced
            </button>
          {/if}

          {#if showAdvanced || !knownProvider}
            <label>Username <input type="text" bind:value={username} /></label>
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
          {/if}

          {#if error}<p class="err">{error}</p>{/if}
          <div class="actions">
            <button type="button" class="cancel" onclick={() => step = 0}>Back</button>
            <button
              type="button"
              class="next"
              onclick={savePassword}
              disabled={loading}
            >
              {loading ? "Verifying…" : appPasswordKind ? "Save app password" : "Save password"}
            </button>
          </div>
        </form>
      {:else if step === 2}
        <h2 id="add-account-title">Account added</h2>
        {#if addedStatus?.state === "syncing"}
          <p>Syncing Inbox…</p>
        {:else if addedStatus?.state === "online"}
          <p>{email} is connected.</p>
        {:else}
          <p>{email} is connected. Origami is syncing Inbox in the background.</p>
        {/if}
        <div class="actions">
          <button type="button" class="next" onclick={close}>Done</button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed; inset: 0;
    background: var(--overlay);
    display: grid; place-items: center; z-index: 50;
  }
  .wizard {
    width: min(520px, 90vw);
    max-height: 85vh; overflow-y: auto;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 28px;
    box-shadow: var(--shadow-panel);
    animation: surface-in var(--transition-med) both;
  }
  h2 { margin: 0 0 4px; font-size: 18px; }
  p { color: var(--fg-muted); font-size: 13px; margin: 0 0 16px; }
  .provider { color: var(--accent); font-weight: 600; }
  p.alt { text-align: center; margin: 12px 0; color: var(--fg-subtle); font-size: 12px; }
  p.hint { color: var(--fg-muted); font-size: 12px; margin: -4px 0 12px; line-height: 1.4; }
  button.link {
    display: inline; padding: 0; border: 0; background: none;
    color: var(--accent); font: inherit; text-decoration: underline; cursor: pointer;
  }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--fg-muted); margin-bottom: 12px; }
  label input, fieldset input {
    background: var(--bg-sunken); border: 1px solid var(--border);
    border-radius: var(--radius-sm); padding: 8px 10px; color: var(--fg); font: inherit; min-height: 36px;
  }
  fieldset { border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 12px; margin-bottom: 12px; }
  fieldset legend { font-size: 12px; color: var(--fg-subtle); }
  .err { color: var(--danger); font-size: 12px !important; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
  .cancel { padding: 8px 16px; border-radius: var(--radius-sm); color: var(--fg-muted); }
  .cancel:hover { background: var(--bg-sunken); }
  .next { background: var(--accent); color: var(--accent-fg); padding: 8px 20px; min-height: 36px; border-radius: var(--radius-sm); font-weight: 600; }
  .next:disabled { opacity: 0.6; }
  .next:hover:not(:disabled) { background: var(--accent-hover); }
  .oauth {
    width: 100%; background: var(--bg-sunken);
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    padding: 12px; font-weight: 600; font-size: 14px; margin-top: 8px;
  }
  .oauth:hover { background: var(--border); }
  .oauth:disabled { opacity: 0.6; }
  .advanced {
    background: none; border: 0; padding: 0; margin: 0 0 12px;
    color: var(--accent); font: inherit; font-size: 12px; cursor: pointer;
  }
  .advanced:hover { text-decoration: underline; }
  @media (max-width: 430px) {
    :global(.wizard-art) { display: none; }
  }
</style>

