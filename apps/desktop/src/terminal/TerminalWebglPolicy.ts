export type TerminalWebglMode = "auto" | "on" | "off";

export type TerminalWebglPlatform =
  | "windows"
  | "macos"
  | "linux"
  | "other";

export type TerminalWebglRendererKind = "hardware" | "software" | "unknown";

export interface TerminalWebglProbeResult {
  webgl2: boolean;
  renderer: string | null;
  rendererKind: TerminalWebglRendererKind;
}

export interface TerminalWebglRecoveryState {
  attachRetryAt: number;
  contextLossLatched: boolean;
}

export type TerminalWebglPolicyReason =
  | "enabled-auto"
  | "enabled-on"
  | "mode-off"
  | "webgl2-unavailable"
  | "auto-non-windows"
  | "auto-renderer-rejected"
  | "attach-backoff"
  | "context-loss-latched";

export interface TerminalWebglPolicyDecision {
  enable: boolean;
  reason: TerminalWebglPolicyReason;
}

export interface TerminalWebglPolicyInput {
  mode: TerminalWebglMode;
  platform: TerminalWebglPlatform;
  probe: TerminalWebglProbeResult;
  recovery: TerminalWebglRecoveryState;
  now: number;
}

export type TerminalWebglPreflightInput = Omit<
  TerminalWebglPolicyInput,
  "probe"
>;

export const TERMINAL_WEBGL_ATTACH_BACKOFF_BASE_MS = 1_000;
export const TERMINAL_WEBGL_ATTACH_BACKOFF_MAX_MS = 30_000;
export const TERMINAL_WEBGL_MAX_ATTACH_FAILURES = 8;
export const TERMINAL_WEBGL_RENDERER_LABEL_MAX_LENGTH = 160;

const SOFTWARE_RENDERER_PATTERN =
  /swiftshader|llvmpipe|softpipe|lavapipe|software rasterizer|software renderer|microsoft basic render driver|\bwarp\b/i;
const GENERIC_RENDERER_PATTERN =
  /^(?:angle|google inc\.|microsoft corporation|mozilla|unknown|webkit webgl|webgl|webgl 2\.0)$/i;

interface WebglDebugRendererInfo {
  UNMASKED_RENDERER_WEBGL: number;
}

interface WebglLoseContext {
  loseContext(): void;
}

export function classifyTerminalWebglRenderer(
  renderer: string | null | undefined,
): TerminalWebglRendererKind {
  const normalized = renderer?.trim();
  if (!normalized || GENERIC_RENDERER_PATTERN.test(normalized)) {
    return "unknown";
  }
  return SOFTWARE_RENDERER_PATTERN.test(normalized) ? "software" : "hardware";
}

export function normalizeTerminalWebglPlatform(
  platform: string | null | undefined,
): TerminalWebglPlatform {
  const normalized = platform?.trim().toLowerCase() ?? "";
  if (normalized.includes("win")) {
    return "windows";
  }
  if (normalized.includes("mac")) {
    return "macos";
  }
  if (normalized.includes("linux") || normalized.includes("x11")) {
    return "linux";
  }
  return "other";
}

export function detectTerminalWebglPlatform(
  navigatorRef: Navigator | undefined = globalThis.navigator,
): TerminalWebglPlatform {
  const navigatorWithUserAgentData = navigatorRef as
    | (Navigator & { userAgentData?: { platform?: string } })
    | undefined;
  return normalizeTerminalWebglPlatform(
    navigatorWithUserAgentData?.userAgentData?.platform ||
      navigatorRef?.platform ||
      navigatorRef?.userAgent,
  );
}

export function terminalWebglAttachBackoffMs(failureCount: number): number {
  const boundedCount = Math.min(
    TERMINAL_WEBGL_MAX_ATTACH_FAILURES,
    Math.max(1, Math.floor(failureCount)),
  );
  return Math.min(
    TERMINAL_WEBGL_ATTACH_BACKOFF_MAX_MS,
    TERMINAL_WEBGL_ATTACH_BACKOFF_BASE_MS * 2 ** (boundedCount - 1),
  );
}

export function decideTerminalWebglPolicy(
  input: TerminalWebglPolicyInput,
): TerminalWebglPolicyDecision {
  const preflightDecision = decideTerminalWebglPreflight(input);
  if (preflightDecision) {
    return preflightDecision;
  }
  if (!input.probe.webgl2) {
    return { enable: false, reason: "webgl2-unavailable" };
  }
  if (input.mode === "auto" && input.probe.rendererKind !== "hardware") {
    return { enable: false, reason: "auto-renderer-rejected" };
  }
  return {
    enable: true,
    reason: input.mode === "auto" ? "enabled-auto" : "enabled-on",
  };
}

export function decideTerminalWebglPreflight(
  input: TerminalWebglPreflightInput,
): TerminalWebglPolicyDecision | null {
  if (input.mode === "off") {
    return { enable: false, reason: "mode-off" };
  }
  if (input.recovery.contextLossLatched) {
    return { enable: false, reason: "context-loss-latched" };
  }
  if (input.now < input.recovery.attachRetryAt) {
    return { enable: false, reason: "attach-backoff" };
  }
  if (input.mode === "auto" && input.platform !== "windows") {
    return { enable: false, reason: "auto-non-windows" };
  }
  return null;
}

export function probeTerminalWebglCapability(
  documentRef: Pick<Document, "createElement"> | undefined = globalThis.document,
): TerminalWebglProbeResult {
  if (!documentRef) {
    return { webgl2: false, renderer: null, rendererKind: "unknown" };
  }

  let canvas: HTMLCanvasElement | undefined;
  let gl: WebGL2RenderingContext | null = null;
  try {
    canvas = documentRef.createElement("canvas");
    gl = canvas.getContext("webgl2", {
      antialias: false,
      depth: false,
      failIfMajorPerformanceCaveat: false,
      powerPreference: "high-performance",
      preserveDrawingBuffer: false,
      stencil: false,
    });
    if (!gl) {
      return { webgl2: false, renderer: null, rendererKind: "unknown" };
    }

    let renderer: string | null = null;
    try {
      const debugInfo = gl.getExtension(
        "WEBGL_debug_renderer_info",
      ) as WebglDebugRendererInfo | null;
      const value = debugInfo
        ? gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL)
        : null;
      renderer = normalizeRendererLabel(value);
    } catch {
      // Privacy settings and hardened drivers may deny renderer details.
    }
    return {
      webgl2: true,
      renderer,
      rendererKind: classifyTerminalWebglRenderer(renderer),
    };
  } catch {
    return { webgl2: false, renderer: null, rendererKind: "unknown" };
  } finally {
    if (gl) {
      try {
        const loseContext = gl.getExtension(
          "WEBGL_lose_context",
        ) as WebglLoseContext | null;
        loseContext?.loseContext();
      } catch {
        // Releasing a best-effort probe must never break terminal startup.
      }
    }
    if (canvas) {
      canvas.width = 0;
      canvas.height = 0;
    }
  }
}

function normalizeRendererLabel(value: unknown): string | null {
  if (typeof value !== "string") {
    return null;
  }
  const normalized = value
    .replace(/[\u0000-\u001f\u007f]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  return normalized
    ? normalized.slice(0, TERMINAL_WEBGL_RENDERER_LABEL_MAX_LENGTH)
    : null;
}
