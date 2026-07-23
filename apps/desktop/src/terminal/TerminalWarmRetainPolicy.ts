export const TERMINAL_WARM_RETAIN_HIDDEN_DELAY_MS = 30_000;
export const TERMINAL_WARM_RETAIN_MAX_TABS = 8;
export const TERMINAL_WARM_RETAIN_TTL_MS = 10 * 60_000;

export interface TerminalWarmRetainTab {
  tabId: string;
  active: boolean;
  lastActive: boolean;
  lastActiveAt: number | null;
  hiddenSince: number | null;
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
 * Choose the tab trees that retain live xterm instances. The active tab is a
 * hard exception; every hidden tab shares the remaining global budget. The
 * last-active tab and tabs hidden during the 30 second grace window are ranked
 * ahead of other hidden tabs, but never bypass the cap.
 */
export function decideTerminalWarmRetain(
  tabs: readonly TerminalWarmRetainTab[],
  now: number,
): TerminalWarmRetainDecision {
  const warmTabIds = new Set<string>();
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
    if (tab.active) {
      warmTabIds.add(tab.tabId);
      continue;
    }

    const lastActiveAt = recency(tab);
    if (!tab.lastActive && now - lastActiveAt >= TERMINAL_WARM_RETAIN_TTL_MS) {
      continue;
    }
    candidates.push(tab);
  }

  const isWithinGrace = (tab: TerminalWarmRetainTab) =>
    isFiniteTimestamp(tab.hiddenSince) &&
    now - tab.hiddenSince < TERMINAL_WARM_RETAIN_HIDDEN_DELAY_MS;
  candidates.sort((left, right) => {
    const priority = (tab: TerminalWarmRetainTab) =>
      (tab.lastActive ? 2 : 0) + (isWithinGrace(tab) ? 1 : 0);
    return priority(right) - priority(left) || recency(right) - recency(left);
  });
  const candidateBudget = Math.max(0, TERMINAL_WARM_RETAIN_MAX_TABS - warmTabIds.size);

  for (const [index, tab] of candidates.entries()) {
    if (index < candidateBudget) {
      warmTabIds.add(tab.tabId);
    }
    if (!tab.lastActive) {
      scheduleAt(recency(tab) + TERMINAL_WARM_RETAIN_TTL_MS);
    }
    if (isWithinGrace(tab) && isFiniteTimestamp(tab.hiddenSince)) {
      scheduleAt(tab.hiddenSince + TERMINAL_WARM_RETAIN_HIDDEN_DELAY_MS);
    }
  }

  const parkedTabIds = new Set(
    tabs.filter((tab) => !warmTabIds.has(tab.tabId)).map((tab) => tab.tabId),
  );
  return { warmTabIds, parkedTabIds, nextTransitionAt };
}
