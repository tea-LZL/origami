import { expect, test, type Page } from "@playwright/test";
import { installTauri, lastCall, tauriCalls } from "./tauri";

async function setSendDelay(page: Page, seconds: number) {
  await page.addInitScript((value) => {
    localStorage.setItem("origami-preferences", JSON.stringify({ sendDelay: value }));
  }, seconds);
}

async function compose(page: Page, subject: string) {
  await page.getByRole("button", { name: "Compose" }).click();
  const pane = page.getByRole("region", { name: "Compose message" });
  await expect(pane).toBeVisible();

  const to = pane.getByRole("combobox", { name: "To" });
  await to.fill("ada@example.org");
  await to.press("Enter");
  await pane.getByLabel("Subject").fill(subject);

  const editor = pane.locator(".ProseMirror");
  await expect(editor).toBeVisible();
  await editor.click();
  await page.keyboard.type("Body typed by the end-to-end test.");
  return pane;
}

test("composes a message and hands it to the shell", async ({ page }) => {
  await installTauri(page);
  await setSendDelay(page, 0);
  await page.goto("/");

  const pane = await compose(page, "E2E subject");
  await pane.getByRole("button", { name: "Send", exact: true }).click();

  await expect(pane).toHaveCount(0);
  const sent = await lastCall(page, "send_message");
  const draft = sent?.args.draft;
  expect(draft).toMatchObject({
    subject: "E2E subject",
    to: ["ada@example.org"],
  });
  const html = typeof draft === "object" && draft !== null && "html" in draft ? draft.html : undefined;
  expect(String(html)).toContain("Body typed by the end-to-end test.");
  await expect(page.getByText("Sending message…")).toBeVisible();
});

test("undo send holds the message before delivery", async ({ page }) => {
  await installTauri(page);
  await setSendDelay(page, 5);
  await page.goto("/");

  const pane = await compose(page, "Held subject");
  await pane.getByRole("button", { name: "Send", exact: true }).click();

  await expect(page.getByText(/Sending in \d+s/)).toBeVisible();
  await pane.getByRole("button", { name: "Undo send" }).click();

  await expect(pane.getByRole("button", { name: "Undo send" })).toHaveCount(0);
  await expect(pane.getByLabel("Subject")).toHaveValue("Held subject");
  const calls = await tauriCalls(page);
  expect(calls.some((call) => call.cmd === "send_message")).toBe(false);
});
