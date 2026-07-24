import { describe, expect, it } from "vitest";
import {
  normalizeBrowserNavigationUrl,
  summarizeBrowserDom,
} from "./BrowserSurfacePanel";

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
});
