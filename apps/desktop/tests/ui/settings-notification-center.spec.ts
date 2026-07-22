import { expect, test, type Page } from "@playwright/test";

async function bootPreview(page: Page) {
  await page.addInitScript(() => {
    (
      window as unknown as {
        __AGENTMUX_PREVIEW_SEED_WORKSPACE__?: boolean;
      }
    ).__AGENTMUX_PREVIEW_SEED_WORKSPACE__ = true;
  });
  await page.goto("/");
  await page.waitForFunction(
    () =>
      (window as unknown as { __AGENTMUX_PREVIEW_READY__?: boolean })
        .__AGENTMUX_PREVIEW_READY__ === true,
  );
}

const visibleFocusableSelector =
  "button:visible, input:visible, textarea:visible, select:visible, a[href]:visible, [tabindex]:not([tabindex='-1']):visible";

async function expectOverlayFocusWraps(page: Page, overlay: ReturnType<Page["locator"]>) {
  const focusable = overlay.locator(visibleFocusableSelector);
  const count = await focusable.count();
  expect(count).toBeGreaterThan(0);

  await focusable.first().focus();
  await expect(focusable.first()).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(focusable.last()).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(focusable.first()).toBeFocused();
}

test("settings separates workspace scope and advanced configuration", async ({ page }) => {
  await bootPreview(page);

  await page.locator(".agentmux-settings-open").click();
  await page.locator(".agentmux-settings-tab-general").click();
  await expect(page.locator("[data-agentmux-update-settings]")).toContainText(
    "AgentMux checks GitHub Releases at startup and periodically while it is running.",
  );
  await page.locator(".agentmux-settings-tab-workspace").click();

  await expect(page.locator("[data-agentmux-workspace-settings]")).toBeVisible();
  await expect(page.locator("[data-agentmux-workspace-scope]")).toContainText(
    "Workspace 1",
  );
  await expect(page.locator("[data-agentmux-config-reload]")).toHaveCount(0);

  await page.locator(".agentmux-settings-tab-advanced").click();
  await expect(page.locator("[data-agentmux-config-reload]")).toBeVisible();
  await expect(page.locator("[data-agentmux-workspace-settings]")).toHaveCount(0);
  await expect(page.locator("[data-agentmux-notification]")).toHaveCount(0);
});

test("app dialogs restore focus after keyboard cancellation", async ({ page }) => {
  await bootPreview(page);

  const trigger = page.locator(".agentmux-workspace-group-create");
  await trigger.focus();
  await trigger.click();

  const dialog = page.locator("[data-agentmux-app-dialog='true']");
  await expect(dialog).toBeVisible();
  await expect(dialog.locator("input")).toBeFocused();

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(trigger).toBeFocused();
});

test("app dialogs keep the application visible without backdrop blur", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-workspace-group-create").click();
  const backdrop = page.getByTestId("agentmux-dialog-backdrop");
  await expect(backdrop).toBeVisible();
  await expect
    .poll(() =>
      backdrop.evaluate((element) => {
        const style = window.getComputedStyle(element);
        return {
          backdropFilter: style.backdropFilter,
          backgroundColor: style.backgroundColor,
        };
      }),
    )
    .toEqual({
      backdropFilter: "none",
      backgroundColor: "rgba(0, 0, 0, 0.5)",
    });
});

test("settings traps focus and restores the caller after Escape", async ({ page }) => {
  await bootPreview(page);

  const trigger = page.locator(".agentmux-settings-open");
  await trigger.focus();
  await trigger.click();

  const dialog = page.getByRole("dialog", { name: "Settings" });
  await expect(dialog).toBeVisible();
  await expect(dialog.locator("[data-overlay-autofocus='true']")).toBeFocused();
  await expectOverlayFocusWraps(page, dialog);

  const generalTab = dialog.getByRole("tab", { name: "General" });
  await generalTab.focus();
  await page.keyboard.press("ArrowDown");
  const appearanceTab = dialog.getByRole("tab", { name: "Appearance" });
  await expect(appearanceTab).toBeFocused();
  await expect(appearanceTab).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("End");
  const diagnosticsTab = dialog.getByRole("tab", { name: "Diagnostics" });
  await expect(diagnosticsTab).toBeFocused();
  await expect(diagnosticsTab).toHaveAttribute("aria-selected", "true");

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(trigger).toBeFocused();
});

test("notification action opens the dedicated attention center", async ({ page }) => {
  await bootPreview(page);
  await page.getByRole("button", { name: "Open terminal" }).click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await page.evaluate(() => {
    (
      window as unknown as {
        __AGENTMUX_PREVIEW__?: {
          syntheticAgentState?: (input: {
            state: string;
            reason: string;
            notificationId: string;
          }) => void;
        };
      }
    ).__AGENTMUX_PREVIEW__?.syntheticAgentState?.({
      state: "waiting_for_input",
      reason: "approval needed from notification center test",
      notificationId: "notification-center-test",
    });
  });

  await page.keyboard.press("Control+Shift+P");
  await page.locator(".agentmux-palette-input").fill("Open notifications");
  await expect(page.locator(".agentmux-palette-item").first()).toContainText(
    "Open notifications",
  );
  await page.keyboard.press("Enter");

  await expect(page.locator("[data-agentmux-notification-center]")).toBeVisible();
  await expect(page.locator("[data-agentmux-notification-summary]")).toContainText("1");
  await expect(
    page.locator('[data-agentmux-notification-severity-count="warning"]'),
  ).toContainText("1");
  const notification = page.locator(
    '[data-agentmux-notification="notification-center-test"]',
  );
  await expect(notification).toContainText("approval needed from notification center test");
  await expect(notification.getByRole("button", { name: "Focus" })).toBeVisible();

  await notification.getByRole("button", { name: "Dismiss" }).click();
  await expect(notification).toHaveCount(0);
});

test("notification center traps focus and restores the caller after Escape", async ({
  page,
}) => {
  await bootPreview(page);

  const caller = page.locator(".agentmux-settings-open");
  await caller.focus();
  await page.keyboard.press("Control+Shift+I");

  const center = page.getByRole("dialog", { name: "Notifications" });
  await expect(center).toBeVisible();
  await expect(center.locator("[data-overlay-autofocus='true']")).toBeFocused();
  await expectOverlayFocusWraps(page, center);

  await page.keyboard.press("Escape");
  await expect(center).toHaveCount(0);
  await expect(caller).toBeFocused();
});

test("shortcut capture leaves plain Tab and Shift+Tab available for navigation", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-settings-open").click();
  await page.locator(".agentmux-settings-tab-keys").click();
  await page
    .locator(
      '[data-agentmux-shortcut-row="workspace.new"] .agentmux-shortcut-edit',
    )
    .click();

  const dialog = page.locator("[data-agentmux-app-dialog='true']");
  const keys = dialog.locator(".agentmux-shortcut-capture__key");
  await expect(keys).toHaveCount(2);
  await expect(keys.first()).toBeFocused();

  await page.keyboard.press("Tab");
  await expect(keys.nth(1)).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(keys.first()).toBeFocused();

  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
});

test("Korean dialogs localize required validation and shortcut capture guidance", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-settings-open").click();
  await page.locator(".agentmux-settings-tab-general").click();
  await page.locator(".agentmux-language-select").selectOption("ko");
  await expect(page.locator("html")).toHaveAttribute("lang", "ko");
  await page.keyboard.press("Escape");

  await page.locator(".agentmux-workspace-group-create").click();
  const groupDialog = page.locator("[data-agentmux-app-dialog='true']");
  await groupDialog.locator("input").fill("");
  await groupDialog.getByRole("button", { name: "만들기" }).click();
  await expect(groupDialog.locator(".agentmux-dialog__error")).toHaveText(
    "그룹 이름 항목은 필수입니다.",
  );
  await page.keyboard.press("Escape");

  await page.locator(".agentmux-settings-open").click();
  await page.locator(".agentmux-settings-tab-keys").click();
  await page
    .locator(
      '[data-agentmux-shortcut-row="workspace.new"] .agentmux-shortcut-edit',
    )
    .click();
  const shortcutDialog = page.locator("[data-agentmux-app-dialog='true']");
  await shortcutDialog.getByRole("button", { name: "할당 해제" }).click();
  await expect(shortcutDialog.locator(".agentmux-shortcut-capture__key").first()).toHaveText(
    "키 조합을 누르세요",
  );
});
