import { expect, test, type Page } from "@playwright/test";

async function waitForPreviewReady(page: Page) {
  await page.waitForFunction(
    () =>
      (window as unknown as { __AGENTMUX_PREVIEW_READY__?: boolean })
        .__AGENTMUX_PREVIEW_READY__ === true,
  );
}

async function bootPreview(
  page: Page,
  options: { ensureWorkspace?: boolean } = {},
) {
  if (options.ensureWorkspace !== false) {
    await page.addInitScript(() => {
      (
        window as unknown as {
          __AGENTMUX_PREVIEW_SEED_WORKSPACE__?: boolean;
        }
      ).__AGENTMUX_PREVIEW_SEED_WORKSPACE__ = true;
    });
  }
  await page.goto("/");
  await waitForPreviewReady(page);
  if (options.ensureWorkspace === false) {
    return;
  }
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

test("design boots without a default workspace and can create one", async ({ page }) => {
  await bootPreview(page, { ensureWorkspace: false });
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(0);
  await page.locator(".agentmux-workspace-plus").click();
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(1);
  await expect(page.getByText("Workspace 1").first()).toBeVisible();
  await expect(page.locator(".agentmux-workspace-filter-input")).toBeVisible();
});

test("browser default context menu is suppressed globally", async ({ page }) => {
  await bootPreview(page);

  const canceled = await page.evaluate(() => {
    const event = new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
      clientX: 24,
      clientY: 24,
    });
    return !document.body.dispatchEvent(event) || event.defaultPrevented;
  });

  expect(canceled).toBe(true);
});

test("status bar shows git branch and short hash", async ({ page }) => {
  await bootPreview(page);

  await page.evaluate(() => {
    (
      window as unknown as {
        __AGENTMUX_PREVIEW__?: {
          sidebarState: (detail: {
            gitBranch?: string | null;
            gitHash?: string | null;
          }) => void;
        };
      }
    ).__AGENTMUX_PREVIEW__?.sidebarState({
      gitBranch: "feature/startup-restore",
      gitHash: "abc1234",
    });
  });

  await expect(page.locator(".agentmux-status-git")).toHaveText(
    "feature/startup-restore @ abc1234",
  );
});

test("opens a live terminal", async ({ page }) => {
  await bootPreview(page);
  await page.locator(".agentmux-new-terminal-tab").click();
  await expect(page.getByText("wsl-direct").first()).toBeVisible({
    timeout: 5000,
  });
  await expect(page.locator(".agentmux-live-terminal-host").first()).toHaveAttribute(
    "data-agentmux-terminal-unicode-version",
    "11",
  );
  await expect(page.locator(".agentmux-live-terminal-host").first()).toHaveAttribute(
    "data-agentmux-terminal-custom-glyphs",
    "false",
  );
  await expect(page.locator(".agentmux-live-terminal-host").first()).toHaveAttribute(
    "data-agentmux-terminal-font-family",
    /Cascadia Code/,
  );
  await expect(page.locator(".agentmux-live-terminal-host").first()).toHaveAttribute(
    "data-agentmux-terminal-ligatures",
    "true",
  );
  await expect(page.locator(".agentmux-live-terminal-host").first()).toHaveAttribute(
    "data-agentmux-terminal-font-feature-settings",
    /"calt" on/,
  );
  await expect
    .poll(() =>
      page
        .locator(".xterm")
        .first()
        .evaluate((element) => (element as HTMLElement).style.fontFeatureSettings),
    )
    .toContain('"calt"');
  await expect
    .poll(() =>
      page.evaluate(() => (window as any).__AGENTMUX_PREVIEW__?.terminalResizes()),
    )
    .toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          columns: expect.any(Number),
          rows: expect.any(Number),
        }),
      ]),
    );
});

test("live terminal accepts clipboard paste shortcuts", async ({ page }) => {
  await bootPreview(page);

  await page.evaluate(() => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        readText: async () => "echo pasted-from-clipboard\r",
        writeText: async (text: string) => {
          (window as unknown as { __AGENTMUX_TEST_COPIED__?: string }).__AGENTMUX_TEST_COPIED__ =
            text;
        },
      },
    });
  });

  await page.locator(".agentmux-new-terminal-tab").click();
  await expect(page.locator(".xterm").first()).toBeVisible();
  await page.locator(".xterm").first().click();
  await page.keyboard.press("Control+V");

  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).__AGENTMUX_PREVIEW__?.terminalOutput(),
      ),
    )
    .toContain("echo pasted-from-clipboard");
});

test("terminal profile picker can launch native shells", async ({ page }) => {
  await bootPreview(page);

  await page.locator(".agentmux-terminal-profile-menu-button").click();
  await expect(page.locator(".agentmux-terminal-profile-menu")).toBeVisible();
  await expect(page.getByText("Windows PowerShell")).toBeVisible();
  await expect(page.getByText("Command Prompt")).toBeVisible();
  await expect(page.getByText("Ubuntu")).toBeVisible();

  await page.getByText("Windows PowerShell").click();
  await expect(page.locator(".agentmux-terminal-profile-menu")).toHaveCount(0);
  await expect(page.locator(".xterm").first()).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).__AGENTMUX_PREVIEW__?.terminalOutput(),
      ),
    )
    .toContain("powershell.exe -NoLogo");
});

test("TextBox composer sends a draft to the active terminal", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-new-terminal-tab").click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await page.keyboard.press("Control+Alt+I");
  await expect(page.locator(".agentmux-textbox")).toBeVisible();
  await page.locator(".agentmux-textbox-input").fill("echo textbox-ready");
  await page.locator(".agentmux-textbox-send").click();
  await expect(page.locator(".agentmux-textbox")).toHaveCount(0);
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).__AGENTMUX_PREVIEW__?.terminalOutput(),
      ),
    )
    .toContain("echo textbox-ready");
});

test("TextBox draft persists for the active terminal until send", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-new-terminal-tab").click();
  await page.keyboard.press("Control+Alt+I");
  await page.locator(".agentmux-textbox-input").fill("echo persisted-draft");
  await page.locator(".agentmux-textbox-close").click();
  await expect(page.locator(".agentmux-textbox")).toHaveCount(0);

  await page.keyboard.press("Control+Alt+I");
  await expect(page.locator(".agentmux-textbox-input")).toHaveValue(
    "echo persisted-draft",
  );
  await page.locator(".agentmux-textbox-send").click();
  await expect(page.locator(".agentmux-textbox")).toHaveCount(0);

  await page.keyboard.press("Control+Alt+I");
  await expect(page.locator(".agentmux-textbox-input")).toHaveValue("");
});

test("TextBox uses project config max-line setting", async ({ page }) => {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "agentmux.preview.project.config.v1.ws_browser_preview_1",
      JSON.stringify({
        ui: {
          text_box_max_lines: 4,
        },
      }),
    );
  });
  await bootPreview(page);

  await page.locator(".agentmux-new-terminal-tab").click();
  await page.keyboard.press("Control+Alt+I");
  const input = page.locator(".agentmux-textbox-input");
  await expect(input).toHaveAttribute("data-agentmux-textbox-max-lines", "4");
  await expect(input).toHaveCSS("max-height", "90px");
});

test("agent launch titlebar button is removed pending rebuild", async ({
  page,
}) => {
  await bootPreview(page);
  await expect(page.locator(".agentmux-agent-launch")).toHaveCount(0);
});

test("workspace add creates one uniquely named workspace without duplicating the default", async ({
  page,
}) => {
  await bootPreview(page);

  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(1);
  await page.locator(".agentmux-workspace-plus").click();
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(2);
  await expect(
    page
      .locator(".agentmux-workspace-card")
      .filter({ hasText: "Workspace 1" }),
  ).toHaveCount(1);
  await expect(
    page.locator(".agentmux-workspace-inline-name-input"),
  ).toHaveValue("Workspace 2");
});

test("new workspace enters inline rename after creation", async ({ page }) => {
  await bootPreview(page);

  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(1);
  await page.locator(".agentmux-workspace-plus").click();
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(2);

  const nameInput = page.locator(".agentmux-workspace-inline-name-input");
  await expect(nameInput).toHaveValue("Workspace 2");
  await nameInput.fill("Second workspace");
  await nameInput.press("Enter");

  await expect(
    page
      .locator(".agentmux-workspace-card")
      .filter({ hasText: "Second workspace" }),
  ).toHaveCount(1);
  await expect(nameInput).toHaveCount(0);
});

test("workspace project settings update metadata and agent preset", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-settings-open").click();
  await page.locator(".agentmux-settings-tab-workspace").click();
  await page.locator(".agentmux-workspace-name-input").fill("Alpha project");
  await page.locator(".agentmux-workspace-root-input").fill("D:\\work\\alpha");
  await page
    .locator(".agentmux-workspace-description-input")
    .fill("Workspace metadata");
  await page.locator(".agentmux-workspace-icon-input").fill("AP");
  await page.locator(".agentmux-workspace-color-green").click();
  await page.locator(".agentmux-workspace-wsl-select").selectOption("Ubuntu");
  await page.locator(".agentmux-workspace-agent-input").fill("codex --resume");
  await page.locator(".agentmux-workspace-save").click();
  await page.keyboard.press("Escape");

  const card = page
    .locator(".agentmux-workspace-card")
    .filter({ hasText: "Alpha project" });
  await expect(card).toHaveCount(1);
  await expect(card).toContainText("Workspace metadata");
  await expect(page.getByText("D:\\work\\alpha").first()).toBeVisible();
});

test("workspace groups can be created edited collapsed and extended", async ({
  page,
}) => {
  await bootPreview(page);

  page.once("dialog", async (dialog) => {
    await dialog.accept("Agents");
  });
  await page.locator(".agentmux-workspace-group-create").click();
  await expect(page.locator("[data-agentmux-workspace-group]")).toHaveCount(1);
  await expect(
    page.locator("[data-agentmux-workspace-group]").first(),
  ).toContainText("Agents");
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(1);

  await page.locator(".agentmux-workspace-group-toggle").click();
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(0);
  await page.locator(".agentmux-workspace-group-toggle").click();
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(1);

  const editValues = ["Core", "CG", "#22C55E"];
  let editIndex = 0;
  page.on("dialog", async (dialog) => {
    await dialog.accept(editValues[editIndex++] ?? "");
  });
  await page.locator(".agentmux-workspace-group-edit").click();
  await expect(
    page.locator("[data-agentmux-workspace-group]").first(),
  ).toContainText("Core");

  await page.locator(".agentmux-workspace-group-new-workspace").click();
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(2);
  await expect(
    page.locator(".agentmux-workspace-inline-name-input"),
  ).toHaveValue("Workspace 2");
});

test("selected workspaces can be grouped and added to an existing group", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-workspace-plus").click();
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(2);
  await page.locator(".agentmux-workspace-select").nth(0).check();
  await page.locator(".agentmux-workspace-select").nth(1).check();
  await expect(page.locator(".agentmux-workspace-selection-bar")).toBeVisible();

  page.once("dialog", async (dialog) => {
    await dialog.accept("Batch");
  });
  await page.locator(".agentmux-workspace-selection-create-group").click();
  const group = page
    .locator("[data-agentmux-workspace-group]")
    .filter({ hasText: "Batch" });
  await expect(group).toHaveCount(1);
  await expect(group.locator(".agentmux-workspace-card")).toHaveCount(2);
  await expect(page.locator(".agentmux-workspace-selection-bar")).toHaveCount(
    0,
  );

  await page.locator(".agentmux-workspace-plus").click();
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(3);
  await page.locator(".agentmux-workspace-select").last().check();
  await page.locator(".agentmux-workspace-group-add-selected").click();
  await expect(group.locator(".agentmux-workspace-card")).toHaveCount(3);
  await expect(page.locator(".agentmux-workspace-selection-bar")).toHaveCount(
    0,
  );
});

test("workspace sidebar filter narrows groups and workspaces", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-workspace-plus").click();
  await page.locator(".agentmux-workspace-inline-name-input").press("Enter");
  await page.locator(".agentmux-workspace-plus").click();
  await page.locator(".agentmux-workspace-inline-name-input").press("Enter");
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(3);

  // Group the LAST ungrouped card ("Workspace 3") so the grouped name stays
  // distinct from the ungrouped "Workspace 1" card the filter assertions target.
  await page.locator(".agentmux-workspace-select").last().check();
  page.once("dialog", async (dialog) => {
    await dialog.accept("Agents");
  });
  await page.locator(".agentmux-workspace-selection-create-group").click();
  const filter = page.locator(".agentmux-workspace-filter-input");

  await filter.fill("Agents");
  const agents = page
    .locator("[data-agentmux-workspace-group]")
    .filter({ hasText: "Agents" });
  await expect(agents).toHaveCount(1);
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(1);
  await expect(
    page.locator(".agentmux-workspace-card").filter({ hasText: "Workspace 1" }),
  ).toHaveCount(0);

  await filter.fill("Workspace 1");
  await expect(page.locator("[data-agentmux-workspace-group]")).toHaveCount(0);
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(1);
  await expect(page.locator(".agentmux-workspace-card").first()).toContainText(
    "Workspace 1",
  );

  await filter.fill("not-here");
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(0);
  await expect(page.locator(".agentmux-workspace-filter-empty")).toBeVisible();

  await page.locator(".agentmux-workspace-filter-clear").click();
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(3);
});

test("workspace groups and members can be reordered from the sidebar", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-workspace-plus").click();
  await page.locator(".agentmux-workspace-plus").click();
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(3);

  await page.locator(".agentmux-workspace-select").nth(0).check();
  await page.locator(".agentmux-workspace-select").nth(1).check();
  page.once("dialog", async (dialog) => {
    await dialog.accept("Alpha");
  });
  await page.locator(".agentmux-workspace-selection-create-group").click();
  const alpha = page
    .locator("[data-agentmux-workspace-group]")
    .filter({ hasText: "Alpha" });
  await expect(alpha.locator(".agentmux-workspace-card")).toHaveCount(2);

  await page.locator(".agentmux-workspace-select").last().check();
  page.once("dialog", async (dialog) => {
    await dialog.accept("Beta");
  });
  await page.locator(".agentmux-workspace-selection-create-group").click();
  await expect(
    page.locator("[data-agentmux-workspace-group]").first(),
  ).toContainText("Alpha");

  await page.locator(".agentmux-workspace-group-move-up").last().click();
  await expect(
    page.locator("[data-agentmux-workspace-group]").first(),
  ).toContainText("Beta");

  await alpha.locator(".agentmux-workspace-member-move-down").first().click();
  await expect(alpha.locator(".agentmux-workspace-card").first()).toContainText(
    "Workspace 2",
  );
});

test("workspace groups and members can be drag reordered from the sidebar", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-workspace-plus").click();
  await page.locator(".agentmux-workspace-plus").click();
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(3);

  await page.locator(".agentmux-workspace-select").nth(0).check();
  await page.locator(".agentmux-workspace-select").nth(1).check();
  page.once("dialog", async (dialog) => {
    await dialog.accept("Alpha");
  });
  await page.locator(".agentmux-workspace-selection-create-group").click();

  await page.locator(".agentmux-workspace-select").last().check();
  page.once("dialog", async (dialog) => {
    await dialog.accept("Beta");
  });
  await page.locator(".agentmux-workspace-selection-create-group").click();

  const groups = page.locator("[data-agentmux-workspace-group]");
  const alpha = groups.filter({ hasText: "Alpha" });
  const beta = groups.filter({ hasText: "Beta" });
  await beta
    .locator(".agentmux-workspace-group-toggle")
    .dragTo(alpha.locator(".agentmux-workspace-group-toggle"), {
      targetPosition: { x: 12, y: 4 },
    });
  await expect(groups.first()).toContainText("Beta");

  await alpha
    .locator(".agentmux-workspace-card")
    .nth(1)
    .dragTo(alpha.locator(".agentmux-workspace-card").nth(0), {
      targetPosition: { x: 16, y: 4 },
    });
  await expect(alpha.locator(".agentmux-workspace-card").first()).toContainText(
    "Workspace 2",
  );
});

test("workspace cards can be reordered with explicit controls", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-workspace-plus").click();
  await page.locator(".agentmux-workspace-plus").click();
  const cards = page.locator(".agentmux-workspace-card");
  await expect(cards).toHaveCount(3);

  await cards.nth(2).locator(".agentmux-workspace-member-move-up").click();
  await expect(cards.nth(1)).toContainText("Workspace 3");

  await cards.nth(1).locator(".agentmux-workspace-member-move-down").click();
  await expect(cards.nth(2)).toContainText("Workspace 3");
});

test("surface tabs can be reordered and moved to another workspace", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-workspace-plus").click();
  const cards = page.locator(".agentmux-workspace-card");
  await expect(cards).toHaveCount(2);
  await cards.filter({ hasText: "Workspace 1" }).click();

  await page.locator(".agentmux-new-terminal-tab").click();
  await page.locator(".agentmux-new-terminal-tab").click();
  const tabs = page.locator(".agentmux-surface-tab");
  await expect(tabs).toHaveCount(2);

  const firstSurfaceId = await tabs
    .nth(0)
    .getAttribute("data-agentmux-surface-tab");
  const secondSurfaceId = await tabs
    .nth(1)
    .getAttribute("data-agentmux-surface-tab");
  expect(firstSurfaceId).toBeTruthy();
  expect(secondSurfaceId).toBeTruthy();

  await tabs.nth(1).locator(".agentmux-surface-tab-move-left").click();
  await expect(tabs.nth(0)).toHaveAttribute(
    "data-agentmux-surface-tab",
    secondSurfaceId ?? "",
  );

  await tabs.nth(0).locator(".agentmux-surface-tab-workspace-menu").click();
  const tabMenu = page.locator(".agentmux-surface-tab-menu");
  await expect(tabMenu).toBeVisible();
  await tabMenu
    .locator(".agentmux-surface-tab-menu-workspace")
    .filter({ hasText: "Workspace 2" })
    .click();

  await expect(
    page.locator('.agentmux-workspace-card[data-agentmux-active="true"]'),
  ).toContainText("Workspace 2");
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
});

test("split pane surfaces can be swapped with explicit controls", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-new-terminal-tab").click();
  await expect(
    page.locator('[data-agentmux-pane][data-agentmux-mounted="true"]'),
  ).toHaveCount(1);

  await page.locator(".agentmux-pane-split-horizontal").click();
  await expect(page.locator("[data-agentmux-pane]")).toHaveCount(2);
  await page.getByRole("button", { name: "Open terminal" }).click();
  const mountedPanes = page.locator(
    '[data-agentmux-pane][data-agentmux-mounted="true"]',
  );
  await expect(mountedPanes).toHaveCount(2);

  const firstSurfaceId = await mountedPanes
    .nth(0)
    .getAttribute("data-agentmux-mounted-surface");
  const secondSurfaceId = await mountedPanes
    .nth(1)
    .getAttribute("data-agentmux-mounted-surface");
  expect(firstSurfaceId).toBeTruthy();
  expect(secondSurfaceId).toBeTruthy();

  await mountedPanes.nth(0).locator(".agentmux-pane-surface-move-next").click();
  await expect(mountedPanes.nth(0)).toHaveAttribute(
    "data-agentmux-mounted-surface",
    secondSurfaceId ?? "",
  );
  await expect(mountedPanes.nth(1)).toHaveAttribute(
    "data-agentmux-mounted-surface",
    firstSurfaceId ?? "",
  );
});

test("workspace group context menu exposes primary actions", async ({
  page,
}) => {
  await bootPreview(page);

  page.once("dialog", async (dialog) => {
    await dialog.accept("Alpha");
  });
  await page.locator(".agentmux-workspace-group-create").click();
  const alpha = page
    .locator("[data-agentmux-workspace-group]")
    .filter({ hasText: "Alpha" });
  await expect(alpha).toHaveCount(1);

  await alpha
    .locator(".agentmux-workspace-group-toggle")
    .click({ button: "right" });
  await expect(page.locator(".agentmux-workspace-group-menu")).toBeVisible();
  await page.locator(".agentmux-workspace-group-menu-new-workspace").click();
  await expect(alpha.locator(".agentmux-workspace-card")).toHaveCount(2);

  await page.locator(".agentmux-workspace-plus").click();
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(3);
  await page.locator(".agentmux-workspace-select").last().check();
  page.once("dialog", async (dialog) => {
    await dialog.accept("Beta");
  });
  await page.locator(".agentmux-workspace-selection-create-group").click();

  const groups = page.locator("[data-agentmux-workspace-group]");
  const beta = groups.filter({ hasText: "Beta" });
  await beta
    .locator(".agentmux-workspace-group-toggle")
    .click({ button: "right" });
  await page.locator(".agentmux-workspace-group-menu-move-up").click();
  await expect(groups.first()).toContainText("Beta");
});

test("workspace context menu warns before closing a group anchor", async ({
  page,
}) => {
  await bootPreview(page);

  page.once("dialog", async (dialog) => {
    await dialog.accept("Anchors");
  });
  await page.locator(".agentmux-workspace-group-create").click();
  const group = page
    .locator("[data-agentmux-workspace-group]")
    .filter({ hasText: "Anchors" });
  const anchorCard = group
    .locator(".agentmux-workspace-card")
    .filter({ hasText: "Workspace 1" });
  await expect(anchorCard).toHaveCount(1);

  await page.locator(".agentmux-workspace-plus").click();
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(2);

  await anchorCard.click({ button: "right" });
  await expect(page.locator(".agentmux-workspace-menu")).toBeVisible();
  await expect(
    page.locator(".agentmux-workspace-menu-anchor-warning"),
  ).toContainText("1개 그룹 anchor");

  await page.locator(".agentmux-workspace-menu-close").click();

  const confirmation = page.locator(".agentmux-confirm-modal");
  await expect(confirmation).toBeVisible();
  await expect(confirmation).toContainText("Anchors");
  await expect(confirmation).toContainText("clear those group anchors");
  await confirmation.locator(".agentmux-confirm-confirm").click();
  await expect(anchorCard).toHaveCount(0);
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(1);
});

test("workspace close warns before terminating open terminal sessions", async ({
  page,
}) => {
  await bootPreview(page);

  const workspaceCard = page
    .locator(".agentmux-workspace-card")
    .filter({ hasText: "Workspace 1" });
  await expect(workspaceCard).toHaveCount(1);

  await page.locator(".agentmux-new-terminal-tab").click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);

  await workspaceCard.click({ button: "right" });
  await page.locator(".agentmux-workspace-menu-close").click();
  const cancelConfirm = page.locator(".agentmux-confirm-modal");
  await expect(cancelConfirm).toBeVisible();
  await expect(cancelConfirm).toContainText("open terminal session");
  await cancelConfirm.locator(".agentmux-confirm-cancel").click();
  await expect(cancelConfirm).toHaveCount(0);
  await expect(workspaceCard).toHaveCount(1);
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);

  await workspaceCard.click({ button: "right" });
  await page.locator(".agentmux-workspace-menu-close").click();
  const acceptConfirm = page.locator(".agentmux-confirm-modal");
  await expect(acceptConfirm).toBeVisible();
  await expect(acceptConfirm).toContainText("terminate those sessions");
  await acceptConfirm.locator(".agentmux-confirm-confirm").click();
  await expect(workspaceCard).toHaveCount(0);
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(0);
});

test("new WSL terminal adds a separate top tab without changing the split layout", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-new-terminal-tab").click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await expect(
    page.locator('[data-agentmux-pane][data-agentmux-mounted="true"]'),
  ).toHaveCount(1);

  await page.locator(".agentmux-new-terminal-tab").click();

  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(2);
  await expect(
    page.locator('[data-agentmux-pane][data-agentmux-mounted="true"]'),
  ).toHaveCount(1);
  await expect(page.locator("[data-agentmux-pane]")).toHaveCount(1);

  await page.locator(".agentmux-surface-tab-close").last().click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await expect(
    page.locator('[data-agentmux-pane][data-agentmux-mounted="true"]'),
  ).toHaveCount(1);
  await expect(page.locator("[data-agentmux-pane]")).toHaveCount(1);
});

test("split panes stay scoped to their top tab", async ({ page }) => {
  await bootPreview(page);

  await page.getByRole("button", { name: "Open terminal" }).last().click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await expect(page.locator("[data-agentmux-pane]")).toHaveCount(1);

  await page.locator(".agentmux-top-split-vertical").click();
  await expect(page.locator("[data-agentmux-pane]")).toHaveCount(2);
  await expect(
    page.locator('[data-agentmux-pane][data-agentmux-mounted="true"]'),
  ).toHaveCount(1);

  await page.getByRole("button", { name: "Open terminal" }).last().click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await expect(page.locator("[data-agentmux-pane]")).toHaveCount(2);
  await expect(
    page.locator('[data-agentmux-pane][data-agentmux-mounted="true"]'),
  ).toHaveCount(2);

  await page.locator(".agentmux-new-terminal-tab").click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(2);
  await expect(page.locator("[data-agentmux-pane]")).toHaveCount(1);
  await expect(
    page.locator('[data-agentmux-pane][data-agentmux-mounted="true"]'),
  ).toHaveCount(1);

  await page.locator(".agentmux-surface-tab").first().click();
  await expect(page.locator("[data-agentmux-pane]")).toHaveCount(2);
  await expect(
    page.locator('[data-agentmux-pane][data-agentmux-mounted="true"]'),
  ).toHaveCount(2);

  await page.locator(".agentmux-surface-tab").last().click();
  await page.locator(".agentmux-surface-tab-close").last().click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await expect(page.locator("[data-agentmux-pane]")).toHaveCount(2);
  await expect(
    page.locator('[data-agentmux-pane][data-agentmux-mounted="true"]'),
  ).toHaveCount(2);
});

test("terminal profile picker can launch native shells in split panes", async ({
  page,
}) => {
  await bootPreview(page);

  await page.getByRole("button", { name: "Open terminal" }).last().click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await page.locator(".agentmux-top-split-vertical").click();
  await expect(page.locator("[data-agentmux-pane]")).toHaveCount(2);
  await expect(
    page.locator('[data-agentmux-pane][data-agentmux-mounted="true"]'),
  ).toHaveCount(1);

  await page.locator(".agentmux-pane-terminal-profile-menu-button").click();
  await expect(page.locator(".agentmux-terminal-profile-menu")).toBeVisible();
  await expect(page.getByText("Open in pane")).toBeVisible();
  await expect(page.getByText("Command Prompt")).toBeVisible();

  await page.getByText("Command Prompt").click();
  await expect(page.locator(".agentmux-terminal-profile-menu")).toHaveCount(0);
  await expect(
    page.locator('[data-agentmux-pane][data-agentmux-mounted="true"]'),
  ).toHaveCount(2);
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).__AGENTMUX_PREVIEW__?.terminalOutput(),
      ),
    )
    .toContain("cmd.exe /d /q");
});

test("browser surface opens as a separate top tab", async ({ page }) => {
  await bootPreview(page);

  await page.locator(".agentmux-new-terminal-tab").click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);

  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("P");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await page.keyboard.type("browser");
  await page.keyboard.press("Enter");

  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(2);
  await expect(page.locator("[data-agentmux-pane]")).toHaveCount(1);
  await expect(page.getByPlaceholder("URL")).toBeVisible();
});

test("agent tmux session opens as a separate top tab without splitting the current tab", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-new-terminal-tab").click();
  await expect(page.getByText("wsl-direct").first()).toBeVisible({
    timeout: 5000,
  });
  await expect(
    page.locator('[data-agentmux-pane][data-agentmux-mounted="true"]'),
  ).toHaveCount(1);

  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("P");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await page.keyboard.type("Claude");
  await page.keyboard.press("Enter");

  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(2);
  await expect(
    page.locator('[data-agentmux-pane][data-agentmux-mounted="true"]'),
  ).toHaveCount(1);
  await expect(page.locator("[data-agentmux-pane]")).toHaveCount(1);
  await expect(page.getByText("wsl-tmux-control").first()).toBeVisible({
    timeout: 5000,
  });
});

test("agent tmux launch shows install guidance when tmux is missing", async ({
  page,
}) => {
  await page.addInitScript(() => {
    (
      window as unknown as { __AGENTMUX_PREVIEW_TMUX_AVAILABLE__?: boolean }
    ).__AGENTMUX_PREVIEW_TMUX_AVAILABLE__ = false;
  });
  await bootPreview(page);

  await page.locator(".agentmux-new-terminal-tab").click();
  await expect(page.getByText("wsl-direct").first()).toBeVisible({
    timeout: 5000,
  });

  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("P");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await page.keyboard.type("Claude");
  await page.keyboard.press("Enter");

  await expect(
    page.getByText("sudo apt update && sudo apt install -y tmux").first(),
  ).toBeVisible({
    timeout: 5000,
  });
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await expect(page.locator("[data-agentmux-pane]")).toHaveCount(1);
  await expect(page.getByText("wsl-tmux-control")).toHaveCount(0);
});

test("shows WSL install guidance when no distribution is available", async ({
  page,
}) => {
  await page.addInitScript(() => {
    (
      window as unknown as {
        __AGENTMUX_PREVIEW_WSL_DISTRIBUTIONS__?: unknown[];
      }
    ).__AGENTMUX_PREVIEW_WSL_DISTRIBUTIONS__ = [];
  });
  await bootPreview(page);
  await expect(page.getByText("wsl --install").first()).toBeVisible({
    timeout: 5000,
  });
  await page.locator(".agentmux-setup-open").click();
  await expect(page.locator(".agentmux-setup-modal")).toBeVisible();
  await expect(
    page
      .locator(".agentmux-setup-modal code")
      .getByText("wsl --install", { exact: true }),
  ).toBeVisible();
});

test("setup wizard saves workspace defaults and probes tmux", async ({
  page,
}) => {
  await bootPreview(page);

  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("P");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await page.keyboard.type("setup");
  await page.keyboard.press("Enter");

  const setup = page.locator(".agentmux-setup-modal");
  await expect(setup).toBeVisible();
  await expect(page.locator(".agentmux-setup-wsl-select")).toHaveValue(
    "Ubuntu",
  );
  await page
    .locator(".agentmux-setup-root-input")
    .fill("D:\\Workspace\\setup-preview");
  await page.locator(".agentmux-setup-tmux-probe").click();
  await expect(
    setup.getByText("tmux is available in the preview WSL distribution."),
  ).toBeVisible();
  await page.locator(".agentmux-setup-save").click();
  await page.keyboard.press("Escape");

  await page.locator(".agentmux-settings-open").click();
  await page.locator(".agentmux-settings-tab-workspace").click();
  await expect(page.locator(".agentmux-workspace-root-input")).toHaveValue(
    "D:\\Workspace\\setup-preview",
  );
  await expect(page.locator(".agentmux-workspace-wsl-select")).toHaveValue(
    "Ubuntu",
  );
});

test("notification action hooks execute configured UI actions", async ({
  page,
}) => {
  await page.addInitScript(() => {
    (
      window as unknown as {
        __AGENTMUX_PREVIEW_WSL_DISTRIBUTIONS__?: unknown[];
      }
    ).__AGENTMUX_PREVIEW_WSL_DISTRIBUTIONS__ = [];
    window.localStorage.setItem(
      "agentmux.preview.config.v1",
      JSON.stringify({
        formatVersion: "agentmux.config.v1",
        configPath: "localStorage://agentmux.preview.config.v1",
        appearance: {
          theme: "dark",
          accentKey: "orange",
          fontSize: 12.5,
        },
        notifications: {
          actions: [
            {
              action: "browser.openNewTab",
              label: "Open setup",
              notificationType: "diagnostics.wsl_required",
              severity: "warning",
              dismissOnRun: true,
            },
          ],
        },
      }),
    );
  });
  await bootPreview(page);

  await page.locator(".agentmux-settings-open").click();
  await page.locator(".agentmux-settings-tab-general").click();
  await expect(page.getByRole("button", { name: "Open setup" })).toBeVisible();
  await page
    .locator(".agentmux-notification-action-browser-openNewTab")
    .click();
  await expect(page.getByPlaceholder("URL")).toBeVisible();
});

test("command palette lists actions", async ({ page }) => {
  await bootPreview(page);
  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("P");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await expect(page.locator(".agentmux-palette-item").first()).toBeVisible();
  await expect(page.locator(".agentmux-palette-item")).not.toHaveCount(0);
  await page.keyboard.press("Escape");
});

test("command palette supports arrow navigation and enter execution", async ({
  page,
}) => {
  await bootPreview(page);

  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("P");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await page.keyboard.type("workspace");

  const selected = page.locator(".agentmux-palette-item-selected");
  await expect(selected).toHaveCount(1);
  const before = await selected.textContent();
  await page.keyboard.press("ArrowDown");
  await expect.poll(async () => selected.textContent()).not.toBe(before);
  await page.keyboard.press("Escape");

  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("P");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await page.keyboard.type("browser");
  await page.keyboard.press("Enter");

  await expect(page.getByPlaceholder("URL")).toBeVisible();
});

test("shortcut bindings support config override and two-step chords", async ({
  page,
}) => {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "agentmux.preview.config.v1",
      JSON.stringify({
        formatVersion: "agentmux.config.v1",
        configPath: "localStorage://agentmux.preview.config.v1",
        appearance: {
          theme: "dark",
          accentKey: "orange",
          fontSize: 12.5,
        },
        shortcuts: {
          bindings: {
            "workspace.new": ["ctrl+b", "c"],
          },
        },
      }),
    );
  });
  await bootPreview(page);

  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(1);
  await page.keyboard.down("Control");
  await page.keyboard.press("B");
  await page.keyboard.up("Control");
  await page.keyboard.press("C");
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(2);
  await expect(
    page.locator(".agentmux-workspace-inline-name-input"),
  ).toHaveValue("Workspace 2");
});

test("settings can edit shortcuts and report conflicts", async ({ page }) => {
  await bootPreview(page);

  await page.locator(".agentmux-settings-open").click();
  await page.locator(".agentmux-settings-tab-keys").click();

  page.once("dialog", async (dialog) => {
    await dialog.accept("ctrl+t");
  });
  await page
    .locator(
      '[data-agentmux-shortcut-row="workspace.new"] .agentmux-shortcut-edit',
    )
    .click();
  await expect(page.locator(".agentmux-shortcut-conflict")).toContainText(
    "workspace.new",
  );
  await expect(page.locator(".agentmux-shortcut-conflict")).toContainText(
    "terminal.newWsl",
  );

  page.once("dialog", async (dialog) => {
    await dialog.accept("ctrl+b, c");
  });
  await page
    .locator(
      '[data-agentmux-shortcut-row="workspace.new"] .agentmux-shortcut-edit',
    )
    .click();
  await expect(page.locator(".agentmux-shortcut-conflict")).toHaveCount(0);
  await expect(page.locator(".agentmux-shortcut-edit-message")).toContainText(
    "Shortcut saved",
  );

  await page.keyboard.press("Escape");
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(1);
  await page.keyboard.down("Control");
  await page.keyboard.press("B");
  await page.keyboard.up("Control");
  await page.keyboard.press("C");
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(2);
});

test("custom config actions appear in palette and execute through shortcuts", async ({
  page,
}) => {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "agentmux.preview.config.v1",
      JSON.stringify({
        formatVersion: "agentmux.config.v1",
        configPath: "localStorage://agentmux.preview.config.v1",
        appearance: {
          theme: "dark",
          accentKey: "orange",
          fontSize: 12.5,
        },
        shortcuts: {
          bindings: {
            "custom.runTests": ["ctrl+b", "t"],
          },
        },
        actions: {
          custom: [
            {
              id: "custom.runTests",
              title: "Run project tests",
              group: "agent",
              target: "agent",
              command: ["npm", "test"],
              keywords: ["verify"],
            },
          ],
        },
      }),
    );
  });
  await bootPreview(page);

  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("P");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await page.keyboard.type("project tests");
  await expect(page.getByText("Run project tests").first()).toBeVisible();
  await page.keyboard.press("Escape");

  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(0);
  await page.keyboard.down("Control");
  await page.keyboard.press("B");
  await page.keyboard.up("Control");
  await page.keyboard.press("T");
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await expect(page.getByText("wsl-tmux-control").first()).toBeVisible({
    timeout: 5000,
  });
});

test("custom browser config actions can navigate presets", async ({ page }) => {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "agentmux.preview.config.v1",
      JSON.stringify({
        formatVersion: "agentmux.config.v1",
        configPath: "localStorage://agentmux.preview.config.v1",
        appearance: {
          theme: "dark",
          accentKey: "orange",
          fontSize: 12.5,
        },
        shortcuts: {
          bindings: {
            "custom.openDocs": ["ctrl+b", "o"],
          },
        },
        actions: {
          custom: [
            {
              id: "custom.openDocs",
              title: "Open docs preset",
              group: "terminal",
              target: "browser",
              command: ["new-tab", "https://example.com/docs"],
              keywords: ["docs", "browser"],
            },
          ],
        },
      }),
    );
  });
  await bootPreview(page);

  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(0);
  await page.keyboard.down("Control");
  await page.keyboard.press("B");
  await page.keyboard.up("Control");
  await page.keyboard.press("O");
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await expect
    .poll(() =>
      page.evaluate(() => (window as any).__AGENTMUX_PREVIEW__?.browserUrl()),
    )
    .toBe("https://example.com/docs");
});

test("custom browser config actions can run automation recipes", async ({
  page,
}) => {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "agentmux.preview.config.v1",
      JSON.stringify({
        formatVersion: "agentmux.config.v1",
        configPath: "localStorage://agentmux.preview.config.v1",
        appearance: {
          theme: "dark",
          accentKey: "orange",
          fontSize: 12.5,
        },
        shortcuts: {
          bindings: {
            "custom.captureBrowser": ["ctrl+b", "s"],
            "custom.fillBrowser": ["ctrl+b", "f"],
          },
        },
        actions: {
          custom: [
            {
              id: "custom.captureBrowser",
              title: "Capture browser",
              group: "view",
              target: "browser",
              command: ["screenshot", "jpeg", "active-pane"],
              keywords: ["browser", "capture"],
            },
            {
              id: "custom.fillBrowser",
              title: "Fill browser",
              group: "view",
              target: "browser",
              command: ["fill", "#q", "agentmux", "frame:frame_1"],
              keywords: ["browser", "form"],
            },
          ],
        },
      }),
    );
  });
  await bootPreview(page);

  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(0);
  await page.keyboard.down("Control");
  await page.keyboard.press("B");
  await page.keyboard.up("Control");
  await page.keyboard.press("S");
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).__AGENTMUX_PREVIEW__?.browserActions()?.join("\n"),
      ),
    )
    .toContain("screenshot:");
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).__AGENTMUX_PREVIEW__?.browserActions()?.join("\n"),
      ),
    )
    .toContain(":jpeg");
  await page.keyboard.down("Control");
  await page.keyboard.press("B");
  await page.keyboard.up("Control");
  await page.keyboard.press("F");
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).__AGENTMUX_PREVIEW__?.browserActions()?.join("\n"),
      ),
    )
    .toContain("fill:");
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).__AGENTMUX_PREVIEW__?.browserActions()?.join("\n"),
      ),
    )
    .toContain("frame=frame_1");
});

test("config can rebind workspace plus and surface tab actions", async ({
  page,
}) => {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "agentmux.preview.config.v1",
      JSON.stringify({
        formatVersion: "agentmux.config.v1",
        configPath: "localStorage://agentmux.preview.config.v1",
        appearance: {
          theme: "dark",
          accentKey: "orange",
          fontSize: 12.5,
        },
        actions: {
          custom: [
            {
              id: "custom.runTests",
              title: "Run project tests",
              group: "agent",
              target: "agent",
              command: ["npm", "test"],
              keywords: ["verify"],
            },
          ],
        },
        ui: {
          workspacePlusAction: "terminal.newWsl",
          surfaceTabPlusAction: "browser.openNewTab",
          surfaceTabActions: ["custom.runTests"],
        },
      }),
    );
  });
  await bootPreview(page);

  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(1);
  await page.locator(".agentmux-workspace-plus").click();
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(1);
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await expect(page.getByText("wsl-direct").first()).toBeVisible({
    timeout: 5000,
  });

  await page.locator(".agentmux-new-terminal-tab").click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(2);
  await expect(page.getByPlaceholder("URL")).toBeVisible();

  await page.locator(".agentmux-tab-action-custom-runTests").click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(3);
  await expect(page.getByText("wsl-tmux-control").first()).toBeVisible({
    timeout: 5000,
  });
});

test("theme toggle switches label", async ({ page }) => {
  await bootPreview(page);
  const toggle = page.locator(".agentmux-theme-toggle");
  const beforeText = await toggle.textContent();
  await toggle.click();
  const afterText = await toggle.textContent();
  expect(afterText).not.toBe(beforeText);
});

test("appearance settings persist through reload", async ({ page }) => {
  await bootPreview(page);
  const toggle = page.locator(".agentmux-theme-toggle");
  await toggle.click();
  await page.waitForFunction(() => {
    const raw = window.localStorage.getItem("agentmux.preview.config.v1");
    return raw ? JSON.parse(raw).appearance?.theme === "light" : false;
  });

  await page.reload();
  await waitForPreviewReady(page);
  await expect
    .poll(() =>
      page.evaluate(() => {
        const raw = window.localStorage.getItem("agentmux.preview.config.v1");
        return raw ? JSON.parse(raw).appearance?.theme : null;
      }),
    )
    .toBe("light");
});

test("terminal inner margin setting applies to live terminals", async ({
  page,
}) => {
  await bootPreview(page);

  await page.locator(".agentmux-new-terminal-tab").click();
  const terminalHost = page
    .locator("[data-agentmux-terminal-inner-margin]")
    .first();
  await expect(terminalHost).toHaveAttribute(
    "data-agentmux-terminal-inner-margin",
    "0",
  );
  await expect(terminalHost).toHaveCSS("background-color", "rgb(14, 17, 22)");

  await page.locator(".agentmux-settings-open").click();
  const marginSlider = page.locator(".agentmux-terminal-inner-margin");
  await expect(marginSlider).toHaveValue("0");
  await marginSlider.focus();
  for (let index = 0; index < 12; index += 1) {
    await page.keyboard.press("ArrowRight");
  }

  await expect(terminalHost).toHaveAttribute(
    "data-agentmux-terminal-inner-margin",
    "12",
  );
  await expect(terminalHost).toHaveCSS("background-color", "rgb(14, 17, 22)");
  await page.waitForFunction(() => {
    const raw = window.localStorage.getItem("agentmux.preview.config.v1");
    return raw ? JSON.parse(raw).ui?.terminalInnerMargin === 12 : false;
  });
});

test("settings reload config applies external changes without restart", async ({
  page,
}) => {
  await bootPreview(page);

  await page.evaluate(() => {
    window.localStorage.setItem(
      "agentmux.preview.config.v1",
      JSON.stringify({
        formatVersion: "agentmux.config.v1",
        configPath: "localStorage://agentmux.preview.config.v1",
        appearance: {
          theme: "light",
          accentKey: "blue",
          fontSize: 15,
        },
        shortcuts: {
          bindings: {
            "workspace.new": ["ctrl+b", "c"],
          },
        },
      }),
    );
  });

  await page.locator(".agentmux-settings-open").click();
  await page.locator(".agentmux-settings-tab-general").click();
  await page.locator(".agentmux-config-reload").click();

  await expect(page.locator(".agentmux-config-reload-message")).toContainText(
    "Config reloaded",
  );
  await expect
    .poll(() =>
      page
        .locator("[data-agentmux-root]")
        .evaluate((node) =>
          getComputedStyle(node).getPropertyValue("--bg").trim(),
        ),
    )
    .toBe("#F4F5F7");
  await expect
    .poll(() =>
      page
        .locator("[data-agentmux-root]")
        .evaluate((node) =>
          getComputedStyle(node).getPropertyValue("--accent").trim(),
        ),
    )
    .toBe("#3B82F6");
});

test("settings can import and reset config JSON", async ({ page }) => {
  await bootPreview(page);

  await page.locator(".agentmux-settings-open").click();
  await page.locator(".agentmux-settings-tab-general").click();
  await expect(page.locator(".agentmux-project-config-import")).toBeEnabled();

  page.once("dialog", async (dialog) => {
    await dialog.accept(
      JSON.stringify({
        format_version: "agentmux.config.v1",
        appearance: {
          theme: "light",
          accent_key: "blue",
          font_size: 15,
        },
        shortcuts: {
          bindings: {
            "workspace.new": "ctrl+j",
          },
        },
        actions: {
          custom: [],
        },
        ui: {},
        notifications: {
          actions: [],
        },
      }),
    );
  });
  await page.locator(".agentmux-config-import").click();
  await expect(page.locator(".agentmux-config-reload-message")).toContainText(
    "Config imported",
  );
  await expect
    .poll(() =>
      page
        .locator("[data-agentmux-root]")
        .evaluate((node) =>
          getComputedStyle(node).getPropertyValue("--bg").trim(),
        ),
    )
    .toBe("#F4F5F7");

  page.once("dialog", async (dialog) => {
    await dialog.accept(
      JSON.stringify({
        ui: {
          workspace_plus_action: "terminal.newWsl",
        },
      }),
    );
  });
  await page.locator(".agentmux-project-config-import").click();
  await expect(page.locator(".agentmux-config-reload-message")).toContainText(
    "Project config imported",
  );
  await page.keyboard.press("Escape");
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(0);
  await page.locator(".agentmux-workspace-plus").click();
  await expect(page.locator(".agentmux-workspace-card")).toHaveCount(1);
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);

  await page.locator(".agentmux-settings-open").click();
  await page.locator(".agentmux-settings-tab-general").click();
  page.once("dialog", async (dialog) => {
    await dialog.accept();
  });
  await page.locator(".agentmux-project-config-reset").click();
  await expect(page.locator(".agentmux-config-reload-message")).toContainText(
    "Project config reset",
  );

  page.once("dialog", async (dialog) => {
    await dialog.accept();
  });
  await page.locator(".agentmux-config-reset").click();
  await expect(page.locator(".agentmux-config-reload-message")).toContainText(
    "Config reset",
  );
  await expect
    .poll(() =>
      page.evaluate(() => {
        const raw = window.localStorage.getItem("agentmux.preview.config.v1");
        return raw ? JSON.parse(raw).appearance?.theme : null;
      }),
    )
    .toBe("dark");
});

test("settings can migrate preview cmux project config", async ({ page }) => {
  await bootPreview(page);

  await page.evaluate(() => {
    const workspaceId = "ws_browser_preview_1";
    window.localStorage.removeItem(
      `agentmux.preview.project.config.v1.${workspaceId}`,
    );
    window.localStorage.setItem(
      `agentmux.preview.cmux.project.config.v1.${workspaceId}`,
      JSON.stringify({
        ui: {
          workspace_plus_action: "terminal.newWsl",
        },
      }),
    );
  });

  await page.locator(".agentmux-settings-open").click();
  await page.locator(".agentmux-settings-tab-general").click();
  await expect(
    page.locator(".agentmux-project-config-migrate-cmux"),
  ).toBeEnabled();
  await page.locator(".agentmux-project-config-migrate-cmux").click();
  await expect(page.locator(".agentmux-config-reload-message")).toContainText(
    ".cmux config migrated",
  );
  await expect(
    page.locator('[data-agentmux-config-diagnostic-source="project"]'),
  ).toContainText("active");
  await expect(
    page.locator('[data-agentmux-config-diagnostic-source="cmux_project"]'),
  ).toContainText("idle");
  await expect
    .poll(() =>
      page.evaluate(() => {
        const raw = window.localStorage.getItem(
          "agentmux.preview.project.config.v1.ws_browser_preview_1",
        );
        return raw ? JSON.parse(raw).ui?.workspacePlusAction : null;
      }),
    )
    .toBe("terminal.newWsl");

  await page.keyboard.press("Escape");
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(0);
  await page.locator(".agentmux-workspace-plus").click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
});

test("unfinished SSH UI is hidden from settings and sidebar", async ({
  page,
}) => {
  await bootPreview(page);

  await expect(page.getByText("?먭꺽 쨌 SSH")).toHaveCount(0);
  await expect(page.getByText("prod-server")).toHaveCount(0);
  await page.locator(".agentmux-settings-open").click();
  await expect(page.locator(".agentmux-settings-tab-profiles")).toHaveCount(0);
  await expect(page.getByText("?꾨줈??쨌 SSH")).toHaveCount(0);
  await expect(page.locator(".agentmux-profile-edit")).toHaveCount(0);
});

test("settings diagnostics runs tmux probe", async ({ page }) => {
  await bootPreview(page);

  await page.locator(".agentmux-settings-open").click();
  await page.locator(".agentmux-settings-tab-diagnostics").click();
  await page.locator(".agentmux-tmux-probe").click();

  const diagnostics = page.locator("[data-agentmux-diagnostics]");
  await expect(diagnostics).toContainText("available", { timeout: 5000 });
  await expect(diagnostics).toContainText("tmux 3.4-preview");
});

test("OMC telemetry bar renders", async ({ page }) => {
  await bootPreview(page);
  const openBtn = page.locator(".agentmux-new-terminal-tab");
  if (await openBtn.isVisible()) {
    await openBtn.click();
  }
  await expect(page.getByText("wsl-direct").first()).toBeVisible({
    timeout: 5000,
  });
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
    }),
  );
  await expect(page.getByText("[OMC]").first()).toBeVisible({ timeout: 5000 });
});

test("sidebar metadata renders status progress and logs", async ({ page }) => {
  await bootPreview(page);

  await page.evaluate(() =>
    (window as any).__AGENTMUX_PREVIEW__?.sidebarState({
      statuses: [
        {
          key: "build",
          label: "compiling",
          icon: "hammer",
          color: "#FBBF24",
          priority: 80,
        },
      ],
      progress: {
        value: 0.5,
        label: "Building",
      },
      logs: [
        {
          level: "success",
          source: "test",
          message: "All tests passed",
        },
      ],
    }),
  );

  const sidebar = page.locator("[data-agentmux-sidebar-state]");
  await expect(sidebar).toContainText("compiling", { timeout: 5000 });
  await expect(sidebar).toContainText("Building");
  await expect(sidebar).toContainText("50%");
  await expect(sidebar).toContainText("All tests passed");
});

test("team collaboration panel renders tasks, mailbox, and context link", async ({ page }) => {
  await bootPreview(page);

  await page.evaluate(() => {
    const preview = (window as any).__AGENTMUX_PREVIEW__;
    preview?.teamTask({
      taskId: "task_ui_blocked",
      title: "UserService API spec",
      status: "blocked",
      blockedReason: "waiting on API spec",
    });
    preview?.teamMessage({
      messageId: "msg_ui_mailbox",
      kind: "mailbox",
      body: "Agent 2 needs https://example.invalid/pr/42 before integration.",
    });
  });

  const teamPanel = page.locator("[data-agentmux-team-panel]");
  await expect(teamPanel).toContainText("UserService API spec", {
    timeout: 5000,
  });
  await expect(teamPanel).toContainText("waiting on API spec");
  await expect(teamPanel).toContainText("Mailbox 1 unread");
  await expect(page.locator("body")).toContainText("Tasks 0/1");

  await page.keyboard.press("Control+Shift+L");
  await expect
    .poll(() =>
      page.evaluate(() => (window as any).__AGENTMUX_PREVIEW__?.browserUrl()),
    )
    .toBe("https://example.invalid/pr/42");

  await page
    .locator("[data-agentmux-team-task='task_ui_blocked']")
    .getByRole("button", { name: "Done" })
    .click();
  await expect(page.locator("body")).toContainText("Tasks 1/1");

  await page
    .locator("[data-agentmux-team-message='msg_ui_mailbox']")
    .getByRole("button", { name: "Read" })
    .click();
  await expect(teamPanel).toContainText("Mailbox all read");
});

test("Dock panel renders project dock controls", async ({ page }) => {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "agentmux.preview.project.dock.v1.ws_browser_preview_1",
      JSON.stringify({
        controls: [
          {
            id: "git",
            title: "Git",
            command: "lazygit",
            height: 300,
          },
          {
            id: "logs",
            title: "Logs",
            command: "tail -f ./logs/development.log",
            cwd: ".",
            env: {
              NO_COLOR: "1",
            },
          },
        ],
      }),
    );
  });
  await bootPreview(page);

  const dock = page.locator(".agentmux-dock-panel");
  await expect(dock).toBeVisible();
  await expect(dock.locator(".agentmux-dock-source")).toContainText(
    ".agentmux",
  );
  await expect(dock.locator(".agentmux-dock-trust")).toContainText("review");
  await expect(
    dock.locator('[data-agentmux-dock-control="git"]'),
  ).toContainText("lazygit");
  await expect(
    dock.locator('[data-agentmux-dock-control="logs"]'),
  ).toContainText("tail -f");
  await expect(
    dock.locator('[data-agentmux-dock-control="logs"]'),
  ).toContainText("env");
});

test("Dock controls require trust and launch inside the Dock panel", async ({
  page,
}) => {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "agentmux.preview.project.dock.v1.ws_browser_preview_1",
      JSON.stringify({
        controls: [
          {
            id: "git",
            title: "Git",
            command: "lazygit",
            cwd: ".",
            env: {
              NO_COLOR: "1",
            },
          },
        ],
      }),
    );
  });
  await bootPreview(page);
  await page.locator(".agentmux-new-terminal-tab").click();
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);

  const dock = page.locator(".agentmux-dock-panel");
  const git = dock.locator('[data-agentmux-dock-control="git"]');
  const run = git.locator(".agentmux-dock-run");

  await expect(run).toBeDisabled();
  await dock.locator(".agentmux-dock-trust-approve").click();
  await expect(dock.locator(".agentmux-dock-trust")).toContainText("trusted");
  await expect(run).toBeEnabled();
  await run.click();

  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await expect(page.locator(".agentmux-surface-tab").first()).not.toContainText(
    "Git",
  );
  await expect(git.locator(".agentmux-dock-terminal")).toBeVisible();
  await expect(git.locator(".agentmux-dock-height")).toHaveValue("180");
  await git.locator(".agentmux-dock-height").evaluate((node) => {
    const input = node as HTMLInputElement;
    input.value = "260";
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await expect(git.locator(".agentmux-dock-height-value")).toContainText(
    "260px",
  );
  await expect(git.locator(".agentmux-dock-terminal")).toHaveCSS(
    "height",
    "260px",
  );
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).__AGENTMUX_PREVIEW__?.terminalOutput(),
      ),
    )
    .toContain("lazygit");
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as any).__AGENTMUX_PREVIEW__?.terminalOutput(),
      ),
    )
    .toContain("env NO_COLOR");

  await git.locator(".agentmux-dock-close").click();
  await expect(git.locator(".agentmux-dock-terminal")).toHaveCount(0);
  await expect(page.locator(".agentmux-surface-tab")).toHaveCount(1);
  await run.click();
  await expect(git.locator(".agentmux-dock-height")).toHaveValue("260");
  await expect(git.locator(".agentmux-dock-terminal")).toHaveCSS(
    "height",
    "260px",
  );
});

test("launches an agent in a durable WSL-tmux session", async ({ page }) => {
  await bootPreview(page);
  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("P");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await page.keyboard.type("Claude");
  await page.keyboard.press("Enter");
  await expect(page.getByText("wsl-tmux-control").first()).toBeVisible({
    timeout: 5000,
  });
});

test("command palette opens over a focused terminal", async ({ page }) => {
  await bootPreview(page);
  await page.keyboard.down("Control");
  await page.keyboard.down("Shift");
  await page.keyboard.press("P");
  await page.keyboard.up("Shift");
  await page.keyboard.up("Control");
  await page.keyboard.type("Claude");
  await page.keyboard.press("Enter");
  await page.waitForTimeout(800);
  await page.keyboard.down("Control");
  await page.keyboard.press("K");
  await page.keyboard.up("Control");
  await expect(page.locator(".agentmux-palette-item").first()).toBeVisible();
});
