import { expect, test } from "@playwright/test";
import { installTauri, lastCall } from "./tauri";

test("composes a message and hands it to the shell", async ({ page }) => {
  await installTauri(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Compose" }).click();
  const pane = page.getByRole("region", { name: "Compose message" });
  await expect(pane).toBeVisible();

  const to = pane.getByRole("combobox", { name: "To" });
  await to.fill("ada@example.org");
  await to.press("Enter");
  await pane.getByLabel("Subject").fill("E2E subject");

  const editor = pane.locator(".ProseMirror");
  await expect(editor).toBeVisible();
  await editor.click();
  await page.keyboard.type("Body typed by the end-to-end test.");

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
