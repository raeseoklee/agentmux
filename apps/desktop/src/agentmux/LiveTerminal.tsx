import { useEffect, useRef } from "react";
import type { ControlClient } from "../control/ControlClient";
import {
  XtermTerminalRenderer,
  XTERM_THEME,
} from "../terminal/XtermTerminalRenderer";

const encoder = new TextEncoder();
const POLL_INTERVAL_MS = 120;
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
// renderer, poll loop, and delta cursor, mirroring the App.tsx terminal recipe.
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

  useEffect(() => {
    const host = hostRef.current;
    if (!host) {
      return;
    }

    const renderer = new XtermTerminalRenderer();
    renderer.mount(host, { columns: 120, rows: 30, bytes: encoder.encode("") });
    rendererRef.current = renderer;
    let renderedText = "";
    let alive = true;
    let pollInFlight = false;
    let pollQueued = false;
    let resizeTimer: number | null = null;
    let pendingResize: { columns: number; rows: number } | null = null;
    let lastResizeSent = { columns: 120, rows: 30 };
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
    const unsubscribeResize = renderer.onResize((columns, rows) => {
      if (
        columns === lastResizeSent.columns &&
        rows === lastResizeSent.rows
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
        lastResizeSent = next;
        client
          .resize(sessionId, next.columns, next.rows)
          .catch(() => onError?.());
      }, 80);
    });
    const resizeObserver = new ResizeObserver(() => renderer.fit());
    resizeObserver.observe(host);

    void poll();
    const timer = window.setInterval(() => void poll(), POLL_INTERVAL_MS);

    return () => {
      alive = false;
      window.clearInterval(timer);
      if (resizeTimer !== null) {
        window.clearTimeout(resizeTimer);
      }
      pendingPollTimers.forEach((pendingTimer) =>
        window.clearTimeout(pendingTimer),
      );
      pendingPollTimers.clear();
      unsubscribeInput();
      unsubscribeResize();
      resizeObserver.disconnect();
      renderer.dispose();
      if (rendererRef.current === renderer) {
        rendererRef.current = null;
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, sessionId]);

  useEffect(() => {
    if (active) {
      rendererRef.current?.focus();
    }
  }, [active]);

  // Visible-only GPU rendering. WebView2/Chromium caps the number of live
  // WebGL contexts (~16), so a multiplexer must not hand every pane its own
  // context. Enable WebGL only while this pane is the active one, and dispose
  // the context as soon as it goes inactive. Keyed on sessionId/client so a
  // renderer recreated by the mount effect (session swap) still gets WebGL
  // when this pane is active. The renderer stays mounted either way; we only
  // toggle the addon. enable/disableWebgl are no-ops if WebGL is unavailable.
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
      // Drop the GPU context when this pane deactivates or unmounts. On full
      // unmount renderer.dispose() also disposes the addon, but this keeps the
      // context-count low immediately on deactivation without waiting for the
      // mount effect's cleanup.
      renderer.disableWebgl();
    };
  }, [active, sessionId, client]);

  return (
    <div
      onMouseDown={onFocus}
      data-agentmux-terminal-inner-margin={margin}
      style={{
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
    </div>
  );
}
