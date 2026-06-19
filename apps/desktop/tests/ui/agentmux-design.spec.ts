import { expect, test } from "@playwright/test";

test("design boots with a live workspace", async ({ page }) => {
  await page.goto("/");
  await page.waitForFunction(
    () =>
      (window as unknown as { __AGENTMUX_PREVIEW_READY__?: boolean })
        .__AGENTMUX_PREVIEW_READY__ === true
  );
  await expect(page.getByText("Local project").first()).toBeVisible();
  await expect(
    page.getByRole("button", { name: /검색/ }).first()
  ).toBeVisible();
});

test("opens a live terminal", async ({ page }) => {
  await page.goto("/");
  await page.waitForFunction(
    () =>
      (window as unknown as { __AGENTMUX_PREVIEW_READY__?: boolean })
        .__AGENTMUX_PREVIEW_READY__ === true
  );
  await page.getByRole("button", { name: "터미널 열기" }).first().click();
  await expect(page.getByText("conpty").first()).toBeVisible({ timeout: 5000 });
});

test("new terminal adds a mounted tab instead of replacing the active one", async ({ page }) => {
  await page.goto("/");
  await page.waitForFunction(
    () =>
      (window as unknown as { __AGENTMUX_PREVIEW_READY__?: boolean })
        .__AGENTMUX_PREVIEW_READY__ === true
  );

  await page.getByRole("button", { name: "터미널 열기" }).first().click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await expect(page.locator('[data-agentmux-pane][data-agentmux-mounted="true"]')).toHaveCount(1);

  await page.keyboard.down("Control");
  await page.keyboard.press("K");
  await page.keyboard.up("Control");
  await page.getByText("새 터미널", { exact: true }).click();

  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(2);
  await expect(page.locator('[data-agentmux-pane][data-agentmux-mounted="true"]')).toHaveCount(2);
});

test("command palette lists actions", async ({ page }) => {
  await page.goto("/");
  await page.waitForFunction(
    () =>
      (window as unknown as { __AGENTMUX_PREVIEW_READY__?: boolean })
        .__AGENTMUX_PREVIEW_READY__ === true
  );
  await page.keyboard.down("Control");
  await page.keyboard.press("K");
  await page.keyboard.up("Control");
  await expect(page.getByText("새 터미널").first()).toBeVisible();
  await expect(page.getByText("워크스페이스").first()).toBeVisible();
  await page.keyboard.press("Escape");
});

test("theme toggle switches label", async ({ page }) => {
  await page.goto("/");
  await page.waitForFunction(
    () =>
      (window as unknown as { __AGENTMUX_PREVIEW_READY__?: boolean })
        .__AGENTMUX_PREVIEW_READY__ === true
  );
  const toggle = page.getByRole("button", { name: /다크|라이트/ });
  const beforeText = await toggle.textContent();
  await toggle.click();
  const afterText = await page
    .getByRole("button", { name: /다크|라이트/ })
    .textContent();
  expect(afterText).not.toBe(beforeText);
});

test("settings shows seeded SSH profiles", async ({ page }) => {
  await page.goto("/");
  await page.waitForFunction(
    () =>
      (window as unknown as { __AGENTMUX_PREVIEW_READY__?: boolean })
        .__AGENTMUX_PREVIEW_READY__ === true
  );
  await page.getByText("설정", { exact: true }).first().click();
  await page.getByText("프로필 · SSH", { exact: true }).click();
  await expect(page.getByText("prod-server").first()).toBeVisible();
  await expect(page.getByText("staging-db").first()).toBeVisible();
  await expect(page.getByText("gpu-box").first()).toBeVisible();
  await page.keyboard.press("Escape");
});

test("OMC telemetry bar renders", async ({ page }) => {
  await page.goto("/");
  await page.waitForFunction(
    () =>
      (window as unknown as { __AGENTMUX_PREVIEW_READY__?: boolean })
        .__AGENTMUX_PREVIEW_READY__ === true
  );
  const openBtn = page.getByRole("button", { name: "터미널 열기" }).first();
  if (await openBtn.isVisible()) {
    await openBtn.click();
  }
  await page.evaluate(() =>
    (window as any).__AGENTMUX_PREVIEW__?.syntheticAgentState({
      state: "running",
      telemetry: {
        activity: "thinking",
        session: "9m",
        cost: "~$0.5",
        tokens: "10k",
        cache: "99%",
        rate: "$3/h",
        ctx: "20%",
      },
    })
  );
  await expect(page.getByText("[OMC]").first()).toBeVisible({ timeout: 5000 });
});

test("launches an agent in a durable WSL-tmux session", async ({ page }) => {
  await page.goto("/");
  await page.waitForFunction(
    () =>
      (window as unknown as { __AGENTMUX_PREVIEW_READY__?: boolean })
        .__AGENTMUX_PREVIEW_READY__ === true
  );
  await page.getByRole("button", { name: /에이전트 실행/ }).first().click();
  await expect(page.getByText("wsl-tmux-control").first()).toBeVisible({ timeout: 5000 });
});

test("command palette opens over a focused terminal", async ({ page }) => {
  await page.goto("/");
  await page.waitForFunction(
    () =>
      (window as unknown as { __AGENTMUX_PREVIEW_READY__?: boolean })
        .__AGENTMUX_PREVIEW_READY__ === true
  );
  await page.getByRole("button", { name: /에이전트 실행/ }).first().click();
  await page.waitForTimeout(800);
  await page.keyboard.down("Control");
  await page.keyboard.press("K");
  await page.keyboard.up("Control");
  await expect(
    page.getByText("Claude Code 실행 (durable tmux)").first()
  ).toBeVisible();
});
