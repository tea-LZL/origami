import { vi } from "vitest";

export function makeSession(id: string, overrides: Record<string, unknown> = {}) {
  return {
    id,
    accountId: "account-1",
    draft: { to: "", cc: "", bcc: "", subject: "Draft subject", html: "<p></p>", composeMode: "rich" },
    attachments: [] as { name: string; mime: string; size: number; dataBase64: string }[],
    threading: { inReplyTo: null as string | null, references: [] as string[] },
    saveState: "idle",
    syncState: "local",
    lastSavedAt: null as number | null,
    lastSyncedHash: null as string | null,
    ...overrides,
  };
}

const initialValue = {
  accounts: [{ id: "account-1", name: "Work", email: "me@example.org" }],
  folders: [],
  selectedFolderId: null,
  correspondents: [],
  composerSessions: [makeSession("session-1")],
  activeComposerId: "session-1" as string | null,
  sendingComposerId: null as string | null,
  discardConfirmId: null as string | null,
  lastError: null as string | null,
  lastNotice: null as string | null,
};

export const app = $state({
  value: structuredClone(initialValue),
});

export const activateComposer = vi.fn();
export const minimizeComposer = vi.fn();
export const requestDiscardComposer = vi.fn();
export const cancelDiscardComposer = vi.fn();
export const discardComposer = vi.fn();
export const sendComposer = vi.fn();
export const scheduleComposerSave = vi.fn();
export const flushComposerSave = vi.fn();
export const syncComposerNow = vi.fn();
export const openComposer = vi.fn();
export const openReplyComposer = vi.fn();

export function resetComposerTestStore() {
  app.value = structuredClone(initialValue);
}
