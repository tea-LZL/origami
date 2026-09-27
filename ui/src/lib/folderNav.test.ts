import { describe, expect, it } from "vitest";
import type { Mailbox } from "./types";
import { flattenFolderTree, folderRoleHue, sidebarFolders, sidebarTree } from "./folderNav";

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

  it("nests user folders and rolls unread into a collapsed parent", () => {
    const folders = [
      folder({ id: "inbox", name: "INBOX", role: "Inbox", unread: 2 }),
      folder({ id: "work", name: "Work", role: "Other", unread: 1 }),
      folder({ id: "acme", name: "Work/Acme", role: "Other", unread: 3, total: 5 }),
    ];
    const tree = sidebarTree(folders);
    const collapsed = flattenFolderTree(tree, new Set());
    expect(collapsed.map((row) => row.id)).toEqual(["inbox", "work"]);
    expect(collapsed[1]?.label).toBe("Work");
    expect(collapsed[1]?.unread).toBe(4);
    expect(collapsed[1]?.expandable).toBe(true);

    const expanded = flattenFolderTree(tree, new Set(["Work"]));
    expect(expanded.map((row) => row.id)).toEqual(["inbox", "work", "acme"]);
    expect(expanded[1]?.unread).toBe(1);
    expect(expanded[2]?.label).toBe("Acme");
    expect(expanded[2]?.depth).toBe(1);
  });

  it("creates a virtual parent when only the child mailbox exists", () => {
    const folders = [
      folder({ id: "acme", name: "Work/Acme", role: "Other", unread: 3 }),
    ];
    const rows = flattenFolderTree(sidebarTree(folders), new Set());
    expect(rows).toHaveLength(1);
    expect(rows[0]?.id).toBe("virtual:Work");
    expect(rows[0]?.unread).toBe(3);
    expect(rows[0]?.folder).toBeNull();
  });
});

describe("folderRoleHue", () => {
  it("folder_role_hue_maps_specials", () => {
    expect(folderRoleHue("Inbox")).toBe("inbox");
    expect(folderRoleHue("Drafts")).toBe("drafts");
    expect(folderRoleHue("Sent")).toBe("sent");
    expect(folderRoleHue("Junk")).toBe("junk");
    expect(folderRoleHue("Trash")).toBe("trash");
    expect(folderRoleHue("Archive")).toBe("archive");
    expect(folderRoleHue("Other")).toBe("neutral");
    expect(folderRoleHue(null)).toBe("neutral");
  });

  it("semantic_surfaces_have_text", () => {
    const folders = [folder({ id: "inbox", name: "INBOX", role: "Inbox" })];
    const rows = flattenFolderTree(sidebarTree(folders), new Set());
    // The hue is decoration; the node label is the accessible name source.
    expect(rows[0]?.label).toBe("Inbox");
  });
});

