import { afterEach, describe, expect, it, vi } from "vitest";
import {
  createControlClient,
  createGitMutationParams,
  createSourceControlActionIdempotencyKey,
  mapServerGitCommitResult,
  mapDevelopmentServerCandidate,
  normalizeGitStatusPageFilter,
  serverSupportsSourceControl,
  serverSupportsSourceControlMethod,
} from "./ControlClient";

const gitStatusPageResult = {
  workspace_id: "workspace-1",
  repository_id: "repository-1",
  generation: 1,
  changes: [],
};

afterEach(() => {
  vi.unstubAllGlobals();
});

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
  it("normalizes blank status filters before IPC validation", () => {
    expect(normalizeGitStatusPageFilter(undefined)).toBeNull();
    expect(normalizeGitStatusPageFilter(null)).toBeNull();
    expect(normalizeGitStatusPageFilter("")).toBeNull();
    expect(normalizeGitStatusPageFilter("   ")).toBeNull();
    expect(normalizeGitStatusPageFilter("  src/core  ")).toBe("src/core");
  });

  it("serializes an unfiltered Tauri status request without rewriting its cursor", async () => {
    let capturedControlArgs: unknown;
    const invoke = vi.fn(async (command: string, args?: unknown) => {
      if (command === "agentmux_control_token") {
        return "control-token";
      }
      capturedControlArgs = args;
      return {
        outcome: {
          Ok: { result_json: JSON.stringify(gitStatusPageResult) },
        },
      };
    });
    vi.stubGlobal("window", { __TAURI__: { core: { invoke } } });

    await createControlClient().getGitStatusPage("workspace-1", {
      query: "   ",
      cursor: "  opaque-cursor  ",
    });

    const request = (capturedControlArgs as {
      request: { params_json: string };
    }).request;
    expect(JSON.parse(request.params_json)).toMatchObject({
      query: null,
      cursor: "  opaque-cursor  ",
    });
  });

  it("serializes an unfiltered server status request without rewriting its cursor", async () => {
    let capturedFetchInit: RequestInit | undefined;
    const fetch = vi.fn(
      async (_input: RequestInfo | URL, init?: RequestInit) => {
        capturedFetchInit = init;
        return new Response(
          JSON.stringify({ ok: true, result: gitStatusPageResult }),
          { status: 200, headers: { "Content-Type": "application/json" } },
        );
      },
    );
    vi.stubGlobal("fetch", fetch);
    vi.stubGlobal("window", {
      addEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
      __AGENTMUX_SERVER__: {
        baseUrl: "http://127.0.0.1:8765",
        mode: "local",
        capabilities: { control_methods: ["git.status_page"] },
        defaults: {},
      },
    });

    await createControlClient().getGitStatusPage("workspace-1", {
      query: "   ",
      cursor: "  opaque-cursor  ",
    });

    const request = JSON.parse(capturedFetchInit?.body as string);
    expect(request).toMatchObject({
      method: "git.status_page",
      params: { query: null, cursor: "  opaque-cursor  " },
    });
  });

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
