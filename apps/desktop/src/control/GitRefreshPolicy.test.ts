import { describe, expect, it } from "vitest";
import {
  nextGitRefreshDelay,
  shouldRefreshForGitEvent,
  shouldReloadGitPage,
} from "./GitRefreshPolicy";

describe("Git refresh policy", () => {
  it("only accepts newer events for the visible repository", () => {
    expect(shouldRefreshForGitEvent({ workspace_id: "w", repository_id: "r", generation: 5 }, "w", "r", 4)).toBe(true);
    expect(shouldRefreshForGitEvent({ workspace_id: "other", repository_id: "r", generation: 5 }, "w", "r", 4)).toBe(false);
    expect(shouldRefreshForGitEvent({ workspace_id: "w", repository_id: "other", generation: 5 }, "w", "r", 4)).toBe(false);
    expect(shouldRefreshForGitEvent({ workspace_id: "w", repository_id: "r", generation: 4 }, "w", "r", 4)).toBe(false);
  });

  it("coalesces bursts and detects paging generation changes", () => {
    expect(nextGitRefreshDelay(1_100, 1_000, 140)).toBe(40);
    expect(nextGitRefreshDelay(1_200, 1_000, 140)).toBe(0);
    expect(shouldReloadGitPage(3, 4)).toBe(true);
    expect(shouldReloadGitPage(4, 4)).toBe(false);
  });
});
