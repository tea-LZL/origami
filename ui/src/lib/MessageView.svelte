<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import {
    app,
    clearMessageView,
    moveSelectedToRole,
    openReplyComposer,
    selectEnvelopeExclusive,
    setLayout,
    setSelectedFlag,
  } from "./stores.svelte";
  import { api } from "./api";
  import { sanitizeMessageHtml } from "./messageHtml";
  import {
    allowRemoteOrigins,
    allowRemoteSender,
    loadRemoteContentAllowlist,
    normalizeRemoteSender,
    REMOTE_CONTENT_POLICY_EVENT,
  } from "./remoteContent";
  import { threadMembers } from "./threads";
  import { messageBodyKeyboard } from "./navigation";

  let contentMode = $state<"html" | "text">("html");
  let detailsOpen = $state(false);
  let lastMessageId = $state<string | null>(null);
  let remoteContentAllowed = $state(false);
  let allowedRemoteOrigins = $state<string[]>([]);
  let senderRemoteContentAllowed = $state(false);
  let frameResizeObserver: ResizeObserver | null = null;

  function senderAddress(): string {
    return normalizeRemoteSender(app.value.message?.envelope.from[0]?.addr ?? "") ?? "";
  }

  function refreshRemoteAllowlist() {
    const allowlist = loadRemoteContentAllowlist();
    allowedRemoteOrigins = allowlist.origins;
    const sender = senderAddress();
    senderRemoteContentAllowed = Boolean(sender && allowlist.senders.includes(sender));
  }

  $effect(() => {
    const messageId = app.value.message?.envelope.id ?? null;
    if (messageId === lastMessageId) return;
    lastMessageId = messageId;
    contentMode = "html";
    detailsOpen = false;
    remoteContentAllowed = false;
    refreshRemoteAllowlist();
  });

  $effect(() => {
    if (typeof window === "undefined") return;
    window.addEventListener(REMOTE_CONTENT_POLICY_EVENT, refreshRemoteAllowlist);
    return () => window.removeEventListener(REMOTE_CONTENT_POLICY_EVENT, refreshRemoteAllowlist);
  });

  const safeHtml = $derived.by(() => {
    const html = app.value.message?.html;
    return html ? sanitizeMessageHtml(html, {
      allowRemoteImages: remoteContentAllowed || senderRemoteContentAllowed,
      allowedOrigins: new Set(allowedRemoteOrigins),
    }) : {
      html: "",
      srcdoc: "",
      blockedResources: 0,
      blockedHosts: [],
      blockedOrigins: [],
    };
  });

  const safeText = $derived(app.value.message?.text ?? "");
  const conversation = $derived(threadMembers(app.value.envelopes, app.value.selectedEnvelope));

  function openExternalLink(event: MouseEvent) {
    const target = event.target;
    if (!target || typeof (target as Element).closest !== "function") return;
    const anchor = (target as Element).closest("a");
    if (!anchor) return;

    event.preventDefault();
    const href = anchor.getAttribute("href");
    if (!href) return;

    try {
      const url = new URL(href);
      if (!["http:", "https:", "mailto:"].includes(url.protocol)) {
        throw new Error(`Unsupported link type: ${url.protocol}`);
      }
      openUrl(url.href).catch((error) => {
        app.value.lastError = `Could not open link: ${String(error)}`;
      });
    } catch (error) {
      app.value.lastError = `Could not open link: ${String(error)}`;
    }
  }

  function bindEmailFrame(event: Event) {
    frameResizeObserver?.disconnect();
    const frame = event.currentTarget;
    if (!(frame instanceof HTMLIFrameElement)) return;
    const document = frame.contentDocument;
    if (!document) return;

    document.addEventListener("click", openExternalLink);
    const resize = () => {
      const height = Math.max(
        document.documentElement.scrollHeight,
        document.body?.scrollHeight ?? 0,
      );
      frame.style.height = `${Math.max(180, height)}px`;
    };
    frameResizeObserver = typeof ResizeObserver === "undefined"
      ? null
      : new ResizeObserver(resize);
    frameResizeObserver?.observe(document.documentElement);
    requestAnimationFrame(resize);
  }

  $effect(() => () => frameResizeObserver?.disconnect());

  function fromText(): string {
    return app.value.message?.envelope.from
      .map((a) => a.name ?? a.addr)
      .join(", ") ?? "";
  }

  function fromAddressText(): string {
    return app.value.message?.envelope.from
      .map((a) => a.addr)
      .join(", ") ?? "";
  }

  function addressText(addresses: { name: string | null; addr: string }[]): string {
    return addresses.map((address) => address.name
      ? `${address.name} <${address.addr}>`
      : address.addr).join(", ");
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function backToMessages() {
    clearMessageView();
    if (app.value.layout === "reading") setLayout("two-pane");
  }

  function loadRemoteContentOnce() {
    remoteContentAllowed = true;
  }

  function alwaysAllowBlockedSources() {
    const next = allowRemoteOrigins(loadRemoteContentAllowlist(), safeHtml.blockedOrigins);
    allowedRemoteOrigins = next.origins;
  }

  function alwaysAllowSender() {
    const sender = senderAddress();
    if (!sender) return;
    const next = allowRemoteSender(loadRemoteContentAllowlist(), sender);
    senderRemoteContentAllowed = next.senders.includes(sender);
  }

  async function downloadAttachment(index: number, name: string | null) {
    const msg = app.value.message;
    const attachment = msg?.attachments.find((item) => item.index === index);
    if (!msg || !attachment) return;
    try {
      const b64 = await api.getAttachment(
        msg.envelope.mailboxId,
        msg.envelope.serverUid!,
        index,
      );
      const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
      const blob = new Blob([bytes], { type: attachment.mime });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = name ?? `attachment-${index}`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (error) {
      app.value.lastError = `Could not download attachment: ${String(error)}`;
    }
  }
</script>

<section class="message">
  {#if app.value.messageLoading}
    <div class="message-loading" role="status" aria-live="polite">
      <span class="sr-only">Loading message</span>
      <div class="message-skeleton" aria-hidden="true">
        <span class="skeleton skeleton-title"></span>
        <span class="skeleton skeleton-meta"></span>
        <span class="skeleton skeleton-rule"></span>
        <span class="skeleton skeleton-copy wide"></span>
        <span class="skeleton skeleton-copy"></span>
        <span class="skeleton skeleton-copy short"></span>
      </div>
    </div>
  {:else if !app.value.message}
    <div class="empty">Select a message</div>
  {:else}
    {@const env = app.value.message.envelope}
    {@const headers = app.value.message.headers}
    <header>
      {#if app.value.layout !== "three-pane" || app.value.selectedEnvelope}
        <button type="button" class="back-to-messages" onclick={backToMessages}>Back to messages</button>
      {/if}
      <div class="subject-row">
        <h1>{env.subject || "(no subject)"}</h1>
        <div class="message-actions" role="toolbar" aria-label="Message actions">
          {#if app.value.message.html && app.value.message.text}
            <div class="content-mode" role="group" aria-label="Message format">
              <button type="button" class:active={contentMode === "html"} onclick={() => contentMode = "html"}>HTML</button>
              <button type="button" class:active={contentMode === "text"} onclick={() => contentMode = "text"}>Text</button>
            </div>
          {/if}
          <button type="button" class:active={detailsOpen} aria-expanded={detailsOpen} onclick={() => detailsOpen = !detailsOpen}>Details</button>
          <button type="button" onclick={() => openReplyComposer("reply")}>Reply</button>
          <button type="button" onclick={() => openReplyComposer("replyAll")}>Reply all</button>
          <button type="button" onclick={() => openReplyComposer("forward")}>Forward</button>
          <button type="button" onclick={() => moveSelectedToRole("Archive")}>Archive</button>
          <button type="button" onclick={() => moveSelectedToRole("Trash")}>Trash</button>
          <button type="button" onclick={() => moveSelectedToRole("Junk")}>Junk</button>
          <button type="button" onclick={() => setSelectedFlag("Flagged", !env.flags.includes("Flagged"))}>
            {env.flags.includes("Flagged") ? "Unstar" : "Star"}
          </button>
        </div>
      </div>
      <div class="meta">
        <span class="from">{fromText()}</span>
        {#if fromAddressText()}<span class="address" title="From address">{fromAddressText()}</span>{/if}
        <span class="date">{env.date ?? ""}</span>
      </div>
      {#if detailsOpen}
        <dl class="details">
          <div><dt>From</dt><dd>{fromText()}</dd></div>
          {#if headers.sender.length > 0}<div><dt>Sender</dt><dd>{addressText(headers.sender)}</dd></div>{/if}
          <div><dt>To</dt><dd>{addressText(env.to)}</dd></div>
          {#if headers.cc.length > 0}<div><dt>Cc</dt><dd>{addressText(headers.cc)}</dd></div>{/if}
          {#if headers.replyTo.length > 0}<div><dt>Reply-To</dt><dd>{addressText(headers.replyTo)}</dd></div>{/if}
          {#if headers.messageId}<div><dt>Message-ID</dt><dd>{headers.messageId}</dd></div>{/if}
          {#if headers.authenticationResults}<div><dt>Authentication</dt><dd>{headers.authenticationResults}</dd></div>{/if}
          {#if headers.listUnsubscribe.length > 0}<div><dt>Unsubscribe</dt><dd>{headers.listUnsubscribe.join(" ")}</dd></div>{/if}
        </dl>
      {/if}
      {#if conversation.length > 1}
        <nav class="conversation" aria-label="Conversation messages">
          <span>{conversation.length} messages</span>
          {#each conversation as member (member.id)}
            <button
              type="button"
              class:current={member.id === env.id}
              onclick={() => selectEnvelopeExclusive(member)}
              title={member.subject}
            >
              {member.from[0]?.name ?? member.from[0]?.addr ?? "Unknown"}
            </button>
          {/each}
        </nav>
      {/if}
    </header>

    {#if safeHtml.blockedResources > 0}
      <div class="remote-bar" role="status">
        <strong>Remote content blocked</strong>
        <span>{safeHtml.blockedResources} image{safeHtml.blockedResources === 1 ? "" : "s"} from {safeHtml.blockedHosts.join(", ") || "unknown sources"}</span>
        <div class="remote-actions">
          <button type="button" onclick={loadRemoteContentOnce}>Load images once</button>
          {#if safeHtml.blockedOrigins.length > 0}
            <button type="button" class="risk" onclick={alwaysAllowBlockedSources}>Always allow these sources</button>
          {/if}
          {#if senderAddress()}
            <button type="button" onclick={alwaysAllowSender}>Always allow this sender</button>
          {/if}
        </div>
      </div>
    {/if}
    {#if app.value.message.parseWarnings.length > 0}
      <div class="parse-warning" role="status">Some MIME parts could not be decoded completely.</div>
    {/if}

    <div class="body" data-navigation="message-body" tabindex="-1" aria-label="Message body" use:messageBodyKeyboard>
      {#if safeHtml.html && (contentMode === "html" || !safeText)}
        <iframe
          class="email-frame"
          title="Email HTML body"
          sandbox="allow-same-origin"
          referrerpolicy="no-referrer"
          srcdoc={safeHtml.srcdoc}
          onload={bindEmailFrame}
        ></iframe>
      {:else if safeText}
        <pre class="text">{safeText}</pre>
      {:else}
        <p class="empty">This message has no displayable body.</p>
      {/if}
    </div>

    {#if app.value.message.attachments.length > 0}
      <footer>
        <h2>Attachments</h2>
        <ul>
          {#each app.value.message.attachments as att (att.index)}
            <li>
              <button type="button" onclick={() => downloadAttachment(att.index, att.name)}>
                {att.name ?? `attachment-${att.index}`}
                <small>{att.mime} · {formatBytes(att.size)}{att.inline ? " · inline" : ""}</small>
              </button>
            </li>
          {/each}
        </ul>
      </footer>
    {/if}
  {/if}
</section>

<style>
  .message {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg-raised);
    overflow: hidden;
    padding: 24px 32px;
  }
  .message > header,
  .message > footer {
    flex-shrink: 0;
    width: 100%;
    max-width: 760px;
    margin-inline: auto;
  }
  .message > .body {
    flex: 1;
    min-width: 0;
    min-height: 0;
    width: 100%;
    overflow-y: auto;
    scrollbar-gutter: stable;
  }
  .message-loading {
    width: 100%;
    max-width: 760px;
    margin-inline: auto;
    padding-top: 8px;
  }
  .message-skeleton { display: grid; gap: 12px; }
  .message-skeleton .skeleton { display: block; border-radius: 4px; }
  .skeleton-title { width: min(72%, 520px); height: 22px; }
  .skeleton-meta { width: min(46%, 340px); height: 10px; }
  .skeleton-rule { width: 100%; height: 1px; margin: 8px 0 14px; border: 0; }
  .skeleton-copy { width: min(76%, 560px); height: 10px; }
  .skeleton-copy.wide { width: min(94%, 700px); }
  .skeleton-copy.short { width: min(52%, 380px); }
  .back-to-messages {
    display: none;
    margin-bottom: 10px;
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--fg-muted);
    font-size: 11px;
    font-weight: 650;
  }
  .back-to-messages:hover { border-color: var(--accent); color: var(--accent); }
  :global(.reading) .back-to-messages { display: inline-flex; }
  .empty { color: var(--fg-muted); padding: 40px; text-align: center; }
  header h1 { flex: 1 1 260px; min-width: 0; overflow-wrap: anywhere; font-size: 20px; margin: 0 0 6px; }
  .subject-row { display: flex; align-items: flex-start; justify-content: space-between; flex-wrap: wrap; gap: 12px 16px; }
  .message-actions { display: flex; max-width: 100%; gap: 4px; flex: 0 1 auto; flex-wrap: wrap; justify-content: flex-end; }
  .message-actions button {
    min-height: 34px;
    padding: 6px 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    color: var(--fg-muted);
    font-size: 11px;
    font-weight: 600;
  }
  .message-actions button:hover { border-color: var(--accent); color: var(--accent); background: color-mix(in oklab, var(--accent) 6%, var(--bg)); }
  .message-actions button.active { border-color: var(--accent); color: var(--accent); background: color-mix(in oklab, var(--accent) 10%, var(--bg)); }
  .content-mode {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .content-mode button { border: 0; border-radius: 0; }
  .meta { color: var(--fg-muted); display: flex; flex-wrap: wrap; gap: 4px 16px; font-size: 12px; margin-bottom: 18px; }
  .meta .address { color: var(--fg); overflow-wrap: anywhere; user-select: text; }
  .details {
    display: grid;
    gap: 5px;
    margin: -6px 0 18px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg);
    font-size: 11px;
  }
  .details div { display: grid; grid-template-columns: 92px minmax(0, 1fr); gap: 10px; }
  .details dt { color: var(--fg-subtle); font-weight: 700; }
  .details dd { min-width: 0; margin: 0; color: var(--fg-muted); overflow-wrap: anywhere; user-select: text; }
  .remote-bar,
  .parse-warning {
    flex-shrink: 0;
    width: 100%;
    max-width: 760px;
    margin: 0 auto 12px;
    padding: 9px 12px;
    border: 1px solid color-mix(in oklab, var(--accent) 38%, var(--border));
    border-radius: var(--radius-md);
    display: flex;
    flex-wrap: wrap;
    gap: 6px 12px;
    align-items: baseline;
    background: color-mix(in oklab, var(--accent) 7%, var(--bg-raised));
    color: var(--fg-muted);
    font-size: 11px;
  }
  .remote-bar strong { color: var(--accent); }
  .remote-actions {
    flex-basis: 100%;
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .remote-actions button {
    padding: 4px 7px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-raised);
    color: var(--fg-muted);
    font-size: 10px;
  }
  .remote-actions button:hover { border-color: var(--accent); color: var(--accent); }
  .remote-actions button.risk {
    border-color: color-mix(in oklab, var(--warning) 48%, var(--border));
    background: color-mix(in oklab, var(--warning) 9%, var(--bg-raised));
    color: var(--warning);
  }
  .remote-actions button.risk:hover {
    border-color: var(--warning);
    background: color-mix(in oklab, var(--warning) 18%, var(--bg-raised));
    color: var(--warning);
  }
  .parse-warning { border-color: color-mix(in oklab, var(--danger) 38%, var(--border)); }
  .conversation {
    margin: -8px 0 18px;
    padding: 7px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    display: flex;
    align-items: center;
    gap: 5px;
    overflow-x: auto;
    background: var(--bg);
  }
  .conversation span { margin-right: 3px; color: var(--fg-subtle); font-size: 10px; white-space: nowrap; }
  .conversation button {
    flex-shrink: 0;
    padding: 3px 7px;
    border-radius: 999px;
    color: var(--fg-muted);
    font-size: 10px;
  }
  .conversation button:hover { background: var(--bg-sunken); }
  .conversation button.current { background: var(--accent); color: var(--accent-fg); }
  .body { min-width: 0; }
  .email-frame {
    display: block;
    width: 100%;
    max-width: 760px;
    margin-inline: auto;
    min-height: 180px;
    height: 180px;
    border: 0;
    background: var(--bg-raised);
  }
  .text {
    margin: 0 auto;
    max-width: 760px;
    font-family: var(--font-mono);
    font-size: 14px;
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  footer {
    margin-top: 32px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }
  footer h2 { font-size: 13px; text-transform: uppercase; color: var(--fg-muted); margin: 0 0 8px; }
  footer ul { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 4px; }
  footer button {
    background: var(--bg-sunken);
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    width: 100%;
    text-align: left;
    display: flex;
    justify-content: space-between;
    gap: 12px;
  }
  footer small { color: var(--fg-subtle); font-size: 11px; }

  @media (max-width: 760px) {
    .back-to-messages { display: inline-flex; }
  }
</style>
