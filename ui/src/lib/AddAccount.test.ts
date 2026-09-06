import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  openUrl: vi.fn(),
  listFolders: vi.fn(),
  pollAccountErrors: vi.fn(),
  app: {
    value: {
      accounts: [] as { id: string; dbId: string; name: string; email: string }[],
      folders: [] as { id: string; accountId: string; role: string }[],
      accountStatuses: {} as Record<string, { state: string }>,
    },
  },
  selectFolder: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: mocks.openUrl }));
vi.mock("./stores.svelte", () => ({
  app: mocks.app,
  selectFolder: mocks.selectFolder,
  pollAccountErrors: mocks.pollAccountErrors,
}));
vi.mock("./api", () => ({
  api: {
    listFolders: mocks.listFolders,
  },
}));

import AddAccount from "./AddAccount.svelte";

async function openWizard() {
  const view = render(AddAccount);
  (view.component as unknown as { show: () => void }).show();
  await screen.findByRole("heading", { name: "Add an email account" });
  return view;
}

async function detectAs(hints: Record<string, unknown>) {
  mocks.invoke.mockResolvedValueOnce(hints);
  await fireEvent.input(screen.getByLabelText("Email address"), {
    target: { value: hints.email as string },
  });
  await fireEvent.click(screen.getByRole("button", { name: "Next" }));
  await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("provider_hints", expect.anything()));
}

const gmailHints = {
  email: "me@gmail.com",
  description: "Google / Gmail",
  imapHost: "imap.gmail.com",
  imapPort: 993,
  smtpHost: "smtp.gmail.com",
  smtpPort: 465,
  auth: "login",
  oauthProvider: "google",
};

const microsoftHints = {
  email: "me@outlook.com",
  description: "Microsoft / Outlook",
  imapHost: "outlook.office365.com",
  imapPort: 993,
  smtpHost: "smtp.office365.com",
  smtpPort: 587,
  auth: "xoauth2",
  oauthProvider: "microsoft",
};

describe("Add account wizard", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.openUrl.mockReset();
    mocks.selectFolder.mockReset();
    mocks.listFolders.mockReset();
    mocks.pollAccountErrors.mockReset();
    mocks.listFolders.mockResolvedValue([]);
    mocks.pollAccountErrors.mockResolvedValue(undefined);
    mocks.app.value.accounts = [];
    mocks.app.value.folders = [];
    mocks.app.value.accountStatuses = {};
  });

  it("uses an app password for Gmail and does not offer Google OAuth", async () => {
    await openWizard();
    await detectAs(gmailHints);

    expect(screen.queryByRole("button", { name: /Sign in with Google/i })).toBeNull();
    expect(screen.getByLabelText("App password")).toBeInTheDocument();
    expect(screen.getByText(/2-Step Verification/i)).toBeInTheDocument();
    expect(screen.queryByLabelText("Host")).toBeNull();
    expect(screen.getByRole("button", { name: "Advanced" })).toBeInTheDocument();
  });

  it("keeps Microsoft OAuth as the primary path", async () => {
    await openWizard();
    await detectAs(microsoftHints);

    expect(screen.getByRole("button", { name: /Sign in with Microsoft/i })).toBeInTheDocument();
  });

  it("selects the new account Inbox after a successful save", async () => {
    await openWizard();
    await detectAs(gmailHints);
    await fireEvent.input(screen.getByLabelText("App password"), {
      target: { value: "abcd efgh ijkl mnop" },
    });
    mocks.invoke.mockResolvedValueOnce({
      id: "me",
      dbId: "db-1",
      name: "me",
      email: "me@gmail.com",
    });
    mocks.listFolders.mockResolvedValueOnce([
      { id: "inbox-1", accountId: "db-1", role: "Inbox" },
    ]);

    await fireEvent.click(screen.getByRole("button", { name: "Save app password" }));

    await waitFor(() => expect(mocks.selectFolder).toHaveBeenCalledWith("inbox-1"));
    expect(screen.getByRole("heading", { name: "Account added" })).toBeInTheDocument();
    expect(screen.getByText(/syncing Inbox/i)).toBeInTheDocument();
  });
});
