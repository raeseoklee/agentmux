import type { AlternateWheelMode } from "./TerminalRenderer";

export type TerminalWheelAction =
  | "scrollback"
  | "consume"
  | "passthrough"
  | "page"
  | "cursor";

export interface TerminalWheelContext {
  bufferType: "normal" | "alternate";
  canScroll: boolean;
  alternateWheelMode: AlternateWheelMode;
  mouseTracking: boolean;
}

/** Keep normal-buffer wheel gestures inside xterm instead of the PTY. */
export function decideTerminalWheelAction(
  context: TerminalWheelContext,
): TerminalWheelAction {
  if (context.bufferType === "normal") {
    return context.canScroll ? "scrollback" : "consume";
  }

  if (context.alternateWheelMode !== "page" && context.mouseTracking) {
    return "passthrough";
  }
  return context.alternateWheelMode === "page" ? "page" : "cursor";
}
