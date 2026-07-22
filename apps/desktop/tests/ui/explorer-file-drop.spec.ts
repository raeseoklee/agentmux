import { expect, test } from "@playwright/test";

test("Explorer file drop inserts a quoted path into the targeted WSL terminal", async ({
  page,
}) => {
  await page.addInitScript(() => {
    (
      window as Window & {
        __AGENTMUX_PREVIEW_SEED_WORKSPACE__?: boolean;
        __AGENTMUX_FILE_DROP_HANDLER__?: (event: {
          payload: {
            paths: string[];
            targetPaneId: string | null;
            targetSessionId: string | null;
          };
        }) => void;
        __TAURI__?: {
          event: {
            listen: (
              _event: string,
              handler: (event: {
                payload: {
                  paths: string[];
                  targetPaneId: string | null;
                  targetSessionId: string | null;
                };
              }) => void,
            ) => Promise<() => void>;
          };
        };
      }
    ).__AGENTMUX_PREVIEW_SEED_WORKSPACE__ = true;
    (
      window as Window & {
        __TAURI__?: {
          event: {
            listen: (
              _event: string,
              handler: (event: {
                payload: {
                  paths: string[];
                  targetPaneId: string | null;
                  targetSessionId: string | null;
                };
              }) => void,
            ) => Promise<() => void>;
          };
        };
        __AGENTMUX_FILE_DROP_HANDLER__?: (event: {
          payload: {
            paths: string[];
            targetPaneId: string | null;
            targetSessionId: string | null;
          };
        }) => void;
      }
    ).__TAURI__ = {
      event: {
        listen: async (eventName, handler) => {
          if (eventName === "agentmux://explorer-file-drop") {
            (
              window as Window & {
                __AGENTMUX_FILE_DROP_HANDLER__?: typeof handler;
              }
            ).__AGENTMUX_FILE_DROP_HANDLER__ = handler;
          }
          return () => undefined;
        },
      },
    };
  });

  await page.goto("/");
  await page.waitForFunction(
    () =>
      (
        window as Window & { __AGENTMUX_PREVIEW_READY__?: boolean }
      ).__AGENTMUX_PREVIEW_READY__ === true,
  );
  await page.locator(".agentmux-new-terminal-tab").click();

  const outsideDropPrevented = await page.evaluate(() => {
    const transfer = new DataTransfer();
    transfer.items.add(new File(["hello"], "do-not-navigate.txt"));
    const event = new DragEvent("drop", {
      bubbles: true,
      cancelable: true,
      dataTransfer: transfer,
    });
    document.body.dispatchEvent(event);
    return event.defaultPrevented;
  });
  expect(outsideDropPrevented).toBe(true);

  const pane = page
    .locator('[data-agentmux-pane][data-agentmux-terminal-session]')
    .first();
  await expect(pane).toBeVisible();
  const target = await pane.evaluate((element) => ({
    paneId: element.getAttribute("data-agentmux-pane"),
    sessionId: element.getAttribute("data-agentmux-terminal-session"),
  }));

  await pane.evaluate((element) => {
    const transfer = new DataTransfer();
    transfer.items.add(new File(["hello"], "release notes.md"));
    element.dispatchEvent(
      new DragEvent("dragover", {
        bubbles: true,
        cancelable: true,
        dataTransfer: transfer,
      }),
    );
  });
  await expect(pane).toHaveAttribute("data-agentmux-file-drag-over", "true");

  await page.evaluate(
    ({ paneId, sessionId }) => {
      (
        window as Window & {
          __AGENTMUX_FILE_DROP_HANDLER__?: (event: {
            payload: {
              paths: string[];
              targetPaneId: string | null;
              targetSessionId: string | null;
            };
          }) => void;
        }
      ).__AGENTMUX_FILE_DROP_HANDLER__?.({
        payload: {
          paths: [String.raw`D:\Agent Mux\release notes.md`],
          targetPaneId: paneId,
          targetSessionId: sessionId,
        },
      });
    },
    target,
  );

  await expect
    .poll(() =>
      page.evaluate((sessionId) =>
        (
          window as Window & {
            __AGENTMUX_PREVIEW__?: {
              terminalOutput: (targetSessionId?: string) => string;
            };
          }
        ).__AGENTMUX_PREVIEW__?.terminalOutput(sessionId ?? undefined),
        target.sessionId,
      ),
    )
    .toContain("'/mnt/d/Agent Mux/release notes.md' ");
});

test("clipboard file and image attachments insert materialized paths", async ({ page }) => {
  await page.addInitScript(() => {
    (
      window as Window & { __AGENTMUX_PREVIEW_SEED_WORKSPACE__?: boolean }
    ).__AGENTMUX_PREVIEW_SEED_WORKSPACE__ = true;
  });
  await page.goto("/");
  await page.waitForFunction(
    () =>
      (window as Window & { __AGENTMUX_PREVIEW_READY__?: boolean })
        .__AGENTMUX_PREVIEW_READY__ === true,
  );
  await page.locator(".agentmux-new-terminal-tab").click();
  await expect(page.locator(".xterm").first()).toBeVisible();

  await page.evaluate(() => {
    const target = window as Window & {
      __TAURI__?: Record<string, unknown>;
    };
    target.__TAURI__ = {
      ...(target.__TAURI__ ?? {}),
      core: {
        invoke: async (command: string) => {
          if (command !== "clipboard_materialize_attachments") {
            throw new Error(`unexpected command: ${command}`);
          }
          return {
            kind: "image",
            paths: [
              String.raw`C:\Users\Roy\AppData\Local\AgentMux\clipboard\shot 1.png`,
            ],
          };
        },
      },
    };
  });

  await page.locator(".xterm").first().click();
  await page.keyboard.press("Control+V");

  await expect
    .poll(() =>
      page.evaluate(() =>
        (
          window as Window & {
            __AGENTMUX_PREVIEW__?: { terminalOutput: () => string };
          }
        ).__AGENTMUX_PREVIEW__?.terminalOutput(),
      ),
    )
    .toContain(
      "'/mnt/c/Users/Roy/AppData/Local/AgentMux/clipboard/shot 1.png' ",
    );
});
