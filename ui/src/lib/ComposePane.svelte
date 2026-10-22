<script lang="ts">
  import { Editor } from "@tiptap/core";
  import Image from "@tiptap/extension-image";
  import StarterKit from "@tiptap/starter-kit";
  import { fileToInlineImage } from "./inlineImage";
  import {
    app,
    type ComposerSession,
    cancelDiscardComposer,
    discardComposer,
    flushComposerSave,
    minimizeComposer,
    requestDiscardComposer,
    scheduleComposerSave,
    sendComposer,
    syncComposerNow,
  } from "./stores.svelte";
  import Select from "./Select.svelte";
  import RecipientInput from "./RecipientInput.svelte";
  import ActionIcon from "./ActionIcon.svelte";

  let editor: Editor | null = null;
  let paneEl = $state<HTMLDivElement | null>(null);
  let ccVisible = $state(false);
  let linkOpen = $state(false);
  let linkUrl = $state("");

  const session = $derived(
    app.value.composerSessions.find((item) => item.id === app.value.activeComposerId) ?? null,
  );
  const sending = $derived(session !== null && app.value.sendingComposerId === session.id);
  const confirmingDiscard = $derived(
    session !== null && app.value.discardConfirmId === session.id,
  );

  // Autosave: touch every field the draft payload includes so any edit
  // schedules a save for this session.
  $effect(() => {
    const current = session;
    if (!current) return;
    void [
      current.accountId,
      current.draft.to,
      current.draft.cc,
      current.draft.bcc,
      current.draft.subject,
      current.draft.html,
      current.draft.composeMode,
      current.attachments.length,
      current.threading.inReplyTo,
      current.threading.references.length,
    ];
    scheduleComposerSave(current.id);
  });

  // Quit/teardown flush: persist the draft immediately, no debounce wait.
  $effect(() => {
    const flush = () => {
      const current = session;
      if (current) void flushComposerSave(current.id);
    };
    window.addEventListener("beforeunload", flush);
    return () => window.removeEventListener("beforeunload", flush);
  });

  // New sessions focus To (RecipientInput); restored ones focus the pane.
  $effect(() => {
    const current = session;
    if (!current || current.saveState === "idle") return;
    const frame = requestAnimationFrame(() => paneEl?.focus());
    return () => cancelAnimationFrame(frame);
  });

  function mountEditor(node: HTMLDivElement) {
    const initial = app.value.composerSessions.find(
      (item) => item.id === app.value.activeComposerId,
    );
    const instance = new Editor({
      element: node,
      extensions: [StarterKit.configure({ link: { openOnClick: false } }), Image],
      content: initial?.draft.html ?? "<p></p>",
      editorProps: {
        attributes: {
          class: "prose",
        },
      },
      onUpdate: ({ editor }) => {
        const target = app.value.composerSessions.find(
          (item) => item.id === app.value.activeComposerId,
        );
        if (target) target.draft.html = editor.getHTML();
      },
    });
    editor = instance;
    const focusOnBlankArea = (event: MouseEvent) => {
      if (event.target === event.currentTarget) instance.chain().focus().run();
    };
    node.addEventListener("click", focusOnBlankArea);
    return {
      destroy() {
        node.removeEventListener("click", focusOnBlankArea);
        instance.destroy();
        if (editor === instance) editor = null;
      },
    };
  }

  function exec(
    cmd: "bold" | "italic" | "underline" | "bulletList" | "orderedList" | "h2" | "blockquote" | "code" | "undo" | "redo",
  ) {
    if (!editor) return;
    const chain = editor.chain().focus();
    switch (cmd) {
      case "bold": chain.toggleBold().run(); break;
      case "italic": chain.toggleItalic().run(); break;
      case "underline": chain.toggleUnderline().run(); break;
      case "bulletList": chain.toggleBulletList().run(); break;
      case "orderedList": chain.toggleOrderedList().run(); break;
      case "h2": chain.toggleHeading({ level: 2 }).run(); break;
      case "blockquote": chain.toggleBlockquote().run(); break;
      case "code": chain.toggleCode().run(); break;
      case "undo": chain.undo().run(); break;
      case "redo": chain.redo().run(); break;
    }
  }

  function applyLink() {
    const href = linkUrl.trim();
    if (!editor || !href) return;
    editor.chain().focus().extendMarkRange("link").setLink({ href }).run();
    linkUrl = "";
    linkOpen = false;
  }

  function removeLink() {
    editor?.chain().focus().unsetLink().run();
    linkOpen = false;
  }

  $effect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Enter" || !(event.ctrlKey || event.metaKey)) return;
      const current = session;
      const target = event.target as Node | null;
      if (!current || !paneEl || !target || !paneEl.contains(target)) return;
      event.preventDefault();
      void sendComposer(current.id);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  function bytesToBase64(bytes: Uint8Array): string {
    let binary = "";
    const chunkSize = 0x8000;
    for (let offset = 0; offset < bytes.length; offset += chunkSize) {
      binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
    }
    return btoa(binary);
  }

  async function addAttachmentFiles(files: File[]) {
    for (const file of files) {
      const current = session;
      if (!current) continue;
      const totalSize = current.attachments.reduce((total, item) => total + item.size, 0);
      if (totalSize + file.size > 25 * 1024 * 1024) {
        app.value.lastError = "Attachments are limited to 25 MB per message";
        continue;
      }
      const bytes = new Uint8Array(await file.arrayBuffer());
      current.attachments.push({
        name: file.name,
        mime: file.type || "application/octet-stream",
        size: file.size,
        dataBase64: bytesToBase64(bytes),
      });
    }
  }

  async function addAttachments(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    await addAttachmentFiles([...(input.files ?? [])]);
    input.value = "";
  }

  function onDragOver(event: DragEvent) {
    if (event.dataTransfer?.types.includes("Files")) event.preventDefault();
  }

  async function onDrop(event: DragEvent) {
    event.preventDefault();
    await addAttachmentFiles([...(event.dataTransfer?.files ?? [])]);
  }

  function attachmentTotal(current: ComposerSession): number {
    return current.attachments.reduce((total, item) => total + item.size, 0);
  }

  async function addInlineImages(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    for (const file of Array.from(input.files ?? [])) {
      try {
        const src = await fileToInlineImage(file);
        editor?.chain().focus().setImage({ src }).run();
      } catch (cause) {
        app.value.lastError = String(cause);
      }
    }
    input.value = "";
  }

  function removeAttachment(index: number) {
    const current = session;
    if (!current) return;
    current.attachments = current.attachments.filter((_, item) => item !== index);
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function focusOnMount(node: HTMLElement) {
    const frame = requestAnimationFrame(() => node.focus());
    return {
      destroy() {
        cancelAnimationFrame(frame);
      },
    };
  }

  function composeTitle(current: ComposerSession): string {
    if (current.threading.inReplyTo) return "Reply";
    if (/^Fwd:/i.test(current.draft.subject)) return "Forward";
    return "New message";
  }

  function statusLabel(current: ComposerSession): string {
    if (current.saveState === "saving") return "Saving…";
    if (current.saveState === "error") return "Save failed";
    if (current.syncState === "syncing") return "Syncing…";
    if (current.syncState === "synced") return "Synced to server";
    if (current.lastSavedAt) {
      const time = new Date(current.lastSavedAt).toLocaleTimeString([], {
        hour: "2-digit",
        minute: "2-digit",
      });
      return `Saved locally ${time}`;
    }
    return "Draft saved locally";
  }
</script>

{#if session}
  <div
    class="compose-pane"
    id={`compose-pane-${session.id}`}
    bind:this={paneEl}
    role="region"
    tabindex="-1"
    aria-label="Compose message"
    ondragover={onDragOver}
    ondrop={onDrop}
  >
    <header>
      <div class="compose-title">
        <span class="eyebrow">Compose</span>
        <h2>{composeTitle(session)}</h2>
      </div>
      <label class="from">From
        <Select
          bind:value={session.accountId}
          ariaLabel="Sender account"
          options={app.value.accounts.map((account) => ({
            value: account.id,
            label: `${account.name} <${account.email}>`,
          }))}
        />
      </label>
      <div class="pane-actions">
        <button
          type="button"
          class="cc-toggle"
          aria-expanded={ccVisible || session.draft.cc.trim() !== "" || session.draft.bcc.trim() !== ""}
          onclick={() => (ccVisible = !ccVisible)}
        >Cc/Bcc</button>
        <button
          type="button"
          class="minimize"
          onclick={minimizeComposer}
          aria-label="Minimize composer"
          title="Minimize"
        ><ActionIcon name="minimize" /></button>
      </div>
    </header>

    <div class="fields">
      <RecipientInput
        label="To"
        value={session.draft.to}
        correspondents={app.value.correspondents}
        onChange={(value) => (session!.draft.to = value)}
        focusOnMount={session.saveState === "idle"}
      />
      {#if ccVisible || session.draft.cc.trim() !== "" || session.draft.bcc.trim() !== ""}
        <RecipientInput
          label="Cc"
          value={session.draft.cc}
          correspondents={app.value.correspondents}
          onChange={(value) => (session!.draft.cc = value)}
        />
        <RecipientInput
          label="Bcc"
          value={session.draft.bcc}
          correspondents={app.value.correspondents}
          onChange={(value) => (session!.draft.bcc = value)}
        />
      {/if}
      <label class="subject field-row">
        <span class="field-label">Subject</span>
        <input type="text" bind:value={session.draft.subject} />
      </label>
    </div>

    <div class="toolbar" role="toolbar" aria-label="Formatting">
      <div class="mode-group">
        <button
          type="button"
          class="mode-toggle"
          class:active={session.draft.composeMode === "rich"}
          title="Rich text"
          onclick={() => (session!.draft.composeMode = "rich")}
        >Rich</button>
        <button
          type="button"
          class="mode-toggle"
          class:active={session.draft.composeMode === "plain"}
          title="Plain text"
          onclick={() => (session!.draft.composeMode = "plain")}
        >Plain</button>
      </div>
      <span class="sep" aria-hidden="true"></span>
      {#if session.draft.composeMode === "rich"}
        <button type="button" class="tool" aria-label="Bold" title="Bold" onclick={() => exec("bold")}><ActionIcon name="bold" /></button>
        <button type="button" class="tool" aria-label="Italic" title="Italic" onclick={() => exec("italic")}><ActionIcon name="italic" /></button>
        <button type="button" class="tool" aria-label="Underline" title="Underline" onclick={() => exec("underline")}><ActionIcon name="underline" /></button>
        <button type="button" class="tool" aria-label="Heading" title="Heading" onclick={() => exec("h2")}><ActionIcon name="heading" /></button>
        <button type="button" class="tool" aria-label="Bulleted list" title="Bulleted list" onclick={() => exec("bulletList")}><ActionIcon name="list" /></button>
        <button type="button" class="tool" aria-label="Numbered list" title="Numbered list" onclick={() => exec("orderedList")}><ActionIcon name="ordered-list" /></button>
        <button type="button" class="tool" aria-label="Quote" title="Quote" onclick={() => exec("blockquote")}><ActionIcon name="quote" /></button>
        <button type="button" class="tool" aria-label="Code" title="Code" onclick={() => exec("code")}><ActionIcon name="code" /></button>
        <button
          type="button"
          class="tool"
          class:active={linkOpen}
          aria-label="Link"
          title="Link"
          aria-pressed={linkOpen}
          onclick={() => (linkOpen = !linkOpen)}
        ><ActionIcon name="link" /></button>
        <span class="sep" aria-hidden="true"></span>
        <button type="button" class="tool" aria-label="Undo" title="Undo" onclick={() => exec("undo")}><ActionIcon name="undo" /></button>
        <button type="button" class="tool" aria-label="Redo" title="Redo" onclick={() => exec("redo")}><ActionIcon name="redo" /></button>
        <label class="tool attach-button inline-image" title="Insert image" aria-label="Insert image">
          <input type="file" accept="image/*" multiple hidden aria-label="Insert image file" onchange={addInlineImages} />
          <ActionIcon name="image" />
        </label>
      {/if}
      <label class="tool attach-button push-end" title="Attach files">
        <ActionIcon name="paperclip" />
        <span>Attach</span>
        <input type="file" multiple aria-label="Attach files" onchange={addAttachments} />
      </label>
    </div>

    {#if linkOpen}
      <div class="link-row">
        <label>Link <input
          type="url"
          bind:value={linkUrl}
          placeholder="https://example.org"
          onkeydown={(event) => {
            if (event.key === "Enter") applyLink();
          }}
        /></label>
        <button type="button" onclick={applyLink}>Apply</button>
        <button type="button" onclick={removeLink}>Remove</button>
      </div>
    {/if}

    {#if session.draft.composeMode === "plain"}
      <textarea
        class="plain-editor"
        aria-label="Plain text body"
        bind:value={session.draft.html}
      ></textarea>
    {:else}
      {#key session.id}
        <div class="editor" use:mountEditor></div>
      {/key}
    {/if}

    {#if session.attachments.length > 0}
      <div class="attachments" aria-label="Attachments">
        {#each session.attachments as attachment, index (`${attachment.name}-${index}`)}
          <span>
            {#if attachment.mime.startsWith("image/")}
              <img
                class="thumb"
                src={`data:${attachment.mime};base64,${attachment.dataBase64}`}
                alt=""
              />
            {/if}
            <strong>{attachment.name}</strong>
            <small>{formatSize(attachment.size)}</small>
            <button
              type="button"
              onclick={() => removeAttachment(index)}
              aria-label={`Remove ${attachment.name}`}
            >×</button>
          </span>
        {/each}
        <small class="attach-total">
          {formatSize(attachmentTotal(session))} / 25 MB
        </small>
      </div>
    {/if}

    {#if confirmingDiscard}
      <div class="discard-confirm" role="alertdialog" aria-label="Discard draft?">
        <span>Discard draft?</span>
        <button type="button" use:focusOnMount onclick={cancelDiscardComposer}>Keep editing</button>
        <button type="button" class="discard" onclick={() => void discardComposer(session!.id)}>
          Discard
        </button>
      </div>
    {/if}

    <footer>
      {#if session.syncState === "error"}
        <button
          type="button"
          class="sync-retry"
          onclick={() => void syncComposerNow(session!.id)}
        >Sync failed · Retry</button>
      {:else}
        <span class="draft-state"><span class="ink-dot" aria-hidden="true"></span>{statusLabel(session)}</span>
      {/if}
      <button type="button" class="cancel" onclick={() => requestDiscardComposer(session!.id)}>
        Discard
      </button>
      <button
        type="button"
        class="send"
        onclick={() => void sendComposer(session!.id)}
        disabled={sending}
      >
        {sending ? "Sending…" : "Send"}
        <span class="send-fold" aria-hidden="true"></span>
      </button>
    </footer>
  </div>
{/if}

<style>
  .compose-pane {
    flex: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    background:
      radial-gradient(circle at 12% 0, color-mix(in oklab, var(--accent) 7%, transparent), transparent 32%),
      var(--bg-raised);
    border-left: 1px solid var(--border);
  }
  .compose-pane:focus { outline: none; }
  header {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 12px 16px;
    padding: 12px 18px;
    border-bottom: 1px solid var(--border);
    box-shadow: inset 0 1px var(--paper-highlight);
  }
  .compose-title { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
  .eyebrow {
    color: var(--fg-subtle);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.16em;
    text-transform: uppercase;
  }
  .compose-title h2 {
    margin: 0;
    color: var(--fg);
    font: 500 19px/1.15 var(--font-display);
    letter-spacing: -0.015em;
  }
  .from {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 3px 4px 3px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: color-mix(in oklab, var(--bg-sunken) 62%, var(--bg-raised));
    color: var(--fg-muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .pane-actions { display: flex; align-items: center; gap: 6px; }
  .cc-toggle {
    min-height: 28px;
    padding: 4px 11px;
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 11px;
    color: var(--fg-muted);
    background: var(--bg-raised);
  }
  .cc-toggle:hover,
  .cc-toggle[aria-expanded="true"] {
    border-color: var(--accent);
    color: var(--accent);
    background: color-mix(in oklab, var(--accent) 8%, var(--bg-raised));
  }
  .minimize {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 999px;
    color: var(--fg-muted);
  }
  .minimize:hover { background: var(--bg-sunken); color: var(--fg); }
  .fields {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 18px 6px;
  }
  .fields label,
  .fields .field-row {
    display: grid;
    grid-template-columns: 58px minmax(0, 1fr);
    align-items: center;
    gap: 12px;
    border-bottom: 1px solid color-mix(in oklab, var(--border) 72%, transparent);
    font-size: 12px;
    color: var(--fg-muted);
  }
  .field-label {
    color: var(--fg-subtle);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }
  .fields input {
    flex: 1;
    min-height: 38px;
    padding: 7px 0;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--fg);
    font: inherit;
    outline: none;
  }
  .fields input:focus-visible {
    box-shadow: inset 0 -2px var(--accent);
  }
  .fields label.subject {
    grid-template-columns: 58px minmax(0, 1fr);
    border-bottom-color: var(--border-strong);
    font-size: 13px;
  }
  .fields label.subject input {
    min-height: 44px;
    font: 500 21px/1.2 var(--font-display);
    letter-spacing: -0.01em;
  }
  .toolbar {
    position: relative;
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
    padding: 7px 18px;
    border-block: 1px solid var(--border);
    background:
      linear-gradient(180deg, color-mix(in oklab, var(--paper-highlight) 55%, transparent), transparent 70%),
      color-mix(in oklab, var(--bg-sunken) 70%, var(--bg-raised));
  }
  .mode-group {
    display: inline-flex;
    padding: 2px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--bg-raised);
  }
  .toolbar button,
  .tool {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-width: 30px;
    min-height: 30px;
    padding: 4px 8px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--fg-muted);
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
  }
  .toolbar button:hover,
  .tool:hover {
    border-color: color-mix(in oklab, var(--accent) 35%, transparent);
    background: color-mix(in oklab, var(--accent) 9%, var(--bg-raised));
    color: var(--accent);
  }
  .toolbar .tool.active,
  .toolbar button.active {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-fg);
  }
  .attach-button {
    padding: 4px 10px;
    border: 1px solid color-mix(in oklab, var(--border) 78%, var(--paper-highlight));
    border-radius: 999px;
    background: var(--bg-raised);
    color: var(--fg-muted);
    cursor: pointer;
  }
  .attach-button:hover { border-color: var(--accent); color: var(--accent); }
  .attach-button.push-end { margin-left: auto; }
  .attach-button input { display: none; }
  .editor {
    flex: 1;
    overflow-y: auto;
    margin: 10px 18px 12px;
    padding: 14px 18px 14px 22px;
    min-height: 160px;
    cursor: text;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-sheet);
    background:
      linear-gradient(90deg, transparent 0 15px, color-mix(in oklab, var(--accent) 26%, transparent) 15px 16px, transparent 16px),
      linear-gradient(to bottom, transparent 0, transparent 29px, color-mix(in oklab, var(--paper-rule) 34%, transparent) 30px) 0 5px / 100% 30px,
      radial-gradient(circle at 20% 10%, var(--paper-grain), transparent 55%),
      var(--bg-raised);
  }
  .editor:focus-within {
    border-color: color-mix(in oklab, var(--accent) 62%, var(--border));
    box-shadow: var(--shadow-sheet), inset 3px 0 0 var(--accent);
  }
  .editor :global(.prose) { min-height: 100%; outline: none; }
  .editor :global(.prose p) { margin: 0 0 8px; }
  .attachments {
    padding: 8px 18px 10px;
    border-top: 1px solid color-mix(in oklab, var(--border) 70%, transparent);
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .attachments span {
    position: relative;
    padding: 5px 8px;
    border: 1px solid var(--border);
    border-radius: 3px 8px 3px 3px;
    display: flex;
    align-items: center;
    gap: 7px;
    background:
      linear-gradient(135deg, transparent calc(100% - 9px), color-mix(in oklab, var(--paper-crease) 70%, var(--bg-raised)) calc(100% - 8px)),
      color-mix(in oklab, var(--bg-sunken) 72%, var(--bg-raised));
    font-size: 11px;
  }
  .attachments small { color: var(--fg-subtle); }
  .attachments .thumb {
    width: 26px;
    height: 26px;
    object-fit: cover;
    border-radius: 3px;
    border: 1px solid var(--border);
  }
  .attachments .attach-total { margin-left: auto; align-self: center; }
  .attachments button { width: 18px; height: 18px; border-radius: 50%; }
  .attachments button:hover { background: var(--border); }
  .link-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 16px;
    border-bottom: 1px solid var(--border);
    background: color-mix(in oklab, var(--bg-sunken) 50%, var(--bg-raised));
    font-size: 12px;
  }
  .link-row label { display: flex; align-items: center; gap: 6px; flex: 1; color: var(--fg-muted); }
  .link-row input {
    flex: 1;
    min-height: 30px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-raised);
    color: var(--fg);
    font: inherit;
  }
  .link-row button {
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-raised);
    font-size: 12px;
  }
  .link-row button:hover { border-color: var(--accent); color: var(--accent); }
  .discard-confirm {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 8px 16px;
    border-top: 1px solid color-mix(in oklab, var(--warning) 40%, var(--border));
    background: color-mix(in oklab, var(--warning) 12%, var(--bg-raised));
    font-size: 12px;
  }
  .discard-confirm span { margin-right: auto; }
  .discard-confirm button {
    padding: 4px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-raised);
    font-size: 12px;
  }
  .discard-confirm .discard { background: var(--danger); color: var(--danger-fg); border-color: var(--danger); }
  footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 10px 18px;
    border-top: 1px solid var(--border);
    background:
      linear-gradient(0deg, color-mix(in oklab, var(--paper-highlight) 42%, transparent), transparent 70%),
      color-mix(in oklab, var(--bg-sunken) 56%, var(--bg-raised));
  }
  .mode-toggle { padding: 3px 10px; border: 1px solid transparent; border-radius: 999px; font-size: 11px; cursor: pointer; color: var(--fg-muted); background: transparent; }
  .mode-toggle.active { color: var(--accent-fg); background: var(--accent); border-color: var(--accent); }
  .toolbar .sep { width: 1px; height: 18px; background: color-mix(in oklab, var(--border) 80%, transparent); }
  .plain-editor {
    flex: 1;
    min-height: 160px;
    resize: none;
    padding: 14px 18px 14px 22px;
    margin: 10px 18px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-sheet);
    background:
      linear-gradient(90deg, transparent 0 15px, color-mix(in oklab, var(--accent) 26%, transparent) 15px 16px, transparent 16px),
      linear-gradient(to bottom, transparent 0, transparent 29px, color-mix(in oklab, var(--paper-rule) 34%, transparent) 30px) 0 5px / 100% 30px,
      var(--bg-raised);
    color: var(--fg);
    font: 13px/30px var(--font-mono);
    white-space: pre-wrap;
    outline: none;
  }
  .plain-editor:focus-visible {
    border-color: color-mix(in oklab, var(--accent) 62%, var(--border));
    box-shadow: var(--shadow-sheet), inset 3px 0 0 var(--accent);
  }
  .draft-state {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    margin-right: auto;
    align-self: center;
    color: var(--fg-subtle);
    font-size: 10px;
    letter-spacing: 0.02em;
  }
  .ink-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--success);
    box-shadow: 0 0 0 2px color-mix(in oklab, var(--success) 18%, transparent);
  }
  .sync-retry {
    margin-right: auto;
    align-self: center;
    padding: 3px 10px;
    border: 1px solid color-mix(in oklab, var(--warning) 45%, var(--border));
    border-radius: var(--radius-sm);
    background: color-mix(in oklab, var(--warning) 12%, var(--bg-raised));
    color: var(--fg);
    font-size: 11px;
  }
  .sync-retry:hover { border-color: var(--warning); }
  .cancel {
    padding: 6px 12px;
    border-radius: 999px;
    color: var(--fg-muted);
  }
  .cancel:hover { background: var(--bg-sunken); color: var(--fg); }
  .send {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 10px;
    padding: 7px 20px 7px 18px;
    border-radius: 3px 10px 3px 3px;
    background: var(--accent);
    color: var(--accent-fg);
    font-weight: 650;
    letter-spacing: 0.01em;
    box-shadow: 0 2px 0 color-mix(in oklab, var(--accent-active) 38%, transparent);
  }
  .send:hover { background: var(--accent-hover); }
  .send:disabled { opacity: 0.6; cursor: wait; }
  .send-fold {
    width: 0;
    height: 0;
    border-style: solid;
    border-width: 0 0 7px 7px;
    border-color: transparent transparent color-mix(in oklab, var(--accent-fg) 55%, transparent) transparent;
  }
</style>
