import { describe, expect, it } from "vitest";
import type { Mailbox } from "./types";
import { sidebarFolders } from "./folderNav";

function folder(partial: Partial<Mailbox> & Pick<Mailbox, "id" | "name" | "role">): Mailbox {
  return {
    accountId: "acct",
    total: 1,
    unread: 0,
    sourceIds: [],
    ...partial,
  };
}

describe("sidebarFolders", () => {
  it("orders role folders then user folders and hides Gmail system mailboxes", () => {
    const folders = [
      folder({ id: "all", name: "[Gmail]/All Mail", role: "Other" }),
      folder({ id: "work", name: "Work", role: "Other" }),
      folder({ id: "trash", name: "[Gmail]/Trash", role: "Trash" }),
      folder({ id: "inbox", name: "INBOX", role: "Inbox" }),
      folder({ id: "important", name: "[Gmail]/Important", role: "Other" }),
      folder({ id: "sent", name: "[Gmail]/Sent Mail", role: "Sent" }),
    ];

    expect(sidebarFolders(folders).map((item) => item.id)).toEqual([
      "inbox",
      "sent",
      "trash",
      "work",
    ]);
  });
});
