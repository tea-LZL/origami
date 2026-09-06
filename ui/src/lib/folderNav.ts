import type { Mailbox, MailboxRole } from "./types";

const ROLE_ORDER: MailboxRole[] = ["Inbox", "Drafts", "Sent", "Archive", "Junk", "Trash"];

const HIDDEN_OTHER_NAMES = /^(all mail|important|starred)$/i;
const GMAIL_NAMESPACE = /^\[(gmail|google mail)\]\//i;

export function sidebarFolders(folders: Mailbox[]): Mailbox[] {
  const visible = folders.filter((folder) => {
    if (folder.role !== "Other") return true;
    const leaf = folder.name.replace(GMAIL_NAMESPACE, "").trim();
    return !HIDDEN_OTHER_NAMES.test(leaf);
  });

  const byRole = new Map<MailboxRole, Mailbox>();
  const rest: Mailbox[] = [];
  for (const folder of visible) {
    if (folder.role === "Other") {
      rest.push(folder);
      continue;
    }
    if (!byRole.has(folder.role)) byRole.set(folder.role, folder);
  }
  rest.sort((a, b) => a.name.localeCompare(b.name));
  return [
    ...ROLE_ORDER.flatMap((role) => {
      const folder = byRole.get(role);
      return folder ? [folder] : [];
    }),
    ...rest,
  ];
}
