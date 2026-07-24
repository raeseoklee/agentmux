import { expect, test, type Page } from "@playwright/test";

async function waitForPreviewReady(page: Page) {
  await page.waitForFunction(
    () =>
      (window as unknown as { __AGENTMUX_PREVIEW_READY__?: boolean })
        .__AGENTMUX_PREVIEW_READY__ === true,
  );
}

async function bootPreview(page: Page) {
  await page.addInitScript(() => {
    (
      window as unknown as {
        __AGENTMUX_PREVIEW_SEED_WORKSPACE__?: boolean;
      }
    ).__AGENTMUX_PREVIEW_SEED_WORKSPACE__ = true;
  });
  await page.goto("/");
  await waitForPreviewReady(page);
  const cards = page.locator(".agentmux-workspace-card");
  if ((await cards.count()) === 0) {
    await page.locator(".agentmux-workspace-plus").click();
    await expect(cards).toHaveCount(1);
    const inlineName = page.locator(".agentmux-workspace-inline-name-input");
    if (await inlineName.isVisible().catch(() => false)) {
      await inlineName.press("Enter");
    }
  }
}

// Helper: dispatch a synthetic keydown at window level (bypasses Chromium tab interception)
function dispatchKey(page: Page, options: {
  key: string;
  code: string;
  ctrlKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
}) {
  return page.evaluate((opts) => {
    const ev = new KeyboardEvent("keydown", {
      key: opts.key,
      code: opts.code,
      ctrlKey: opts.ctrlKey ?? false,
      altKey: opts.altKey ?? false,
      shiftKey: opts.shiftKey ?? false,
      bubbles: true,
      cancelable: true,
    });
    window.dispatchEvent(ev);
  }, options);
}

// TS-17: workspace cycling ----------------------------------------------------

test("TS-17: workspace.next / workspace.prev cycle workspaces in sidebar order", async ({ page }) => {
  await bootPreview(page);

  // Create a second workspace.
  await page.locator(".agentmux-workspace-plus").click();
  const inlineName = page.locator(".agentmux-workspace-inline-name-input");
  if (await inlineName.isVisible().catch(() => false)) {
    await inlineName.press("Enter");
  }
  const cards = page.locator(".agentmux-workspace-card");
  await expect(cards).toHaveCount(2);

  // Click Workspace 1 to make it active.
  await cards.filter({ hasText: "Workspace 1" }).click();
  await expect(
    cards.filter({ hasText: "Workspace 1" }),
  ).toHaveAttribute("data-agentmux-active", "true");

  // Ctrl+Alt+Down should move to Workspace 2.
  await dispatchKey(page, { key: "ArrowDown", code: "ArrowDown", ctrlKey: true, altKey: true });
  await expect(
    cards.filter({ hasText: "Workspace 2" }),
  ).toHaveAttribute("data-agentmux-active", "true");

  // Ctrl+Alt+Down again should wrap back to Workspace 1.
  await dispatchKey(page, { key: "ArrowDown", code: "ArrowDown", ctrlKey: true, altKey: true });
  await expect(
    cards.filter({ hasText: "Workspace 1" }),
  ).toHaveAttribute("data-agentmux-active", "true");

  // Ctrl+Alt+Up should wrap back to Workspace 2.
  await dispatchKey(page, { key: "ArrowUp", code: "ArrowUp", ctrlKey: true, altKey: true });
  await expect(
    cards.filter({ hasText: "Workspace 2" }),
  ).toHaveAttribute("data-agentmux-active", "true");

  // Ctrl+Alt+Up again should go back to Workspace 1.
  await dispatchKey(page, { key: "ArrowUp", code: "ArrowUp", ctrlKey: true, altKey: true });
  await expect(
    cards.filter({ hasText: "Workspace 1" }),
  ).toHaveAttribute("data-agentmux-active", "true");
});

// TS-13: pane zoom ------------------------------------------------------------

test("TS-13: pane zoom toggle shows zoomed pane and hides siblings, toggle restores", async ({
  page,
}) => {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "agentmux.preview.config.v1",
      JSON.stringify({
        formatVersion: "agentmux.config.v1",
        configPath: "localStorage://agentmux.preview.config.v1",
        ui: { terminalSplitBehavior: "empty" },
      }),
    );
  });
  await bootPreview(page);

  // Open a terminal and split it.
  await page.getByRole("button", { name: "Open terminal" }).last().click();
  await page.locator(".agentmux-pane-split-horizontal").click();
  const panes = page.locator("[data-agentmux-pane]");
  await expect(panes).toHaveCount(2);

  // Activate zoom on the active pane via Ctrl+Shift+Z.
  await dispatchKey(page, { key: "z", code: "KeyZ", ctrlKey: true, shiftKey: true });

  // One pane should have data-agentmux-zoomed="true".
  const zoomedPane = page.locator('[data-agentmux-pane][data-agentmux-zoomed="true"]');
  await expect(zoomedPane).toHaveCount(1);

  // The ZOOM badge should be visible in the zoomed pane's header.
  await expect(zoomedPane.locator(".agentmux-pane-zoom-badge")).toBeVisible();

  // The pane tree wrapper should have data-agentmux-zoomed-pane set.
  await expect(page.locator("[data-agentmux-pane-tree][data-agentmux-zoomed-pane]")).toHaveCount(1);

  // Toggle zoom off — Ctrl+Shift+Z again.
  await dispatchKey(page, { key: "z", code: "KeyZ", ctrlKey: true, shiftKey: true });

  // No pane should be zoomed.
  await expect(page.locator('[data-agentmux-pane][data-agentmux-zoomed="true"]')).toHaveCount(0);
  await expect(page.locator("[data-agentmux-pane-tree][data-agentmux-zoomed-pane]")).toHaveCount(0);
});

// TS-14: broadcast input ------------------------------------------------------

test("TS-14: broadcast toggle via palette shows BCAST badge on panes", async ({
  page,
}) => {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "agentmux.preview.config.v1",
      JSON.stringify({
        formatVersion: "agentmux.config.v1",
        configPath: "localStorage://agentmux.preview.config.v1",
        ui: { terminalSplitBehavior: "empty" },
      }),
    );
  });
  await bootPreview(page);

  // Open a terminal and split so there are 2 panes.
  await page.getByRole("button", { name: "Open terminal" }).last().click();
  await page.locator(".agentmux-pane-split-horizontal").click();
  const panes = page.locator("[data-agentmux-pane]");
  await expect(panes).toHaveCount(2);

  // No BCAST badges yet.
  await expect(page.locator(".agentmux-pane-broadcast-badge")).toHaveCount(0);

  // Open palette and run "broadcast" action.
  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("P");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await page.keyboard.type("broadcast");
  await expect(page.locator(".agentmux-palette-item").first()).toBeVisible();
  await page.keyboard.press("Enter");

  // BCAST badges should appear on all leaf panes.
  await expect(page.locator(".agentmux-pane-broadcast-badge")).toHaveCount(2);

  // Toggle off via palette again.
  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("P");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await page.keyboard.type("broadcast");
  await expect(page.locator(".agentmux-palette-item").first()).toBeVisible();
  await page.keyboard.press("Enter");

  // BCAST badges should be gone.
  await expect(page.locator(".agentmux-pane-broadcast-badge")).toHaveCount(0);
});

// Palette glue: terminal.clearBuffer / terminal.selectAll --------------------

test("palette glue: terminal.clearBuffer and terminal.selectAll appear in palette and are callable", async ({
  page,
}) => {
  await bootPreview(page);

  // Open a terminal so the session-dependent actions are enabled.
  await page.locator(".agentmux-new-terminal-tab").click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);

  // Open palette and search for clear buffer.
  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("P");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await page.keyboard.type("buffer");
  const clearItem = page
    .locator(".agentmux-palette-item")
    .filter({ hasText: "Clear terminal buffer" });
  await expect(clearItem).toBeVisible();

  // Execute — should not crash (palette closes).
  await page.keyboard.press("Enter");
  await expect(page.locator(".agentmux-palette-input")).toHaveCount(0);

  // Open palette again and search for select all.
  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("P");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await page.keyboard.type("select all");
  const selectAllItem = page
    .locator(".agentmux-palette-item")
    .filter({ hasText: "Select all terminal content" });
  await expect(selectAllItem).toBeVisible();

  // Execute — should not crash.
  await page.keyboard.press("Enter");
  await expect(page.locator(".agentmux-palette-input")).toHaveCount(0);
});
