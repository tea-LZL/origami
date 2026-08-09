import { beforeEach, describe, expect, it } from "vitest";
import {
  allowRemoteOrigins,
  allowRemoteSender,
  emptyRemoteContentAllowlist,
  loadRemoteContentAllowlist,
  normalizeRemoteOrigin,
  normalizeRemoteSender,
  revokeRemoteOrigin,
  revokeRemoteSender,
} from "./remoteContent";

describe("remoteContent", () => {
  const values = new Map<string, string>();
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
    removeItem: (key: string) => values.delete(key),
    clear: () => values.clear(),
  };

  beforeEach(() => {
    Object.defineProperty(globalThis, "localStorage", { value: storage, configurable: true });
    storage.clear();
  });

  it("normalizes exact HTTP origins and sender addresses", () => {
    expect(normalizeRemoteOrigin("HTTPS://Images.Example/path")).toBe("https://images.example");
    expect(normalizeRemoteOrigin("data:image/png;base64,abc")).toBeNull();
    expect(normalizeRemoteSender("  Alice@Example.org ")).toBe("alice@example.org");
    expect(normalizeRemoteSender("unknown")).toBeNull();
  });

  it("persists deduplicated source and sender permissions", () => {
    let allowlist = emptyRemoteContentAllowlist();
    allowlist = allowRemoteOrigins(allowlist, [
      "https://images.example/a",
      "https://images.example/b",
    ]);
    allowlist = allowRemoteSender(allowlist, "Sender@Example.org");

    expect(loadRemoteContentAllowlist()).toEqual({
      origins: ["https://images.example"],
      senders: ["sender@example.org"],
    });
    expect(allowlist).toEqual(loadRemoteContentAllowlist());
  });

  it("fails closed for malformed saved state", () => {
    localStorage.setItem("origami-remote-content-allowlist", "not-json");
    expect(loadRemoteContentAllowlist()).toEqual(emptyRemoteContentAllowlist());
  });

  it("supports revoking individual permissions", () => {
    let allowlist = allowRemoteOrigins(emptyRemoteContentAllowlist(), ["https://images.example"]);
    allowlist = allowRemoteSender(allowlist, "sender@example.org");
    allowlist = revokeRemoteOrigin(allowlist, "https://images.example/path");
    allowlist = revokeRemoteSender(allowlist, "SENDER@example.org");

    expect(allowlist).toEqual(emptyRemoteContentAllowlist());
    expect(loadRemoteContentAllowlist()).toEqual(emptyRemoteContentAllowlist());
  });
});
