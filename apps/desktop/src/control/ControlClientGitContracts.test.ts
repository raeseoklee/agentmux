import { describe, expect, it } from "vitest";
import { mapDevelopmentServerCandidate } from "./ControlClient";

describe("development-server IPC contract mapping", () => {
  it("preserves candidate identity and dismissal state for preview and server clients", () => {
    expect(mapDevelopmentServerCandidate({
      candidate_id: "server-1",
      workspace_id: "workspace-1",
      session_id: "session-1",
      url: "http://127.0.0.1:5173",
      source: "vite",
      detected_at: "2026-07-23T08:00:00Z",
      dismissed: true,
    })).toEqual({
      candidateId: "server-1",
      workspaceId: "workspace-1",
      sessionId: "session-1",
      url: "http://127.0.0.1:5173",
      source: "vite",
      detectedAt: "2026-07-23T08:00:00Z",
      dismissed: true,
    });
  });
});
