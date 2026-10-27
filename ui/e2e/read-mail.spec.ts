import { expect, test } from "@playwright/test";
import { envelope, message } from "./fixtures";
import { installTauri, lastCall } from "./tauri";

test("opens a message from the list and renders the cached body", async ({ page }) => {
  const target = envelope(1, "Quarterly planning notes");
  const other = envelope(2, "Second message", ["Seen"]);
  await installTauri(page, {
    envelopes: [target, other],
    messages: { "inbox:1": message(target, "Hello from the reading pane.") },
  });
  await page.goto("/");

  const row = page.getByRole("option", { name: /Quarterly planning notes/ });
  await expect(row).toBeVisible();
  await expect(page.locator(".sidebar .folder-name").filter({ hasText: "Inbox" })).toBeVisible();
  await row.click();

  await expect(page.getByRole("heading", { name: "Quarterly planning notes" })).toBeVisible();
  await expect(page.locator("[aria-label='Message body'] pre")).toHaveText(
    "Hello from the reading pane.",
  );
  expect(await lastCall(page, "get_cached_message")).toBeDefined();
  await expect
    .poll(async () => (await lastCall(page, "store_flags"))?.args.flags)
    .toContain("Seen");
});
