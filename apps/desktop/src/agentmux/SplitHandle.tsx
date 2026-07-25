import {
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  useEffect,
  useRef,
  useState,
} from "react";
import { notifyNativeBrowserLayoutChanged } from "./nativeBrowser";

export function SplitHandle({
  vertical,
  spacing = 6,
  onResize,
}: {
  vertical: boolean;
  spacing?: number;
  onResize: (ratio: number) => void;
}) {
  const [hovered, setHovered] = useState(false);
  const dragging = useRef(false);
  const resizeFrame = useRef<number | null>(null);
  const pendingRatio = useRef(0.5);

  useEffect(
    () => () => {
      if (resizeFrame.current !== null) {
        window.cancelAnimationFrame(resizeFrame.current);
      }
    },
    [],
  );

  function computeRatio(clientX: number, clientY: number, parent: HTMLElement): number {
    const rect = parent.getBoundingClientRect();
    const raw = vertical
      ? (clientX - rect.left) / rect.width
      : (clientY - rect.top) / rect.height;
    return Math.min(0.9, Math.max(0.1, raw));
  }

  function applyOptimisticRatio(target: HTMLElement, ratio: number) {
    const before = target.previousElementSibling as HTMLElement | null;
    const after = target.nextElementSibling as HTMLElement | null;
    if (before) before.style.flex = `${ratio} 1 0`;
    if (after) after.style.flex = `${1 - ratio} 1 0`;
    // Native WebView2 children are positioned outside React's layout tree.
    // Notify them in the same pointer frame so an old, wider browser view
    // cannot temporarily paint over the neighboring terminal.
    notifyNativeBrowserLayoutChanged();
  }

  function queueResize(ratio: number) {
    pendingRatio.current = ratio;
    if (resizeFrame.current !== null) return;
    resizeFrame.current = window.requestAnimationFrame(() => {
      resizeFrame.current = null;
      onResize(pendingRatio.current);
    });
  }

  function handlePointerDown(e: ReactPointerEvent) {
    (e.target as Element).setPointerCapture(e.pointerId);
    dragging.current = true;
    const parent = (e.currentTarget as HTMLElement).parentElement;
    if (!parent) return;
    const ratio = computeRatio(e.clientX, e.clientY, parent);
    applyOptimisticRatio(e.currentTarget as HTMLElement, ratio);
    queueResize(ratio);
  }

  function handlePointerMove(e: ReactPointerEvent) {
    if (!dragging.current) return;
    const parent = (e.currentTarget as HTMLElement).parentElement;
    if (!parent) return;
    const ratio = computeRatio(e.clientX, e.clientY, parent);
    applyOptimisticRatio(e.currentTarget as HTMLElement, ratio);
    queueResize(ratio);
  }

  function handlePointerUp(e: ReactPointerEvent) {
    (e.target as Element).releasePointerCapture(e.pointerId);
    dragging.current = false;
    const parent = (e.currentTarget as HTMLElement).parentElement;
    if (parent) {
      const ratio = computeRatio(e.clientX, e.clientY, parent);
      applyOptimisticRatio(e.currentTarget as HTMLElement, ratio);
      pendingRatio.current = ratio;
    }
    if (resizeFrame.current !== null) {
      window.cancelAnimationFrame(resizeFrame.current);
      resizeFrame.current = null;
    }
    onResize(pendingRatio.current);
  }

  function handlePointerCancel() {
    if (!dragging.current) return;
    dragging.current = false;
    if (resizeFrame.current !== null) {
      window.cancelAnimationFrame(resizeFrame.current);
      resizeFrame.current = null;
    }
    onResize(pendingRatio.current);
  }

  const hitSize = Math.max(6, spacing);
  const layoutOffset = (spacing - hitSize) / 2;
  const barStyle: CSSProperties = vertical
    ? {
        flex: "none",
        width: hitSize,
        marginLeft: layoutOffset,
        marginRight: layoutOffset,
        alignSelf: "stretch",
        cursor: "col-resize",
        background: hovered ? "var(--accent-soft)" : "transparent",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        touchAction: "none",
        userSelect: "none",
      }
    : {
        flex: "none",
        height: hitSize,
        marginTop: layoutOffset,
        marginBottom: layoutOffset,
        alignSelf: "stretch",
        cursor: "row-resize",
        background: hovered ? "var(--accent-soft)" : "transparent",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        touchAction: "none",
        userSelect: "none",
      };

  const gripStyle: CSSProperties = vertical
    ? {
        width: 2,
        height: 24,
        borderRadius: 1,
        background: "var(--border-strong)",
        flexShrink: 0,
      }
    : {
        width: 24,
        height: 2,
        borderRadius: 1,
        background: "var(--border-strong)",
        flexShrink: 0,
      };

  return (
    <div
      data-agentmux-split-handle={vertical ? "vertical" : "horizontal"}
      data-agentmux-split-spacing={spacing}
      style={barStyle}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={handlePointerCancel}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <span style={gripStyle} />
    </div>
  );
}
