import { vi } from "vitest";

const initialValue = {
  composerOpen: true,
  composerAccountId: "account-1",
  composerDraft: { to: "", cc: "", bcc: "", subject: "", html: "<p></p>" },
  composerAttachments: [] as { name: string; mime: string; size: number; dataBase64: string }[],
  composerThreading: { inReplyTo: null as string | null, references: [] as string[] },
  correspondents: [] as { addr: string; name: string | null }[],
  accounts: [{ id: "account-1", name: "Work", email: "me@example.org" }],
  sending: false,
};

export const app = $state({
  value: structuredClone(initialValue),
});

export const closeComposer = vi.fn();
export const sendComposer = vi.fn();

export function resetComposerTestStore() {
  app.value = structuredClone(initialValue);
}
