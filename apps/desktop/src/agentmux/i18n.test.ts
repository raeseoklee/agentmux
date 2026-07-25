import { describe, expect, it } from "vitest";

import { createTranslator } from "./i18n";

describe("Korean product terminology", () => {
  it("keeps shared product concepts aligned with CLI and documentation", () => {
    const translate = createTranslator("ko");

    expect(translate("workspace.section")).toBe("Workspace");
    expect(translate("surface.tab.rename")).toContain("Tab");
    expect(translate("pane.empty")).toContain("Pane");
    expect(translate("action.terminal.new")).toContain("Terminal");
    expect(translate("settings.terminalSplitBehaviorHint")).toContain(
      "Pane",
    );
    expect(translate("sourceControl.notRepository")).toBe(
      "Git \uC800\uC7A5\uC18C\uAC00 \uC544\uB2D9\uB2C8\uB2E4",
    );
  });
});
