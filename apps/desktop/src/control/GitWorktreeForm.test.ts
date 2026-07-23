import { describe, expect, it } from "vitest";
import {
  createAgentWorktreeIdempotencyKey,
  parseAgentWorktreeCommand,
} from "./GitWorktreeForm";

describe("agent worktree form helpers", () => {
  it("keeps quoted agent command arguments together", () => {
    expect(parseAgentWorktreeCommand('claude --append-system-prompt "review this diff"')).toEqual([
      "claude",
      "--append-system-prompt",
      "review this diff",
    ]);
  });

  it("creates a stable idempotency key for a repeated form submit", () => {
    expect(createAgentWorktreeIdempotencyKey("ws-1", "agent/api-review", "D:\\worktrees\\api review"))
      .toBe("ui-worktree:ws-1:agent/api-review:D:\\worktrees\\api-review");
  });
});
