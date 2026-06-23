import { useEffect, useRef, useState } from "react";
import type { ControlClient } from "../control/ControlClient";
import {
  XtermTerminalRenderer,
  XTERM_THEME,
} from "../terminal/XtermTerminalRenderer";

const encoder = new TextEncoder();
const POLL_INTERVAL_MS = 120;
const SNAPSHOT_POLL_MS = 40;
const INPUT_POLL_DELAYS_MS = [16, 0, 40, 100] as const;

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
    const bootingBackstop = window.setTimeout(() => setBooting(false), 20000);
    const markOutput = () => {
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
    const resizeObserver = new ResizeObserver(() => renderer.fit());
    resizeObserver.observe(host);

    const teardownShared = () => {
      alive = false;
      window.clearTimeout(bootingBackstop);
      if (resizeTimer !== null) {
        window.clearTimeout(resizeTimer);
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

      const pollSnapshot = async () => {
        if (polling) {
          queued = true;
          return;
        }
        polling = true;
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
          if (alive && queued) {
            void pollSnapshot();
          }
        }
      };

      const unsubscribeInput = renderer.onData((data) => {
        client
          .sendText(sessionId, data)
          .then(() => {
            // Poll promptly so the echo appears without waiting for the tick.
            queued = true;
            void pollSnapshot();
          })
          .catch(() => onError?.());
      });

      void pollSnapshot();
      const timer = window.setInterval(
        () => void pollSnapshot(),
        SNAPSHOT_POLL_MS,
      );

      return () => {
        window.clearInterval(timer);
        unsubscribeInput();
        teardownShared();
      };
    }

    // --- readRecent polling fallback (preview / server clients) ---
    let renderedText = "";
    let pollInFlight = false;
    let pollQueued = false;
    const pendingPollTimers = new Set<number>();

    const poll = async () => {
      if (pollInFlight) {
        pollQueued = true;
        return;
      }
      pollInFlight = true;
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
            }
            continue;
          }
          const next = text.slice(renderedText.length);
          renderedText = text;
          if (next.length > 0) {
            renderer.write(encoder.encode(next));
          }
        } while (alive && pollQueued);
      } catch {
        onError?.();
      } finally {
        pollInFlight = false;
        if (alive && pollQueued) {
          void poll();
        }
      }
    };

    const schedulePoll = (delayMs: number) => {
      if (!alive) {
        return;
      }
      if (delayMs <= 0) {
        void poll();
        return;
      }
      const timer = window.setTimeout(() => {
        pendingPollTimers.delete(timer);
        void poll();
      }, delayMs);
      pendingPollTimers.add(timer);
    };

    const unsubscribeInput = renderer.onData((data) => {
      schedulePoll(INPUT_POLL_DELAYS_MS[0]);
      client
        .sendText(sessionId, data)
        .then(() => {
          INPUT_POLL_DELAYS_MS.slice(1).forEach(schedulePoll);
        })
        .catch(() => onError?.());
    });

    void poll();
    const timer = window.setInterval(() => void poll(), POLL_INTERVAL_MS);

    return () => {
      window.clearInterval(timer);
      pendingPollTimers.forEach((pendingTimer) =>
        window.clearTimeout(pendingTimer),
      );
      pendingPollTimers.clear();
      unsubscribeInput();
      teardownShared();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, sessionId]);

  useEffect(() => {
    if (active) {
      rendererRef.current?.focus();
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
