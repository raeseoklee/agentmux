export const UPDATE_NOTIFICATION_OPEN_EVENT =
  "agentmux://open-update-settings";

const ACKNOWLEDGED_VERSIONS_STORAGE_KEY =
  "agentmux.updates.acknowledged-versions.v2";
const MAX_ACKNOWLEDGED_VERSIONS = 16;

interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

interface TauriInvokeApi {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
}

export interface UpdateNotificationSession {
  readonly handledVersions: Set<string>;
  readonly requestsInFlight: Set<string>;
}

export interface UpdateNotificationInput {
  currentVersion: string;
  version: string;
  title: string;
  body: string;
  actionLabel: string;
}

export type UpdateNotificationResult =
  | "shown"
  | "duplicate"
  | "not_newer"
  | "unsupported"
  | "failed";

interface ParsedVersion {
  core: number[];
  prerelease: string[];
}

const defaultNotificationSession = createUpdateNotificationSession();

export function createUpdateNotificationSession(): UpdateNotificationSession {
  return {
    handledVersions: new Set<string>(),
    requestsInFlight: new Set<string>(),
  };
}

function parseVersion(value: string): ParsedVersion | null {
  const normalized = value.trim().replace(/^v/i, "").split("+")[0];
  const prereleaseIndex = normalized.indexOf("-");
  const coreText =
    prereleaseIndex >= 0 ? normalized.slice(0, prereleaseIndex) : normalized;
  const prereleaseText =
    prereleaseIndex >= 0 ? normalized.slice(prereleaseIndex + 1) : "";
  const coreParts = coreText.split(".");
  if (
    coreParts.length === 0 ||
    coreParts.some((part) => !/^\d+$/.test(part))
  ) {
    return null;
  }
  return {
    core: coreParts.map(Number),
    prerelease: prereleaseText ? prereleaseText.split(".") : [],
  };
}

function comparePrerelease(left: string[], right: string[]): number {
  if (left.length === 0 || right.length === 0) {
    return left.length === right.length ? 0 : left.length === 0 ? 1 : -1;
  }
  const length = Math.max(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    const leftPart = left[index];
    const rightPart = right[index];
    if (leftPart === undefined || rightPart === undefined) {
      return leftPart === rightPart ? 0 : leftPart === undefined ? -1 : 1;
    }
    if (leftPart === rightPart) {
      continue;
    }
    const leftNumeric = /^\d+$/.test(leftPart);
    const rightNumeric = /^\d+$/.test(rightPart);
    if (leftNumeric && rightNumeric) {
      return Number(leftPart) > Number(rightPart) ? 1 : -1;
    }
    if (leftNumeric !== rightNumeric) {
      return leftNumeric ? -1 : 1;
    }
    return leftPart > rightPart ? 1 : -1;
  }
  return 0;
}

export function isNewerRelease(
  currentVersion: string,
  candidateVersion: string,
): boolean {
  const current = parseVersion(currentVersion);
  const candidate = parseVersion(candidateVersion);
  if (!current || !candidate) {
    return false;
  }
  const coreLength = Math.max(current.core.length, candidate.core.length);
  for (let index = 0; index < coreLength; index += 1) {
    const currentPart = current.core[index] ?? 0;
    const candidatePart = candidate.core[index] ?? 0;
    if (currentPart !== candidatePart) {
      return candidatePart > currentPart;
    }
  }
  return comparePrerelease(candidate.prerelease, current.prerelease) > 0;
}

function normalizeVersion(value: string): string {
  return value.trim().replace(/^v/i, "");
}

function readAcknowledgedVersions(storage: StorageLike): string[] {
  try {
    const parsed = JSON.parse(
      storage.getItem(ACKNOWLEDGED_VERSIONS_STORAGE_KEY) ?? "[]",
    );
    return Array.isArray(parsed)
      ? parsed.filter((value): value is string => typeof value === "string")
      : [];
  } catch {
    return [];
  }
}

function rememberAcknowledgedVersion(
  storage: StorageLike,
  version: string,
): void {
  const versions = readAcknowledgedVersions(storage).filter(
    (candidate) => candidate !== version,
  );
  versions.unshift(version);
  try {
    storage.setItem(
      ACKNOWLEDGED_VERSIONS_STORAGE_KEY,
      JSON.stringify(versions.slice(0, MAX_ACKNOWLEDGED_VERSIONS)),
    );
  } catch {
    // Opening update settings must still work when storage is unavailable.
  }
}

export function acknowledgeUpdateNotification(
  version: string,
  storage: StorageLike = window.localStorage,
): boolean {
  const normalized = normalizeVersion(version);
  if (!normalized) {
    return false;
  }
  rememberAcknowledgedVersion(storage, normalized);
  return true;
}

export function shouldUseInAppUpdateFallback(
  result: UpdateNotificationResult,
): result is "unsupported" | "failed" {
  return result === "unsupported" || result === "failed";
}

function tauriInvokeApi(): TauriInvokeApi | null {
  const core = (
    window as Window & {
      __TAURI__?: { core?: Partial<TauriInvokeApi> };
    }
  ).__TAURI__?.core;
  return typeof core?.invoke === "function" ? (core as TauriInvokeApi) : null;
}

export async function notifyUpdateAvailable(
  input: UpdateNotificationInput,
  options: {
    session?: UpdateNotificationSession;
    storage?: StorageLike;
    tauri?: TauriInvokeApi | null;
  } = {},
): Promise<UpdateNotificationResult> {
  if (!isNewerRelease(input.currentVersion, input.version)) {
    return "not_newer";
  }

  const version = normalizeVersion(input.version);
  const storage = options.storage ?? window.localStorage;
  const session = options.session ?? defaultNotificationSession;
  if (
    readAcknowledgedVersions(storage).includes(version) ||
    session.handledVersions.has(version) ||
    session.requestsInFlight.has(version)
  ) {
    return "duplicate";
  }

  const tauri = options.tauri === undefined ? tauriInvokeApi() : options.tauri;
  if (!tauri) {
    session.handledVersions.add(version);
    return "unsupported";
  }

  session.requestsInFlight.add(version);
  try {
    await tauri.invoke("show_update_available_notification", {
      version,
      title: input.title,
      body: input.body,
      action_label: input.actionLabel,
    });
    // OS delivery is not acknowledgement. Keep this only for the current app
    // run; a dismissed or unseen toast is eligible again after relaunch.
    session.handledVersions.add(version);
    return "shown";
  } catch {
    session.handledVersions.add(version);
    return "failed";
  } finally {
    session.requestsInFlight.delete(version);
  }
}
