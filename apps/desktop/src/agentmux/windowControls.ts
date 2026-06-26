// Window controls for the custom/frameless titlebar (decorations: false in
// tauri.conf.json). Uses the global Tauri API exposed by withGlobalTauri, so the
// desktop app needs no @tauri-apps/api dependency. In a plain browser (vite dev
// without the Tauri host) __TAURI__ is absent and every call is a safe no-op.

interface TauriWindowApi {
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  close(): Promise<void>;
  isMaximized(): Promise<boolean>;
  onResized(handler: () => void): Promise<() => void>;
}

function currentWindow(): TauriWindowApi | null {
  const windowApi = (
    window as unknown as {
      __TAURI__?: {
        window?: {
          getCurrentWindow?: () => TauriWindowApi;
          getCurrent?: () => TauriWindowApi;
        };
      };
    }
  ).__TAURI__?.window;
  const getter = windowApi?.getCurrentWindow ?? windowApi?.getCurrent;
  return getter ? getter() : null;
}

export function minimizeWindow(): void {
  void currentWindow()?.minimize();
}

export function toggleMaximizeWindow(): void {
  void currentWindow()?.toggleMaximize();
}

export function closeWindow(): void {
  void currentWindow()?.close();
}

/**
 * Scale the whole UI via the WebView2 zoom factor (browser-level zoom, so the
 * terminal stays crisp — xterm re-renders for the new devicePixelRatio). 1.0 is
 * 100%. No-op in a plain browser. Requires `core:webview:allow-set-webview-zoom`.
 */
export function setUiZoom(factor: number): void {
  const webview = (
    window as unknown as {
      __TAURI__?: {
        webview?: {
          getCurrentWebview?: () => { setZoom?: (factor: number) => Promise<void> };
        };
      };
    }
  ).__TAURI__?.webview?.getCurrentWebview?.();
  void webview?.setZoom?.(factor);
}

/**
 * Track the window's maximized state. Calls `onChange` once with the current
 * value, then again on every resize (which covers maximize/restore, including
 * double-clicking the drag region). Returns an unsubscribe function. No-op in a
 * plain browser. Requires the `core:window:allow-is-maximized` capability.
 */
export function watchMaximized(onChange: (maximized: boolean) => void): () => void {
  const win = currentWindow();
  if (!win) return () => {};
  let cancelled = false;
  let unlisten: (() => void) | null = null;
  const refresh = () => {
    void win.isMaximized().then((maximized) => {
      if (!cancelled) onChange(maximized);
    });
  };
  refresh();
  void win.onResized(refresh).then((fn) => {
    if (cancelled) fn();
    else unlisten = fn;
  });
  return () => {
    cancelled = true;
    if (unlisten) unlisten();
  };
}
