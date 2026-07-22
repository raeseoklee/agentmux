export const AUTO_UPDATE_PERIODIC_INTERVAL_MS = 4 * 60 * 60 * 1_000;
export const AUTO_UPDATE_RESUME_STALE_MS = 30 * 60 * 1_000;

export type UpdateLifecycleStatus =
  | "idle"
  | "checking"
  | "available"
  | "not_available"
  | "downloading"
  | "installed"
  | "error"
  | "unsupported";

export function shouldPauseAutomaticUpdateChecks(
  status: UpdateLifecycleStatus,
  hasUpdateResource: boolean,
): boolean {
  return (
    hasUpdateResource ||
    status === "available" ||
    status === "downloading" ||
    status === "installed"
  );
}

export function isAutomaticUpdateCheckDue(
  lastAttemptAt: number | null,
  now: number,
  minimumIntervalMs: number,
): boolean {
  if (lastAttemptAt === null) {
    return true;
  }

  const elapsed = now - lastAttemptAt;
  return elapsed < 0 || elapsed >= minimumIntervalMs;
}
