import { describe, expect, it, vi } from "vitest";
import {
  TERMINAL_WEBGL_ATTACH_BACKOFF_MAX_MS,
  TERMINAL_WEBGL_RENDERER_LABEL_MAX_LENGTH,
  classifyTerminalWebglRenderer,
  decideTerminalWebglPolicy,
  normalizeTerminalWebglPlatform,
  probeTerminalWebglCapability,
  terminalWebglAttachBackoffMs,
  type TerminalWebglPolicyInput,
  type TerminalWebglProbeResult,
} from "./TerminalWebglPolicy";

const HARDWARE_PROBE: TerminalWebglProbeResult = {
  webgl2: true,
  renderer: "ANGLE (NVIDIA GeForce RTX 4070 Direct3D11)",
  rendererKind: "hardware",
};

function decide(
  overrides: Partial<TerminalWebglPolicyInput> = {},
) {
  return decideTerminalWebglPolicy({
    mode: "auto",
    platform: "windows",
    probe: HARDWARE_PROBE,
    recovery: { attachRetryAt: 0, contextLossLatched: false },
    now: 10_000,
    ...overrides,
  });
}

describe("decideTerminalWebglPolicy", () => {
  it("enables auto for Windows WebGL2 unless software rendering is explicit", () => {
    expect(decide()).toEqual({ enable: true, reason: "enabled-auto" });
    expect(decide({ platform: "linux" })).toEqual({
      enable: false,
      reason: "auto-non-windows",
    });
    expect(
      decide({
        platform: "linux",
        probe: { webgl2: false, renderer: null, rendererKind: "unknown" },
      }),
    ).toEqual({ enable: false, reason: "auto-non-windows" });
    expect(
      decide({
        probe: {
          webgl2: true,
          renderer: "Google SwiftShader",
          rendererKind: "software",
        },
      }),
    ).toEqual({ enable: false, reason: "auto-renderer-rejected" });
    expect(
      decide({
        probe: { webgl2: true, renderer: null, rendererKind: "unknown" },
      }),
    ).toEqual({ enable: true, reason: "enabled-auto" });
  });

  it("lets explicit on use any available WebGL2 renderer", () => {
    expect(
      decide({
        mode: "on",
        platform: "linux",
        probe: {
          webgl2: true,
          renderer: "Google SwiftShader",
          rendererKind: "software",
        },
      }),
    ).toEqual({ enable: true, reason: "enabled-on" });
    expect(
      decide({
        mode: "on",
        probe: { webgl2: false, renderer: null, rendererKind: "unknown" },
      }),
    ).toEqual({ enable: false, reason: "webgl2-unavailable" });
  });

  it("honors off, attach backoff, and the context-loss latch", () => {
    expect(decide({ mode: "off" })).toEqual({
      enable: false,
      reason: "mode-off",
    });
    expect(
      decide({ recovery: { attachRetryAt: 10_001, contextLossLatched: false } }),
    ).toEqual({ enable: false, reason: "attach-backoff" });
    expect(
      decide({ recovery: { attachRetryAt: 0, contextLossLatched: true } }),
    ).toEqual({ enable: false, reason: "context-loss-latched" });
  });
});

describe("renderer and platform classification", () => {
  it("rejects known software and generic renderer labels", () => {
    expect(classifyTerminalWebglRenderer("Google SwiftShader")).toBe("software");
    expect(classifyTerminalWebglRenderer("llvmpipe (LLVM 18.1.8)")).toBe(
      "software",
    );
    expect(classifyTerminalWebglRenderer("WebKit WebGL")).toBe("unknown");
    expect(classifyTerminalWebglRenderer(null)).toBe("unknown");
    expect(classifyTerminalWebglRenderer(HARDWARE_PROBE.renderer)).toBe(
      "hardware",
    );
  });

  it("normalizes browser platform values", () => {
    expect(normalizeTerminalWebglPlatform("Win32")).toBe("windows");
    expect(normalizeTerminalWebglPlatform("macOS")).toBe("macos");
    expect(normalizeTerminalWebglPlatform("Linux x86_64")).toBe("linux");
    expect(normalizeTerminalWebglPlatform(undefined)).toBe("other");
  });

  it("caps exponential attach backoff", () => {
    expect(terminalWebglAttachBackoffMs(1)).toBe(1_000);
    expect(terminalWebglAttachBackoffMs(3)).toBe(4_000);
    expect(terminalWebglAttachBackoffMs(99)).toBe(
      TERMINAL_WEBGL_ATTACH_BACKOFF_MAX_MS,
    );
  });
});

describe("probeTerminalWebglCapability", () => {
  it("reads the unmasked renderer, loses the probe context, and zeros the canvas", () => {
    const loseContext = vi.fn();
    const debugInfo = { UNMASKED_RENDERER_WEBGL: 0x9246 };
    const gl = {
      getExtension: vi.fn((name: string) => {
        if (name === "WEBGL_debug_renderer_info") {
          return debugInfo;
        }
        if (name === "WEBGL_lose_context") {
          return { loseContext };
        }
        return null;
      }),
      getParameter: vi.fn(() => `ANGLE (${"N".repeat(220)})`),
    };
    const canvas = {
      width: 64,
      height: 64,
      getContext: vi.fn(() => gl),
    };
    const documentRef = {
      createElement: vi.fn(() => canvas),
    } as unknown as Pick<Document, "createElement">;

    const result = probeTerminalWebglCapability(documentRef);

    expect(result.webgl2).toBe(true);
    expect(result.rendererKind).toBe("hardware");
    expect(result.renderer).toHaveLength(
      TERMINAL_WEBGL_RENDERER_LABEL_MAX_LENGTH,
    );
    expect(loseContext).toHaveBeenCalledOnce();
    expect(canvas.width).toBe(0);
    expect(canvas.height).toBe(0);
  });

  it("fails closed when context creation throws", () => {
    const canvas = {
      width: 64,
      height: 64,
      getContext: vi.fn(() => {
        throw new Error("driver failure");
      }),
    };
    const documentRef = {
      createElement: vi.fn(() => canvas),
    } as unknown as Pick<Document, "createElement">;

    expect(probeTerminalWebglCapability(documentRef)).toEqual({
      webgl2: false,
      renderer: null,
      rendererKind: "unknown",
    });
    expect(canvas.width).toBe(0);
    expect(canvas.height).toBe(0);
  });
});
