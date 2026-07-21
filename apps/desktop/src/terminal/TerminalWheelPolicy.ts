import type { AlternateWheelMode } from "./TerminalRenderer";

export type TerminalWheelAction =
  | "scrollback"
  | "consume"
  | "passthrough"
  | "page"
  | "codex-transcript"
  | "cursor";

export interface TerminalWheelContext {
  bufferType: "normal" | "alternate";
  hasScrollback: boolean;
  canScroll: boolean;
  alternateWheelMode: AlternateWheelMode;
  mouseTracking: boolean;
}

/** Keep normal-buffer wheel gestures inside xterm instead of the PTY. */
export function decideTerminalWheelAction(
  context: TerminalWheelContext,
): TerminalWheelAction {
  if (context.alternateWheelMode === "codex") {
    return "codex-transcript";
  }

  // Claude owns a virtual conversation history. Its repaints can leave a
  // shallow xterm scrollback behind, but switching to that buffer strands the
  // wheel at the repaint boundary instead of reaching older messages.
  if (context.alternateWheelMode === "page") {
    return "page";
  }

  if (context.bufferType === "normal") {
    if (context.hasScrollback) {
      return context.canScroll ? "scrollback" : "consume";
    }
    // Inline TUIs can repaint in place while leaving xterm with no local
    // history. Page-mode agents receive explicit page navigation in that state;
    // cursor keys would edit composer history instead of the conversation.
    return "consume";
  }

  // Other full-screen TUIs that enabled mouse tracking own wheel semantics.
  // Let xterm encode and forward their native wheel report.
  if (context.mouseTracking) {
    return "passthrough";
  }
  return "cursor";
}
