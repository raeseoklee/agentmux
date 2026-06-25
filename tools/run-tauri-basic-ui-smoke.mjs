#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import playwright from "../apps/desktop/node_modules/playwright/index.js";

const { chromium } = playwright;

const root = path.resolve(import.meta.dirname, "..");
const args = new Map();
for (let index = 2; index < process.argv.length; index += 1) {
  const arg = process.argv[index];
  if (!arg.startsWith("--")) {
    continue;
  }
  const key = arg.slice(2);
  const value = process.argv[index + 1]?.startsWith("--")
    ? "true"
    : (process.argv[index + 1] ?? "true");
  args.set(key, value);
  if (value !== "true") {
    index += 1;
  }
}

const cdp = args.get("cdp") ?? "http://127.0.0.1:9223";
const outputPath =
  args.get("output") ?? path.join(root, ".codexus", "basic-ui-smoke.json");
const screenshotPath =
  args.get("screenshot") ??
  path.join(root, ".codexus", "basic-ui-smoke.png");

const startedAt = new Date().toISOString();
const stepResults = [];
const consoleMessages = [];

let requestCounter = 0;
let smokeWorkspaceId = null;
let smokeWorkspaceName = null;

function pushStep(name, data = {}) {
  stepResults.push({
    name,
    at: new Date().toISOString(),
    ...data,
  });
}

async function waitFor(predicate, description, timeoutMs = 15000) {
  const started = Date.now();
  let lastError = null;
  while (Date.now() - started < timeoutMs) {
    try {
      const value = await predicate();
      if (value) {
        return value;
      }
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  const suffix = lastError ? ` Last error: ${lastError.message}` : "";
  throw new Error(`Timed out waiting for ${description}.${suffix}`);
}

async function count(page, selector) {
  return page.locator(selector).count();
}

async function waitForCount(page, selector, expected, timeoutMs = 15000) {
  await waitFor(
    async () => (await count(page, selector)) === expected,
    `${selector} to have count ${expected}`,
    timeoutMs,
  );
}

async function callControl(page, method, params = {}) {
  return page.evaluate(
    async ({ method: controlMethod, params: controlParams, requestId }) => {
      const invoke = window.__TAURI__?.core?.invoke;
      if (!invoke) {
        throw new Error("Tauri invoke bridge is not available.");
      }
      const token = await invoke("agentmux_control_token");
      const response = await invoke("agentmux_control", {
        request: {
          schema: "agentmux.control.v1",
          id: requestId,
          method: controlMethod,
          params_json: JSON.stringify(controlParams),
          auth: { token },
        },
      });
      if ("Error" in response.outcome) {
        throw new Error(response.outcome.Error.message);
      }
      return JSON.parse(response.outcome.Ok.result_json);
    },
    {
      method,
      params,
      requestId: `basic_smoke_${++requestCounter}`,
    },
  );
}

async function waitForAppReady(page) {
  await page.waitForLoadState("domcontentloaded");
  await page.locator("[data-agentmux-root]").waitFor({ timeout: 15000 });
  const filter = page.locator(".agentmux-workspace-filter-input");
  if (await filter.isVisible().catch(() => false)) {
    await filter.fill("");
  }
}

function workspaceCard(page, workspaceId, workspaceName) {
  return page
    .locator(`.agentmux-workspace-card[data-agentmux-workspace="${workspaceId}"]`)
    .filter({ hasText: workspaceName });
}

async function closeSmokeWorkspace(page, policy = "terminate_sessions") {
  if (!smokeWorkspaceId) {
    return;
  }
  try {
    const list = await callControl(page, "workspace.list", {});
    const exists = list.workspaces.some(
      (workspace) => workspace.workspace_id === smokeWorkspaceId,
    );
    if (exists) {
      await callControl(page, "workspace.close", {
        workspace_id: smokeWorkspaceId,
        close_policy: policy,
      });
      await page.reload({ waitUntil: "domcontentloaded" }).catch(() => {});
    }
  } catch (error) {
    consoleMessages.push({
      type: "cleanup",
      text: error instanceof Error ? error.message : String(error),
    });
  } finally {
    smokeWorkspaceId = null;
  }
}

async function waitForNotification(page, predicate, description, timeoutMs = 20000) {
  return waitFor(
    async () => {
      const result = await callControl(page, "notification.list", {
        workspace_id: smokeWorkspaceId,
        severity: null,
        include_dismissed: false,
      });
      return result.notifications.find(predicate);
    },
    description,
    timeoutMs,
  );
}

async function waitForSessionOutput(page, sessionId, predicate, description, timeoutMs = 10000) {
  return waitFor(
    async () => {
      const result = await callControl(page, "session.read_recent", {
        session_id: sessionId,
        max_bytes: 4096,
      });
      return predicate(result.text) ? result.text : null;
    },
    description,
    timeoutMs,
  );
}

async function run() {
  const browser = await chromium.connectOverCDP(cdp);
  let page;
  try {
    const context = browser.contexts()[0];
    page =
      context.pages().find((candidate) =>
        candidate.url().includes("127.0.0.1:5173"),
      ) ?? context.pages()[0];

    page.on("console", (message) => {
      consoleMessages.push({ type: message.type(), text: message.text() });
    });
    page.on("pageerror", (error) => {
      consoleMessages.push({ type: "pageerror", text: String(error) });
    });

    await page.reload({ waitUntil: "domcontentloaded" });
    await waitForAppReady(page);

    const initialWorkspaceList = await callControl(page, "workspace.list", {});
    smokeWorkspaceName = `AgentMux Basic Smoke ${Date.now()}`;
    const created = await callControl(page, "workspace.create", {
      name: smokeWorkspaceName,
      project_root: null,
      backend_profile: null,
    });
    smokeWorkspaceId = created.workspace_id;

    await page.reload({ waitUntil: "domcontentloaded" });
    await waitForAppReady(page);

    const smokeCard = workspaceCard(page, smokeWorkspaceId, smokeWorkspaceName);
    await smokeCard.first().waitFor({ timeout: 15000 });
    await smokeCard.first().click();
    await waitFor(
      async () =>
        (await smokeCard.first().getAttribute("data-agentmux-active")) ===
        "true",
      "smoke workspace to become active",
      15000,
    );
    await waitForCount(page, "[data-agentmux-pane]", 1, 15000);
    pushStep("workspace open", {
      initialWorkspaceCount: initialWorkspaceList.workspaces.length,
      workspaceId: smokeWorkspaceId,
      workspaceName: smokeWorkspaceName,
    });

    await waitFor(
      async () => (await count(page, ".agentmux-surface-tab")) > 0,
      "initial workspace terminal tab to render",
      15000,
    );
    const initialTabCount = await count(page, ".agentmux-surface-tab");
    const initialPaneCount = await count(page, "[data-agentmux-pane]");
    const tabSpawn = await callControl(page, "session.spawn", {
      workspace_id: smokeWorkspaceId,
      backend: "conpty",
      command: ["cmd.exe", "/d", "/q"],
      cwd: null,
      columns: 80,
      rows: 24,
      durability: "ephemeral",
      placement: "new_tab",
    });
    await waitForCount(
      page,
      ".agentmux-surface-tab",
      initialTabCount + 1,
      45000,
    );
    await page.locator(".agentmux-surface-tab-close").last().click();
    await waitForCount(page, ".agentmux-surface-tab", initialTabCount, 25000);
    await waitForCount(page, "[data-agentmux-pane]", initialPaneCount, 15000);
    pushStep("tab open/close", {
      spawnedSessionId: tabSpawn.session_id,
      initialTabCount,
      finalTabCount: await count(page, ".agentmux-surface-tab"),
      paneCountAfterTabClose: await count(page, "[data-agentmux-pane]"),
    });

    const splitInitialTabCount = await count(page, ".agentmux-surface-tab");
    const splitInitialPaneCount = await count(page, "[data-agentmux-pane]");
    await page.locator(".agentmux-pane-split-vertical").first().click();
    await waitForCount(
      page,
      "[data-agentmux-pane]",
      splitInitialPaneCount + 1,
      15000,
    );
    await waitForCount(page, ".agentmux-surface-tab", splitInitialTabCount);
    await page.locator(".agentmux-pane-close").last().click();
    await waitForCount(page, "[data-agentmux-pane]", splitInitialPaneCount);
    await waitForCount(page, ".agentmux-surface-tab", splitInitialTabCount);
    pushStep("split pane open/close", {
      initialPaneCount: splitInitialPaneCount,
      finalPaneCount: await count(page, "[data-agentmux-pane]"),
      tabCountAfterSplitClose: await count(page, ".agentmux-surface-tab"),
    });

    const agentCommand = [
      "cmd.exe",
      "/d",
      "/q",
    ];
    const agentSpawn = await callControl(page, "session.spawn", {
      workspace_id: smokeWorkspaceId,
      backend: "conpty",
      command: agentCommand,
      cwd: null,
      columns: 80,
      rows: 24,
      durability: "ephemeral",
    });
    await callControl(page, "agent.set_state", {
      session_id: agentSpawn.session_id,
      state: "running",
      reason: "Agent started: tauri-ui-e2e",
      telemetry: {
        activity: "agent",
        session: "tauri-ui-e2e",
      },
    });
    await waitForSessionOutput(
      page,
      agentSpawn.session_id,
      (text) => text.includes(">"),
      "agent command prompt before sending exit",
    );
    await callControl(page, "session.send_text", {
      session_id: agentSpawn.session_id,
      text: "exit",
    });
    await callControl(page, "session.send_key", {
      session_id: agentSpawn.session_id,
      key: "enter",
    });
    const completedNotification = await waitForNotification(
      page,
      (notification) =>
        notification.notification_type === "agent.completed" &&
        notification.session_id === agentSpawn.session_id,
      "agent.completed notification for Tauri UI smoke agent",
      30000,
    );
    await page.locator(".agentmux-settings-open").click();
    await page.locator(".agentmux-settings-tab-general").click();
    await page
      .locator(
        `[data-agentmux-notification="${completedNotification.notification_id}"]`,
      )
      .waitFor({ timeout: 15000 });
    await page
      .locator(
        `[data-agentmux-notification="${completedNotification.notification_id}"]`,
      )
      .filter({ hasText: "Agent completed" })
      .waitFor({ timeout: 15000 });
    await page.locator(".agentmux-settings-close").click();
    await page
      .locator(".agentmux-settings-tab-general")
      .waitFor({ state: "detached", timeout: 15000 });
    pushStep("agent completion notification", {
      sessionId: agentSpawn.session_id,
      notificationId: completedNotification.notification_id,
      notificationType: completedNotification.notification_type,
      message: completedNotification.message,
    });

    const workspaceCountBeforeClose = await count(
      page,
      ".agentmux-workspace-card",
    );
    page.once("dialog", async (dialog) => {
      await dialog.accept();
    });
    await smokeCard.first().click({ button: "right" });
    await page.locator(".agentmux-workspace-menu-close").click();
    const confirmModal = page.locator(".agentmux-confirm-modal");
    if (
      await confirmModal
        .waitFor({ state: "visible", timeout: 1500 })
        .then(() => true)
        .catch(() => false)
    ) {
      await page.locator(".agentmux-confirm-confirm").click();
    }
    await waitFor(
      async () => (await smokeCard.count()) === 0,
      "smoke workspace to close from UI",
      15000,
    );
    smokeWorkspaceId = null;
    pushStep("workspace close", {
      workspaceCountBeforeClose,
      finalWorkspaceCount: await count(page, ".agentmux-workspace-card"),
    });

    await page.screenshot({ path: screenshotPath, fullPage: false });

    const blockingMessages = consoleMessages.filter(
      (message) => message.type === "error" || message.type === "pageerror",
    );
    if (blockingMessages.length > 0) {
      throw new Error(
        `Browser console/page errors observed: ${JSON.stringify(blockingMessages)}`,
      );
    }

    const result = {
      ok: true,
      startedAt,
      completedAt: new Date().toISOString(),
      cdp,
      steps: stepResults,
      consoleMessages,
      screenshotPath,
    };
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`);
    console.log(JSON.stringify(result, null, 2));
  } catch (error) {
    if (page) {
      await closeSmokeWorkspace(page);
    }
    throw error;
  } finally {
    await browser.close();
  }
}

run().catch((error) => {
  const result = {
    ok: false,
    startedAt,
    completedAt: new Date().toISOString(),
    cdp,
    steps: stepResults,
    consoleMessages,
    error: error instanceof Error ? error.message : String(error),
  };
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`);
  console.error(JSON.stringify(result, null, 2));
  process.exit(1);
});
