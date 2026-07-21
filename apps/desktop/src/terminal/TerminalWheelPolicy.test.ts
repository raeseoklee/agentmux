import { describe, expect, it } from "vitest";
import { decideTerminalWheelAction } from "./TerminalWheelPolicy";

describe("decideTerminalWheelAction", () => {
  it("scrolls normal-buffer conversation history locally", () => {
    expect(
      decideTerminalWheelAction({
        bufferType: "normal",
        hasScrollback: true,
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
        hasScrollback: false,
        canScroll: false,
        alternateWheelMode: "auto",
        mouseTracking: false,
      }),
    ).toBe("consume");
  });

  it("uses Codex paging when an inline repaint leaves no local scrollback", () => {
    expect(
      decideTerminalWheelAction({
        bufferType: "normal",
        hasScrollback: false,
        canScroll: false,
        alternateWheelMode: "page",
        mouseTracking: false,
      }),
    ).toBe("page");
  });

  it("does not leak paging keys at a real local scrollback boundary", () => {
    expect(
      decideTerminalWheelAction({
        bufferType: "normal",
        hasScrollback: true,
        canScroll: false,
        alternateWheelMode: "page",
        mouseTracking: false,
      }),
    ).toBe("consume");
  });

  it("routes Codex wheel input through its transcript overlay", () => {
    expect(
      decideTerminalWheelAction({
        bufferType: "normal",
        hasScrollback: true,
        canScroll: true,
        alternateWheelMode: "codex",
        mouseTracking: false,
      }),
    ).toBe("codex-transcript");
  });

  it("keeps the Codex transcript route when an alternate buffer tracks the mouse", () => {
    expect(
      decideTerminalWheelAction({
        bufferType: "alternate",
        hasScrollback: false,
        canScroll: false,
        alternateWheelMode: "codex",
        mouseTracking: true,
      }),
    ).toBe("codex-transcript");
  });

  it("preserves mouse tracking for a true alternate-screen TUI", () => {
    expect(
      decideTerminalWheelAction({
        bufferType: "alternate",
        hasScrollback: false,
        canScroll: false,
        alternateWheelMode: "auto",
        mouseTracking: true,
      }),
    ).toBe("passthrough");
  });

  it("lets a page-mode agent handle tracked wheel input itself", () => {
    expect(
      decideTerminalWheelAction({
        bufferType: "alternate",
        hasScrollback: false,
        canScroll: false,
        alternateWheelMode: "page",
        mouseTracking: true,
      }),
    ).toBe("passthrough");
  });

  it("uses the configured alternate-screen paging policy", () => {
    expect(
      decideTerminalWheelAction({
        bufferType: "alternate",
        hasScrollback: false,
        canScroll: false,
        alternateWheelMode: "page",
        mouseTracking: false,
      }),
    ).toBe("page");
  });
});
