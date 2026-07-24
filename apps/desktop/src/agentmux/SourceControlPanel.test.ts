import { describe, expect, it } from "vitest";
import { isRecoverableGitSnapshotError } from "./SourceControlPanel";

describe("source control snapshot recovery", () => {
  it.each([
    "Git status scan was cancelled.",
    "Git status scan was canceled.",
    "Git status snapshot is no longer available; refresh page zero.",
    "Git repository changed while its status snapshot was loading; refresh and retry.",
  ])("treats transient snapshot failure as recoverable: %s", (message) => {
    expect(isRecoverableGitSnapshotError(new Error(message))).toBe(true);
  });

  it("keeps unrelated Git failures visible", () => {
    expect(isRecoverableGitSnapshotError(new Error("Git authentication failed"))).toBe(false);
  });
});
