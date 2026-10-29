import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";
import { envelope, message } from "./fixtures";
import { installTauri } from "./tauri";

/** Run axe and fail with the full report so the fix is obvious. */
async function expectNoViolations(page: Page, label: string): Promise<void> {
  const { violations } = await new AxeBuilder({ page }).analyze();
  const summary = violations.map(({ id, impact, nodes }) => ({
    id,
    impact,
    nodes: nodes.map((node) => node.target.join(" ")),
  }));
  expect(summary, `${label}: ${JSON.stringify(violations, null, 2)}`).toEqual([]);
}

test("message list exposes listbox semantics and passes axe", async ({ page }) => {
  const first = envelope(1, "Quarterly planning notes");
  const second = envelope(2, "Second message", ["Seen"]);
  await installTauri(page, {
    envelopes: [first, second],
    messages: { "inbox:1": message(first, "Hello from the reading pane.") },
  });
  await page.goto("/");

  const list = page.getByRole("listbox", { name: "Messages" });
  await expect(list).toBeVisible();
  await expect(list).toHaveAttribute("aria-multiselectable", "true");
  await expect(page.getByRole("option")).toHaveCount(2);

  await expectNoViolations(page, "message list");
});

test("preferences dialog passes axe", async ({ page }) => {
  await installTauri(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Preferences" }).click();
  const dialog = page.getByRole("dialog", { name: "Preferences" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("Origami e2e")).toBeVisible();

  await expectNoViolations(page, "preferences");
});

test("compose pane passes axe", async ({ page }) => {
  await installTauri(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Compose" }).click();
  await expect(page.getByRole("region", { name: "Compose message" })).toBeVisible();

  await expectNoViolations(page, "compose");
});

for (const theme of ["dark", "ember", "cyberpunk", "synthwave"] as const) {
  test(`${theme} theme message list passes axe`, async ({ page }) => {
    await installTauri(page, {
      envelopes: [envelope(1, "Quarterly planning notes"), envelope(2, "Second message", ["Seen"])],
    });
    await page.addInitScript((value) => {
      localStorage.setItem("origami-preferences", JSON.stringify({ theme: value }));
    }, theme);
    await page.goto("/");

    await expect(page.getByRole("option").first()).toBeVisible();
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
    await expectNoViolations(page, `${theme} message list`);
  });
}

for (const theme of ["light", "dark", "ember", "cyberpunk", "synthwave"] as const) {
  test(`threaded conversation passes axe (${theme})`, async ({ page }) => {
    const first = envelope(1, "Thread subject");
    const second = { ...envelope(2, "Thread subject", ["Seen"]), threadId: first.threadId };
    await installTauri(page, { envelopes: [first, second] });
    if (theme !== "light") {
      await page.addInitScript((value) => {
        localStorage.setItem("origami-preferences", JSON.stringify({ theme: value }));
      }, theme);
    }
    await page.goto("/");

    const row = page.getByRole("option");
    await expect(row).toHaveCount(1);
    await expect(row).toHaveAccessibleName(/2 message conversation/);
    await expectNoViolations(page, `threaded conversation (${theme})`);
  });
}

test("keyboard shortcuts sheet passes axe", async ({ page }) => {
  await installTauri(page);
  await page.goto("/");

  await page.keyboard.type("?");
  await expect(page.getByRole("dialog", { name: "Keyboard shortcuts" })).toBeVisible();
  await expectNoViolations(page, "keyboard shortcuts");
});
