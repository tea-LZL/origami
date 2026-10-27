import { expect, test } from "@playwright/test";
import { envelope, message } from "./fixtures";
import { installTauri } from "./tauri";

test("j/k move the list selection and Enter opens the focused row", async ({ page }) => {
  const first = envelope(1, "First message");
  const second = envelope(2, "Second message", ["Seen"]);
  await installTauri(page, {
    envelopes: [first, second],
    messages: { "inbox:2": message(second, "Second body.") },
  });
  await page.goto("/");

  await page.getByRole("option", { name: /First message/ }).click();
  await page.keyboard.press("j");

  const focused = page.getByRole("option", { name: /Second message/ });
  await expect(focused).toHaveAttribute("aria-selected", "true");
  await expect(focused).toBeFocused();

  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Second message" })).toBeVisible();
});

test("Escape closes preferences and Tab stays inside the dialog", async ({ page }) => {
  await installTauri(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Preferences" }).click();
  const dialog = page.getByRole("dialog", { name: "Preferences" });
  await expect(dialog).toBeVisible();

  for (let step = 0; step < 40; step += 1) await page.keyboard.press("Tab");
  expect(await dialog.evaluate((node) => node.contains(document.activeElement))).toBe(true);

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
});
