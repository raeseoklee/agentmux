import { describe, expect, it } from "vitest";
import { buildServerWebSocketUrl } from "./ControlClient";

describe("server WebSocket credential transport", () => {
  it("uses a short-lived ticket without putting the reusable server token in the URL", () => {
    const url = new URL(
      buildServerWebSocketUrl(
        "http://127.0.0.1:8765",
        "/api/session/session-1/stream",
        "wst_single_use",
      ),
    );

    expect(url.protocol).toBe("ws:");
    expect(url.searchParams.get("ticket")).toBe("wst_single_use");
    expect(url.searchParams.has("token")).toBe(false);
    expect(url.toString()).not.toContain("srv_reusable");
  });

  it("preserves TLS when constructing the WebSocket endpoint", () => {
    expect(
      buildServerWebSocketUrl(
        "https://agentmux.example",
        "/api/session/session-1/stream",
        "wst_single_use",
      ),
    ).toMatch(/^wss:\/\/agentmux\.example\//);
  });
});
