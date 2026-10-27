import { expect, test } from "@playwright/test";
import { envelope } from "./fixtures";
import { installTauri, lastCall } from "./tauri";

test("searches all mail and labels the query context", async ({ page }) => {
  const hit = envelope(9, "Invoice 2026-09");
  await installTauri(page, { searchResults: [hit] });
  await page.goto("/");

  await page.getByLabel("Search all mail").fill("invoice");

  await expect(page.getByRole("option", { name: /Invoice 2026-09/ })).toBeVisible();
  await expect(page.locator(".threadlist .eyebrow")).toHaveText("Search");
  const call = await lastCall(page, "search_page");
  expect(call?.args.query).toBe("invoice");
});
