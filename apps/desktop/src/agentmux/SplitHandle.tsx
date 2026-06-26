import { type CSSProperties, type PointerEvent as ReactPointerEvent, useRef, useState } from "react";

export function SplitHandle({ vertical, onResize }: { vertical: boolean; onResize: (ratio: number) => void }) {
  const [hovered, setHovered] = useState(false);
  const dragging = useRef(false);

  function computeRatio(clientX: number, clientY: number, parent: HTMLElement): number {
    const rect = parent.getBoundingClientRect();
    const raw = vertical
      ? (clientX - rect.left) / rect.width
      : (clientY - rect.top) / rect.height;
    return Math.min(0.9, Math.max(0.1, raw));
  }

  function handlePointerDown(e: ReactPointerEvent) {
    (e.target as Element).setPointerCapture(e.pointerId);
    dragging.current = true;
    const parent = (e.currentTarget as HTMLElement).parentElement;
    if (!parent) return;
    onResize(computeRatio(e.clientX, e.clientY, parent));
  }

  function handlePointerMove(e: ReactPointerEvent) {
    if (!dragging.current) return;
    const parent = (e.currentTarget as HTMLElement).parentElement;
    if (!parent) return;
    onResize(computeRatio(e.clientX, e.clientY, parent));
  }

  function handlePointerUp(e: ReactPointerEvent) {
    (e.target as Element).releasePointerCapture(e.pointerId);
    dragging.current = false;
  }

  const barStyle: CSSProperties = vertical
    ? {
        flex: "none",
        width: 6,
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
        height: 6,
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
      style={barStyle}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <span style={gripStyle} />
    </div>
  );
}
