import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import type {
  ControlClient,
  OutputPressureReport,
  OutputSnapshot,
} from "../control/ControlClient";
import {
  XtermTerminalRenderer,
  XTERM_THEME,
} from "../terminal/XtermTerminalRenderer";
import { TerminalInputScheduler } from "../terminal/TerminalInputScheduler";
import { observeDevicePixelRatio } from "../terminal/DevicePixelRatioObserver";
import { TerminalResizeCoordinator } from "../terminal/TerminalResizeCoordinator";
import { terminalViewStateCache } from "../terminal/TerminalViewStateCache";
import type {
  TerminalWebglMode as TerminalGpuAccelerationMode,
} from "../terminal/TerminalWebglPolicy";
import {
  discardTerminalOutput,
  getTerminalOutputStats,
  resetTerminalOutput,
  setTerminalOutputForeground,
  writeTerminalOutput,
} from "../terminal/TerminalOutputScheduler";

// ---------------------------------------------------------------------------
// Command registry (TS-9 / TS-4 / TS-6 wave-2 app integration)
// ---------------------------------------------------------------------------

/** Public imperative commands exposed per live terminal session. */
export interface TerminalCommands {
  clearBuffer(): void;
  selectAll(): void;
  findNext(term: string): boolean;
  findPrevious(term: string): boolean;
  scrollToBottom(): void;
  pasteText(text: string): void;
}

const _terminalCommandRegistry = new Map<string, TerminalCommands>();

/**
 * Return the TerminalCommands for the given sessionId, or null when no
 * LiveTerminal for that session is currently mounted.
 */
export function terminalCommandsForSession(sessionId: string): TerminalCommands | null {
  return _terminalCommandRegistry.get(sessionId) ?? null;
}

// ---------------------------------------------------------------------------
// Broadcast resolver (TS-14)
// ---------------------------------------------------------------------------

/**
 * When set, called with the typing session's ID. Return an array of peer
 * session IDs that should also receive the same input, or null/empty to
 * disable broadcast for that session.
 */
type BroadcastResolver = (sessionId: string) => string[] | null;

let _broadcastResolver: BroadcastResolver | null = null;

/**
 * Register (or clear) the broadcast resolver. AgentmuxTerminalApp calls this
 * once (useEffect) and updates it via a stable ref so closures stay fresh.
 */
export function setBroadcastResolver(fn: BroadcastResolver | null): void {
  _broadcastResolver = fn;
}

const encoder = new TextEncoder();
const SNAPSHOT_HOT_POLL_MS = 32;
const SNAPSHOT_BOOT_POLL_MS = 80;
const SNAPSHOT_IDLE_POLL_MS = 250;
const SNAPSHOT_INACTIVE_POLL_MS = 500;
const SNAPSHOT_HIDDEN_POLL_MS = 1000;
const FALLBACK_HOT_POLL_MS = 80;
const FALLBACK_IDLE_POLL_MS = 350;
const FALLBACK_INACTIVE_POLL_MS = 700;
const ACTIVITY_HOT_POLLS = 12;
const MAX_PENDING_STREAM_FRAMES = 256;
const MAX_PENDING_STREAM_BYTES = 1024 * 1024;
const TRANSPORT_DIAGNOSTIC_FLUSH_MS = 250;
// Retain a recently visible terminal's GPU context through ordinary tab
// switching. This avoids tearing down and rebuilding the renderer while the
// user compares adjacent tabs, while still releasing inactive contexts.
const WEBGL_DISABLE_DEBOUNCE_MS = 1500;
const TERMINAL_RESIZE_SETTLE_MS = 160;
const TERMINAL_LAYOUT_SETTLE_MS = 90;
const TERMINAL_LINE_HEIGHT = 1.0;
const PREVIEW_CACHE_ENABLED_KEY = "agentmux.terminal.previewCache";
const PREVIEW_CACHE_PREFIX = "agentmux.terminal.preview.v1.";
const PREVIEW_CACHE_MAX_BYTES = 64 * 1024;
const PREVIEW_CACHE_FLUSH_MS = 350;
const PREVIEW_CACHE_MAX_AGE_MS = 24 * 60 * 60 * 1000;

interface TerminalPreviewCacheEntry {
  version: 1;
  sessionId: string;
  bytesBase64: string;
  byteCount: number;
  updatedAt: number;
}

function terminalPreviewCacheEnabled(): boolean {
  try {
    return window.localStorage?.getItem(PREVIEW_CACHE_ENABLED_KEY) === "1";
  } catch {
    return false;
  }
}

interface OutputStreamFrame {
  fromOffset: number;
  bytes: Uint8Array;
}

type OutputTransportMode =
  | "tauri-channel"
  | "websocket"
  | "snapshot-poll"
  | "read-recent-poll";

interface TerminalTransportDiagnostics {
  mode: OutputTransportMode;
  sessionId: string;
  frames: number;
  bytes: number;
  resyncs: number;
  queuedBytes: number;
  maxQueuedBytes: number;
  backpressureEvents: number;
  writeInFlight: boolean;
  updatedAt: string;
}

interface TerminalWebglDiagnostics {
  sessionId: string;
  mode: TerminalGpuAccelerationMode;
  focused: boolean;
  visible: boolean;
  requested: boolean;
  renderer: ReturnType<XtermTerminalRenderer["getWebglDiagnostics"]>;
  updatedAt: string;
}

function terminalDiagnostics() {
  return window as Window & {
    __AGENTMUX_TERMINAL_TRANSPORT__?: Record<string, TerminalTransportDiagnostics>;
    __AGENTMUX_TERMINAL_WEBGL__?: Record<string, TerminalWebglDiagnostics>;
  };
}

function recordWebgl(
  sessionId: string,
  mode: TerminalGpuAccelerationMode,
  focused: boolean,
  visible: boolean,
  requested: boolean,
  renderer: XtermTerminalRenderer,
) {
  const target = terminalDiagnostics();
  const registry = target.__AGENTMUX_TERMINAL_WEBGL__ ?? {};
  registry[sessionId] = {
    sessionId,
    mode,
    focused,
    visible,
    requested,
    renderer: renderer.getWebglDiagnostics(),
    updatedAt: new Date().toISOString(),
  };
  target.__AGENTMUX_TERMINAL_WEBGL__ = registry;
}

function removeWebglDiagnostics(sessionId: string) {
  const target = terminalDiagnostics();
  const registry = target.__AGENTMUX_TERMINAL_WEBGL__;
  if (!registry) {
    return;
  }
  delete registry[sessionId];
}

function recordTransport(
  sessionId: string,
  mode: OutputTransportMode,
  patch?: Partial<Omit<TerminalTransportDiagnostics, "mode" | "sessionId" | "updatedAt">>,
) {
  const target = terminalDiagnostics();
  const registry = target.__AGENTMUX_TERMINAL_TRANSPORT__ ?? {};
  const previous = registry[sessionId] ?? {
    mode,
    sessionId,
    frames: 0,
    bytes: 0,
    resyncs: 0,
    queuedBytes: 0,
    maxQueuedBytes: 0,
    backpressureEvents: 0,
    writeInFlight: false,
    updatedAt: new Date().toISOString(),
  };
  registry[sessionId] = {
    ...previous,
    mode,
    sessionId,
    ...patch,
    updatedAt: new Date().toISOString(),
  };
  target.__AGENTMUX_TERMINAL_TRANSPORT__ = registry;
}

function documentHidden(): boolean {
  return typeof document !== "undefined" && document.visibilityState === "hidden";
}

function previewCacheKey(sessionId: string): string {
  return `${PREVIEW_CACHE_PREFIX}${sessionId}`;
}

function trimPreviewBytes(bytes: Uint8Array): Uint8Array {
  if (bytes.length <= PREVIEW_CACHE_MAX_BYTES) {
    return bytes;
  }
  return bytes.subarray(bytes.length - PREVIEW_CACHE_MAX_BYTES);
}

function concatPreviewBytes(left: Uint8Array, right: Uint8Array): Uint8Array {
  if (left.length === 0) {
    return trimPreviewBytes(right);
  }
  if (right.length >= PREVIEW_CACHE_MAX_BYTES) {
    return trimPreviewBytes(right);
  }
  const total = Math.min(PREVIEW_CACHE_MAX_BYTES, left.length + right.length);
  const merged = new Uint8Array(total);
  const leftTake = Math.min(left.length, total - right.length);
  if (leftTake > 0) {
    merged.set(left.subarray(left.length - leftTake), 0);
  }
  merged.set(right, leftTake);
  return merged;
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunkSize = 0x8000;
  for (let index = 0; index < bytes.length; index += chunkSize) {
    const chunk = bytes.subarray(index, index + chunkSize);
    for (let i = 0; i < chunk.length; i++) {
      binary += String.fromCharCode(chunk[i]);
    }
  }
  return window.btoa(binary);
}

function base64ToBytes(base64: string): Uint8Array {
  const binary = window.atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

export function readTerminalPreviewCache(sessionId: string): Uint8Array | null {
  if (!terminalPreviewCacheEnabled()) {
    return null;
  }

  try {
    const raw = window.localStorage?.getItem(previewCacheKey(sessionId));
    if (!raw) {
      return null;
    }
    const entry = JSON.parse(raw) as Partial<TerminalPreviewCacheEntry>;
    if (
      entry.version !== 1 ||
      entry.sessionId !== sessionId ||
      typeof entry.bytesBase64 !== "string" ||
      typeof entry.updatedAt !== "number" ||
      Date.now() - entry.updatedAt > PREVIEW_CACHE_MAX_AGE_MS
    ) {
      window.localStorage?.removeItem(previewCacheKey(sessionId));
      return null;
    }
    const bytes = trimPreviewBytes(base64ToBytes(entry.bytesBase64));
    return bytes.length > 0 ? bytes : null;
  } catch {
    return null;
  }
}

function writeTerminalPreviewCache(sessionId: string, bytes: Uint8Array): void {
  if (!terminalPreviewCacheEnabled()) {
    return;
  }

  try {
    const trimmed = trimPreviewBytes(bytes);
    if (trimmed.length === 0) {
      window.localStorage?.removeItem(previewCacheKey(sessionId));
      return;
    }
    const entry: TerminalPreviewCacheEntry = {
      version: 1,
      sessionId,
      bytesBase64: bytesToBase64(trimmed),
      byteCount: trimmed.length,
      updatedAt: Date.now(),
    };
    window.localStorage?.setItem(previewCacheKey(sessionId), JSON.stringify(entry));
  } catch {
    // Preview cache is an optional UX accelerator; terminal IO must never fail
    // because storage quota or WebView persistence is unavailable.
  }
}

interface LiveTerminalProps {
  client: ControlClient;
  sessionId: string;
  /**
   * The terminal is laid out in the current tab and can consume geometry
   * changes. This intentionally differs from `active`, which represents
   * keyboard focus and the sole WebGL-eligible pane.
   */
  visible: boolean;
  active: boolean;
  terminalGpuAcceleration: TerminalGpuAccelerationMode;
  agentKind?: "claude" | "codex" | null;
  innerMargin?: number;
  fontSize?: number;
  onFocus?: () => void;
  onError?: () => void;
  onOpenLink?: (url: string, event: MouseEvent) => void;
  onPastePaths?: (paths: string[]) => void;
  onExitIntent?: () => void;
}

function terminalWheelMode(
  agentKind: LiveTerminalProps["agentKind"],
): "auto" | "page" | "codex" {
  if (agentKind === "codex") {
    return "codex";
  }
  if (agentKind === "claude") {
    return "page";
  }
  return "auto";
}

interface TerminalRestorePreviewProps {
  sessionId: string;
  innerMargin?: number;
  fontSize?: number;
  fallback: ReactNode;
}

export function TerminalRestorePreview({
  sessionId,
  innerMargin = 0,
  fontSize = 12.5,
  fallback,
}: TerminalRestorePreviewProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const [cachedBytes, setCachedBytes] = useState<Uint8Array | null>(() =>
    readTerminalPreviewCache(sessionId),
  );
  const margin = Math.min(32, Math.max(0, Math.round(innerMargin)));

  useEffect(() => {
    setCachedBytes(readTerminalPreviewCache(sessionId));
  }, [sessionId]);

  useEffect(() => {
    if (!cachedBytes || cachedBytes.length === 0) {
      return;
    }
    const host = hostRef.current;
    if (!host) {
      return;
    }
    const renderer = new XtermTerminalRenderer();
    renderer.mount(
      host,
      { columns: 120, rows: 30, bytes: cachedBytes },
      { fontSize, lineHeight: TERMINAL_LINE_HEIGHT },
    );

    let fitFrame: number | null = null;
    const requestFit = () => {
      if (fitFrame !== null) {
        return;
      }
      fitFrame = window.requestAnimationFrame(() => {
        fitFrame = null;
        renderer.fit();
      });
    };
    const resizeObserver = new ResizeObserver(requestFit);
    resizeObserver.observe(host);

    return () => {
      if (fitFrame !== null) {
        window.cancelAnimationFrame(fitFrame);
      }
      resizeObserver.disconnect();
      renderer.dispose();
    };
  }, [cachedBytes, fontSize]);

  if (!cachedBytes || cachedBytes.length === 0) {
    return <>{fallback}</>;
  }

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        minHeight: 0,
        minWidth: 0,
        position: "relative",
        overflow: "hidden",
        background: "var(--term)",
        padding: margin,
        boxSizing: "border-box",
      }}
    >
      <div
        ref={hostRef}
        aria-label="Restored terminal preview"
        style={{
          flex: "1 1 0",
          height: "100%",
          minHeight: 0,
          minWidth: 0,
          overflow: "hidden",
        }}
      />
      <div
        style={{
          position: "absolute",
          top: 8 + margin,
          right: 10 + margin,
          display: "flex",
          alignItems: "center",
          gap: 6,
          border: "1px solid rgba(88, 166, 255, 0.28)",
          borderRadius: 6,
          background: "rgba(13, 17, 23, 0.82)",
          color: "var(--fg3)",
          font: "600 10px/1 var(--font-sans, system-ui, sans-serif)",
          padding: "5px 7px",
          pointerEvents: "none",
        }}
      >
        <span className="agentmux-term-booting-spinner" />
        Restoring
      </div>
    </div>
  );
}

// A self-contained, live xterm terminal bound to one backend session. Multiple
// instances can render simultaneously (one per mosaic pane) — each owns its own
// renderer and output loop.
//
// With a real Tauri host the renderer streams RAW BYTES through a per-session
// Tauri Channel after one cold-start `session.snapshot`. Because the bytes are
// the live VT stream (not a re-sliced text buffer), full-screen cursor-addressed
// TUIs such as vim, htop, and Claude Code render faithfully. On preview/server
// clients it falls back through snapshot polling, then `readRecent` polling.
export function LiveTerminal({
  client,
  sessionId,
  visible,
  active,
  terminalGpuAcceleration,
  agentKind,
  innerMargin = 0,
  fontSize = 12.5,
  onFocus,
  onError,
  onOpenLink,
  onPastePaths,
  onExitIntent,
}: LiveTerminalProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const rendererRef = useRef<XtermTerminalRenderer | null>(null);
  const visibleRef = useRef(visible);
  const activeRef = useRef(active);
  const agentKindRef = useRef(agentKind ?? null);
  const onOpenLinkRef = useRef(onOpenLink);
  const onPastePathsRef = useRef(onPastePaths);
  const onExitIntentRef = useRef(onExitIntent);
  const inputLineRef = useRef("");
  const bootingRef = useRef(true);
  const pollNowRef = useRef<(() => void) | null>(null);
  const synchronizeLayoutRef = useRef<(() => void) | null>(null);
  const checkpointViewStateRef = useRef<(() => void) | null>(null);
  const webglDisableTimerRef = useRef<number | null>(null);
  const [rendererEpoch, setRendererEpoch] = useState(0);
  const margin = Math.min(32, Math.max(0, Math.round(innerMargin)));
  // True until this session's first output byte is rendered. The component is
  // keyed by sessionId upstream, so this resets for every session. It drives a
  // "starting…" overlay so a slow cold start (notably the first WSL2 VM boot,
  // ~5s, during which the PTY emits nothing) never looks like a broken pane.
  const [booting, setBooting] = useState(true);

  onPastePathsRef.current = onPastePaths;

  const notePossibleExitInput = useCallback((data: string) => {
    let shouldRefresh = false;
    for (const char of data) {
      if (char === "\u0004") {
        shouldRefresh = true;
        inputLineRef.current = "";
        continue;
      }
      if (char === "\u0003") {
        inputLineRef.current = "";
        continue;
      }
      if (char === "\r" || char === "\n") {
        const command = inputLineRef.current.trim().toLowerCase();
        inputLineRef.current = "";
        if (/^(?:exit|logout)(?:\s+\d+)?\s*;?$/.test(command)) {
          shouldRefresh = true;
        }
        continue;
      }
      if (char === "\b" || char === "\u007f") {
        inputLineRef.current = inputLineRef.current.slice(0, -1);
        continue;
      }
      if (char >= " " && char !== "\u007f") {
        inputLineRef.current = `${inputLineRef.current}${char}`.slice(-256);
      }
    }
    if (shouldRefresh) {
      onExitIntentRef.current?.();
    }
  }, []);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) {
      return;
    }
    // Backstop: never leave the overlay up forever if a session legitimately
    // produces no output. Well clear of the worst-case cold WSL boot.
    bootingRef.current = true;
    setBooting(true);
    const bootingBackstop = window.setTimeout(() => {
      bootingRef.current = false;
      setBooting(false);
    }, 20000);
    const markOutput = () => {
      if (!bootingRef.current) {
        return;
      }
      bootingRef.current = false;
      window.clearTimeout(bootingBackstop);
      setBooting(false);
    };

    const restoredViewState = terminalViewStateCache.read(sessionId);
    const renderer = new XtermTerminalRenderer();
    const initialStateReady = renderer.mount(
      host,
      {
        columns: 120,
        rows: 30,
        bytes: restoredViewState
          ? encoder.encode(restoredViewState.serialized)
          : encoder.encode(""),
      },
      { fontSize, lineHeight: TERMINAL_LINE_HEIGHT },
    );
    if (restoredViewState) {
      markOutput();
    }
    renderer.setAlternateWheelMode(terminalWheelMode(agentKindRef.current));
    const unsubscribeOpenLink = renderer.onOpenLink((url, event) => {
      onOpenLinkRef.current?.(url, event);
    });
    const unsubscribePastePaths = renderer.onPastePaths((paths) => {
      onPastePathsRef.current?.(paths);
    });
    rendererRef.current = renderer;
    let alive = true;
    let restoredViewReady = restoredViewState === null;
    void initialStateReady.then(() => {
      if (alive) {
        restoredViewReady = true;
        renderer.restoreViewportY(restoredViewState?.viewportY);
      }
    });
    let renderedOutputOffset = restoredViewState?.outputOffset ?? null;
    let viewCheckpointTimer: number | null = null;
    const checkpointRendererViewState = (allowDuringTeardown = false) => {
      if (
        (!alive && !allowDuringTeardown) ||
        !restoredViewReady ||
        renderedOutputOffset === null
      ) {
        return;
      }
      const outputStats = getTerminalOutputStats(renderer);
      if (outputStats.writeInFlight || outputStats.queuedBytes > 0) {
        return;
      }
      const serialized = renderer.serialize({ scrollback: 10_000 });
      if (!serialized) {
        return;
      }
      terminalViewStateCache.write(sessionId, {
        serialized,
        outputOffset: renderedOutputOffset,
        viewportY: renderer.viewportY() ?? undefined,
        updatedAt: Date.now(),
      });
    };
    checkpointViewStateRef.current = () => checkpointRendererViewState();
    const scheduleViewCheckpoint = () => {
      if (viewCheckpointTimer !== null) {
        window.clearTimeout(viewCheckpointTimer);
      }
      viewCheckpointTimer = window.setTimeout(() => {
        viewCheckpointTimer = null;
        checkpointRendererViewState();
      }, 120);
    };
    const inputSchedulers = new Map<string, TerminalInputScheduler>();
    const inputSchedulerFor = (targetSessionId: string) => {
      const current = inputSchedulers.get(targetSessionId);
      if (current) {
        return current;
      }
      const scheduler = new TerminalInputScheduler(
        {
          sendText: (text) => client.sendText(targetSessionId, text),
          sendPaste: (text) => client.sendPaste(targetSessionId, text),
        },
        {
          onDelivered:
            targetSessionId === sessionId
              ? () => pollNowRef.current?.()
              : undefined,
          onError:
            targetSessionId === sessionId
              ? () => {
                  if (alive) {
                    onError?.();
                  }
                }
              : undefined,
        },
      );
      inputSchedulers.set(targetSessionId, scheduler);
      return scheduler;
    };
    const enqueueText = (targetSessionId: string, text: string) => {
      inputSchedulerFor(targetSessionId).enqueueText(text);
    };
    const enqueuePaste = (targetSessionId: string, text: string) => {
      inputSchedulerFor(targetSessionId).enqueuePaste(text);
    };
    const broadcastText = (text: string) => {
      const peers = _broadcastResolver?.(sessionId);
      if (!peers || peers.length === 0) {
        return;
      }
      for (const peerId of peers) {
        enqueueText(peerId, text);
      }
    };
    let previewCacheBytes = readTerminalPreviewCache(sessionId) ?? new Uint8Array(0);
    let previewFlushTimer: number | null = null;

    const clearPreviewFlush = () => {
      if (previewFlushTimer !== null) {
        window.clearTimeout(previewFlushTimer);
        previewFlushTimer = null;
      }
    };

    const flushPreviewCache = () => {
      clearPreviewFlush();
      writeTerminalPreviewCache(sessionId, previewCacheBytes);
    };

    const schedulePreviewFlush = () => {
      if (previewFlushTimer !== null) {
        return;
      }
      previewFlushTimer = window.setTimeout(
        flushPreviewCache,
        PREVIEW_CACHE_FLUSH_MS,
      );
    };

    const replacePreviewCache = (bytes: Uint8Array) => {
      if (bytes.length === 0) {
        return;
      }
      previewCacheBytes = trimPreviewBytes(bytes);
      schedulePreviewFlush();
    };

    const appendPreviewCache = (bytes: Uint8Array) => {
      if (bytes.length === 0) {
        return;
      }
      previewCacheBytes = concatPreviewBytes(previewCacheBytes, bytes);
      schedulePreviewFlush();
    };

    // --- resize (shared by both output paths) ---
    const resizeCoordinator = new TerminalResizeCoordinator({
      delayMs: TERMINAL_RESIZE_SETTLE_MS,
      send: ({ columns, rows }) => client.resize(sessionId, columns, rows),
      onError: () => {
        if (alive) {
          onError?.();
        }
      },
    });
    const reportRendererSize = (immediate: boolean) => {
      const size = renderer.size();
      if (!size) {
        return;
      }
      resizeCoordinator.request(size, immediate);
    };
    const unsubscribeResize = renderer.onResize((columns, rows) => {
      if (!alive || !visibleRef.current) {
        return;
      }
      resizeCoordinator.request({ columns, rows });
    });
    if (visibleRef.current) {
      reportRendererSize(true);
    }
    const resizeRetryTimers = [120, 400].map((delay) =>
      window.setTimeout(() => {
        if (!alive || !visibleRef.current) {
          return;
        }
        renderer.refreshDisplayMetrics();
        reportRendererSize(true);
      }, delay)
    );
    let fitFrame: number | null = null;
    let layoutSettleTimer: number | null = null;
    let displayMetricsFrame: number | null = null;
    const synchronizeLayout = () => {
      if (!alive || !visibleRef.current) {
        return;
      }
      if (layoutSettleTimer !== null) {
        window.clearTimeout(layoutSettleTimer);
      }
      layoutSettleTimer = window.setTimeout(() => {
        layoutSettleTimer = null;
        if (fitFrame !== null) {
          window.cancelAnimationFrame(fitFrame);
        }
        fitFrame = window.requestAnimationFrame(() => {
          fitFrame = null;
          if (!alive || !visibleRef.current) {
            return;
          }
          renderer.refreshDisplayMetrics();
          reportRendererSize(true);
        });
      }, TERMINAL_LAYOUT_SETTLE_MS);
    };
    synchronizeLayoutRef.current = synchronizeLayout;
    const requestFit = () => synchronizeLayout();
    const requestDisplayMetricsRefresh = () => {
      if (displayMetricsFrame !== null) {
        return;
      }
      displayMetricsFrame = window.requestAnimationFrame(() => {
        displayMetricsFrame = null;
        if (!alive) {
          return;
        }
        if (!visibleRef.current) {
          return;
        }
        renderer.refreshDisplayMetrics?.();
        reportRendererSize(true);
      });
    };
    const resizeObserver = new ResizeObserver(requestFit);
    resizeObserver.observe(host);
    const stopObservingDevicePixelRatio = observeDevicePixelRatio(
      requestDisplayMetricsRefresh,
    );

    // Register imperative commands for this session so app-level code can call
    // clearBuffer / selectAll / findNext / findPrevious / scrollToBottom without
    // holding a direct renderer reference.
    const commands: TerminalCommands = {
      clearBuffer: () => renderer.clearBuffer(),
      selectAll: () => renderer.selectAll(),
      findNext: (term: string) => renderer.findNext(term),
      findPrevious: (term: string) => renderer.findPrevious(term),
      scrollToBottom: () => renderer.scrollToBottom(),
      pasteText: (text: string) => {
        const trimmed = text.trim();
        if (!trimmed) {
          return;
        }
        const needsLeadingSpace =
          inputLineRef.current.length > 0 &&
          !/\s$/.test(inputLineRef.current);
        const payload = `${needsLeadingSpace ? " " : ""}${trimmed} `;
        notePossibleExitInput(payload);
        renderer.focus();
        enqueuePaste(sessionId, payload);
      },
    };
    _terminalCommandRegistry.set(sessionId, commands);

    const teardownShared = () => {
      alive = false;
      for (const scheduler of inputSchedulers.values()) {
        scheduler.close();
      }
      // Unregister commands if this instance still owns the slot.
      if (_terminalCommandRegistry.get(sessionId) === commands) {
        _terminalCommandRegistry.delete(sessionId);
      }
      window.clearTimeout(bootingBackstop);
      resizeCoordinator.dispose();
      if (synchronizeLayoutRef.current === synchronizeLayout) {
        synchronizeLayoutRef.current = null;
      }
      if (layoutSettleTimer !== null) {
        window.clearTimeout(layoutSettleTimer);
      }
      if (fitFrame !== null) {
        window.cancelAnimationFrame(fitFrame);
      }
      if (displayMetricsFrame !== null) {
        window.cancelAnimationFrame(displayMetricsFrame);
      }
      for (const timer of resizeRetryTimers) {
        window.clearTimeout(timer);
      }
      unsubscribeResize();
      unsubscribeOpenLink();
      unsubscribePastePaths();
      resizeObserver.disconnect();
      stopObservingDevicePixelRatio();
      if (viewCheckpointTimer !== null) {
        window.clearTimeout(viewCheckpointTimer);
        viewCheckpointTimer = null;
      }
      // Keep the last parsed checkpoint when an output burst is still queued;
      // replacing it with a partial framebuffer would create an offset gap.
      checkpointRendererViewState(true);
      if (checkpointViewStateRef.current) {
        checkpointViewStateRef.current = null;
      }
      discardTerminalOutput(renderer);
      renderer.dispose();
      flushPreviewCache();
      if (rendererRef.current === renderer) {
        rendererRef.current = null;
      }
    };

    // --- live byte stream (Tauri Channel / server WebSocket) ---
    const liveOutputMode = client.outputStreamMode?.() ?? null;
    if (
      typeof client.snapshot === "function" &&
      typeof client.subscribeOutput === "function" &&
      liveOutputMode !== null
    ) {
      recordTransport(sessionId, liveOutputMode);
      console.info(`[agentmux] terminal output transport: ${liveOutputMode}`, {
        sessionId,
      });
      let expected = restoredViewState?.outputOffset ?? 0;
      let streamReady = restoredViewState !== null;
      let resyncInFlight = false;
      let resyncQueued = false;
      let pendingFrames: OutputStreamFrame[] = [];
      let pendingFrameBytes = 0;
      let pendingDiagnosticFrames = 0;
      let pendingDiagnosticBytes = 0;
      let diagnosticFlushTimer: number | null = null;
      let pressureReportTimer: number | null = null;
      let resyncRetryTimer: number | null = null;
      let unsubscribeOutput: (() => void) | null = null;

      const clearResyncRetry = () => {
        if (resyncRetryTimer !== null) {
          window.clearTimeout(resyncRetryTimer);
          resyncRetryTimer = null;
        }
      };

      const flushTransportDiagnostics = () => {
        if (diagnosticFlushTimer !== null) {
          window.clearTimeout(diagnosticFlushTimer);
          diagnosticFlushTimer = null;
        }
        if (pendingDiagnosticFrames === 0 && pendingDiagnosticBytes === 0) {
          return;
        }
        const diagnostics =
          terminalDiagnostics().__AGENTMUX_TERMINAL_TRANSPORT__?.[sessionId];
        const pressure = getTerminalOutputStats(renderer);
        recordTransport(sessionId, liveOutputMode, {
          frames: (diagnostics?.frames ?? 0) + pendingDiagnosticFrames,
          bytes: (diagnostics?.bytes ?? 0) + pendingDiagnosticBytes,
          queuedBytes: pressure.queuedBytes,
          maxQueuedBytes: pressure.maxQueuedBytes,
          backpressureEvents: pressure.backpressureEvents,
          writeInFlight: pressure.writeInFlight,
        });
        pendingDiagnosticFrames = 0;
        pendingDiagnosticBytes = 0;
      };

      const queueTransportDiagnostics = (byteCount: number) => {
        pendingDiagnosticFrames += 1;
        pendingDiagnosticBytes += byteCount;
        if (diagnosticFlushTimer !== null) {
          return;
        }
        diagnosticFlushTimer = window.setTimeout(
          flushTransportDiagnostics,
          TRANSPORT_DIAGNOSTIC_FLUSH_MS,
        );
      };

      const currentPressureReport = (): OutputPressureReport => {
        const pressure = getTerminalOutputStats(renderer);
        return {
          queuedBytes: pressure.queuedBytes,
          maxQueuedBytes: pressure.maxQueuedBytes,
          backpressureEvents: pressure.backpressureEvents,
          writeInFlight: pressure.writeInFlight,
        };
      };

      const flushPressureReport = () => {
        if (pressureReportTimer !== null) {
          window.clearTimeout(pressureReportTimer);
          pressureReportTimer = null;
        }
        const report = currentPressureReport();
        recordTransport(sessionId, liveOutputMode, {
          queuedBytes: report.queuedBytes,
          maxQueuedBytes: report.maxQueuedBytes,
          backpressureEvents: report.backpressureEvents,
          writeInFlight: report.writeInFlight,
        });
        void client.reportOutputPressure?.(sessionId, report).catch(() => {});
      };

      const queuePressureReport = () => {
        if (pressureReportTimer !== null) {
          return;
        }
        pressureReportTimer = window.setTimeout(
          flushPressureReport,
          TRANSPORT_DIAGNOSTIC_FLUSH_MS,
        );
      };

      const enqueueRenderBytes = (bytes: Uint8Array) => {
        if (bytes.length === 0) {
          return;
        }
        writeTerminalOutput(renderer, bytes, {
          foreground: activeRef.current,
          onParsed: (byteCount) => {
            if (!alive) {
              return;
            }
            markOutput();
            renderedOutputOffset = (renderedOutputOffset ?? 0) + byteCount;
            scheduleViewCheckpoint();
            queueTransportDiagnostics(byteCount);
            queuePressureReport();
            if (
              resyncQueued &&
              !getTerminalOutputStats(renderer).writeInFlight
            ) {
              scheduleResync(0);
            }
          },
          onPressureChange: () => queuePressureReport(),
          onRecoveryRequired: (reason) => {
            if (!alive) {
              return;
            }
            if (reason !== "backlog-overflow") {
              // A stalled or throwing parser is no longer a trustworthy view.
              // The session survives; React rebuilds the disposable xterm view
              // and the next subscription restores it from an atomic snapshot.
              setRendererEpoch((current) => current + 1);
              return;
            }
            // The recent-output ring remains authoritative for recovery. A
            // bounded scheduler can drop a flood without leaving xterm in a
            // partially parsed VT state.
            resyncQueued = true;
            flushPressureReport();
            if (!getTerminalOutputStats(renderer).writeInFlight) {
              scheduleResync(0);
            }
          },
        });
        if (getTerminalOutputStats(renderer).recovering) {
          resyncQueued = true;
        }
      };

      const queueFrame = (fromOffset: number, bytes: Uint8Array) => {
        if (bytes.length === 0) {
          return;
        }
        pendingFrames.push({ fromOffset, bytes });
        pendingFrameBytes += bytes.length;
        if (
          pendingFrames.length > MAX_PENDING_STREAM_FRAMES ||
          pendingFrameBytes > MAX_PENDING_STREAM_BYTES
        ) {
          pendingFrames = [];
          pendingFrameBytes = 0;
          resyncQueued = true;
        }
      };

      const writeSnapshot = (snap: OutputSnapshot) => {
        const diagnostics =
          terminalDiagnostics().__AGENTMUX_TERMINAL_TRANSPORT__?.[sessionId];
        recordTransport(sessionId, liveOutputMode, {
          resyncs: (diagnostics?.resyncs ?? 0) + 1,
        });
        resetTerminalOutput(renderer);
        renderer.reset();
        replacePreviewCache(snap.bytes);
        expected = snap.endOffset;
        renderedOutputOffset = snap.baseOffset;
        streamReady = true;
        if (snap.bytes.length > 0) {
          enqueueRenderBytes(snap.bytes);
        }
      };

      async function resync() {
        if (getTerminalOutputStats(renderer).writeInFlight) {
          resyncQueued = true;
          return;
        }
        if (resyncInFlight) {
          resyncQueued = true;
          return;
        }
        // Freeze the renderer queue before awaiting the snapshot. Otherwise a
        // delayed background drain could start while the snapshot request is
        // in flight and mutate xterm immediately before reset/replay.
        resetTerminalOutput(renderer);
        resyncQueued = false;
        resyncInFlight = true;
        clearResyncRetry();
        try {
          const snap = await client.snapshot!(
            sessionId,
            streamReady ? expected : undefined,
          );
          if (!alive) {
            return;
          }
          if (streamReady && snap.baseOffset === expected) {
            if (snap.bytes.length > 0) {
              appendPreviewCache(snap.bytes);
              enqueueRenderBytes(snap.bytes);
            }
            expected = snap.endOffset;
          } else {
            writeSnapshot(snap);
          }
        } catch {
          if (alive) {
            scheduleResync(activeRef.current ? SNAPSHOT_BOOT_POLL_MS : SNAPSHOT_INACTIVE_POLL_MS);
          }
          return;
        } finally {
          resyncInFlight = false;
        }
        if (!alive) {
          return;
        }
        flushPendingFrames();
        if (resyncQueued) {
          resyncQueued = false;
          scheduleResync(0);
        }
      }

      const scheduleResync = (delayMs: number) => {
        clearResyncRetry();
        if (!alive) {
          return;
        }
        if (delayMs <= 0) {
          void resync();
          return;
        }
        resyncRetryTimer = window.setTimeout(() => {
          resyncRetryTimer = null;
          void resync();
        }, delayMs);
      };

      const applyFrame = (fromOffset: number, bytes: Uint8Array) => {
        if (!alive || bytes.length === 0) {
          return;
        }
        if (!streamReady || resyncInFlight) {
          queueFrame(fromOffset, bytes);
          return;
        }

        const frameEnd = fromOffset + bytes.length;
        if (frameEnd <= expected) {
          return;
        }
        if (fromOffset > expected) {
          queueFrame(fromOffset, bytes);
          scheduleResync(0);
          return;
        }

        const duplicateBytes = Math.max(0, expected - fromOffset);
        const next = duplicateBytes > 0 ? bytes.subarray(duplicateBytes) : bytes;
        if (next.length > 0) {
          appendPreviewCache(next);
          enqueueRenderBytes(next);
        }
        expected = frameEnd;
      };

      function flushPendingFrames() {
        if (pendingFrames.length === 0) {
          return;
        }
        const frames = pendingFrames;
        pendingFrames = [];
        pendingFrameBytes = 0;
        frames.sort((left, right) => left.fromOffset - right.fromOffset);
        for (const frame of frames) {
          applyFrame(frame.fromOffset, frame.bytes);
        }
      }

      const unsubscribeInput = renderer.onData((data) => {
        notePossibleExitInput(data);
        enqueueText(sessionId, data);
        broadcastText(data);
      });
      const unsubscribePaste = renderer.onPaste((text) => {
        notePossibleExitInput(text);
        enqueuePaste(sessionId, text);
        broadcastText(text);
      });

      void client
        .subscribeOutput(sessionId, applyFrame)
        .then((unsubscribe) => {
          if (!alive) {
            unsubscribe();
            return;
          }
          unsubscribeOutput = unsubscribe;
          scheduleResync(0);
        })
        .catch(() => {
          if (alive) {
            onError?.();
          }
        });

      return () => {
        clearResyncRetry();
        discardTerminalOutput(renderer);
        flushTransportDiagnostics();
        flushPressureReport();
        unsubscribeInput();
        unsubscribePaste();
        unsubscribeOutput?.();
        teardownShared();
      };
    }

    // --- raw-byte snapshot polling fallback (Tauri without Channel) ---
    if (typeof client.snapshot === "function") {
      recordTransport(sessionId, "snapshot-poll");
      console.info("[agentmux] terminal output transport: snapshot-poll", {
        sessionId,
      });
      // Absolute offset already written into xterm. Each poll asks for bytes at
      // or after it and writes the delta. A returned base_offset greater than
      // `expected` means the bounded ring rotated past us — reset and resync.
      let expected = restoredViewState?.outputOffset ?? 0;
      let polling = false;
      let queued = false;
      let hotPollsRemaining = ACTIVITY_HOT_POLLS;
      let snapshotTimer: number | null = null;

      const clearSnapshotTimer = () => {
        if (snapshotTimer !== null) {
          window.clearTimeout(snapshotTimer);
          snapshotTimer = null;
        }
      };

      const snapshotDelay = (hadOutput: boolean) => {
        if (documentHidden()) {
          return SNAPSHOT_HIDDEN_POLL_MS;
        }
        if (hadOutput) {
          hotPollsRemaining = ACTIVITY_HOT_POLLS;
          return SNAPSHOT_HOT_POLL_MS;
        }
        if (hotPollsRemaining > 0) {
          hotPollsRemaining -= 1;
          return SNAPSHOT_HOT_POLL_MS;
        }
        if (bootingRef.current && activeRef.current) {
          return SNAPSHOT_BOOT_POLL_MS;
        }
        return activeRef.current ? SNAPSHOT_IDLE_POLL_MS : SNAPSHOT_INACTIVE_POLL_MS;
      };

      const scheduleSnapshotPoll = (delayMs: number) => {
        clearSnapshotTimer();
        if (!alive) {
          return;
        }
        snapshotTimer = window.setTimeout(() => {
          snapshotTimer = null;
          void pollSnapshot();
        }, delayMs);
      };

      const pollSnapshot = async () => {
        if (polling) {
          queued = true;
          return;
        }
        polling = true;
        let hadOutput = false;
        try {
          do {
            queued = false;
            const snap = await client.snapshot!(sessionId, expected);
            if (!alive) {
              return;
            }
            if (snap.endOffset === expected) {
              continue; // no new output
            }
            if (snap.baseOffset !== expected) {
              renderer.reset(); // fell behind the ring; resync from base
              replacePreviewCache(snap.bytes);
              renderedOutputOffset = snap.baseOffset;
            } else {
              appendPreviewCache(snap.bytes);
            }
            if (snap.bytes.length > 0) {
              if (renderedOutputOffset === null) {
                renderedOutputOffset = snap.baseOffset;
              }
              renderer.write(snap.bytes, () => {
                renderedOutputOffset =
                  (renderedOutputOffset ?? snap.baseOffset) + snap.bytes.length;
                scheduleViewCheckpoint();
              });
              hadOutput = true;
              markOutput();
              const diagnostics =
                terminalDiagnostics().__AGENTMUX_TERMINAL_TRANSPORT__?.[sessionId];
              recordTransport(sessionId, "snapshot-poll", {
                frames: (diagnostics?.frames ?? 0) + 1,
                bytes: (diagnostics?.bytes ?? 0) + snap.bytes.length,
              });
            }
            expected = snap.endOffset;
          } while (alive && queued);
        } catch {
          // Transient snapshot failures (session still spawning, brief lock
          // contention) are retried by the interval below. Do NOT call onError
          // here: it triggers a workspace refresh, and at a 40ms cadence that
          // would be a refresh storm that never lets the terminal settle.
        } finally {
          polling = false;
          if (!alive) {
            return;
          }
          if (queued) {
            void pollSnapshot();
            return;
          }
          scheduleSnapshotPoll(snapshotDelay(hadOutput));
        }
      };

      const requestSnapshotPoll = () => {
        hotPollsRemaining = ACTIVITY_HOT_POLLS;
        clearSnapshotTimer();
        void pollSnapshot();
      };
      pollNowRef.current = requestSnapshotPoll;

      const unsubscribeInput = renderer.onData((data) => {
        notePossibleExitInput(data);
        enqueueText(sessionId, data);
        broadcastText(data);
      });
      const unsubscribePaste = renderer.onPaste((text) => {
        notePossibleExitInput(text);
        enqueuePaste(sessionId, text);
        broadcastText(text);
      });

      void pollSnapshot();

      return () => {
        clearSnapshotTimer();
        if (pollNowRef.current === requestSnapshotPoll) {
          pollNowRef.current = null;
        }
        unsubscribeInput();
        unsubscribePaste();
        teardownShared();
      };
    }

    // --- readRecent polling fallback (preview / server clients) ---
    if (restoredViewState) {
      // This legacy transport has no absolute cursor with which to deduplicate
      // the serialized framebuffer. Prefer a clean recent-output replay.
      renderer.reset();
      renderedOutputOffset = null;
      terminalViewStateCache.delete(sessionId);
    }
    recordTransport(sessionId, "read-recent-poll");
    console.info("[agentmux] terminal output transport: read-recent-poll", {
      sessionId,
    });
    let renderedText = "";
    let pollInFlight = false;
    let pollQueued = false;
    let hotPollsRemaining = ACTIVITY_HOT_POLLS;
    let fallbackTimer: number | null = null;

    const clearFallbackTimer = () => {
      if (fallbackTimer !== null) {
        window.clearTimeout(fallbackTimer);
        fallbackTimer = null;
      }
    };

    const fallbackDelay = (hadOutput: boolean) => {
      if (documentHidden()) {
        return SNAPSHOT_HIDDEN_POLL_MS;
      }
      if (hadOutput) {
        hotPollsRemaining = ACTIVITY_HOT_POLLS;
        return FALLBACK_HOT_POLL_MS;
      }
      if (hotPollsRemaining > 0) {
        hotPollsRemaining -= 1;
        return FALLBACK_HOT_POLL_MS;
      }
      return activeRef.current ? FALLBACK_IDLE_POLL_MS : FALLBACK_INACTIVE_POLL_MS;
    };

    const scheduleFallbackPoll = (delayMs: number) => {
      clearFallbackTimer();
      if (!alive) {
        return;
      }
      fallbackTimer = window.setTimeout(() => {
        fallbackTimer = null;
        void poll();
      }, delayMs);
    };

    const poll = async () => {
      if (pollInFlight) {
        pollQueued = true;
        return;
      }
      pollInFlight = true;
      let hadOutput = false;
      try {
        do {
          pollQueued = false;
          const text = await client.readRecent(sessionId, 65536);
          if (!alive) {
            return;
          }
          if (text.length > 0) {
            markOutput();
          }
          if (text === renderedText) {
            continue;
          }
          if (!text.startsWith(renderedText)) {
            renderedText = text;
            renderer.reset();
            if (text.length > 0) {
              const bytes = encoder.encode(text);
              replacePreviewCache(bytes);
              renderer.write(bytes);
              hadOutput = true;
              const diagnostics =
                terminalDiagnostics().__AGENTMUX_TERMINAL_TRANSPORT__?.[sessionId];
              recordTransport(sessionId, "read-recent-poll", {
                frames: (diagnostics?.frames ?? 0) + 1,
                bytes: (diagnostics?.bytes ?? 0) + text.length,
              });
            }
            continue;
          }
          const next = text.slice(renderedText.length);
          renderedText = text;
          if (next.length > 0) {
            const bytes = encoder.encode(next);
            appendPreviewCache(bytes);
            renderer.write(bytes);
            hadOutput = true;
            const diagnostics =
              terminalDiagnostics().__AGENTMUX_TERMINAL_TRANSPORT__?.[sessionId];
            recordTransport(sessionId, "read-recent-poll", {
              frames: (diagnostics?.frames ?? 0) + 1,
              bytes: (diagnostics?.bytes ?? 0) + next.length,
            });
          }
        } while (alive && pollQueued);
      } catch {
        onError?.();
      } finally {
        pollInFlight = false;
        if (!alive) {
          return;
        }
        if (pollQueued) {
          void poll();
          return;
        }
        scheduleFallbackPoll(fallbackDelay(hadOutput));
      }
    };

    const requestFallbackPoll = () => {
      hotPollsRemaining = ACTIVITY_HOT_POLLS;
      clearFallbackTimer();
      void poll();
    };
    pollNowRef.current = requestFallbackPoll;

    const unsubscribeInput = renderer.onData((data) => {
      notePossibleExitInput(data);
      enqueueText(sessionId, data);
      broadcastText(data);
    });
    const unsubscribePaste = renderer.onPaste((text) => {
      notePossibleExitInput(text);
      enqueuePaste(sessionId, text);
      broadcastText(text);
    });

    void poll();

    return () => {
      clearFallbackTimer();
      if (pollNowRef.current === requestFallbackPoll) {
        pollNowRef.current = null;
      }
      unsubscribeInput();
      unsubscribePaste();
      teardownShared();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, rendererEpoch, sessionId]);

  useEffect(() => {
    onOpenLinkRef.current = onOpenLink;
  }, [onOpenLink]);

  useEffect(() => {
    onExitIntentRef.current = onExitIntent;
  }, [onExitIntent]);

  useEffect(() => {
    const normalized = agentKind ?? null;
    agentKindRef.current = normalized;
    rendererRef.current?.setAlternateWheelMode(terminalWheelMode(normalized));
  }, [agentKind]);

  useEffect(() => {
    visibleRef.current = visible;
    if (visible) {
      // Every pane visible in the active tab owns a live terminal grid. A
      // focused-only policy leaves sibling WSL/tmux PTYs at stale dimensions
      // when docked UI or the window changes size.
      synchronizeLayoutRef.current?.();
      pollNowRef.current?.();
    } else {
      // A warm-retained background tab stays mounted for instant switching but
      // must not react to its off-screen layout box.
      checkpointViewStateRef.current?.();
    }
  }, [visible]);

  useEffect(() => {
    activeRef.current = active;
    const renderer = rendererRef.current;
    if (renderer) {
      setTerminalOutputForeground(renderer, active);
    }
    if (active) {
      synchronizeLayoutRef.current?.();
      renderer?.focus();
      pollNowRef.current?.();
    } else {
      // Capture before this tree becomes eligible for cold parking. Cleanup
      // retains the last complete checkpoint if xterm is still parsing.
      checkpointViewStateRef.current?.();
    }
  }, [active]);

  useEffect(() => {
    const renderer = rendererRef.current;
    if (!renderer) {
      return;
    }
    renderer.setTypography({ fontSize, lineHeight: TERMINAL_LINE_HEIGHT });
  }, [fontSize]);

  // Terminal preview cache is also opt-in because terminal output can contain
  // secrets. Local users can enable it with:
  // localStorage.agentmux.terminal.previewCache = "1".
  useEffect(() => {
    const renderer = rendererRef.current;
    if (!renderer) {
      return;
    }
    const clearWebglDisableTimer = () => {
      if (webglDisableTimerRef.current !== null) {
        window.clearTimeout(webglDisableTimerRef.current);
        webglDisableTimerRef.current = null;
      }
    };

    const applyWebglPolicy = (recoveryBoundary: boolean) => {
      const visible = !documentHidden();
      if (
        recoveryBoundary &&
        visible &&
        activeRef.current &&
        terminalGpuAcceleration !== "off"
      ) {
        renderer.resetWebglRecovery();
      }
      const requested =
        activeRef.current && visible && terminalGpuAcceleration !== "off";

      recordWebgl(
        sessionId,
        terminalGpuAcceleration,
        activeRef.current,
        visible,
        requested,
        renderer,
      );

      clearWebglDisableTimer();
      if (requested) {
        renderer.enableWebgl(terminalGpuAcceleration);
        return;
      }
      webglDisableTimerRef.current = window.setTimeout(() => {
        webglDisableTimerRef.current = null;
        if (rendererRef.current === renderer) {
          renderer.disableWebgl();
          recordWebgl(
            sessionId,
            terminalGpuAcceleration,
            activeRef.current,
            !documentHidden(),
            false,
            renderer,
          );
        }
      }, WEBGL_DISABLE_DEBOUNCE_MS);
    };

    const recordRendererState = () => {
      const visible = !documentHidden();
      const requested =
        activeRef.current &&
        visible &&
        terminalGpuAcceleration !== "off";
      recordWebgl(
        sessionId,
        terminalGpuAcceleration,
        activeRef.current,
        visible,
        requested,
        renderer,
      );
    };
    const unsubscribeWebglState = renderer.onWebglStateChange(recordRendererState);
    const onVisibilityChange = () => applyWebglPolicy(!documentHidden());
    const onWake = () => {
      if (!documentHidden()) {
        applyWebglPolicy(true);
      }
    };

    document.addEventListener("visibilitychange", onVisibilityChange);
    window.addEventListener("focus", onWake);
    window.addEventListener("pageshow", onWake);
    applyWebglPolicy(false);

    return () => {
      clearWebglDisableTimer();
      document.removeEventListener("visibilitychange", onVisibilityChange);
      window.removeEventListener("focus", onWake);
      window.removeEventListener("pageshow", onWake);
      unsubscribeWebglState();
      removeWebglDiagnostics(sessionId);
    };
  }, [active, rendererEpoch, sessionId, terminalGpuAcceleration]);

  return (
    <div
      onMouseDown={onFocus}
      data-agentmux-terminal-session={sessionId}
      data-agentmux-terminal-inner-margin={margin}
      data-agentmux-terminal-gpu-acceleration={terminalGpuAcceleration}
      style={{
        display: "flex",
        flexDirection: "column",
        position: "relative",
        height: "100%",
        width: "100%",
        minHeight: 0,
        minWidth: 0,
        boxSizing: "border-box",
        overflow: "hidden",
        padding: margin,
        background: XTERM_THEME.background,
      }}
    >
      <div
        ref={hostRef}
        className="agentmux-live-terminal-host"
        style={{
          flex: "1 1 0",
          height: "100%",
          width: "100%",
          minHeight: 0,
          minWidth: 0,
          overflow: "hidden",
          background: XTERM_THEME.background,
        }}
      />
      {booting && (
        <div
          className="agentmux-term-booting"
          aria-hidden
          style={{
            position: "absolute",
            inset: margin,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 10,
            pointerEvents: "none",
            background: XTERM_THEME.background,
            color: "#8b949e",
          }}
        >
          <span className="agentmux-term-booting-spinner" />
          <span style={{ fontSize: 13, letterSpacing: 0.2 }}>
            Starting terminal...
          </span>
        </div>
      )}
    </div>
  );
}
