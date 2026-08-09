import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  app: {
    value: {
      layout: "three-pane",
      message: null as unknown,
      envelopes: [],
      selectedEnvelope: null as unknown,
      lastError: null,
    },
  },
  clearMessageView: vi.fn(),
  moveSelectedToRole: vi.fn(),
  openReplyComposer: vi.fn(),
  selectEnvelopeExclusive: vi.fn(),
  setLayout: vi.fn(),
  setSelectedFlag: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("./api", () => ({ api: { getAttachment: vi.fn() } }));
vi.mock("./stores.svelte", () => mocks);

import MessageView from "./MessageView.svelte";

const message = {
  envelope: {
    id: "message-1",
    mailboxId: "folder-1",
    subject: "Remote images",
    from: [{ name: "Sender", addr: "sender@example.org" }],
    to: [],
    date: null,
    flags: ["Seen"],
    keywords: [],
    hasAttachment: false,
    size: 10,
    serverUid: 1,
    messageId: "message@example.org",
    threadId: "thread@example.org",
  },
  text: "fallback",
  html: '<p>Body</p><img src="https://images.example/photo.png" />',
  headers: {
    cc: [],
    replyTo: [],
    sender: [],
    messageId: "message@example.org",
    inReplyTo: [],
    references: [],
    listUnsubscribe: [],
    authenticationResults: null,
  },
  attachments: [],
  parts: [],
  parseWarnings: [],
};

describe("MessageView remote content", () => {
  beforeEach(() => {
    const values = new Map<string, string>();
    Object.defineProperty(globalThis, "localStorage", {
      value: {
        getItem: (key: string) => values.get(key) ?? null,
        setItem: (key: string, value: string) => values.set(key, value),
        removeItem: (key: string) => values.delete(key),
        clear: () => values.clear(),
      },
      configurable: true,
    });
    mocks.app.value = {
      layout: "three-pane",
      message,
      envelopes: [],
      selectedEnvelope: message.envelope,
      lastError: null,
    };
  });

  it("offers a one-message remote image override", async () => {
    render(MessageView);
    expect(screen.getByText("Remote content blocked")).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Load images once" }));
    expect(screen.queryByText("Remote content blocked")).not.toBeInTheDocument();
  });

  it("renders HTML email in an isolated sandboxed document", () => {
    render(MessageView);

    const frame = screen.getByTitle("Email HTML body");
    expect(frame.tagName).toBe("IFRAME");
    expect(frame).toHaveAttribute("sandbox", "allow-same-origin");
    expect(frame.getAttribute("srcdoc")).toContain("<p>Body</p>");
    expect(frame.getAttribute("srcdoc")).toContain("Content-Security-Policy");
  });
});
