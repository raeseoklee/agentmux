import { describe, expect, it } from "vitest";
import { formatDroppedPaths } from "./explorerFileDrop";

describe("formatDroppedPaths", () => {
  it("converts Windows drive paths and quotes them for WSL", () => {
    expect(
      formatDroppedPaths(
        [String.raw`D:\Workspace\Agent Mux\notes' draft.md`],
        { backendKind: "wsl-direct" },
      ),
    ).toBe(String.raw`'/mnt/d/Workspace/Agent Mux/notes'\'' draft.md'`);
  });

  it("converts WSL UNC paths back to Linux paths", () => {
    expect(
      formatDroppedPaths(
        [String.raw`\\wsl.localhost\Ubuntu\home\irae\project\README.md`],
        { backendKind: "wsl-tmux-control" },
      ),
    ).toBe("'/home/irae/project/README.md'");
  });

  it("uses PowerShell single-quote escaping for ConPTY by default", () => {
    expect(
      formatDroppedPaths(
        [String.raw`C:\Users\Roy\it's ready.txt`],
        { backendKind: "conpty" },
        "powershell.exe",
      ),
    ).toBe(String.raw`'C:\Users\Roy\it''s ready.txt'`);
  });

  it("uses cmd-compatible double quotes when the surface is cmd", () => {
    expect(
      formatDroppedPaths(
        [String.raw`C:\Program Files\AgentMux\one.txt`, String.raw`D:\two.txt`],
        { backendKind: "conpty" },
        "cmd.exe",
      ),
    ).toBe(String.raw`"C:\Program Files\AgentMux\one.txt" "D:\two.txt"`);
  });
});
