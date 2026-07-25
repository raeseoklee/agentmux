import { afterEach, describe, expect, it, vi } from "vitest";
import {
  hideNativeBrowser,
  measureNativeBrowserBounds,
  mountNativeBrowser,
  supportsNativeBrowser,
  updateNativeBrowserBounds,
} from "./nativeBrowser";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("native browser bridge", () => {
  it("only enables the native path when the Tauri invoke bridge is present", () => {
    vi.stubGlobal("window", {});
    expect(supportsNativeBrowser()).toBe(false);

    vi.stubGlobal("window", {
      __TAURI__: { core: { invoke: vi.fn() } },
    });
    expect(supportsNativeBrowser()).toBe(true);
  });

  it("converts a DOM viewport to positive logical child-webview bounds", () => {
    const element = {
      getBoundingClientRect: () => ({
        left: -4.4,
        top: 18.6,
        width: 801.2,
        height: 599.7,
      }),
    } as HTMLElement;

    expect(measureNativeBrowserBounds(element)).toEqual({
      x: 0,
      y: 19,
      width: 801,
      height: 600,
    });
  });

  it("preserves snake-case Tauri command arguments across the IPC boundary", async () => {
    const invoke = vi.fn(async () => ({ url: "https://example.com/" }));
    vi.stubGlobal("window", { __TAURI__: { core: { invoke } } });

    await expect(
      mountNativeBrowser(
        "surface-1",
        "https://example.com",
        { x: 10, y: 20, width: 900, height: 640 },
        true,
      ),
    ).resolves.toEqual({ url: "https://example.com/" });
    expect(invoke).toHaveBeenCalledWith("native_browser_mount", {
      surface_id: "surface-1",
      url: "https://example.com",
      bounds: { x: 10, y: 20, width: 900, height: 640 },
      visible: true,
      revision: 1,
    });
  });

  it("orders resize and hide requests with monotonically increasing revisions", async () => {
    const invoke = vi.fn(async () => undefined);
    vi.stubGlobal("window", { __TAURI__: { core: { invoke } } });
    const bounds = { x: 25, y: 40, width: 640, height: 480 };

    await updateNativeBrowserBounds("surface-layout", bounds, true);
    await hideNativeBrowser("surface-layout");

    expect(invoke).toHaveBeenNthCalledWith(1, "native_browser_update_bounds", {
      surface_id: "surface-layout",
      bounds,
      visible: true,
      revision: 1,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "native_browser_hide", {
      surface_id: "surface-layout",
      revision: 2,
    });
  });
});
