import { useEffect, useRef, useState } from "react";
import type { ControlClient } from "../control/ControlClient";
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

function documentHidden(): boolean {
  return typeof document !== "undefined" && document.visibilityState === "hidden";
}

interface LiveTerminalProps {
  client: ControlClient;
  sessionId: string;
  active: boolean;
  innerMargin?: number;
  onFocus?: () => void;
  onError?: () => void;
}

// A self-contained, live xterm terminal bound to one backend session. Multiple
// instances can render simultaneously (one per mosaic pane) — each owns its own
// renderer and output loop.
//
// With a real Tauri host the renderer streams RAW BYTES: it polls
// `session.snapshot` with the absolute offset it has consumed and writes only
// the new delta straight into xterm. Because the bytes are the live VT stream
// (not a re-sliced text buffer), full-screen cursor-addressed TUIs — vim, htop,
// Claude Code — render faithfully. On the preview/server clients (no snapshot)
// it falls back to polling `readRecent`.
export function LiveTerminal({
  client,
  sessionId,
  active,
  innerMargin = 0,
  onFocus,
  onError,
}: LiveTerminalProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const rendererRef = useRef<XtermTerminalRenderer | null>(null);
  const activeRef = useRef(active);
  const bootingRef = useRef(true);
  const pollNowRef = useRef<(() => void) | null>(null);
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
    renderer.mount(host, { columns: 120, rows: 30, bytes: encoder.encode("") });
    rendererRef.current = renderer;
    let alive = true;

    // --- resize (shared by both output paths) ---
    let resizeTimer: number | null = null;
    let pendingResize: { columns: number; rows: number } | null = null;
    let lastResizeSent = { columns: 120, rows: 30 };
    const unsubscribeResize = renderer.onResize((columns, rows) => {
      if (columns === lastResizeSent.columns && rows === lastResizeSent.rows) {
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
        lastResizeSent = next;
        client
          .resize(sessionId, next.columns, next.rows)
          .catch(() => onError?.());
      }, 80);
    });
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

    const teardownShared = () => {
      alive = false;
      window.clearTimeout(bootingBackstop);
      if (resizeTimer !== null) {
        window.clearTimeout(resizeTimer);
      }
      if (fitFrame !== null) {
        window.cancelAnimationFrame(fitFrame);
      }
      unsubscribeResize();
      resizeObserver.disconnect();
      renderer.dispose();
      if (rendererRef.current === renderer) {
        rendererRef.current = null;
      }
    };

    // --- raw-byte snapshot polling (real Tauri host) ---
    if (typeof client.snapshot === "function") {
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

      void pollSnapshot();

      return () => {
        clearSnapshotTimer();
        if (pollNowRef.current === requestSnapshotPoll) {
          pollNowRef.current = null;
        }
        unsubscribeInput();
        teardownShared();
      };
    }

    // --- readRecent polling fallback (preview / server clients) ---
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
            }
            continue;
          }
          const next = text.slice(renderedText.length);
          renderedText = text;
          if (next.length > 0) {
            renderer.write(encoder.encode(next));
            hadOutput = true;
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

    void poll();

    return () => {
      clearFallbackTimer();
      if (pollNowRef.current === requestFallbackPoll) {
        pollNowRef.current = null;
      }
      unsubscribeInput();
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

  // Visible-only GPU rendering. WebView2/Chromium caps the number of live WebGL
  // contexts (~16), so a multiplexer must not hand every pane its own context.
  // Enable WebGL only while this pane is active, and dispose it on deactivation.
  // The xterm instance stays mounted, so the buffer and output loop survive the
  // toggle. enable/disableWebgl no-op if WebGL is unavailable.
  useEffect(() => {
    const renderer = rendererRef.current;
    if (!renderer) {
      return;
    }
    if (active) {
      renderer.enableWebgl();
    } else {
      renderer.disableWebgl();
    }
    return () => {
      renderer.disableWebgl();
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
