import { describe, expect, it } from "vitest";
import { isRecoverableGitSnapshotError } from "./SourceControlPanel";

describe("source control snapshot recovery", () => {
  it.each([
    "Git status scan was cancelled.",
    "Git status scan was canceled.",
    "Git status snapshot is no longer available; refresh page zero.",
    "Git repository changed while its status snapshot was loading; refresh and retry.",
    "Git repository generation changed to 8; refresh and retry.",
  ])("treats transient snapshot failure as recoverable: %s", (message) => {
    expect(isRecoverableGitSnapshotError(new Error(message))).toBe(true);
  });

  it("does not retry a full Git command timeout", () => {
    expect(
      isRecoverableGitSnapshotError(
        new Error("read repository status timed out after 90 seconds"),
      ),
    ).toBe(false);
  });

  it("keeps unrelated Git failures visible", () => {
    expect(isRecoverableGitSnapshotError(new Error("Git authentication failed"))).toBe(false);
  });
});
