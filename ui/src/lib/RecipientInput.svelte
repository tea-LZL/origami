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
    if (!candidate) return;
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
      {@const valid = isAddress(recipient)}
      <span class="chip" class:invalid={!valid} role="listitem">
        <span
          class="chip-text"
          aria-invalid={valid ? undefined : "true"}
          title={valid ? undefined : "Not a valid email address"}
        >{recipient}</span>
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
    display: grid;
    grid-template-columns: 58px minmax(0, 1fr);
    align-items: center;
    gap: 12px;
    border-bottom: 1px solid color-mix(in oklab, var(--border) 72%, transparent);
    font-size: var(--fs-11);
    color: var(--fg-muted);
  }
  .recipients-label {
    color: var(--fg-subtle);
    font-size: var(--fs-9);
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    min-height: 38px;
    padding: 5px 0;
  }
  .chips:focus-within { box-shadow: inset 0 -2px var(--accent); }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 7px;
    border-radius: 999px;
    background: color-mix(in oklab, var(--accent) 14%, var(--bg-raised));
    color: var(--fg);
    font-size: var(--fs-11);
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
    font-size: var(--fs-12);
    line-height: 1;
    cursor: pointer;
  }
  .chip.invalid {
    background: color-mix(in oklab, var(--danger) 16%, var(--bg-raised));
    border: 1px solid color-mix(in oklab, var(--danger) 55%, var(--border));
  }
  .chip-remove:hover { background: var(--bg-sunken); color: var(--fg); }
  .chips input {
    flex: 1;
    min-width: 140px;
    padding: 4px 0;
    border: none;
    background: transparent;
    color: var(--fg);
    font-size: var(--fs-12);
    outline: none;
  }
</style>
