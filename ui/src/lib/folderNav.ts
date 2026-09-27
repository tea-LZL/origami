import type { Mailbox, MailboxRole } from "./types";

const ROLE_ORDER: MailboxRole[] = ["Inbox", "Drafts", "Sent", "Archive", "Junk", "Trash"];

const HIDDEN_OTHER_NAMES = /^(all mail|important|starred)$/i;
const GMAIL_NAMESPACE = /^\[(gmail|google mail)\]\//i;

export type FolderNode = {
  path: string;
  label: string;
  folder: Mailbox | null;
  children: FolderNode[];
};

export type FolderRow = {
  id: string;
  path: string;
  label: string;
  depth: number;
  folder: Mailbox | null;
  unread: number;
  total: number;
  expandable: boolean;
  expanded: boolean;
};

export type FolderHue =
  | "inbox"
  | "drafts"
  | "sent"
  | "junk"
  | "trash"
  | "archive"
  | "neutral";

/** Decorative hue for a special-role folder (the label is the a11y name). */
export function folderRoleHue(role: string | null | undefined): FolderHue {
  switch (role) {
    case "Inbox": return "inbox";
    case "Drafts": return "drafts";
    case "Sent": return "sent";
    case "Junk": return "junk";
    case "Trash": return "trash";
    case "Archive": return "archive";
    default: return "neutral";
  }
}

export function sidebarFolders(folders: Mailbox[]): Mailbox[] {
  const tree = sidebarTree(folders);
  return flattenFolderTree(tree, allExpandablePaths(tree))
    .map((row) => row.folder)
    .filter((folder): folder is Mailbox => folder != null);
}

export function sidebarTree(folders: Mailbox[]): FolderNode[] {
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

  const roleNodes = ROLE_ORDER.flatMap((role) => {
    const folder = byRole.get(role);
    if (!folder) return [];
    return [{
      path: folder.id,
      label: role,
      folder,
      children: [] as FolderNode[],
    }];
  });

  return [...roleNodes, ...userFolderTree(rest)];
}

export function flattenFolderTree(nodes: FolderNode[], expanded: Set<string>): FolderRow[] {
  const rows: FolderRow[] = [];
  function walk(node: FolderNode, depth: number) {
    const isExpanded = expanded.has(node.path);
    const ownUnread = node.folder?.unread ?? 0;
    const ownTotal = node.folder?.total ?? 0;
    const childUnread = node.children.reduce((sum, child) => sum + unreadOf(child), 0);
    const childTotal = node.children.reduce((sum, child) => sum + totalOf(child), 0);
    rows.push({
      id: node.folder?.id ?? `virtual:${node.path}`,
      path: node.path,
      label: node.label,
      depth,
      folder: node.folder,
      unread: isExpanded ? ownUnread : ownUnread + childUnread,
      total: isExpanded ? ownTotal : ownTotal + childTotal,
      expandable: node.children.length > 0,
      expanded: isExpanded,
    });
    if (isExpanded) {
      for (const child of node.children) walk(child, depth + 1);
    }
  }
  for (const node of nodes) walk(node, 0);
  return rows;
}

function unreadOf(node: FolderNode): number {
  return (node.folder?.unread ?? 0) + node.children.reduce((sum, child) => sum + unreadOf(child), 0);
}

function totalOf(node: FolderNode): number {
  return (node.folder?.total ?? 0) + node.children.reduce((sum, child) => sum + totalOf(child), 0);
}

function allExpandablePaths(nodes: FolderNode[]): Set<string> {
  const paths = new Set<string>();
  function walk(node: FolderNode) {
    if (node.children.length > 0) paths.add(node.path);
    for (const child of node.children) walk(child);
  }
  for (const node of nodes) walk(node);
  return paths;
}

function inferDelimiter(names: string[]): "/" | "." {
  let slashes = 0;
  let dots = 0;
  for (const name of names) {
    if (name.includes("/")) slashes += 1;
    if (name.includes(".") && !name.includes("@")) dots += 1;
  }
  return slashes >= dots ? "/" : ".";
}

function userFolderTree(folders: Mailbox[]): FolderNode[] {
  const delimiter = inferDelimiter(folders.map((folder) => folder.name));
  const roots: FolderNode[] = [];
  const index = new Map<string, FolderNode>();

  function ensure(path: string, label: string): FolderNode {
    const existing = index.get(path);
    if (existing) return existing;
    const node: FolderNode = { path, label, folder: null, children: [] };
    index.set(path, node);
    const parentPath = path.includes(delimiter)
      ? path.slice(0, path.lastIndexOf(delimiter))
      : "";
    if (!parentPath) {
      roots.push(node);
    } else {
      const parentLabel = parentPath.includes(delimiter)
        ? parentPath.slice(parentPath.lastIndexOf(delimiter) + delimiter.length)
        : parentPath;
      ensure(parentPath, parentLabel).children.push(node);
    }
    return node;
  }

  const sorted = [...folders].sort((a, b) => a.name.localeCompare(b.name));
  for (const folder of sorted) {
    const segments = folder.name.split(delimiter).filter(Boolean);
    if (segments.length === 0) continue;
    let path = "";
    segments.forEach((segment, offset) => {
      path = path ? `${path}${delimiter}${segment}` : segment;
      const node = ensure(path, segment);
      if (offset === segments.length - 1) node.folder = folder;
    });
  }

  function sortNodes(nodes: FolderNode[]) {
    nodes.sort((a, b) => a.label.localeCompare(b.label));
    for (const node of nodes) sortNodes(node.children);
  }
  sortNodes(roots);
  return roots;
}
