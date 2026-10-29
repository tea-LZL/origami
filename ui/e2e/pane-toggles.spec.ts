import { expect, test } from "@playwright/test";
import { envelope, message } from "./fixtures";
import { installTauri } from "./tauri";

test("reading does not open bulk actions; panes collapse independently", async ({ page }) => {
  const first = envelope(1, "First message");
  const second = envelope(2, "Second message", ["Seen"]);
  await installTauri(page, {
    envelopes: [first, second],
    messages: { "inbox:1": message(first, "Body."), "inbox:2": message(second, "Other.") },
  });
  await page.goto("/");

  // Opening a row must not populate the bulk toolbar.
  await page.getByRole("option", { name: /First message/ }).click();
  await expect(page.getByRole("heading", { name: "First message" })).toBeVisible();
  await expect(page.getByRole("toolbar", { name: "Selected messages" })).toHaveCount(0);

  // The checkbox is what starts a selection.
  await page.getByRole("option", { name: /Second message/ }).locator(".select-control").click();
  const bar = page.getByRole("toolbar", { name: "Selected messages" });
  await expect(bar).toBeVisible();
  await expect(bar.getByText("1 selected")).toBeVisible();
  await bar.getByRole("button", { name: "Clear selection" }).click();
  await expect(bar).toHaveCount(0);

  // Sidebar collapses without touching the list.
  await page.getByRole("button", { name: "Hide sidebar" }).click();
  await expect(page.locator(".sidebar")).toHaveCount(0);
  await expect(page.locator(".threadlist")).toBeVisible();

  // The list collapses while the sidebar stays visible after it is restored.
  await page.getByRole("button", { name: "Show sidebar" }).click();
  await page.getByRole("button", { name: "Hide message list" }).click();
  await expect(page.locator(".threadlist")).toHaveCount(0);
  await expect(page.locator(".sidebar")).toBeVisible();
  await page.screenshot({ path: "/tmp/pane-toggles.png" });
});
