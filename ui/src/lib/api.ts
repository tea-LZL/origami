import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Envelope, Flag, Mailbox } from "./types";

export interface AccountDto {
  id: string;
  dbId: string;
  name: string;
  email: string;
  hasImap: boolean;
  hasSmtp: boolean;
}

export interface AccountStatusDto {
  state: "online" | "syncing" | "error";
  error: string | null;
  pendingOperations: number;
}

export interface OutboxSummary {
  id: number;
  kind: string;
  detail: string;
  createdAt: number;
  attempts: number;
  lastError: string | null;
}

export interface AttachmentMeta {
  index: number;
  partPath: string;
  name: string | null;
  mime: string;
  size: number;
  inline: boolean;
  cid: string | null;
}

export interface MessageHeaders {
  cc: { name: string | null; addr: string }[];
  replyTo: { name: string | null; addr: string }[];
  sender: { name: string | null; addr: string }[];
  messageId: string | null;
  inReplyTo: string[];
  references: string[];
  listUnsubscribe: string[];
  authenticationResults: string | null;
}

export interface MimePart {
  path: string;
  mime: string;
  charset: string | null;
  disposition: string | null;
  filename: string | null;
  contentId: string | null;
  contentLocation: string | null;
  encodedSize: number;
  decodedSize: number;
  inline: boolean;
  attachment: boolean;
  text: boolean;
  html: boolean;
  encodingProblem: boolean;
}

export interface MessageDto {
  envelope: Envelope;
  text: string | null;
  html: string | null;
  headers: MessageHeaders;
  attachments: AttachmentMeta[];
  parts: MimePart[];
  parseWarnings: string[];
}

export interface Draft {
  fromName: string | null;
  fromAddr: string;
  to: string[];
  cc?: string[];
  bcc?: string[];
  subject: string;
  html: string;
  text?: string | null;
  inReplyTo?: string | null;
  references?: string[];
  attachments?: {
    name: string;
    mime: string;
    dataBase64: string;
  }[];
}

export interface SavedComposerDraft {
  accountId: string | null;
  draft: {
    to: string;
    cc: string;
    bcc: string;
    subject: string;
    html: string;
  };
  attachments: {
    name: string;
    mime: string;
    size: number;
    dataBase64: string;
  }[];
  threading?: {
    inReplyTo: string | null;
    references: string[];
  };
}

export interface SavedSearch {
  id: string;
  name: string;
  query: string;
}

export interface Correspondent {
  name: string | null;
  addr: string;
  messageCount: number;
}

export interface NotificationSettings {
  preview: "full" | "sender_only" | "hidden";
  folderScope: "all" | "inbox";
  quietHours: { start: string; end: string } | null;
}

export const api = {
  appInfo: () => invoke<{ name: string; coreVersion: string; shellVersion: string }>("app_info"),
  listAccounts: () => invoke<AccountDto[]>("list_accounts"),
  listFolders: (accountDbId?: string) => invoke<Mailbox[]>("list_folders", { accountDbId: accountDbId ?? null }),
  createFolder: (accountId: string, name: string) =>
    invoke<void>("create_folder", { accountId, name }),
  renameFolder: (folderId: string, name: string) =>
    invoke<void>("rename_folder", { folderId, name }),
  deleteFolder: (folderId: string) => invoke<void>("delete_folder", { folderId }),
  listEnvelopes: (folderId: string, page = 1, pageSize = 50) =>
    invoke<Envelope[]>("list_envelopes", { folderId, page, pageSize }),
  listUnifiedInbox: (page = 1, pageSize = 50) =>
    invoke<Envelope[]>("list_unified_inbox", { page, pageSize }),
  getCachedMessage: (folderId: string, serverUid: number) =>
    invoke<MessageDto | null>("get_cached_message", { folderId, serverUid }),
  getMessage: (folderId: string, serverUid: number) =>
    invoke<MessageDto>("get_message", { folderId, serverUid }),
  getAttachment: (folderId: string, serverUid: number, index: number) =>
    invoke<string>("get_attachment", { folderId, serverUid, index }),
  storeFlags: (folderId: string, serverUid: number, flags: Flag[]) =>
    invoke<void>("store_flags", { folderId, serverUid, flags }),
  storeFlagsBatch: (folderId: string, updates: { serverUid: number; flags: Flag[] }[]) =>
    invoke<void>("store_flags_batch", { folderId, updates }),
  storeKeywordsBatch: (
    folderId: string,
    updates: { serverUid: number; flags: Flag[]; keywords: string[] }[],
  ) => invoke<void>("store_keywords_batch", { folderId, updates }),
  moveMessages: (folderId: string, destinationFolderId: string, serverUids: number[]) =>
    invoke<void>("move_messages", { folderId, destinationFolderId, serverUids }),
  deleteMessages: (folderId: string, serverUids: number[]) =>
    invoke<void>("delete_messages", { folderId, serverUids }),
  syncNow: (accountId?: string) => invoke<void>("sync_now", { accountId: accountId ?? null }),
  search: (query: string, limit = 50) => invoke<Envelope[]>("search", { query, limit }),
  searchPage: (query: string, page = 1, pageSize = 200) =>
    invoke<Envelope[]>("search_page", { query, page, pageSize }),
  listSavedSearches: () => invoke<SavedSearch[]>("list_saved_searches"),
  saveSearch: (name: string, query: string) =>
    invoke<SavedSearch>("save_search", { name, query }),
  deleteSavedSearch: (id: string) => invoke<void>("delete_saved_search", { id }),
  listCorrespondents: (limit = 200) => invoke<Correspondent[]>("list_correspondents", { limit }),
  saveComposerDraft: (draft: SavedComposerDraft) =>
    invoke<void>("save_composer_draft", { draft }),
  loadComposerDraft: () => invoke<SavedComposerDraft | null>("load_composer_draft"),
  syncComposerDraft: () => invoke<void>("sync_composer_draft"),
  deleteComposerDraft: () => invoke<void>("delete_composer_draft"),
  sendMessage: (draft: Draft) => invoke<{ queued: boolean }>("send_message", { draft }),
  removeAccount: (accountId: string) => invoke<AccountDto[]>("remove_account", { accountId }),
  accountStatuses: () => invoke<Record<string, AccountStatusDto>>("account_statuses"),
  listOutbox: (accountId: string) => invoke<OutboxSummary[]>("list_outbox", { accountId }),
  retryOutbox: (accountId: string) => invoke<void>("retry_outbox", { accountId }),
  getNotificationSettings: () =>
    invoke<NotificationSettings>("get_notification_settings"),
  updateNotificationSettings: (settings: NotificationSettings) =>
    invoke<void>("update_notification_settings", { settings }),
  updateAccount: (args: {
    accountId: string;
    name?: string;
    email?: string;
    imapHost?: string;
    imapPort?: number;
    smtpHost?: string;
    smtpPort?: number;
    auth?: string;
    username?: string;
    password?: string;
    oauthAccessToken?: string;
    oauthRefreshToken?: string;
  }) => invoke<void>("update_account", args),
};

export function onSyncEvent(cb: (event: unknown) => void): Promise<UnlistenFn> {
  return listen("sync-event", (e) => cb(e.payload));
}
