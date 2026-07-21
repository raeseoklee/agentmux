import { describe, expect, it } from "vitest";
import { detectCodexTerminalScreen } from "./TerminalScreenProfile";

describe("detectCodexTerminalScreen", () => {
  it("recognizes the Codex welcome screen", () => {
    expect(
      detectCodexTerminalScreen([
        ">_ OpenAI Codex (v0.144.6)",
        "model: gpt-5.5 /model to change",
        "directory: /mnt/c/work/agentmux",
      ]),
    ).toBe(true);
  });

  it("recognizes a restored composer without the welcome banner", () => {
    expect(
      detectCodexTerminalScreen([
        "\u203a Summarize recent commits",
        "gpt-5.5 default \u00b7 /mnt/c/work/agentmux",
      ]),
    ).toBe(true);
  });

  it("recognizes the transcript overlay", () => {
    expect(detectCodexTerminalScreen(["/ T R A N S C R I P T /"])).toBe(true);
  });

  it("does not classify ordinary shell output as Codex", () => {
    expect(
      detectCodexTerminalScreen([
        "PS C:\\Workspace\\agentmux>",
        "Read the OpenAI Codex documentation before continuing.",
        "release model: gpt-5.5 default",
      ]),
    ).toBe(false);
  });
});
