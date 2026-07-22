export const SURFACE_TITLE_OVERRIDE_STORAGE_KEY =
  "agentmux.surfaceTitleOverrides.v1";

export type SurfaceTitleOverrides = Record<string, string>;

function storageOrNull(storage?: Storage | null): Storage | null {
  if (storage) return storage;
  return typeof window === "undefined" ? null : window.localStorage;
}

export function readSurfaceTitleOverrides(
  storage?: Storage | null,
): SurfaceTitleOverrides {
  try {
    const raw = storageOrNull(storage)?.getItem(
      SURFACE_TITLE_OVERRIDE_STORAGE_KEY,
    );
    if (!raw) return {};

    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
      return {};
    }

    const overrides: SurfaceTitleOverrides = {};
    for (const [surfaceId, title] of Object.entries(parsed)) {
      if (
        surfaceId.length > 0 &&
        typeof title === "string" &&
        title.trim().length > 0
      ) {
        overrides[surfaceId] = title;
      }
    }
    return overrides;
  } catch {
    return {};
  }
}

export function writeSurfaceTitleOverrides(
  overrides: SurfaceTitleOverrides,
  storage?: Storage | null,
): void {
  try {
    storageOrNull(storage)?.setItem(
      SURFACE_TITLE_OVERRIDE_STORAGE_KEY,
      JSON.stringify(overrides),
    );
  } catch {
    // A privacy-restricted storage area must not block terminal work.
  }
}

export function surfaceTitleOverride(
  value: string | null | undefined,
): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}
