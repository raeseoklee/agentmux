import { useEffect, useRef, useState } from "react";
import type {
  ControlClient,
  OutputPressureReport,
  OutputSnapshot,
} from "../control/ControlClient";
import {
  XtermTerminalRenderer,
  XTERM_THEME,
} from "../terminal/XtermTerminalRenderer";

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
const MAX_RENDER_QUEUE_BYTES = 2 * 1024 * 1024;
const MAX_RENDER_BATCH_BYTES = 64 * 1024;
const TRANSPORT_DIAGNOSTIC_FLUSH_MS = 250;
const WEBGL_DISABLE_DEBOUNCE_MS = 250;
const TERMINAL_LINE_HEIGHT = 1.0;

function terminalWebglEnabled(): boolean {
  try {
    return window.localStorage?.getItem("agentmux.terminal.webgl") === "1";
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

function terminalDiagnostics() {
  return window as Window & {
    __AGENTMUX_TERMINAL_TRANSPORT__?: Record<string, TerminalTransportDiagnostics>;
  };
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

interface LiveTerminalProps {
  client: ControlClient;
  sessionId: string;
  active: boolean;
  innerMargin?: number;
  fontSize?: number;
  onFocus?: () => void;
  onError?: () => void;
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
  active,
  innerMargin = 0,
  fontSize = 12.5,
  onFocus,
  onError,
}: LiveTerminalProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const rendererRef = useRef<XtermTerminalRenderer | null>(null);
  const activeRef = useRef(active);
  const bootingRef = useRef(true);
  const pollNowRef = useRef<(() => void) | null>(null);
  const webglDisableTimerRef = useRef<number | null>(null);
  const margin = Math.min(32, Math.max(0, Math.round(innerMargin)));
  // True until this session's first output byte is rendered. The component is
  // keyed by sessionId upstream, so this resets for every session. It drives a
  // "starting…" overlay so a slow cold start (notably the first WSL2 VM boot,
  // ~5s, during which the PTY emits nothing) never looks like a broken pane.
  const [booting, setBooting] = useState(true);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) {
      return;
    }
    // Backstop: never leave the overlay up forever if a session legitimately
    // produces no output. Well clear of the worst-case cold WSL boot.
    bootingRef.current = true;
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

    const renderer = new XtermTerminalRenderer();
    renderer.mount(
      host,
      { columns: 120, rows: 30, bytes: encoder.encode("") },
      { fontSize, lineHeight: TERMINAL_LINE_HEIGHT },
    );
    rendererRef.current = renderer;
    let alive = true;

    // --- resize (shared by both output paths) ---
    let resizeTimer: number | null = null;
    let pendingResize: { columns: number; rows: number } | null = null;
    let lastResizeSent = { columns: 0, rows: 0 };
    const sendResize = (columns: number, rows: number, force = false) => {
      if (columns <= 0 || rows <= 0) {
        return;
      }
      if (!force && columns === lastResizeSent.columns && rows === lastResizeSent.rows) {
        return;
      }
      lastResizeSent = { columns, rows };
      client
        .resize(sessionId, columns, rows)
        .catch(() => onError?.());
    };
    const reportRendererSize = (immediate: boolean) => {
      const size = renderer.size();
      if (!size) {
        return;
      }
      if (immediate) {
        if (resizeTimer !== null) {
          window.clearTimeout(resizeTimer);
          resizeTimer = null;
        }
        pendingResize = null;
        sendResize(size.columns, size.rows, true);
        return;
      }
      pendingResize = { columns: size.columns, rows: size.rows };
      if (resizeTimer !== null) {
        window.clearTimeout(resizeTimer);
      }
      resizeTimer = window.setTimeout(() => {
        resizeTimer = null;
        const next = pendingResize;
        pendingResize = null;
        if (!next || !alive) {
          return;
        }
        sendResize(next.columns, next.rows);
      }, 80);
    };
    const unsubscribeResize = renderer.onResize((columns, rows) => {
      if (
        !alive ||
        (columns === lastResizeSent.columns && rows === lastResizeSent.rows)
      ) {
        return;
      }
      pendingResize = { columns, rows };
      if (resizeTimer !== null) {
        window.clearTimeout(resizeTimer);
      }
      resizeTimer = window.setTimeout(() => {
        resizeTimer = null;
        const next = pendingResize;
        pendingResize = null;
        if (!next || !alive) {
          return;
        }
        sendResize(next.columns, next.rows);
      }, 80);
    });
    reportRendererSize(true);
    const forceResizeTimers = [120, 400, 1000].map((delay) =>
      window.setTimeout(() => {
        if (!alive) {
          return;
        }
        renderer.fit();
        reportRendererSize(true);
      }, delay)
    );
    let fitFrame: number | null = null;
    const requestFit = () => {
      if (fitFrame !== null) {
        return;
      }
      fitFrame = window.requestAnimationFrame(() => {
        fitFrame = null;
        renderer.fit();
        reportRendererSize(false);
      });
    };
    const resizeObserver = new ResizeObserver(requestFit);
    resizeObserver.observe(host);

    const teardownShared = () => {
      alive = false;
      window.clearTimeout(bootingBackstop);
      if (resizeTimer !== null) {
        window.clearTimeout(resizeTimer);
      }
      if (fitFrame !== null) {
        window.cancelAnimationFrame(fitFrame);
      }
      for (const timer of forceResizeTimers) {
        window.clearTimeout(timer);
      }
      unsubscribeResize();
      resizeObserver.disconnect();
      renderer.dispose();
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
      let expected = 0;
      let streamReady = false;
      let resyncInFlight = false;
      let resyncQueued = false;
      let pendingFrames: OutputStreamFrame[] = [];
      let pendingFrameBytes = 0;
      let renderQueue: Uint8Array[] = [];
      let renderQueueBytes = 0;
      let maxRenderQueueBytes = 0;
      let renderBackpressureEvents = 0;
      let renderWriteInFlight = false;
      let renderFlushFrame: number | null = null;
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

      const clearRenderFlush = () => {
        if (renderFlushFrame !== null) {
          window.cancelAnimationFrame(renderFlushFrame);
          renderFlushFrame = null;
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
        recordTransport(sessionId, liveOutputMode, {
          frames: (diagnostics?.frames ?? 0) + pendingDiagnosticFrames,
          bytes: (diagnostics?.bytes ?? 0) + pendingDiagnosticBytes,
          queuedBytes: renderQueueBytes,
          maxQueuedBytes: maxRenderQueueBytes,
          backpressureEvents: renderBackpressureEvents,
          writeInFlight: renderWriteInFlight,
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

      const clearRenderQueue = () => {
        renderQueue = [];
        renderQueueBytes = 0;
      };

      const currentPressureReport = (): OutputPressureReport => ({
        queuedBytes: renderQueueBytes,
        maxQueuedBytes: maxRenderQueueBytes,
        backpressureEvents: renderBackpressureEvents,
        writeInFlight: renderWriteInFlight,
      });

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

      const takeRenderBatch = () => {
        const byteCount = Math.min(renderQueueBytes, MAX_RENDER_BATCH_BYTES);
        if (byteCount <= 0) {
          return null;
        }
        if (renderQueue.length === 1 && renderQueue[0].length <= byteCount) {
          const [only] = renderQueue;
          renderQueue = [];
          renderQueueBytes = 0;
          return only;
        }

        const batch = new Uint8Array(byteCount);
        let copied = 0;
        while (copied < byteCount && renderQueue.length > 0) {
          const head = renderQueue[0];
          const take = Math.min(head.length, byteCount - copied);
          batch.set(head.subarray(0, take), copied);
          copied += take;
          renderQueueBytes -= take;
          if (take === head.length) {
            renderQueue.shift();
          } else {
            renderQueue[0] = head.subarray(take);
          }
        }
        return batch;
      };

      const scheduleRenderFlush = () => {
        if (!alive || renderWriteInFlight || renderFlushFrame !== null) {
          return;
        }
        renderFlushFrame = window.requestAnimationFrame(() => {
          renderFlushFrame = null;
          flushRenderQueue();
        });
      };

      function flushRenderQueue() {
        if (!alive || renderWriteInFlight) {
          return;
        }
        if (resyncQueued) {
          scheduleResync(0);
          return;
        }
        const batch = takeRenderBatch();
        if (!batch) {
          return;
        }

        renderWriteInFlight = true;
        renderer.write(batch, () => {
          renderWriteInFlight = false;
          if (!alive) {
            return;
          }
          markOutput();
          queueTransportDiagnostics(batch.length);
          queuePressureReport();
          if (resyncQueued) {
            scheduleResync(0);
            return;
          }
          if (renderQueueBytes > 0) {
            scheduleRenderFlush();
          }
        });
      }

      const enqueueRenderBytes = (bytes: Uint8Array) => {
        if (bytes.length === 0) {
          return;
        }
        const wasBackpressured = renderWriteInFlight || renderQueueBytes > 0;
        renderQueue.push(bytes);
        renderQueueBytes += bytes.length;
        maxRenderQueueBytes = Math.max(maxRenderQueueBytes, renderQueueBytes);
        if (wasBackpressured) {
          renderBackpressureEvents += 1;
          queuePressureReport();
        }
        if (renderQueueBytes > MAX_RENDER_QUEUE_BYTES) {
          clearRenderQueue();
          resyncQueued = true;
          flushPressureReport();
          if (!renderWriteInFlight) {
            scheduleResync(0);
          }
          return;
        }
        scheduleRenderFlush();
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
        renderer.reset();
        clearRenderQueue();
        if (snap.bytes.length > 0) {
          enqueueRenderBytes(snap.bytes);
        }
        expected = snap.endOffset;
        streamReady = true;
      };

      async function resync() {
        if (renderWriteInFlight) {
          resyncQueued = true;
          return;
        }
        if (resyncInFlight) {
          resyncQueued = true;
          return;
        }
        resyncQueued = false;
        resyncInFlight = true;
        clearResyncRetry();
        try {
          const snap = await client.snapshot!(sessionId);
          if (!alive) {
            return;
          }
          writeSnapshot(snap);
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
        client.sendText(sessionId, data).catch(() => onError?.());
      });
      const unsubscribePaste = renderer.onPaste((text) => {
        const sendPaste = client.sendPaste
          ? client.sendPaste.bind(client)
          : client.sendText.bind(client);
        sendPaste(sessionId, text).catch(() => onError?.());
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
        clearRenderFlush();
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
      let expected = 0;
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
            if (snap.baseOffset > expected) {
              renderer.reset(); // fell behind the ring; resync from base
            }
            if (snap.bytes.length > 0) {
              renderer.write(snap.bytes);
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
        client
          .sendText(sessionId, data)
          .then(() => {
            // Poll promptly so the echo appears without waiting for the tick.
            requestSnapshotPoll();
          })
          .catch(() => onError?.());
      });
      const unsubscribePaste = renderer.onPaste((text) => {
        const sendPaste = client.sendPaste
          ? client.sendPaste.bind(client)
          : client.sendText.bind(client);
        requestSnapshotPoll();
        sendPaste(sessionId, text)
          .then(requestSnapshotPoll)
          .catch(() => onError?.());
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
              renderer.write(encoder.encode(text));
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
            renderer.write(encoder.encode(next));
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
      requestFallbackPoll();
      client
        .sendText(sessionId, data)
        .then(requestFallbackPoll)
        .catch(() => onError?.());
    });
    const unsubscribePaste = renderer.onPaste((text) => {
      const sendPaste = client.sendPaste
        ? client.sendPaste.bind(client)
        : client.sendText.bind(client);
      requestFallbackPoll();
      sendPaste(sessionId, text)
        .then(requestFallbackPoll)
        .catch(() => onError?.());
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
  }, [client, sessionId]);

  useEffect(() => {
    activeRef.current = active;
    if (active) {
      rendererRef.current?.focus();
      pollNowRef.current?.();
    }
  }, [active]);

  useEffect(() => {
    const renderer = rendererRef.current;
    if (!renderer) {
      return;
    }
    renderer.setTypography({ fontSize, lineHeight: TERMINAL_LINE_HEIGHT });
    renderer.fit();
    const size = renderer.size();
    if (size) {
      client.resize(sessionId, size.columns, size.rows).catch(() => onError?.());
    }
  }, [client, fontSize, onError, sessionId]);

  // WebGL remains opt-in because Chromium's glyph atlas can render private-use
  // Nerd Font fallback symbols as tofu on some Windows/WebView2 stacks. Keep
  // the default path glyph-faithful; allow explicit perf testing with
  // localStorage.agentmux.terminal.webgl = "1".
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
    if (active && terminalWebglEnabled()) {
      clearWebglDisableTimer();
      renderer.enableWebgl();
    } else {
      clearWebglDisableTimer();
      webglDisableTimerRef.current = window.setTimeout(() => {
        webglDisableTimerRef.current = null;
        if (rendererRef.current === renderer) {
          renderer.disableWebgl();
        }
      }, WEBGL_DISABLE_DEBOUNCE_MS);
    }
    return () => {
      clearWebglDisableTimer();
    };
  }, [active, sessionId, client]);

  return (
    <div
      onMouseDown={onFocus}
      data-agentmux-terminal-inner-margin={margin}
      style={{
        position: "relative",
        height: "100%",
        width: "100%",
        minHeight: 0,
        minWidth: 0,
        boxSizing: "border-box",
        padding: margin,
        background: XTERM_THEME.background,
      }}
    >
      <div
        ref={hostRef}
        className="agentmux-live-terminal-host"
        style={{
          height: "100%",
          width: "100%",
          minHeight: 0,
          minWidth: 0,
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
            터미널 시작 중…
          </span>
        </div>
      )}
    </div>
  );
}
