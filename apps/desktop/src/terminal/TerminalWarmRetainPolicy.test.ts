import { describe, expect, it } from "vitest";
import {
  decideTerminalWarmRetain,
  TERMINAL_WARM_RETAIN_HIDDEN_DELAY_MS,
  TERMINAL_WARM_RETAIN_MAX_TABS,
  TERMINAL_WARM_RETAIN_TTL_MS,
  type TerminalWarmRetainTab,
} from "./TerminalWarmRetainPolicy";

const NOW = 1_000_000;

function tab(
  tabId: string,
  overrides: Partial<TerminalWarmRetainTab> = {},
): TerminalWarmRetainTab {
  return {
    tabId,
    active: false,
    lastActive: false,
    lastActiveAt: NOW - 1_000,
    hiddenSince: NOW - 1_000,
    protectedFromParking: false,
    ...overrides,
  };
}

describe("decideTerminalWarmRetain", () => {
  it("keeps the active and last-active tabs even after TTL expiry", () => {
    const decision = decideTerminalWarmRetain(
      [
        tab("active", { active: true, lastActiveAt: 0, hiddenSince: null }),
        tab("previous", { lastActive: true, lastActiveAt: 0 }),
        tab("expired", { lastActiveAt: NOW - TERMINAL_WARM_RETAIN_TTL_MS }),
      ],
      NOW,
    );

    expect([...decision.warmTabIds]).toEqual(["active", "previous"]);
    expect([...decision.parkedTabIds]).toEqual(["expired"]);
  });

  it("keeps structurally unsafe tabs mounted outside the normal cap", () => {
    const decision = decideTerminalWarmRetain(
      Array.from({ length: TERMINAL_WARM_RETAIN_MAX_TABS + 2 }, (_, index) =>
        tab(`protected-${index}`, { protectedFromParking: true }),
      ),
      NOW,
    );

    expect(decision.warmTabIds.size).toBe(TERMINAL_WARM_RETAIN_MAX_TABS + 2);
    expect(decision.parkedTabIds.size).toBe(0);
  });

  it("uses recency to enforce the capped normal warm set", () => {
    const decision = decideTerminalWarmRetain(
      Array.from({ length: TERMINAL_WARM_RETAIN_MAX_TABS + 3 }, (_, index) =>
        tab(`tab-${index}`, {
          lastActiveAt: NOW - index * 1_000,
          hiddenSince:
            NOW - TERMINAL_WARM_RETAIN_HIDDEN_DELAY_MS - index * 1_000,
        }),
      ),
      NOW,
    );

    expect([...decision.warmTabIds]).toHaveLength(TERMINAL_WARM_RETAIN_MAX_TABS);
    expect(decision.warmTabIds.has("tab-0")).toBe(true);
    expect(decision.warmTabIds.has("tab-7")).toBe(true);
    expect(decision.warmTabIds.has("tab-8")).toBe(false);
  });

  it("parks ordinary tabs at the ten minute TTL boundary", () => {
    const decision = decideTerminalWarmRetain(
      [
        tab("fresh", { lastActiveAt: NOW - TERMINAL_WARM_RETAIN_TTL_MS + 1 }),
        tab("expired", { lastActiveAt: NOW - TERMINAL_WARM_RETAIN_TTL_MS }),
      ],
      NOW,
    );

    expect(decision.warmTabIds.has("fresh")).toBe(true);
    expect(decision.parkedTabIds.has("expired")).toBe(true);
  });

  it("announces the first grace or TTL transition", () => {
    const hiddenSince = NOW - 500;
    const decision = decideTerminalWarmRetain(
      [tab("recent", { hiddenSince, lastActiveAt: NOW - 1_000 })],
      NOW,
    );

    expect(decision.nextTransitionAt).toBe(
      hiddenSince + TERMINAL_WARM_RETAIN_HIDDEN_DELAY_MS,
    );
  });

  it("gives a newly hidden tab a grace window before normal cap eviction", () => {
    const decision = decideTerminalWarmRetain(
      Array.from({ length: TERMINAL_WARM_RETAIN_MAX_TABS + 3 }, (_, index) =>
        tab(`tab-${index}`, {
          lastActiveAt: NOW - index * 1_000,
          hiddenSince: NOW - 1_000,
        }),
      ),
      NOW,
    );

    expect(decision.warmTabIds.size).toBe(TERMINAL_WARM_RETAIN_MAX_TABS + 3);
    expect(decision.nextTransitionAt).toBe(
      NOW - 1_000 + TERMINAL_WARM_RETAIN_HIDDEN_DELAY_MS,
    );
  });
});
