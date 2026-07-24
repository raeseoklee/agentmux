import { describe, expect, it } from "vitest";
import {
  ACCENTS,
  accentForeground,
  buildRootVars,
  customAccentHex,
  customAccentKey,
  normalizeAccentHex,
  resolveAccent,
} from "./theme";

describe("custom accent colors", () => {
  it("normalizes supported HEX forms and rejects incomplete colors", () => {
    expect(normalizeAccentHex(" #1a2b3c ")).toBe("#1A2B3C");
    expect(normalizeAccentHex("#abc")).toBe("#AABBCC");
    expect(normalizeAccentHex("#12")).toBeNull();
    expect(normalizeAccentHex("red")).toBeNull();
  });

  it("round-trips a custom color through the persisted accent key", () => {
    const key = customAccentKey("#123456");
    expect(key).toBe("custom:#123456");
    expect(customAccentHex(key ?? "")).toBe("#123456");
    expect(resolveAccent(key ?? "")).toMatchObject({
      key: "custom:#123456",
      hex: "#123456",
      soft: "rgba(18,52,86,0.16)",
    });
  });

  it("falls back to Azure for an unsupported config value", () => {
    expect(resolveAccent("not-a-color")).toBe(ACCENTS[0]);
  });

  it("chooses a readable foreground for bright and dark custom accents", () => {
    expect(accentForeground("#FFFFFF")).toBe("#0A0A0B");
    expect(accentForeground("#FFFF00")).toBe("#0A0A0B");
    expect(accentForeground("#000000")).toBe("#FFFFFF");
    expect(accentForeground("#12002B")).toBe("#FFFFFF");
  });

  it("exposes the computed foreground as a root theme token", () => {
    const vars = buildRootVars(
      {
        bg: "#000000",
        canvas: "#000000",
        surface: "#000000",
        s2: "#000000",
        s3: "#000000",
        border: "#000000",
        borderStrong: "#000000",
        borderSubtle: "#000000",
        fg1: "#FFFFFF",
        fg2: "#FFFFFF",
        fg3: "#FFFFFF",
        fg4: "#FFFFFF",
        term: "#000000",
        desk: "#000000",
        green: "#00FF00",
        red: "#FF0000",
        warn: "#FFFF00",
        info: "#0000FF",
        cyan: "#00FFFF",
        shadow: "none",
      },
      resolveAccent("custom:#FFFFFF"),
      14,
    ) as Record<string, string>;
    expect(vars["--accent-fg"]).toBe("#0A0A0B");
  });
});
