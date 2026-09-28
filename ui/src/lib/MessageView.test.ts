import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  app: {
    value: {
      layout: "three-pane",
      message: null as unknown,
      messageLoading: false,
      messageZoom: 100,
      threadCrossFolder: [] as unknown[],
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
    sources: [{ mailboxId: "folder-1", serverUid: 1 }],
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

describe("MessageView", () => {
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
      messageLoading: false,
      messageZoom: 100,
      threadCrossFolder: [] as unknown[],
      envelopes: [],
      selectedEnvelope: message.envelope,
      lastError: null,
    };
  });

  it("offers a one-message remote image override", async () => {
    render(MessageView);
    expect(screen.getByText("Remote content blocked")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Always allow these sources" })).toHaveClass("risk");
    const frame = screen.getByTitle("Email HTML body");
    expect(frame.getAttribute("srcdoc")).not.toContain('src="https://images.example/photo.png"');
    await fireEvent.click(screen.getByRole("button", { name: "Load images once" }));
    expect(screen.queryByText("Remote content blocked")).not.toBeInTheDocument();
    expect(frame.getAttribute("srcdoc")).toContain('src="https://images.example/photo.png"');
    expect(frame.getAttribute("srcdoc")).toContain("img-src data: cid: http: https:");
  });

  it("renders HTML email in an isolated sandboxed document", () => {
    render(MessageView);

    const frame = screen.getByTitle("Email HTML body");
    expect(frame.tagName).toBe("IFRAME");
    expect(frame).toHaveAttribute("sandbox", "allow-same-origin");
    expect(frame.getAttribute("srcdoc")).toContain("<p>Body</p>");
    expect(frame.getAttribute("srcdoc")).toContain("Content-Security-Policy");
  });

  it("shows the sender address in the message metadata", () => {
    render(MessageView);

    expect(screen.getByText("sender@example.org")).toBeInTheDocument();
  });

  it("announces message loading while showing a non-interactive skeleton", () => {
    mocks.app.value.message = null;
    mocks.app.value.messageLoading = true;
    render(MessageView);

    // Header meta keeps its own "Fetching…" status; the skeleton one is
    // the region carrying "Loading message".
    const status = screen.getByText("Loading message").closest('[role="status"]')!;
    expect(status).toHaveTextContent("Loading message");
    expect(status.querySelector('[aria-hidden="true"]')).not.toBeNull();
  });

  it("keeps a cached message visible while a refresh is in flight", () => {
    mocks.app.value.message = message;
    mocks.app.value.messageLoading = true;
    render(MessageView);

    expect(screen.getByTitle("Email HTML body")).toBeInTheDocument();
    expect(screen.queryByText("Loading message")).toBeNull();
    expect(screen.getByText("Fetching…")).toBeInTheDocument();
  });

  it("marks a loaded message as stored locally", () => {
    mocks.app.value.message = message;
    mocks.app.value.messageLoading = false;
    render(MessageView);

    expect(screen.getByText("Stored locally")).toBeInTheDocument();
  });

  it("exposes every message action as an icon control with a stable name and tooltip", () => {
    render(MessageView);

    for (const name of ["Reply", "Reply all", "Forward", "Archive", "Trash", "Junk", "Star"]) {
      const button = screen.getByRole("button", { name });
      expect(button).toHaveClass("icon");
      expect(button).toHaveAttribute("title", name);
      expect(button.querySelector("svg")).not.toBeNull();
    }
  });

  it("routes icon actions to their message handlers", async () => {
    render(MessageView);

    await fireEvent.click(screen.getByRole("button", { name: "Reply" }));
    expect(mocks.openReplyComposer).toHaveBeenCalledWith("reply");
    await fireEvent.click(screen.getByRole("button", { name: "Reply all" }));
    expect(mocks.openReplyComposer).toHaveBeenCalledWith("replyAll");
    await fireEvent.click(screen.getByRole("button", { name: "Forward" }));
    expect(mocks.openReplyComposer).toHaveBeenCalledWith("forward");

    await fireEvent.click(screen.getByRole("button", { name: "Archive" }));
    expect(mocks.moveSelectedToRole).toHaveBeenCalledWith("Archive");
    await fireEvent.click(screen.getByRole("button", { name: "Trash" }));
    expect(mocks.moveSelectedToRole).toHaveBeenCalledWith("Trash");
    await fireEvent.click(screen.getByRole("button", { name: "Junk" }));
    expect(mocks.moveSelectedToRole).toHaveBeenCalledWith("Junk");
  });

  it("keeps the star name stable and reports its state with aria-pressed", async () => {
    render(MessageView);

    const star = screen.getByRole("button", { name: "Star" });
    expect(star).toHaveAttribute("aria-pressed", "false");

    await fireEvent.click(star);
    expect(mocks.setSelectedFlag).toHaveBeenCalledWith("Flagged", true);
  });

  it("marks a flagged message's star as pressed and filled", () => {
    mocks.app.value.message = {
      ...message,
      envelope: { ...message.envelope, flags: ["Seen", "Flagged"] },
    };
    render(MessageView);

    const star = screen.getByRole("button", { name: "Star" });
    expect(star).toHaveAttribute("aria-pressed", "true");
    expect(star.querySelector("path")).toHaveAttribute("fill", "currentColor");
  });

  it("keeps the reading controls available next to the icon actions", async () => {
    render(MessageView);

    expect(screen.getByRole("button", { name: "HTML" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Text" })).toHaveAttribute("aria-pressed", "false");

    await fireEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(screen.getByText("Message-ID")).toBeInTheDocument();
  });

  it("applies_message_zoom_to_text_body", () => {
    mocks.app.value.message = { ...message, html: null, text: "plain body" };
    mocks.app.value.messageZoom = 150;
    const { container } = render(MessageView);

    const body = container.querySelector(".body");
    expect(body).toBeTruthy();
    expect(body?.getAttribute("style")).toContain("font-size: 150%");
  });

  it("header_paints_from_envelope_while_loading", () => {
    mocks.app.value.message = null;
    mocks.app.value.messageLoading = true;
    mocks.app.value.selectedEnvelope = {
      ...message.envelope,
      subject: "Warm subject",
    };
    render(MessageView);

    expect(screen.getByRole("heading", { name: "Warm subject" })).toBeInTheDocument();
    expect(document.querySelector(".message-skeleton")).toBeTruthy();
  });

  it("skeleton_confined_to_body", () => {
    mocks.app.value.message = null;
    mocks.app.value.messageLoading = true;
    mocks.app.value.selectedEnvelope = message.envelope;
    const { container } = render(MessageView);

    expect(container.querySelector("header .message-skeleton")).toBeNull();
    expect(container.querySelector("header")).toBeTruthy();
  });

  it("header_switches_to_message_when_loaded", () => {
    mocks.app.value.messageLoading = false;
    mocks.app.value.selectedEnvelope = {
      ...message.envelope,
      subject: "Stale envelope subject",
    };
    mocks.app.value.message = message;
    render(MessageView);

    expect(screen.getByRole("heading", { name: "Remote images" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Stale envelope subject" })).toBeNull();
  });
});
