import { describe, expect, it } from "vitest";
import { decideTerminalWheelAction } from "./TerminalWheelPolicy";

describe("decideTerminalWheelAction", () => {
  it("scrolls normal-buffer conversation history locally", () => {
    expect(
      decideTerminalWheelAction({
        bufferType: "normal",
        canScroll: true,
        alternateWheelMode: "page",
        mouseTracking: false,
      }),
    ).toBe("scrollback");
  });

  it("does not turn a normal-buffer wheel gesture into PTY input", () => {
    expect(
      decideTerminalWheelAction({
        bufferType: "normal",
        canScroll: false,
        alternateWheelMode: "auto",
        mouseTracking: false,
      }),
    ).toBe("consume");
  });

  it("preserves mouse tracking for a true alternate-screen TUI", () => {
    expect(
      decideTerminalWheelAction({
        bufferType: "alternate",
        canScroll: false,
        alternateWheelMode: "auto",
        mouseTracking: true,
      }),
    ).toBe("passthrough");
  });

  it("uses the configured alternate-screen paging policy", () => {
    expect(
      decideTerminalWheelAction({
        bufferType: "alternate",
        canScroll: false,
        alternateWheelMode: "page",
        mouseTracking: false,
      }),
    ).toBe("page");
  });
});
