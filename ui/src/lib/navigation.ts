export type NavigationShortcut = "1" | "2" | "3";

type KeyboardAction = (event: KeyboardEvent, node: HTMLElement) => void;

function focusElement(element: HTMLElement | null): boolean {
  if (!element) return false;
  element.focus();
  return true;
}

export function focusNavigationTarget(
  shortcut: NavigationShortcut,
  selectedMessageId: string | null,
): boolean {
  if (shortcut === "1") {
    return focusElement(
      document.querySelector<HTMLElement>(
        '[data-navigation="sidebar"] .folder.selected, [data-navigation="sidebar"] .unified.selected',
      )
      ?? document.querySelector<HTMLElement>('[data-navigation="sidebar"] .unified'),
    );
  }

  if (shortcut === "2") {
    const rows = document.querySelectorAll<HTMLElement>(
      '[data-navigation="message-list"] [data-item-id]',
    );
    const selectedRow = [...rows].find((row) => row.dataset.itemId === selectedMessageId);
    return focusElement(
      selectedRow
      ?? document.querySelector<HTMLElement>('[data-navigation="message-list"]'),
    );
  }

  return focusElement(document.querySelector<HTMLElement>('[data-navigation="message-body"]'));
}

export const handleSidebarKeydown: KeyboardAction = (event, node) => {
  if (event.defaultPrevented) return;
  const target = event.target as Element | null;
  const focusedButton = target?.closest<HTMLButtonElement>("button");
  if (focusedButton && !focusedButton.matches(".unified, .folder")) return;
  if (target?.closest("input, textarea, [contenteditable='true'], [role='dialog'], [role='menu']")) return;

  const direction = event.key === "j" || event.key === "ArrowDown"
    ? 1
    : event.key === "k" || event.key === "ArrowUp"
      ? -1
      : 0;
  if (direction === 0) return;

  const buttons = [...node.querySelectorAll<HTMLButtonElement>("button.unified, button.folder")];
  if (buttons.length === 0) return;
  const active = focusedButton && buttons.includes(focusedButton)
    ? focusedButton
    : node.ownerDocument.activeElement as HTMLButtonElement | null;
  const currentIndex = active ? buttons.indexOf(active) : buttons.findIndex((button) =>
    button.classList.contains("selected"),
  );
  const nextIndex = Math.max(0, Math.min(
    currentIndex < 0 ? 0 : currentIndex + direction,
    buttons.length - 1,
  ));
  event.preventDefault();
  buttons[nextIndex].focus();
  buttons[nextIndex].click();
};

/**
 * Scroll the message body by `delta` px. The HTML body lives in a sandboxed
 * iframe that owns its own document scroll (the composited fast path); the
 * plain-text body scrolls the wrapper element directly.
 */
export function scrollMessageBody(node: HTMLElement, delta: number): boolean {
  const contentWindow = node.querySelector("iframe")?.contentWindow;
  if (contentWindow) {
    contentWindow.scrollBy({ top: delta, left: 0 });
    return true;
  }
  node.scrollTop = Math.max(0, node.scrollTop + delta);
  return true;
}

export const handleMessageBodyKeydown: KeyboardAction = (event, node) => {
  if (event.defaultPrevented) return;
  const direction = event.key === "j" || event.key === "ArrowDown"
    ? 1
    : event.key === "k" || event.key === "ArrowUp"
      ? -1
      : 0;
  if (direction === 0) return;

  event.preventDefault();
  const step = Math.max(120, node.clientHeight * 0.8);
  scrollMessageBody(node, direction * step);
};

export function sidebarKeyboard(node: HTMLElement) {
  const onKeydown = (event: KeyboardEvent) => handleSidebarKeydown(event, node);
  node.addEventListener("keydown", onKeydown);
  return {
    destroy() {
      node.removeEventListener("keydown", onKeydown);
    },
  };
}

export function messageBodyKeyboard(node: HTMLElement) {
  const onKeydown = (event: KeyboardEvent) => handleMessageBodyKeydown(event, node);
  node.addEventListener("keydown", onKeydown);
  return {
    destroy() {
      node.removeEventListener("keydown", onKeydown);
    },
  };
}
