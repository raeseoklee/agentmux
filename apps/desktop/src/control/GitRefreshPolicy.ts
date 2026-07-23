export interface GitRepositoryChangedPayload {
  workspace_id?: string;
  repository_id?: string;
  generation?: number;
}

export const GIT_EVENT_COALESCE_MS = 140;
export const SERVER_GIT_REFRESH_MS = 30_000;

export function shouldRefreshForGitEvent(
  payload: GitRepositoryChangedPayload | null | undefined,
  workspaceId: string,
  repositoryId?: string | null,
  generation?: number | null,
): boolean {
  if (!payload || payload.workspace_id !== workspaceId) return false;
  if (repositoryId && payload.repository_id && payload.repository_id !== repositoryId) {
    return false;
  }
  return payload.generation === undefined || generation === null || generation === undefined || payload.generation > generation;
}

export function nextGitRefreshDelay(
  now: number,
  lastRefreshAt: number,
  coalesceMs = GIT_EVENT_COALESCE_MS,
): number {
  return Math.max(0, coalesceMs - Math.max(0, now - lastRefreshAt));
}

export function shouldReloadGitPage(
  expectedGeneration: number | null | undefined,
  receivedGeneration: number,
): boolean {
  return expectedGeneration !== null && expectedGeneration !== undefined && expectedGeneration !== receivedGeneration;
}
