import { expect, test, type Page } from "@playwright/test";

async function bootWithBrowser(page: Page) {
  await page.addInitScript(() => {
    (
      window as unknown as { __AGENTMUX_PREVIEW_SEED_WORKSPACE__?: boolean }
    ).__AGENTMUX_PREVIEW_SEED_WORKSPACE__ = true;
  });
  await page.goto("/");
  await page.waitForFunction(
    () =>
      (window as unknown as { __AGENTMUX_PREVIEW_READY__?: boolean })
        .__AGENTMUX_PREVIEW_READY__ === true,
  );
  await page.keyboard.press("Control+Shift+P");
  await page.keyboard.type("browser");
  await expect(page.getByText("Open new browser tab", { exact: true })).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page.getByLabel("Page address")).toBeVisible();
  await expect(page.locator('[data-agentmux-tab-icon="browser"]')).toBeVisible();
  await expect(page.locator(".agentmux-browser-surface-icon")).toHaveCount(0);
}

async function injectBrowserDialog(
  page: Page,
  detail: {
    kind: "alert" | "confirm" | "prompt";
    message: string;
    defaultValue?: string;
  },
) {
  return page.evaluate((input) => {
    const preview = (
      window as unknown as {
        __AGENTMUX_PREVIEW__?: {
          browserDialog: (dialog: typeof input) => string | null;
        };
      }
    ).__AGENTMUX_PREVIEW__;
    return preview?.browserDialog(input) ?? null;
  }, detail);
}

test("embedded browser confirm requires an explicit app-dialog response", async ({
  page,
}) => {
  await bootWithBrowser(page);
  const dialogId = await injectBrowserDialog(page, {
    kind: "confirm",
    message: "Delete remote data?",
  });
  expect(dialogId).toBeTruthy();

  const dialog = page.locator("[data-agentmux-app-dialog='true']");
  await expect(dialog).toContainText("Delete remote data?");
  await dialog.getByRole("button", { name: "Cancel" }).click();

  await expect
    .poll(() =>
      page.evaluate(() =>
        (
          window as unknown as {
            __AGENTMUX_PREVIEW__?: { browserActions: () => string[] };
          }
        ).__AGENTMUX_PREVIEW__?.browserActions() ?? [],
      ),
    )
    .toContainEqual(expect.stringContaining(`${dialogId}:dismiss`));
});

test("embedded browser prompt returns only the submitted app-dialog value", async ({
  page,
}) => {
  await bootWithBrowser(page);
  const dialogId = await injectBrowserDialog(page, {
    kind: "prompt",
    message: "Deployment label",
    defaultValue: "staging",
  });

  const dialog = page.locator("[data-agentmux-app-dialog='true']");
  await expect(dialog).toContainText("Deployment label");
  await dialog.locator("input").fill("production-safe");
  await dialog.getByRole("button", { name: "Submit" }).click();

  await expect
    .poll(() =>
      page.evaluate(() =>
        (
          window as unknown as {
            __AGENTMUX_PREVIEW__?: { browserActions: () => string[] };
          }
        ).__AGENTMUX_PREVIEW__?.browserActions() ?? [],
      ),
    )
    .toContainEqual(
      expect.stringContaining(`${dialogId}:accept:production-safe`),
    );
});

test("embedded browser alert uses the localized Korean confirmation action", async ({
  page,
}) => {
  await bootWithBrowser(page);
  await page.locator(".agentmux-settings-open").click();
  await page.locator(".agentmux-settings-tab-general").click();
  await page.locator(".agentmux-language-select").selectOption("ko");
  await page.keyboard.press("Escape");

  const dialogId = await injectBrowserDialog(page, {
    kind: "alert",
    message: "배포가 완료되었습니다.",
  });
  expect(dialogId).toBeTruthy();

  const dialog = page.locator("[data-agentmux-app-dialog='true']");
  await expect(dialog).toContainText("배포가 완료되었습니다.");
  await dialog.getByRole("button", { name: "확인", exact: true }).click();

  await expect
    .poll(() =>
      page.evaluate(() =>
        (
          window as unknown as {
            __AGENTMUX_PREVIEW__?: { browserActions: () => string[] };
          }
        ).__AGENTMUX_PREVIEW__?.browserActions() ?? [],
      ),
    )
    .toContainEqual(expect.stringContaining(`${dialogId}:accept`));
});
