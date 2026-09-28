// Domain types mirroring the Rust models (camelCase via serde).

export type MailboxRole = "Inbox" | "Sent" | "Drafts" | "Trash" | "Archive" | "Junk" | "Other";
export type Flag = "Seen" | "Answered" | "Flagged" | "Deleted" | "Draft";

export interface Address {
  name: string | null;
  addr: string;
}

export interface EnvelopeSource {
  mailboxId: string;
  serverUid: number;
}

export interface Mailbox {
  subscribed: boolean;
  id: string;
  accountId: string;
  name: string;
  role: MailboxRole;
  total: number;
  unread: number;
  sourceIds: string[];
}

export interface Envelope {
  id: string;
  mailboxId: string;
  subject: string;
  from: Address[];
  to: Address[];
  date: string | null;
  /** Server-received time (IMAP INTERNALDATE), when available. */
  receivedAt?: number | null;
  flags: Flag[];
  hasAttachment: boolean;
  size: number;
  serverUid: number | null;
  messageId: string | null;
  threadId: string | null;
  keywords: string[];
  sources: EnvelopeSource[];
}
