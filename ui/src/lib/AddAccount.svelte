<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { app } from "./stores.svelte";
  import OrigamiArtwork from "./OrigamiArtwork.svelte";

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
      step = 1;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function oauthSignIn() {
    if (!oauthProvider) return;
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
      app.value.accounts = [...app.value.accounts, { ...account, hasImap: !!imapHost, hasSmtp: !!smtpHost }];
      step = 2;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function savePassword() {
    if (!password) {
      error = "Enter a password or app-specific token";
      return;
    }
    loading = true;
    error = "";
    try {
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
          password,
          oauthAccessToken: null,
          oauthRefreshToken: null,
        });
      app.value.accounts = [...app.value.accounts, { ...account, hasImap: !!imapHost, hasSmtp: !!smtpHost }];
      step = 2;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function close() {
    open = false;
    step = 0;
    email = "";
    name = "";
    error = "";
  }

  export function show() {
    open = true;
    step = 0;
    error = "";
  }
</script>

{#if open}
  <div class="overlay" role="dialog" aria-modal="true" aria-labelledby="add-account-title">
      <div class="wizard">
       {#if step === 0}
          <OrigamiArtwork variant="setup" theme={app.value.theme} className="wizard-art" />
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
        <h2 id="add-account-title">Configure account</h2>
        {#if description}<p class="provider">{description}</p>{/if}

        <form onsubmit={(e) => e.preventDefault()}>
          <label>Display name <input type="text" bind:value={name} /></label>
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

          {#if oauthProvider}
            <button
              type="button"
              class="oauth"
              onclick={oauthSignIn}
              disabled={loading}
            >
              {loading
                ? "Signing in…"
                : `Sign in with ${oauthProvider === "google" ? "Google" : "Microsoft"}`}
            </button>
            <p class="alt">or</p>
          {/if}

          <label>
            Password / app token
            <input type="password" bind:value={password} />
          </label>

          {#if error}<p class="err">{error}</p>{/if}
          <div class="actions">
            <button type="button" class="cancel" onclick={() => step = 0}>Back</button>
            <button
              type="button"
              class="next"
              onclick={savePassword}
              disabled={loading}
            >
              {loading ? "Saving…" : "Save password"}
            </button>
          </div>
        </form>
       {:else if step === 2}
          <OrigamiArtwork variant="success" theme={app.value.theme} className="wizard-art success-art" />
          <h2 id="add-account-title">Account added!</h2>
        <p>{email} is configured and syncing.</p>
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
    background: linear-gradient(145deg, var(--bg-raised), color-mix(in oklab, var(--bg-raised) 94%, var(--bg-sunken)));
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 28px;
    box-shadow: var(--shadow-float);
    animation: surface-in var(--transition-med) both;
  }
  :global(.wizard-art) { width: 92px; margin: -4px 0 12px auto; }
  :global(.success-art) { width: 128px; margin-inline: auto; }
  h2 { margin: 0 0 6px; font: 400 28px/1.05 var(--font-display); letter-spacing: -0.025em; }
  p { color: var(--fg-muted); font-size: 13px; margin: 0 0 16px; }
  .provider { color: var(--accent); font-weight: 600; }
  p.alt { text-align: center; margin: 12px 0; color: var(--fg-subtle); font-size: 12px; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--fg-muted); margin-bottom: 12px; }
  label input, fieldset input {
    background: color-mix(in oklab, var(--bg-sunken) 78%, var(--bg-raised)); border: 1px solid var(--border);
    border-radius: var(--radius-sm); padding: 8px 10px; color: var(--fg); font: inherit; min-height: 36px;
  }
  fieldset { border: 1px solid var(--paper-crease); border-radius: var(--radius-sm); padding: 12px; margin-bottom: 12px; }
  fieldset legend { font-size: 12px; color: var(--fg-subtle); }
  .err { color: var(--danger); font-size: 12px !important; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
  .cancel { padding: 8px 16px; border-radius: var(--radius-sm); color: var(--fg-muted); }
  .cancel:hover { background: var(--bg-sunken); }
  .next { background: var(--accent); color: var(--accent-fg); padding: 8px 20px; min-height: 36px; border-radius: var(--radius-sm); font-weight: 600; }
  .next:disabled { opacity: 0.6; }
  .next:hover:not(:disabled) { background: var(--accent-hover); }
  .oauth {
    width: 100%; background: color-mix(in oklab, var(--bg-sunken) 78%, var(--bg-raised));
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    padding: 12px; font-weight: 600; font-size: 14px; margin-top: 8px;
  }
  .oauth:hover { background: var(--border); }
  .oauth:disabled { opacity: 0.6; }
  @media (max-width: 430px) {
    :global(.wizard-art) { display: none; }
  }
</style>
