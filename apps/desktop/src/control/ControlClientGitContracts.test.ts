import { describe, expect, it } from "vitest";
import {
  createGitMutationParams,
  createSourceControlActionIdempotencyKey,
  mapServerGitCommitResult,
  mapDevelopmentServerCandidate,
  serverSupportsSourceControl,
  serverSupportsSourceControlMethod,
} from "./ControlClient";

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

describe("server source-control capability handshake", () => {
  it("enables local Git only when the server probe reports success", () => {
    expect(serverSupportsSourceControl({ source_control: true }, "local")).toBe(true);
    expect(serverSupportsSourceControl({ source_control: false }, "local")).toBe(false);
    expect(serverSupportsSourceControl(undefined, "local")).toBe(false);
  });

  it("keeps compatibility with desktop-bridge servers predating capabilities", () => {
    expect(serverSupportsSourceControl(undefined, "desktop-bridge")).toBe(true);
  });

  it("uses the advertised method list instead of a single source-control switch", () => {
    const capabilities = {
      source_control: false,
      control_methods: ["git.status_page", "git.status_summary", "git.diff"],
    };

    expect(serverSupportsSourceControl(capabilities, "local")).toBe(true);
    expect(
      serverSupportsSourceControlMethod(
        capabilities,
        "git.review_thread.list",
        "local",
      ),
    ).toBe(false);
    expect(
      serverSupportsSourceControlMethod(capabilities, "git.diff", "local"),
    ).toBe(true);
  });

  it("maps desktop-bridge and local-server commit responses", () => {
    expect(mapServerGitCommitResult({ commit: "abc123", summary: "created" }, "message"))
      .toEqual({ commit: "abc123", summary: "created" });
    expect(mapServerGitCommitResult({ commit_oid: "def456" }, " local commit "))
      .toEqual({ commit: "def456", summary: "local commit" });
    expect(() => mapServerGitCommitResult({ commit_oid: null }, "message"))
      .toThrow("did not return the created commit");
  });
});

describe("durable Git mutation payloads", () => {
  it("binds the repository, pane, and stable action key into each mutation", () => {
    expect(
      createGitMutationParams(
        "workspace-1",
        {
          repositoryId: "repository-1",
          paneId: "pane-1",
          idempotencyKey: "action-1",
        },
        { message: "commit this", amend: false },
      ),
    ).toEqual({
      workspace_id: "workspace-1",
      repository_id: "repository-1",
      pane_id: "pane-1",
      idempotency_key: "action-1",
      message: "commit this",
      amend: false,
    });
  });

  it("keeps retries stable while assigning a fresh key to a new action", () => {
    const base = {
      method: "git.stage" as const,
      workspaceId: "workspace-1",
      repositoryId: "repository-1",
      paneId: "pane-1",
    };
    const retry = createSourceControlActionIdempotencyKey({
      ...base,
      actionId: "panel-1",
    });

    expect(
      createSourceControlActionIdempotencyKey({
        ...base,
        actionId: "panel-1",
      }),
    ).toBe(retry);
    expect(
      createSourceControlActionIdempotencyKey({
        ...base,
        actionId: "panel-2",
      }),
    ).not.toBe(retry);
  });
});
