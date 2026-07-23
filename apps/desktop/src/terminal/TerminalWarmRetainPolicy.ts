export const TERMINAL_WARM_RETAIN_HIDDEN_DELAY_MS = 30_000;
export const TERMINAL_WARM_RETAIN_MAX_TABS = 8;
export const TERMINAL_WARM_RETAIN_TTL_MS = 10 * 60_000;

export interface TerminalWarmRetainTab {
  tabId: string;
  active: boolean;
  lastActive: boolean;
  lastActiveAt: number | null;
  hiddenSince: number | null;
  /** Tabs that cannot safely be reconstructed from a terminal framebuffer. */
  protectedFromParking: boolean;
}

export interface TerminalWarmRetainDecision {
  warmTabIds: ReadonlySet<string>;
  parkedTabIds: ReadonlySet<string>;
  /** The next time the caller should re-evaluate the decision, if any. */
  nextTransitionAt: number | null;
}

function isFiniteTimestamp(value: number | null): value is number {
  return value !== null && Number.isFinite(value) && value >= 0;
}

function recency(tab: TerminalWarmRetainTab): number {
  return tab.lastActiveAt ?? tab.hiddenSince ?? 0;
}

/**
 * Choose the tab trees that retain live xterm instances. Active, last-active,
 * and structurally unsafe tabs are hard exceptions; all other tabs share a
 * bounded LRU budget. A newly hidden tab receives a 30 second grace window
 * before it enters that budget, and otherwise remains warm for at most ten
 * minutes. Explicit safety exceptions may temporarily exceed the normal cap.
 */
export function decideTerminalWarmRetain(
  tabs: readonly TerminalWarmRetainTab[],
  now: number,
): TerminalWarmRetainDecision {
  const warmTabIds = new Set<string>();
  const protectedTabs: TerminalWarmRetainTab[] = [];
  const graceTabs: TerminalWarmRetainTab[] = [];
  const candidates: TerminalWarmRetainTab[] = [];
  let nextTransitionAt: number | null = null;

  const scheduleAt = (timestamp: number) => {
    if (timestamp <= now) {
      return;
    }
    nextTransitionAt =
      nextTransitionAt === null ? timestamp : Math.min(nextTransitionAt, timestamp);
  };

  for (const tab of tabs) {
    if (tab.active || tab.lastActive || tab.protectedFromParking) {
      protectedTabs.push(tab);
      warmTabIds.add(tab.tabId);
      continue;
    }

    const lastActiveAt = recency(tab);
    if (now - lastActiveAt >= TERMINAL_WARM_RETAIN_TTL_MS) {
      continue;
    }
    if (
      isFiniteTimestamp(tab.hiddenSince) &&
      now - tab.hiddenSince < TERMINAL_WARM_RETAIN_HIDDEN_DELAY_MS
    ) {
      graceTabs.push(tab);
      warmTabIds.add(tab.tabId);
      scheduleAt(tab.hiddenSince + TERMINAL_WARM_RETAIN_HIDDEN_DELAY_MS);
      continue;
    }
    candidates.push(tab);
  }

  candidates.sort((left, right) => recency(right) - recency(left));
  const candidateBudget = Math.max(
    0,
    TERMINAL_WARM_RETAIN_MAX_TABS - protectedTabs.length - graceTabs.length,
  );

  for (const [index, tab] of candidates.entries()) {
    if (index >= candidateBudget) {
      continue;
    }
    warmTabIds.add(tab.tabId);
    scheduleAt(recency(tab) + TERMINAL_WARM_RETAIN_TTL_MS);
    if (isFiniteTimestamp(tab.hiddenSince)) {
      scheduleAt(tab.hiddenSince + TERMINAL_WARM_RETAIN_HIDDEN_DELAY_MS);
    }
  }

  const parkedTabIds = new Set(
    tabs.filter((tab) => !warmTabIds.has(tab.tabId)).map((tab) => tab.tabId),
  );
  return { warmTabIds, parkedTabIds, nextTransitionAt };
}
