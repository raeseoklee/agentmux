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

  if (context.bufferType === "normal") {
    if (context.hasScrollback) {
      return context.canScroll ? "scrollback" : "consume";
    }
    // Codex can repaint in place even with --no-alt-screen, leaving xterm's
    // normal buffer with no local history. Its page keys are the only useful
    // fallback in that state; cursor keys would edit composer history.
    return context.alternateWheelMode === "page" ? "page" : "consume";
  }

  // A full-screen TUI that enabled mouse tracking owns wheel semantics. Let
  // xterm encode the native wheel report even when the fallback mode is page;
  // synthetic PageUp bypasses apps such as Codex that scroll their own history.
  if (context.mouseTracking) {
    return "passthrough";
  }
  return context.alternateWheelMode === "page" ? "page" : "cursor";
}
