<script lang="ts">
  import { Editor } from "@tiptap/core";
  import StarterKit from "@tiptap/starter-kit";
  import { app, closeComposer, sendComposer } from "./stores.svelte";
  import { api } from "./api";
  import Select from "./Select.svelte";

  let editor: Editor | null = null;
  let toInput: HTMLInputElement | null = $state(null);

  $effect(() => {
    if (!app.value.composerOpen || !toInput) return;
    const frame = requestAnimationFrame(() => toInput?.focus());
    return () => cancelAnimationFrame(frame);
  });

  $effect(() => {
    if (!app.value.composerOpen) return;
    const snapshot = {
      accountId: app.value.composerAccountId,
      draft: app.value.composerDraft,
      attachments: app.value.composerAttachments,
    };
    const timer = setTimeout(() => {
      api.saveComposerDraft(snapshot).then(() => {
        localStorage.removeItem("origami-composer-draft");
      }).catch(() => {});
    }, 400);
    return () => clearTimeout(timer);
  });

  function mountEditor(node: HTMLDivElement) {
    const instance = new Editor({
      element: node,
      extensions: [StarterKit],
      content: app.value.composerDraft.html,
      editorProps: {
        attributes: {
          class: "prose",
        },
      },
      onUpdate: ({ editor }) => {
        app.value.composerDraft.html = editor.getHTML();
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

  function exec(cmd: "bold" | "italic" | "bulletList" | "orderedList" | "h2" | "blockquote" | "code") {
    if (!editor) return;
    const chain = editor.chain().focus();
    switch (cmd) {
      case "bold": chain.toggleBold().run(); break;
      case "italic": chain.toggleItalic().run(); break;
      case "bulletList": chain.toggleBulletList().run(); break;
      case "orderedList": chain.toggleOrderedList().run(); break;
      case "h2": chain.toggleHeading({ level: 2 }).run(); break;
      case "blockquote": chain.toggleBlockquote().run(); break;
      case "code": chain.toggleCode().run(); break;
    }
  }

  function onClose() {
    closeComposer();
  }

  function bytesToBase64(bytes: Uint8Array): string {
    let binary = "";
    const chunkSize = 0x8000;
    for (let offset = 0; offset < bytes.length; offset += chunkSize) {
      binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
    }
    return btoa(binary);
  }

  async function addAttachments(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const files = [...(input.files ?? [])];
    for (const file of files) {
      const totalSize = app.value.composerAttachments.reduce((total, item) => total + item.size, 0);
      if (totalSize + file.size > 25 * 1024 * 1024) {
        app.value.lastError = "Attachments are limited to 25 MB per message";
        continue;
      }
      const bytes = new Uint8Array(await file.arrayBuffer());
      app.value.composerAttachments.push({
        name: file.name,
        mime: file.type || "application/octet-stream",
        size: file.size,
        dataBase64: bytesToBase64(bytes),
      });
    }
    input.value = "";
  }

  function removeAttachment(index: number) {
    app.value.composerAttachments = app.value.composerAttachments.filter((_, item) => item !== index);
  }

  function formatSize(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
</script>

{#if app.value.composerOpen}
  <div class="overlay" role="dialog" aria-modal="true" aria-labelledby="composer-title">
    <div class="composer">
      <header>
        <h2 id="composer-title">New message</h2>
        <button type="button" class="close" onclick={onClose} aria-label="Close composer">×</button>
      </header>

      <div class="fields">
        <label>From
          <Select
            bind:value={app.value.composerAccountId}
            ariaLabel="Sender account"
            options={app.value.accounts.map((account) => ({
              value: account.id,
              label: `${account.name} <${account.email}>`,
            }))}
          />
        </label>
        <label>To <input bind:this={toInput} type="text" list="origami-correspondents" bind:value={app.value.composerDraft.to} /></label>
        <label>Cc <input type="text" list="origami-correspondents" bind:value={app.value.composerDraft.cc} /></label>
        <label>Bcc <input type="text" list="origami-correspondents" bind:value={app.value.composerDraft.bcc} /></label>
        <label>Subject <input type="text" bind:value={app.value.composerDraft.subject} /></label>
        <datalist id="origami-correspondents">
          {#each app.value.correspondents as contact (contact.addr)}
            <option value={contact.addr}>{contact.name ?? contact.addr}</option>
          {/each}
        </datalist>
      </div>

      <div class="toolbar">
        <button type="button" onclick={() => exec("bold")} title="Bold"><b>B</b></button>
        <button type="button" onclick={() => exec("italic")} title="Italic"><i>I</i></button>
        <button type="button" onclick={() => exec("h2")} title="Heading">H</button>
        <button type="button" onclick={() => exec("bulletList")} title="Bulleted list">•</button>
        <button type="button" onclick={() => exec("orderedList")} title="Numbered list">1.</button>
        <button type="button" onclick={() => exec("blockquote")} title="Quote">❝</button>
        <button type="button" onclick={() => exec("code")} title="Code">{`</>`}</button>
        <label class="attach-button">
          Attach
          <input type="file" multiple onchange={addAttachments} />
        </label>
      </div>

      <div class="editor" use:mountEditor></div>

      {#if app.value.composerAttachments.length > 0}
        <div class="attachments" aria-label="Attachments">
          {#each app.value.composerAttachments as attachment, index (`${attachment.name}-${index}`)}
            <span>
              <strong>{attachment.name}</strong>
              <small>{formatSize(attachment.size)}</small>
              <button type="button" onclick={() => removeAttachment(index)} aria-label={`Remove ${attachment.name}`}>×</button>
            </span>
          {/each}
        </div>
      {/if}

      <footer>
        <span class="draft-state">Draft saved locally</span>
        <button type="button" class="cancel" onclick={onClose}>Close</button>
        <button type="button" class="send" onclick={sendComposer} disabled={app.value.sending}>
          {app.value.sending ? "Sending…" : "Send"}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: var(--overlay);
    display: grid;
    place-items: center;
    z-index: 50;
  }
  .composer {
    width: min(720px, 90vw);
    max-height: 80vh;
    background: linear-gradient(145deg, var(--bg-raised), color-mix(in oklab, var(--bg-raised) 94%, var(--bg-sunken)));
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-float);
    animation: surface-in var(--transition-med) both;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
    box-shadow: inset 0 1px var(--paper-highlight);
  }
  header h2 { font: 400 22px/1 var(--font-display); letter-spacing: -0.02em; margin: 0; }
  .close {
    width: 28px; height: 28px;
    font-size: 18px;
    border-radius: var(--radius-sm);
  }
  .close:hover { background: var(--bg-sunken); }
  .fields {
    display: flex; flex-direction: column; gap: 6px;
    padding: 10px 16px 4px;
  }
  .fields label {
    display: grid; grid-template-columns: 58px minmax(0, 1fr); align-items: center; gap: 10px;
    font-size: 12px; color: var(--fg-muted);
  }
  .fields input {
    flex: 1; background: color-mix(in oklab, var(--bg-sunken) 82%, var(--bg-raised));
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 7px 8px;
    color: var(--fg);
    min-height: 36px;
    font: inherit;
  }
  .toolbar {
    display: flex; gap: 4px; padding: 6px 16px;
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
    margin-left: auto;
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    background: var(--bg-raised);
    border: 1px solid color-mix(in oklab, var(--border) 78%, var(--paper-highlight));
    color: var(--fg-muted);
    font-size: 12px;
    cursor: pointer;
  }
  .attach-button:hover { background: color-mix(in oklab, var(--accent) 10%, var(--bg-raised)); color: var(--accent); }
  .attach-button input { display: none; }
  .editor {
    flex: 1; overflow-y: auto;
    padding: 12px 16px;
    min-height: 200px;
    cursor: text;
    background:
      linear-gradient(to bottom, transparent 0, transparent 29px, color-mix(in oklab, var(--paper-rule) 36%, transparent) 30px) 0 4px / 100% 30px,
      var(--bg-raised);
  }
  .editor:focus-within { box-shadow: inset 3px 0 0 var(--accent), inset 0 1px var(--paper-highlight); }
  .editor :global(.prose) { min-height: 100%; outline: none; }
  .editor :global(.prose p) { margin: 0 0 8px; }
  @media (forced-colors: active) {
    .editor:focus-within {
      outline: 2px solid Highlight;
      outline-offset: -2px;
      box-shadow: none;
    }
  }
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
  .attachments button { width: 18px; height: 18px; border-radius: 50%; }
  .attachments button:hover { background: var(--border); }
  footer {
    display: flex; justify-content: flex-end; gap: 8px;
    padding: 10px 16px;
    border-top: 1px solid var(--border);
    background: color-mix(in oklab, var(--bg-sunken) 56%, var(--bg-raised));
  }
  .draft-state { margin-right: auto; align-self: center; color: var(--fg-subtle); font-size: 10px; }
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
