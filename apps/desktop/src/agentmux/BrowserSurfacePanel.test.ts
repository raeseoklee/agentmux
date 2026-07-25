import { describe, expect, it } from "vitest";
import {
  normalizeBrowserNavigationUrl,
  summarizeBrowserDom,
} from "./BrowserSurfacePanel";
import { normalizeBrowserScrollDelta } from "../control/ControlClient";

describe("browser surface helpers", () => {
  it("normalizes a user-entered host without using an invalid placeholder", () => {
    expect(normalizeBrowserNavigationUrl("")).toBeNull();
    expect(normalizeBrowserNavigationUrl("example.com/docs")).toBe("https://example.com/docs");
    expect(normalizeBrowserNavigationUrl("localhost:5173")).toBe("http://localhost:5173");
    expect(normalizeBrowserNavigationUrl("about:blank")).toBe("about:blank");
  });

  it("creates a safe readable preview instead of treating page HTML as an interactive document", () => {
    const preview = summarizeBrowserDom(`
      <html><head><title>Example &amp; Docs</title><style>.hidden { display: none; }</style></head>
      <body><h1>Welcome</h1><p>Read <strong>this</strong> page.</p><script>alert('nope')</script></body></html>
    `);

    expect(preview).toEqual({
      title: "Example & Docs",
      text: "Welcome\nRead this page.",
      truncated: false,
    });
  });

  it("marks an oversized page snapshot as truncated", () => {
    const preview = summarizeBrowserDom(`<html><body>${"x".repeat(12_001)}</body></html>`);

    expect(preview.text).toHaveLength(12_000);
    expect(preview.truncated).toBe(true);
  });

  it("converts high-resolution wheel deltas to the integer IPC contract", () => {
    expect(normalizeBrowserScrollDelta(18.75)).toBe(18);
    expect(normalizeBrowserScrollDelta(-9.25)).toBe(-9);
    expect(normalizeBrowserScrollDelta(Number.NaN)).toBeNull();
    expect(normalizeBrowserScrollDelta(null)).toBeNull();
  });
});
