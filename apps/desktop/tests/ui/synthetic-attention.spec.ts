import { expect, test } from "@playwright/test";

test("synthetic attention appears in workspace, pane, and notification UI", async ({ page }) => {
  await page.goto("/#console");
  await page.waitForFunction(() => window.__AGENTMUX_PREVIEW_READY__ === true);

  await expect(page.getByRole("button", { name: /Local project/ })).toBeVisible();
  await page.getByRole("button", { name: "Native shell" }).click();
  await expect(page.getByRole("application")).toBeVisible();

  await page.evaluate(() => {
    window.__AGENTMUX_PREVIEW__?.syntheticAgentState({
      state: "waiting_for_input",
      reason: "approval needed from ui test",
      notificationId: "not_ui_test_attention"
    });
  });

  await expect(page.locator(".workspace-list .badge.is-attention")).toHaveText("1");
  await expect(page.locator(".pane-titlebar .attention-pill")).toHaveText("Attention");
  await expect(page.locator(".attention-row")).toContainText("approval needed from ui test");
  await expect(page.locator(".notification-row")).toContainText("Agent needs input");
  await expect(page.locator(".notification-row")).toContainText("approval needed from ui test");

  await page.locator(".attention-row").getByRole("button", { name: "Clear" }).click();
  await expect(page.locator(".attention-row")).toHaveCount(0);
  await expect(page.locator(".pane-titlebar .attention-pill")).toHaveCount(0);
  await expect(page.locator(".workspace-list .badge.is-attention")).toHaveCount(0);

  await page.locator(".notification-row").getByRole("button", { name: "Dismiss" }).click();
  await expect(page.locator(".notification-row")).toHaveCount(0);
});

test("browser surface can be created and driven from the pane UI", async ({ page }) => {
  await page.goto("/#console");
  await page.waitForFunction(() => window.__AGENTMUX_PREVIEW_READY__ === true);

  await expect(page.getByRole("button", { name: /Local project/ })).toBeVisible();
  await page.getByRole("button", { name: "Browser" }).click();

  await expect(page.getByRole("article", { name: "Browser pane" })).toBeVisible();
  await expect(page.getByRole("region", { name: "Browser preview" })).toBeVisible();

  await page.getByLabel("Browser URL").fill("https://example.invalid");
  await page.getByRole("button", { name: "Go" }).click();
  await expect(page.locator(".browser-url")).toHaveText("https://example.invalid");
  await expect(page.locator(".browser-frame")).toHaveAttribute("src", "https://example.invalid");
  await expect(page.locator(".browser-page")).toContainText("Navigated");

  await page.getByRole("button", { name: "Snapshot" }).click();
  await expect(page.locator(".browser-output pre")).toContainText("https://example.invalid");

  await page.getByRole("button", { name: "Screenshot" }).click();
  await expect(page.locator(".browser-output")).toContainText("memory://browser-preview/");

  await page.getByLabel("Browser selector").fill("#login");
  await page.getByRole("button", { name: "Click", exact: true }).click();
  await expect(page.locator(".browser-page")).toContainText("Clicked");

  await page.getByLabel("Browser text").fill("agentmux");
  await page.getByRole("button", { name: "Type" }).click();
  await expect(page.locator(".browser-page")).toContainText("Typed");

  await page.getByLabel("Browser script").fill("document.title");
  await page.getByRole("button", { name: "Evaluate" }).click();
  await expect(page.locator(".browser-output")).toContainText('{"ok":true}');
});
