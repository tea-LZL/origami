<script lang="ts">
  import type { Correspondent } from "./api";

  interface Props {
    value: string;
    label: string;
    correspondents?: Correspondent[];
    onChange?: (value: string) => void;
    focusOnMount?: boolean;
  }

  let { value, label, correspondents = [], onChange, focusOnMount = false }: Props = $props();

  let typed = $state("");

  function parseRecipients(raw: string): string[] {
    return raw
      .split(/[,\s]+/)
      .map((item) => item.trim())
      .filter(Boolean);
  }

  const recipients = $derived(parseRecipients(value));
  const listId = $derived(`recipients-${label.toLowerCase()}`);

  function isAddress(candidate: string): boolean {
    return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(candidate);
  }

  function commit(): void {
    const candidate = typed.trim();
    typed = "";
    if (!candidate || !isAddress(candidate)) return;
    if (recipients.some((item) => item.toLowerCase() === candidate.toLowerCase())) return;
    onChange?.([...recipients, candidate].join(", "));
  }

  function remove(recipient: string): void {
    onChange?.(recipients.filter((item) => item !== recipient).join(", "));
  }

  function removeLast(): void {
    if (recipients.length === 0) return;
    onChange?.(recipients.slice(0, -1).join(", "));
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter" || event.key === ",") {
      event.preventDefault();
      commit();
    } else if (event.key === "Backspace" && typed === "") {
      event.preventDefault();
      removeLast();
    }
  }

  function autofocusAction(node: HTMLInputElement) {
    if (!focusOnMount) return;
    const frame = requestAnimationFrame(() => node.focus());
    return { destroy() { cancelAnimationFrame(frame); } };
  }
</script>

<label class="recipients">
  <span class="recipients-label">{label}</span>
  <span class="chips" role="list" aria-label={`${label} recipients`}>
    {#each recipients as recipient (recipient)}
      <span class="chip" role="listitem">
        <span class="chip-text">{recipient}</span>
        <button
          type="button"
          class="chip-remove"
          aria-label={`Remove ${recipient}`}
          onclick={() => remove(recipient)}
        >×</button>
      </span>
    {/each}
    <input
      use:autofocusAction
      type="text"
      aria-label={label}
      role="textbox"
      list={listId}
      bind:value={typed}
      onkeydown={onKeydown}
      onblur={commit}
      placeholder={recipients.length === 0 ? `Add ${label.toLowerCase()} recipient` : ""}
    />
  </span>
</label>

<datalist id={listId}>
  {#each correspondents as contact (contact.addr)}
    <option value={contact.addr}>{contact.name ?? contact.addr}</option>
  {/each}
</datalist>

<style>
  .recipients {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--fg-muted);
  }
  .recipients-label { font-size: 11px; }
  .chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    min-height: 30px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-raised);
  }
  .chips:focus-within { border-color: var(--accent); }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 6px;
    border-radius: 999px;
    background: color-mix(in oklab, var(--accent) 18%, var(--bg-raised));
    color: var(--fg);
    font-size: 11px;
  }
  .chip-text { max-width: 240px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .chip-remove {
    width: 16px;
    height: 16px;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--fg-muted);
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
  }
  .chip-remove:hover { background: var(--bg-sunken); color: var(--fg); }
  .chips input {
    flex: 1;
    min-width: 140px;
    border: none;
    background: transparent;
    color: var(--fg);
    font-size: 12px;
    outline: none;
  }
</style>
