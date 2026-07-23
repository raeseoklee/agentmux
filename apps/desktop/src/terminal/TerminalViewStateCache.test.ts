import { describe, expect, it } from "vitest";
import { TerminalViewStateCache } from "./TerminalViewStateCache";

describe("TerminalViewStateCache", () => {
  it("preserves a session framebuffer and output cursor across remounts", () => {
    const cache = new TerminalViewStateCache();
    expect(cache.write("session-1", {
      serialized: "\u001b[31mhistory\u001b[0m",
      outputOffset: 42,
      viewportY: 18,
      updatedAt: 100,
    })).toBe(true);

    expect(cache.read("session-1")).toEqual({
      serialized: "\u001b[31mhistory\u001b[0m",
      outputOffset: 42,
      viewportY: 18,
      updatedAt: 100,
    });
  });

  it("rejects invalid viewport positions without discarding the prior snapshot", () => {
    const cache = new TerminalViewStateCache();
    cache.write("session-1", {
      serialized: "history",
      outputOffset: 4,
      viewportY: 3,
      updatedAt: 1,
    });

    expect(cache.write("session-1", {
      serialized: "new history",
      outputOffset: 5,
      viewportY: -1,
      updatedAt: 2,
    })).toBe(false);
    expect(cache.read("session-1")?.viewportY).toBe(3);
  });

  it("retains live sessions until lifecycle cleanup removes them", () => {
    const cache = new TerminalViewStateCache();
    for (let index = 0; index < 30; index += 1) {
      cache.write(`session-${index}`, {
        serialized: "x".repeat(1024),
        outputOffset: index,
        updatedAt: index,
      });
    }
    expect(cache.read("session-0")?.outputOffset).toBe(0);
    cache.deleteMany(["session-0", "session-1"]);
    expect(cache.read("session-0")).toBeNull();
    expect(cache.read("session-1")).toBeNull();
    expect(cache.read("session-29")?.outputOffset).toBe(29);
  });
});
