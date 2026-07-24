import { describe, expect, it } from "vitest";
import {
  isMarkdownDocumentLink,
  resolveRelativeDocumentPath,
} from "./MarkdownSurfacePanel";

describe("MarkdownSurfacePanel paths", () => {
  it("recognizes supported Markdown links with fragments", () => {
    expect(isMarkdownDocumentLink("../README.md#setup")).toBe(true);
    expect(isMarkdownDocumentLink("guide.markdown?plain=1")).toBe(true);
    expect(isMarkdownDocumentLink("image.png")).toBe(false);
  });

  it("resolves Windows relative links without dropping a directory", () => {
    expect(
      resolveRelativeDocumentPath(
        String.raw`D:\Workspace\agentmux\docs\guide.md`,
        "../README.md#top",
      ),
    ).toBe(String.raw`D:\Workspace\agentmux\README.md`);
  });

  it("resolves WSL relative links and preserves the leading slash", () => {
    expect(
      resolveRelativeDocumentPath(
        "/home/irae/agentmux/docs/guide.md",
        "../README.md",
      ),
    ).toBe("/home/irae/agentmux/README.md");
  });
});
