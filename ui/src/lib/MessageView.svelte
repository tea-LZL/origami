<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import {
    app,
    clearMessageView,
    moveSelectedToRole,
    openReplyComposer,
    selectEnvelopeExclusive,
    setLayout,
    setPreferences,
    setSelectedFlag,
  } from "./stores.svelte";
  import { api } from "./api";
  import { attachmentAction, imageDataUrl, previewText } from "./attachmentOpen";
  import { allowedLinkHref, sanitizeMessageHtml } from "./messageHtml";
  import {
    allowRemoteOrigins,
    allowRemoteSender,
    loadRemoteContentAllowlist,
    normalizeRemoteSender,
    REMOTE_CONTENT_POLICY_EVENT,
  } from "./remoteContent";
  import { mergeThreadMembers, threadMembers } from "./threads";
  import { messageBodyKeyboard } from "./navigation";
  import OrigamiArtwork from "./OrigamiArtwork.svelte";
  import ActionIcon from "./ActionIcon.svelte";

  let contentMode = $state<"html" | "text">("html");
  let detailsOpen = $state(false);
  let lastMessageId = $state<string | null>(null);
  let remoteContentAllowed = $state(false);
  let allowedRemoteOrigins = $state<string[]>([]);
  let senderRemoteContentAllowed = $state(false);
  let frameResizeObserver: ResizeObserver | null = null;
  let emailFrame: HTMLIFrameElement | null = null;
  let preview = $state<{
    index: number;
    name: string;
    kind: "image" | "text";
    url: string | null;
    text: string | null;
  } | null>(null);
  let busyIndex = $state<number | null>(null);
  let busyMode = $state<"view" | "open" | null>(null);

  function focusNode(node: HTMLElement) {
    node.focus();
  }

  $effect(() => {
    if (!preview || typeof window === "undefined") return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      preview = null;
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  function applyFrameZoom() {
    const doc = emailFrame?.contentDocument;
    if (doc) doc.documentElement.style.fontSize = `${app.value.messageZoom}%`;
  }

  function onBodyKeydown(event: KeyboardEvent) {
    if (!(event.ctrlKey || event.metaKey)) return;
    if (event.key === "=" || event.key === "+") {
      event.preventDefault();
      setPreferences({ messageZoom: Math.min(300, app.value.messageZoom + 10) });
    } else if (event.key === "-") {
      event.preventDefault();
      setPreferences({ messageZoom: Math.max(50, app.value.messageZoom - 10) });
    } else if (event.key === "0") {
      event.preventDefault();
      setPreferences({ messageZoom: 100 });
    }
  }

  $effect(() => {
    void app.value.messageZoom;
    applyFrameZoom();
  });

  $effect(() => {
    if (typeof window === "undefined") return;
    window.addEventListener("keydown", onBodyKeydown);
    return () => window.removeEventListener("keydown", onBodyKeydown);
  });

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
    preview = null;
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
  const conversation = $derived(
    mergeThreadMembers(
      threadMembers(app.value.envelopes, app.value.selectedEnvelope),
      app.value.threadCrossFolder,
    ),
  );

  function openExternalLink(event: MouseEvent) {
    const target = event.target;
    if (!target || typeof (target as Element).closest !== "function") return;
    const anchor = (target as Element).closest("a");
    if (!anchor) return;

    event.preventDefault();
    const href = anchor.getAttribute("href");
    if (!href) return;

    const normalized = allowedLinkHref(href);
    if (!normalized) {
      app.value.lastError = `Could not open link: Unsupported link type: ${href}`;
      return;
    }

    try {
      openUrl(normalized).catch((error) => {
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
    emailFrame = frame;
    document.documentElement.style.fontSize = `${app.value.messageZoom}%`;

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

  function headerEnvelope() {
    return app.value.message?.envelope ?? app.value.selectedEnvelope;
  }

  function fromText(): string {
    return headerEnvelope()?.from
      .map((a) => a.name ?? a.addr)
      .join(", ") ?? "";
  }

  function fromAddressText(): string {
    return headerEnvelope()?.from
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

  function attachmentLabel(attachment: { index: number; name: string | null }): string {
    return attachment.name ?? `attachment-${attachment.index}`;
  }

  function closePreview() {
    preview = null;
  }

  async function viewAttachment(index: number) {
    const msg = app.value.message;
    const attachment = msg?.attachments.find((item) => item.index === index);
    if (!msg || !attachment) return;
    const name = attachmentLabel(attachment);
    if (msg.envelope.serverUid == null) {
      app.value.lastError = `Could not view attachment: this message has no server copy`;
      return;
    }
    const messageId = msg.envelope.id;
    const folderId = msg.envelope.mailboxId;
    const serverUid = msg.envelope.serverUid;
    busyIndex = index;
    busyMode = "view";
    try {
      const b64 = await api.getAttachment(folderId, serverUid, index);
      if (app.value.message?.envelope.id !== messageId) return;
      const kind = attachmentAction(attachment.mime, attachment.size);
      if (kind === "image") {
        preview = { index, name, kind, url: imageDataUrl(attachment.mime, b64), text: null };
      } else if (kind === "text") {
        preview = { index, name, kind, url: null, text: previewText(b64) };
      }
    } catch (error) {
      app.value.lastError = `Could not view attachment: ${String(error)}`;
    } finally {
      if (busyIndex === index && busyMode === "view") {
        busyIndex = null;
        busyMode = null;
      }
    }
  }

  async function openAttachment(index: number) {
    const msg = app.value.message;
    const attachment = msg?.attachments.find((item) => item.index === index);
    if (!msg || !attachment) return;
    if (msg.envelope.serverUid == null) {
      app.value.lastError = `Could not open attachment: this message has no server copy`;
      return;
    }
    const messageId = msg.envelope.id;
    busyIndex = index;
    busyMode = "open";
    try {
      await api.openAttachment(msg.envelope.mailboxId, msg.envelope.serverUid, index);
      if (app.value.message?.envelope.id !== messageId) return;
    } catch (error) {
      app.value.lastError = `Could not open attachment: ${String(error)}`;
    } finally {
      if (busyIndex === index && busyMode === "open") {
        busyIndex = null;
        busyMode = null;
      }
    }
  }
</script>

<section class="message">
  {#if !app.value.message && !app.value.selectedEnvelope}
    <div class="empty">
      <OrigamiArtwork variant="empty" theme={app.value.theme} className="message-empty-art" />
      <span>Select a message</span>
    </div>
  {:else}
    {@const env = headerEnvelope()!}
    {@const message = app.value.message}
    {@const headers = message?.headers ?? null}
    <header>
      {#if app.value.layout !== "three-pane" || app.value.selectedEnvelope}
        <button type="button" class="back-to-messages" onclick={backToMessages}>Back to messages</button>
      {/if}
      <div class="subject-row">
        <h1>{env.subject || "(no subject)"}</h1>
        <div class="message-actions" role="toolbar" aria-label="Message actions">
          {#if message && message.html && message.text}
            <div class="content-mode" role="group" aria-label="Message format">
              <button
                type="button"
                class:active={contentMode === "html"}
                aria-pressed={contentMode === "html"}
                onclick={() => contentMode = "html"}
              >HTML</button>
              <button
                type="button"
                class:active={contentMode === "text"}
                aria-pressed={contentMode === "text"}
                onclick={() => contentMode = "text"}
              >Text</button>
            </div>
          {/if}
          <button
            type="button"
            class:active={detailsOpen}
            aria-expanded={detailsOpen}
            aria-pressed={detailsOpen}
            onclick={() => detailsOpen = !detailsOpen}
          >Details</button>
          <span class="sep" aria-hidden="true"></span>
          <button type="button" class="icon" aria-label="Reply" title="Reply" onclick={() => openReplyComposer("reply")}>
            <ActionIcon name="reply" />
          </button>
          <button type="button" class="icon" aria-label="Reply all" title="Reply all" onclick={() => openReplyComposer("replyAll")}>
            <ActionIcon name="reply-all" />
          </button>
          <button type="button" class="icon" aria-label="Forward" title="Forward" onclick={() => openReplyComposer("forward")}>
            <ActionIcon name="forward" />
          </button>
          <span class="sep" aria-hidden="true"></span>
          <button type="button" class="icon" aria-label="Archive" title="Archive" onclick={() => moveSelectedToRole("Archive")}>
            <ActionIcon name="archive" />
          </button>
          <button type="button" class="icon" aria-label="Trash" title="Trash" onclick={() => moveSelectedToRole("Trash")}>
            <ActionIcon name="trash" />
          </button>
          <button type="button" class="icon" aria-label="Junk" title="Junk" onclick={() => moveSelectedToRole("Junk")}>
            <ActionIcon name="junk" />
          </button>
          <span class="sep" aria-hidden="true"></span>
          <button
            type="button"
            class="icon"
            class:active={env.flags.includes("Flagged")}
            aria-label="Star"
            title="Star"
            aria-pressed={env.flags.includes("Flagged")}
            onclick={() => setSelectedFlag("Flagged", !env.flags.includes("Flagged"))}
          >
            <ActionIcon name="star" filled={env.flags.includes("Flagged")} />
          </button>
        </div>
      </div>
      <div class="meta">
        <span class="from">{fromText()}</span>
        {#if fromAddressText()}<span class="address" title="From address">{fromAddressText()}</span>{/if}
        <span class="date">{env.date ?? ""}</span>
        {#if app.value.messageLoading}
          <span role="status" aria-live="polite">Fetching…</span>
        {:else}
          <span>Stored locally</span>
        {/if}
      </div>
      {#if headers && detailsOpen}
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

    {#if message}
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
      {#if message.parseWarnings.length > 0}
        <div class="parse-warning" role="status">Some MIME parts could not be decoded completely.</div>
      {/if}

      {#if preview}
        <div
          class="attachment-view"
          role="region"
          aria-label={`Preview of ${preview.name}`}
          tabindex="-1"
          use:focusNode
        >
          <div class="attachment-view-bar">
            <strong>{preview.name}</strong>
            <button type="button" onclick={closePreview}>Close preview</button>
          </div>
          {#if preview.kind === "image" && preview.url}
            <img src={preview.url} alt={preview.name} />
          {:else if preview.text}
            <pre>{preview.text}</pre>
          {/if}
        </div>
      {/if}

      {#key env.id}
      <div
        class="body"
        data-navigation="message-body"
        tabindex="-1"
        aria-label="Message body"
        style:font-size="{app.value.messageZoom ?? 100}%"
        use:messageBodyKeyboard
      >
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
      {/key}

      {#if message.attachments.length > 0}
        <footer>
          <h2>Attachments</h2>
          <ul>
            {#each message.attachments as att (att.index)}
              {@const name = attachmentLabel(att)}
              {@const action = attachmentAction(att.mime, att.size)}
              <li>
                <div class="file">
                  <div class="file-name">
                    <strong>{name}</strong>
                    <small>{att.mime} · {formatBytes(att.size)}{att.inline ? " · inline" : ""}</small>
                  </div>
                  <div class="file-actions">
                    {#if action === "image" || action === "text"}
                      <button
                        type="button"
                        aria-label={`View ${name}`}
                        disabled={busyIndex !== null}
                        onclick={() => viewAttachment(att.index)}
                      >{busyIndex === att.index && busyMode === "view" ? "Viewing…" : "View"}</button>
                    {/if}
                    <button
                      type="button"
                      aria-label={`Open ${name}`}
                      disabled={busyIndex !== null}
                      onclick={() => openAttachment(att.index)}
                    >{busyIndex === att.index && busyMode === "open" ? "Opening…" : "Open"}</button>
                  </div>
                </div>
              </li>
            {/each}
          </ul>
        </footer>
      {/if}
    {:else}
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
    {/if}
  {/if}
</section>

<style>
  .message {
    /* Width of the reading sheet. It grows into the pane up to this measure; the
       old fixed 760px cap left the email frame ~700px of content, so mail wider
       than that scrolled sideways while the pane still had room to spare. */
    --message-measure: 1100px;
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: transparent;
    overflow: hidden;
    padding: 24px 32px;
  }
  .message > header,
  .message > footer {
    flex-shrink: 0;
    width: 100%;
    max-width: min(100%, var(--message-measure));
    margin-inline: auto;
  }
  .message > header {
    position: relative;
    padding: 26px 30px 14px;
    border: 1px solid var(--border);
    border-bottom-color: var(--paper-crease);
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    background: linear-gradient(145deg, var(--bg-raised), color-mix(in oklab, var(--bg-raised) 94%, var(--bg-sunken)));
    box-shadow: var(--shadow-sheet), inset 0 1px var(--paper-highlight);
  }
  .message > .body {
    flex: 1;
    min-width: 0;
    min-height: 0;
    width: 100%;
    overflow-y: auto;
    scrollbar-gutter: stable;
    max-width: min(100%, var(--message-measure));
    margin-inline: auto;
    padding: 18px 30px 28px;
    border-inline: 1px solid var(--border);
    background: var(--bg-raised);
    box-shadow: inset 0 1px var(--paper-highlight);
    animation: fade-in var(--transition-fast) both;
  }
  @keyframes fade-in {
    from { opacity: 0.4; }
    to { opacity: 1; }
  }
  .message-loading {
    width: 100%;
    max-width: min(100%, var(--message-measure));
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
  .empty { color: var(--fg-muted); padding: 40px; text-align: center; display: grid; place-content: center; gap: 12px; }
  :global(.message-empty-art) { width: min(148px, 38vw); opacity: 0.64; }
  header h1 { min-width: 0; overflow-wrap: anywhere; font: 400 clamp(25px, 3vw, 34px)/1.08 var(--font-display); letter-spacing: -0.025em; margin: 0; }
  /* Subject and its action bar each get a full-width row: sharing one row squeezed
     the subject into a narrow column and wrapped the title. */
  .subject-row { display: flex; flex-direction: column; gap: 10px; margin-bottom: 9px; }
  .message-actions { display: flex; max-width: 100%; gap: 4px; flex: 0 1 auto; flex-wrap: wrap; justify-content: flex-end; align-items: center; }
  .message-actions button {
    min-height: 34px;
    padding: 6px 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: color-mix(in oklab, var(--bg-sunken) 82%, var(--bg-raised));
    color: var(--fg-muted);
    font-size: 11px;
    font-weight: 600;
  }
  .message-actions button.icon {
    width: 30px;
    min-height: 30px;
    padding: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .message-actions .sep { width: 1px; align-self: stretch; min-height: 18px; background: var(--border); margin: 0 2px; }
  .message-actions button:hover { border-color: var(--accent); color: var(--accent); background: color-mix(in oklab, var(--accent) 7%, var(--bg-raised)); }
  .message-actions button.active { border-color: var(--accent); color: var(--accent); background: color-mix(in oklab, var(--accent) 12%, var(--bg-raised)); }
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
    background: color-mix(in oklab, var(--bg-sunken) 72%, var(--bg-raised));
    font-size: 11px;
  }
  .details div { display: grid; grid-template-columns: 92px minmax(0, 1fr); gap: 10px; }
  .details dt { color: var(--fg-subtle); font-weight: 700; }
  .details dd { min-width: 0; margin: 0; color: var(--fg-muted); overflow-wrap: anywhere; user-select: text; }
  .remote-bar,
  .parse-warning {
    flex-shrink: 0;
    width: 100%;
    max-width: min(100%, var(--message-measure));
    margin: 0 auto 12px;
    padding: 9px 12px;
    border: 1px solid color-mix(in oklab, var(--accent) 30%, var(--paper-rule));
    border-radius: var(--radius-md);
    display: flex;
    flex-wrap: wrap;
    gap: 6px 12px;
    align-items: baseline;
    background: color-mix(in oklab, var(--accent) 6%, var(--bg-raised));
    box-shadow: inset 0 1px var(--paper-highlight);
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
    background: color-mix(in oklab, var(--bg-sunken) 72%, var(--bg-raised));
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
    max-width: min(100%, var(--message-measure));
    margin-inline: auto;
    min-height: 180px;
    height: 180px;
    border: 1px solid color-mix(in oklab, var(--border) 70%, var(--paper-highlight));
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
    background: var(--bg-raised);
    box-shadow: 0 10px 18px rgba(49, 45, 38, 0.06);
  }
  .text {
    margin: 0 auto;
    max-width: min(100%, var(--message-measure));
    font-family: var(--font-mono);
    font-size: 14px;
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    padding-bottom: 18px;
  }
  footer {
    margin-top: 32px;
    padding: 18px 30px 26px;
    border: 1px solid var(--border);
    border-top: 1px solid var(--paper-crease);
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
    background: var(--bg-raised);
    box-shadow: var(--shadow-sheet), inset 0 1px var(--paper-highlight);
  }
  footer h2 { font-size: 13px; text-transform: uppercase; color: var(--fg-muted); margin: 0 0 8px; }
  footer ul { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 4px; }
  footer .file {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--bg-sunken);
    border-radius: var(--radius-sm);
    padding: 6px 8px 6px 12px;
  }
  footer .file-name {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  footer .file-name strong { overflow-wrap: anywhere; }
  footer .file-actions { display: flex; flex-shrink: 0; gap: 4px; }
  footer .file-actions button {
    width: auto;
    background: var(--bg-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 6px 10px;
    color: var(--fg);
    font-size: 12px;
    font-weight: 650;
  }
  footer .file-actions button:hover { border-color: var(--accent); color: var(--accent); }
  footer .file-actions button:disabled { opacity: 0.55; }
  footer small { color: var(--fg-subtle); font-size: 11px; }
  .attachment-view {
    flex-shrink: 0;
    width: 100%;
    max-width: min(100%, var(--message-measure));
    margin: 0 auto 12px;
    max-height: 45vh;
    overflow: auto;
    padding: 12px 16px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-sunken);
  }
  .attachment-view:focus { outline: 2px solid var(--accent); outline-offset: 2px; }
  .attachment-view-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 10px;
  }
  .attachment-view-bar strong { overflow-wrap: anywhere; }
  .attachment-view-bar button {
    flex-shrink: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-raised);
    padding: 4px 8px;
    color: var(--fg-muted);
    font-size: 12px;
  }
  .attachment-view img {
    display: block;
    max-width: 100%;
    max-height: 36vh;
    margin: 0 auto;
    object-fit: contain;
  }
  .attachment-view pre {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-family: var(--font-mono);
    font-size: 13px;
    line-height: 1.5;
  }

  @media (max-width: 760px) {
    .back-to-messages { display: inline-flex; }
    .message > header { padding: 20px 18px 12px; }
    .message > .body { padding: 16px 18px 22px; }
    footer { padding: 16px 18px 22px; }
  }
</style>
