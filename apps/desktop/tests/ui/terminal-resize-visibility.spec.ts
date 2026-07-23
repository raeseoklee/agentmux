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

async function resizeCounts(page: Page, sessionIds: string[]) {
  return page.evaluate((ids) => {
    const preview = (
      window as unknown as {
        __AGENTMUX_PREVIEW__?: {
          terminalResizes(sessionId: string): Array<{ columns: number; rows: number }>;
        };
      }
    ).__AGENTMUX_PREVIEW__;
    return Object.fromEntries(
      ids.map((sessionId) => [
        sessionId,
        preview?.terminalResizes(sessionId).length ?? 0,
      ]),
    );
  }, sessionIds);
}

test("visible split terminals resize with dock changes while warm hidden tabs stay idle", async ({
  page,
}) => {
  await bootPreview(page);
  await page.locator(".agentmux-new-terminal-tab").click();
  await page.locator(".agentmux-pane-split-horizontal").click();

  const activeTree = page.locator(
    '[data-agentmux-tab-pane-tree][data-agentmux-tab-active="true"]',
  );
  await expect(
    activeTree.locator("[data-agentmux-terminal-inner-margin]"),
  ).toHaveCount(2);
  const splitSessionIds = await activeTree
    .locator("[data-agentmux-terminal-inner-margin]")
    .evaluateAll((panes) =>
      panes
        .map((pane) => pane.getAttribute("data-agentmux-terminal-session"))
        .filter((sessionId): sessionId is string => Boolean(sessionId)),
    );

  await page.locator(".agentmux-new-terminal-tab").click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(2);
  const hiddenSessionId = await page
    .locator(
      '[data-agentmux-tab-pane-tree][data-agentmux-tab-active="true"] [data-agentmux-terminal-inner-margin]',
    )
    .getAttribute("data-agentmux-terminal-session");
  expect(hiddenSessionId).toBeTruthy();

  await page.locator(".agentmux-surface-tab").first().click();
  await expect(
    activeTree.locator("[data-agentmux-terminal-inner-margin]"),
  ).toHaveCount(2);
  await page.waitForTimeout(500);

  const sessionIds = [...splitSessionIds, hiddenSessionId!];
  const before = await resizeCounts(page, sessionIds);

  await page.locator(".agentmux-status-git-button").click();
  const panel = page.getByTestId("source-control-panel");
  await expect(panel).toBeVisible();
  await page.setViewportSize({ width: 1220, height: 820 });
  await page.waitForTimeout(450);
  await panel.getByRole("button", { name: "Close" }).click();
  await expect(panel).toBeHidden();
  await page.setViewportSize({ width: 1280, height: 780 });
  await page.waitForTimeout(500);

  const after = await resizeCounts(page, sessionIds);
  for (const sessionId of splitSessionIds) {
    expect(after[sessionId]).toBeGreaterThan(before[sessionId]);
  }
  expect(after[hiddenSessionId!]).toBe(before[hiddenSessionId!]);
});
