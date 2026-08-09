export interface RemoteContentAllowlist {
  origins: string[];
  senders: string[];
}

const STORAGE_KEY = "origami-remote-content-allowlist";
export const REMOTE_CONTENT_POLICY_EVENT = "origami-remote-content-policy-changed";

export function emptyRemoteContentAllowlist(): RemoteContentAllowlist {
  return { origins: [], senders: [] };
}

export function normalizeRemoteOrigin(value: string): string | null {
  try {
    const url = new URL(value.trim());
    if (url.protocol !== "http:" && url.protocol !== "https:") return null;
    return url.origin;
  } catch {
    return null;
  }
}

export function normalizeRemoteSender(value: string): string | null {
  const sender = value.trim().toLocaleLowerCase();
  return sender && sender.includes("@") ? sender : null;
}

export function loadRemoteContentAllowlist(): RemoteContentAllowlist {
  if (typeof localStorage === "undefined") return emptyRemoteContentAllowlist();
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null") as {
      origins?: unknown;
      senders?: unknown;
    } | null;
    return {
      origins: Array.isArray(saved?.origins)
        ? normalizeOrigins(saved.origins)
        : [],
      senders: Array.isArray(saved?.senders)
        ? normalizeSenders(saved.senders)
        : [],
    };
  } catch {
    return emptyRemoteContentAllowlist();
  }
}

export function allowRemoteOrigins(
  allowlist: RemoteContentAllowlist,
  origins: Iterable<string>,
): RemoteContentAllowlist {
  const next = {
    origins: normalizeOrigins([...allowlist.origins, ...origins]),
    senders: normalizeSenders(allowlist.senders),
  };
  saveRemoteContentAllowlist(next);
  return next;
}

export function allowRemoteSender(
  allowlist: RemoteContentAllowlist,
  sender: string,
): RemoteContentAllowlist {
  const next = {
    origins: normalizeOrigins(allowlist.origins),
    senders: normalizeSenders([...allowlist.senders, sender]),
  };
  saveRemoteContentAllowlist(next);
  return next;
}

export function revokeRemoteOrigin(
  allowlist: RemoteContentAllowlist,
  origin: string,
): RemoteContentAllowlist {
  const normalized = normalizeRemoteOrigin(origin);
  const next = {
    origins: allowlist.origins.filter((item) => item !== normalized),
    senders: normalizeSenders(allowlist.senders),
  };
  saveRemoteContentAllowlist(next);
  return next;
}

export function revokeRemoteSender(
  allowlist: RemoteContentAllowlist,
  sender: string,
): RemoteContentAllowlist {
  const normalized = normalizeRemoteSender(sender);
  const next = {
    origins: normalizeOrigins(allowlist.origins),
    senders: allowlist.senders.filter((item) => item !== normalized),
  };
  saveRemoteContentAllowlist(next);
  return next;
}

export function saveRemoteContentAllowlist(allowlist: RemoteContentAllowlist): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify({
    origins: normalizeOrigins(allowlist.origins),
    senders: normalizeSenders(allowlist.senders),
  }));
  if (typeof window !== "undefined") {
    window.dispatchEvent(new Event(REMOTE_CONTENT_POLICY_EVENT));
  }
}

function normalizeOrigins(values: unknown[]): string[] {
  return [...new Set(values
    .filter((value): value is string => typeof value === "string")
    .map(normalizeRemoteOrigin)
    .filter((value): value is string => value !== null))].sort();
}

function normalizeSenders(values: unknown[]): string[] {
  return [...new Set(values
    .filter((value): value is string => typeof value === "string")
    .map(normalizeRemoteSender)
    .filter((value): value is string => value !== null))].sort();
}
