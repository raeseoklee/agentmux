export type NativeBrowserBounds = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export type NativeBrowserMountResult = {
  url: string;
};

export type NativeBrowserPageLoadEvent = {
  surfaceId: string;
  url: string;
  state: "started" | "finished";
};

export type NativeBrowserTitleEvent = {
  surfaceId: string;
  title: string;
};

export const NATIVE_BROWSER_LAYOUT_EVENT = "agentmux:native-browser-layout";

export function notifyNativeBrowserLayoutChanged(): void {
  window.dispatchEvent(new Event(NATIVE_BROWSER_LAYOUT_EVENT));
}

type TauriEvent<T> = { payload: T };

type TauriBrowserApi = {
  core?: {
    invoke?: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
  };
  event?: {
    listen?: <T>(event: string, handler: (event: TauriEvent<T>) => void) => Promise<() => void>;
  };
};

const layoutRevisions = new Map<string, number>();

function nextLayoutRevision(surfaceId: string): number {
  const revision = (layoutRevisions.get(surfaceId) ?? 0) + 1;
  layoutRevisions.set(surfaceId, revision);
  return revision;
}

function tauriApi(): TauriBrowserApi | null {
  return (
    window as unknown as {
      __TAURI__?: TauriBrowserApi;
    }
  ).__TAURI__ ?? null;
}

function invokeNativeBrowser<T>(command: string, args: Record<string, unknown>): Promise<T> {
  const invoke = tauriApi()?.core?.invoke;
  if (!invoke) {
    return Promise.reject(new Error("Native browser host is unavailable."));
  }
  return invoke<T>(command, args);
}

export function supportsNativeBrowser(): boolean {
  return typeof tauriApi()?.core?.invoke === "function";
}

export function measureNativeBrowserBounds(element: HTMLElement): NativeBrowserBounds | null {
  const rect = element.getBoundingClientRect();
  if (rect.width < 1 || rect.height < 1) return null;
  return {
    x: Math.max(0, Math.round(rect.left)),
    y: Math.max(0, Math.round(rect.top)),
    width: Math.max(1, Math.round(rect.width)),
    height: Math.max(1, Math.round(rect.height)),
  };
}

export function mountNativeBrowser(
  surfaceId: string,
  url: string,
  bounds: NativeBrowserBounds,
  visible: boolean,
): Promise<NativeBrowserMountResult> {
  return invokeNativeBrowser("native_browser_mount", {
    surface_id: surfaceId,
    url,
    bounds,
    visible,
    revision: nextLayoutRevision(surfaceId),
  });
}

export function updateNativeBrowserBounds(
  surfaceId: string,
  bounds: NativeBrowserBounds,
  visible: boolean,
): Promise<void> {
  return invokeNativeBrowser("native_browser_update_bounds", {
    surface_id: surfaceId,
    bounds,
    visible,
    revision: nextLayoutRevision(surfaceId),
  });
}

export function hideNativeBrowser(surfaceId: string): Promise<void> {
  return invokeNativeBrowser("native_browser_hide", {
    surface_id: surfaceId,
    revision: nextLayoutRevision(surfaceId),
  });
}

export function closeNativeBrowser(surfaceId: string): Promise<void> {
  return invokeNativeBrowser("native_browser_close", { surface_id: surfaceId });
}

export function navigateNativeBrowser(surfaceId: string, url: string): Promise<void> {
  return invokeNativeBrowser("native_browser_navigate", { surface_id: surfaceId, url });
}

export function runNativeBrowserNavigation(
  surfaceId: string,
  action: "back" | "forward" | "reload",
): Promise<void> {
  return invokeNativeBrowser("native_browser_navigation", {
    surface_id: surfaceId,
    action,
  });
}

export function setNativeBrowserZoom(surfaceId: string, percent: number): Promise<void> {
  return invokeNativeBrowser("native_browser_set_zoom", {
    surface_id: surfaceId,
    percent,
  });
}

export function findInNativeBrowser(surfaceId: string, query: string): Promise<void> {
  return invokeNativeBrowser("native_browser_find", {
    surface_id: surfaceId,
    query,
  });
}

export async function listenToNativeBrowser(
  surfaceId: string,
  handlers: {
    onPageLoad: (event: NativeBrowserPageLoadEvent) => void;
    onTitle: (event: NativeBrowserTitleEvent) => void;
  },
): Promise<() => void> {
  const listen = tauriApi()?.event?.listen;
  if (!listen) return () => {};
  const [unlistenPageLoad, unlistenTitle] = await Promise.all([
    listen<NativeBrowserPageLoadEvent>("agentmux://native-browser-page-load", (event) => {
      if (event.payload.surfaceId === surfaceId) handlers.onPageLoad(event.payload);
    }),
    listen<NativeBrowserTitleEvent>("agentmux://native-browser-title", (event) => {
      if (event.payload.surfaceId === surfaceId) handlers.onTitle(event.payload);
    }),
  ]);
  return () => {
    unlistenPageLoad();
    unlistenTitle();
  };
}
