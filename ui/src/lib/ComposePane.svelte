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
        >—</button>
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
      <label class="subject">Subject <input type="text" bind:value={session.draft.subject} /></label>
    </div>

    <div class="toolbar">
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
      <span class="sep" aria-hidden="true"></span>
      {#if session.draft.composeMode === "rich"}
        <button type="button" onclick={() => exec("bold")} title="Bold"><b>B</b></button>
        <button type="button" onclick={() => exec("italic")} title="Italic"><i>I</i></button>
        <button type="button" onclick={() => exec("underline")} title="Underline"><u>U</u></button>
        <button type="button" onclick={() => exec("h2")} title="Heading">H</button>
        <button type="button" onclick={() => exec("bulletList")} title="Bulleted list">•</button>
        <button type="button" onclick={() => exec("orderedList")} title="Numbered list">1.</button>
        <button type="button" onclick={() => exec("blockquote")} title="Quote">❝</button>
        <button type="button" onclick={() => exec("code")} title="Code">{`</>`}</button>
        <button type="button" class:active={linkOpen} onclick={() => (linkOpen = !linkOpen)} title="Link">🔗</button>
        <span class="sep" aria-hidden="true"></span>
        <button type="button" onclick={() => exec("undo")} title="Undo">↶</button>
        <button type="button" onclick={() => exec("redo")} title="Redo">↷</button>
        <label class="attach-button inline-image" title="Insert image">
          <input type="file" accept="image/*" multiple hidden onchange={addInlineImages} />
          ⛶
        </label>
      {/if}
      <label class="attach-button push-end">
        Attach
        <input type="file" multiple onchange={addAttachments} />
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
        <span class="draft-state">{statusLabel(session)}</span>
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
    background: var(--bg-raised);
    border-left: 1px solid var(--border);
  }
  .compose-pane:focus { outline: none; }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
    box-shadow: inset 0 1px var(--paper-highlight);
  }
  .from {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    color: var(--fg-muted);
    font-size: 12px;
  }
  .pane-actions { display: flex; align-items: center; gap: 4px; }
  .cc-toggle {
    padding: 4px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-size: 11px;
    color: var(--fg-muted);
    background: var(--bg-raised);
  }
  .cc-toggle:hover { border-color: var(--accent); color: var(--accent); }
  .minimize {
    width: 28px;
    height: 28px;
    font-size: 16px;
    border-radius: var(--radius-sm);
  }
  .minimize:hover { background: var(--bg-sunken); }
  .fields {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 16px 4px;
  }
  .fields label {
    display: grid;
    grid-template-columns: 58px minmax(0, 1fr);
    align-items: center;
    gap: 10px;
    font-size: 12px;
    color: var(--fg-muted);
  }
  .fields input {
    flex: 1;
    background: color-mix(in oklab, var(--bg-sunken) 82%, var(--bg-raised));
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 7px 8px;
    color: var(--fg);
    min-height: 36px;
    font: inherit;
  }
  .fields label.subject {
    grid-template-columns: 58px minmax(0, 1fr);
    font-size: 13px;
  }
  .fields label.subject input {
    font: 500 17px/1.2 var(--font-display);
    letter-spacing: -0.01em;
  }
  .toolbar {
    display: flex;
    gap: 4px;
    padding: 6px 16px;
    border-block: 1px solid var(--border);
    background: color-mix(in oklab, var(--bg-sunken) 64%, var(--bg-raised));
  }
  .toolbar button {
    background: var(--bg-raised);
    border: 1px solid color-mix(in oklab, var(--border) 78%, var(--paper-highlight));
    border-radius: var(--radius-sm);
    padding: 4px 8px;
    font-size: 12px;
    min-width: 28px;
  }
  .toolbar button:hover { background: color-mix(in oklab, var(--accent) 10%, var(--bg-raised)); color: var(--accent); }
  .attach-button {
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    background: var(--bg-raised);
    border: 1px solid color-mix(in oklab, var(--border) 78%, var(--paper-highlight));
    color: var(--fg-muted);
    font-size: 12px;
    cursor: pointer;
  }
  .attach-button:hover { background: color-mix(in oklab, var(--accent) 10%, var(--bg-raised)); color: var(--accent); }
  .attach-button.push-end { margin-left: auto; }
  .attach-button input { display: none; }
  .editor {
    flex: 1;
    overflow-y: auto;
    padding: 12px 16px;
    min-height: 160px;
    cursor: text;
    background:
      linear-gradient(to bottom, transparent 0, transparent 29px, color-mix(in oklab, var(--paper-rule) 36%, transparent) 30px) 0 4px / 100% 30px,
      var(--bg-raised);
  }
  .editor:focus-within { box-shadow: inset 3px 0 0 var(--accent), inset 0 1px var(--paper-highlight); }
  .editor :global(.prose) { min-height: 100%; outline: none; }
  .editor :global(.prose p) { margin: 0 0 8px; }
  .attachments {
    padding: 8px 16px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .attachments span {
    padding: 5px 7px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    gap: 6px;
    background: color-mix(in oklab, var(--bg-sunken) 76%, var(--bg-raised));
    font-size: 11px;
  }
  .attachments small { color: var(--fg-subtle); }
  .attachments .thumb {
    width: 26px;
    height: 26px;
    object-fit: cover;
    border-radius: 4px;
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
    justify-content: flex-end;
    gap: 8px;
    padding: 10px 16px;
    border-top: 1px solid var(--border);
    background: color-mix(in oklab, var(--bg-sunken) 56%, var(--bg-raised));
  }
  .mode-toggle { padding: 4px 10px; border: 1px solid var(--border); border-radius: var(--radius-sm); font-size: 11px; cursor: pointer; color: var(--fg-muted); background: var(--bg-raised); }
  .mode-toggle.active { color: var(--accent-fg); background: var(--accent); border-color: var(--accent); }
  .toolbar .sep { width: 1px; height: 18px; background: var(--border); }
  .plain-editor {
    flex: 1;
    min-height: 160px;
    resize: vertical;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    margin: 8px 16px;
    background: var(--bg-raised);
    color: var(--fg);
    font: 13px/1.5 var(--font-mono);
    white-space: pre-wrap;
  }
  .draft-state { margin-right: auto; align-self: center; color: var(--fg-subtle); font-size: 10px; }
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
  .cancel { padding: 6px 12px; border-radius: var(--radius-sm); }
  .send {
    background: var(--accent);
    color: var(--accent-fg);
    padding: 6px 16px;
    border-radius: var(--radius-sm);
    font-weight: 600;
    box-shadow: 0 2px 0 color-mix(in oklab, var(--accent-active) 30%, transparent);
  }
  .send:hover { background: var(--accent-hover); }
  .send:disabled { opacity: 0.6; cursor: wait; }
</style>
