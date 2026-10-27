import type { Page } from "@playwright/test";
import type { AccountDto, MessageDto } from "../src/lib/api";
import type { Envelope, Mailbox } from "../src/lib/types";
import { account, inbox } from "./fixtures";

/** Commands the stub answers with caller-supplied data. Everything else gets a
 *  benign default so the shell can boot without a Rust process. */
export interface Scenario {
  accounts?: AccountDto[];
  folders?: Mailbox[];
  envelopes?: Envelope[];
  searchResults?: Envelope[];
  /** Keyed by `${mailboxId}:${serverUid}`. */
  messages?: Record<string, MessageDto>;
}

export interface RecordedCall {
  cmd: string;
  args: Record<string, unknown>;
}

declare global {
  interface Window {
    __TAURI_CALLS__?: RecordedCall[];
    __TAURI_INTERNALS__?: unknown;
  }
}

const defaults = (): Required<Scenario> => ({
  accounts: [account()],
  folders: [inbox()],
  envelopes: [],
  searchResults: [],
  messages: {},
});

/** Install the Tauri bridge before any page script runs and record every
 *  command so tests can assert what reached the shell. */
export async function installTauri(page: Page, scenario: Scenario = {}): Promise<void> {
  const data = { ...defaults(), ...scenario };
  await page.addInitScript((seed) => {
    const calls: RecordedCall[] = [];
    const callbacks = new Map<number, unknown>();
    window.__TAURI_CALLS__ = calls;
    const invoke = async (cmd: string, args: Record<string, unknown>) => {
      calls.push({ cmd, args });
      switch (cmd) {
        case "app_info":
          return { name: "Origami", coreVersion: "e2e", shellVersion: "e2e" };
        case "list_accounts":
          return seed.accounts;
        case "list_folders":
          return seed.folders;
        case "account_statuses":
          return {};
        case "list_envelopes":
        case "list_unified_inbox":
          return seed.envelopes;
        case "search_page":
          return seed.searchResults;
        case "search_count":
          return seed.searchResults.length;
        case "get_cached_message":
        case "get_message":
          return seed.messages[`${String(args.folderId)}:${String(args.serverUid)}`] ?? null;
        case "send_message":
          return { accepted: true };
        case "get_notification_settings":
          return { preview: "full", folderScope: "inbox", quietHours: null, groupedPerAccount: false };
        case "plugin:event|listen":
          return 0;
        case "plugin:window|is_maximized":
        case "plugin:window|is_fullscreen":
          return false;
        case "plugin:window|is_focused":
          return true;
        case "plugin:window|theme":
          return "light";
        case "plugin:window|scale_factor":
          return 1;
        case "list_saved_searches":
        case "list_keywords":
        case "list_correspondents":
        case "list_composer_drafts":
        case "list_outbox":
        case "thread_envelopes":
          return [];
        default:
          return null;
      }
    };
    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: "main" },
        currentWebview: { windowLabel: "main", label: "main" },
      },
      transformCallback: (callback: unknown) => {
        const id = Math.floor(Math.random() * 1e9);
        callbacks.set(id, callback);
        return id;
      },
      unregisterCallback: (id: number) => {
        callbacks.delete(id);
      },
      convertFileSrc: (path: string) => path,
      invoke,
    };
  }, data);
}

/** Commands the page sent to the stubbed shell, in order. */
export async function tauriCalls(page: Page): Promise<RecordedCall[]> {
  return page.evaluate(() => window.__TAURI_CALLS__ ?? []);
}

/** Last call for a command, or undefined. */
export async function lastCall(page: Page, cmd: string): Promise<RecordedCall | undefined> {
  return (await tauriCalls(page)).filter((call) => call.cmd === cmd).at(-1);
}
