// Adaptive UI zoom.
//
// WebView2 already honours the Windows display scale via devicePixelRatio, so on
// a UHD panel running at 150–200% the UI is sized correctly and we must NOT zoom
// on top of that. We only bump when the OS renders the panel ~1:1 (DPR ≈ 1),
// which is common on FHD/QHD at 100% and on UHD at 100% — there the raw pixel
// density makes the chrome tiny, so we scale by resolution tier.
//
// Whatever we (or the user) land on is persisted, so it survives restarts and the
// user can fine-tune per machine with Ctrl +/-/0.

import { setUiZoom } from "./windowControls";

const ZOOM_KEY = "agentmux.uiZoom.v2";
export const MIN_ZOOM = 0.8;
export const MAX_ZOOM = 2;
export const ZOOM_STEP = 0.05;

/** A sensible default for the current display when the OS isn't already scaling. */
export function defaultZoomForDisplay(): number {
  const dpr = window.devicePixelRatio || 1;
  // OS scaling (>100%) already sizes the UI — leave it alone.
  if (dpr >= 1.25) return 1;
  // DPR ≈ 1 → the OS draws 1:1. UHD/4K at 100% is genuinely tiny so we bump it,
  // but FHD/QHD lean on OS scaling and the user's Ctrl +/- preference rather than
  // an automatic bump.
  const width = window.screen?.width ?? 1920;
  if (width >= 3840) return 1.5; // UHD / 4K @ 100%
  return 1;
}

function clamp(zoom: number): number {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.round(zoom * 100) / 100));
}

/** The persisted zoom, or the adaptive default if none/invalid is stored. */
export function loadZoom(): number {
  let stored: string | null = null;
  try {
    stored = localStorage.getItem(ZOOM_KEY);
  } catch {
    /* storage unavailable */
  }
  const raw = Number(stored);
  if (stored !== null && Number.isFinite(raw) && raw >= MIN_ZOOM && raw <= MAX_ZOOM) {
    return raw;
  }
  return defaultZoomForDisplay();
}

/** Apply a zoom factor to the webview, persisting it unless `persist` is false.
 *  Returns the clamped value. */
export function applyZoom(zoom: number, persist = true): number {
  const next = clamp(zoom);
  if (persist) {
    try {
      localStorage.setItem(ZOOM_KEY, String(next));
    } catch {
      /* storage unavailable */
    }
  }
  setUiZoom(next);
  return next;
}

/** Apply the user's saved zoom (if any) or the adaptive default — WITHOUT
 *  persisting, so the default is recomputed each launch and code changes to it
 *  take effect immediately. Only explicit Ctrl +/-/0 changes are persisted. */
export function initZoom(): number {
  return applyZoom(loadZoom(), false);
}

/** Nudge the current zoom by `delta` (reads the persisted value, so no stale state). */
export function nudgeZoom(delta: number): number {
  return applyZoom(loadZoom() + delta);
}

/** Reset to the adaptive default for this display. */
export function resetZoom(): number {
  return applyZoom(defaultZoomForDisplay());
}
