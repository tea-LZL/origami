import type { AccountDto, MessageDto } from "../src/lib/api";
import type { Envelope, Flag, Mailbox } from "../src/lib/types";

export const account = (): AccountDto => ({
  id: "acc-1",
  dbId: "db-1",
  name: "Work",
  email: "me@example.org",
  hasImap: true,
  hasSmtp: true,
});

export const inbox = (): Mailbox => ({
  subscribed: true,
  id: "inbox",
  // Folders key to the account's dbId (Sidebar rowsFor / openComposer), not its id.
  accountId: "db-1",
  name: "Inbox",
  role: "Inbox",
  total: 1,
  unread: 1,
  sourceIds: ["inbox"],
});

export const envelope = (serverUid: number, subject: string, flags: Flag[] = []): Envelope => ({
  id: `envelope-${serverUid}`,
  mailboxId: "inbox",
  subject,
  from: [{ name: "Ada Lovelace", addr: "ada@example.org" }],
  to: [{ name: "Me", addr: "me@example.org" }],
  date: "Mon, 26 Oct 2026 09:00:00 +0000",
  receivedAt: 1_793_000_000,
  flags,
  hasAttachment: false,
  size: 2048,
  serverUid,
  messageId: `<message-${serverUid}@example.org>`,
  threadId: `<thread-${serverUid}@example.org>`,
  keywords: [],
  sources: [{ mailboxId: "inbox", serverUid }],
});

export const message = (item: Envelope, text: string): MessageDto => ({
  envelope: item,
  text,
  html: null,
  headers: {
    cc: [],
    replyTo: [],
    sender: [],
    messageId: item.messageId,
    inReplyTo: [],
    references: [],
    listUnsubscribe: [],
    authenticationResults: null,
  },
  attachments: [],
  parts: [],
  parseWarnings: [],
});
