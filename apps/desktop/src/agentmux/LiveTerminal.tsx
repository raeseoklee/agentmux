import { useEffect, useRef } from "react";
import type { ControlClient } from "../control/ControlClient";
import { XtermTerminalRenderer } from "../terminal/XtermTerminalRenderer";

const encoder = new TextEncoder();
const POLL_INTERVAL_MS = 120;

interface LiveTerminalProps {
  client: ControlClient;
  sessionId: string;
  active: boolean;
  onFocus?: () => void;
  onError?: () => void;
}

// A self-contained, live xterm terminal bound to one backend session. Multiple
// instances can render simultaneously (one per mosaic pane) — each owns its own
// renderer, poll loop, and delta cursor, mirroring the App.tsx terminal recipe.
export function LiveTerminal({ client, sessionId, active, onFocus, onError }: LiveTerminalProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const rendererRef = useRef<XtermTerminalRenderer | null>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) {
      return;
    }

    const renderer = new XtermTerminalRenderer();
    renderer.mount(host, { columns: 120, rows: 30, bytes: encoder.encode("") });
    rendererRef.current = renderer;
    let renderedLength = 0;
    let alive = true;

    const unsubscribeInput = renderer.onData((data) => {
      client.sendText(sessionId, data).catch(() => onError?.());
    });
    const unsubscribeResize = renderer.onResize((columns, rows) => {
      client.resize(sessionId, columns, rows).catch(() => onError?.());
    });
    const resizeObserver = new ResizeObserver(() => renderer.fit());
    resizeObserver.observe(host);

    const poll = async () => {
      try {
        const text = await client.readRecent(sessionId, 65536);
        if (!alive) {
          return;
        }
        const next = text.slice(renderedLength);
        renderedLength = text.length;
        if (next.length > 0) {
          renderer.write(encoder.encode(next));
        }
      } catch {
        onError?.();
      }
    };

    void poll();
    const timer = window.setInterval(() => void poll(), POLL_INTERVAL_MS);

    return () => {
      alive = false;
      window.clearInterval(timer);
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

  return (
    <div
      ref={hostRef}
      onMouseDown={onFocus}
      style={{ height: "100%", width: "100%", minHeight: 0, minWidth: 0 }}
    />
  );
}
