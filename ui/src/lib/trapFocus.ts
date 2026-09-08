const FOCUSABLE = "button, [href], input, select, textarea, [tabindex]:not([tabindex='-1'])";

function focusableIn(node: HTMLElement): HTMLElement[] {
  return [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (element) => !element.hasAttribute("disabled") && element.tabIndex !== -1,
  );
}

export function trapFocus(node: HTMLElement) {
  const previously = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const frame = requestAnimationFrame(() => {
    if (node.contains(document.activeElement)) return;
    focusableIn(node)[0]?.focus();
  });

  function onKeydown(event: KeyboardEvent) {
    if (event.key !== "Tab") return;
    const items = focusableIn(node);
    if (items.length === 0) return;
    const first = items[0];
    const last = items[items.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  node.addEventListener("keydown", onKeydown);
  return {
    destroy() {
      cancelAnimationFrame(frame);
      node.removeEventListener("keydown", onKeydown);
      previously?.focus();
    },
  };
}
