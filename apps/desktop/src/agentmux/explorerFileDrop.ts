import type { TerminalSession } from "../control/ControlClient";

const FILE_DRAG_ATTRIBUTE = "data-agentmux-file-drag-over";
const FILE_DROP_MESSAGE_TYPE = "agentmux.explorer-file-drop";
export const EXPLORER_FILE_DROP_EVENT = "agentmux://explorer-file-drop";

export interface ExplorerFileDropPayload {
  paths: string[];
  targetPaneId?: string | null;
  targetSessionId?: string | null;
}

interface ExplorerFileDropMetadata {
  type: typeof FILE_DROP_MESSAGE_TYPE;
  targetPaneId: string | null;
  targetSessionId: string | null;
}

interface FileDropTarget {
  element: HTMLElement;
  paneId: string | null;
  sessionId: string;
}

interface WebView2FileBridge {
  postMessageWithAdditionalObjects(
    message: string,
    additionalObjects: File[],
  ): void;
}

interface TauriFileDropEventApi {
  listen(
    event: string,
    handler: (event: { payload: ExplorerFileDropPayload }) => void,
  ): Promise<() => void>;
}

type FileWithNativePath = File & { path?: string };

function isFileDrag(event: DragEvent): boolean {
  return Array.from(event.dataTransfer?.types ?? []).includes("Files");
}

function dropTargetForElement(element: EventTarget | null): FileDropTarget | null {
  if (!(element instanceof Element)) {
    return null;
  }
  const target =
    element.closest<HTMLElement>(
      "[data-agentmux-pane][data-agentmux-terminal-session]",
    ) ?? element.closest<HTMLElement>("[data-agentmux-terminal-session]");
  const sessionId = target?.dataset.agentmuxTerminalSession?.trim() ?? "";
  if (!target || !sessionId) {
    return null;
  }
  return {
    element: target,
    paneId: target.dataset.agentmuxPane?.trim() || null,
    sessionId,
  };
}

function webView2FileBridge(): WebView2FileBridge | null {
  const webview = (
    window as Window & {
      chrome?: { webview?: Partial<WebView2FileBridge> };
    }
  ).chrome?.webview;
  return typeof webview?.postMessageWithAdditionalObjects === "function"
    ? (webview as WebView2FileBridge)
    : null;
}

function tauriEventApi(): TauriFileDropEventApi | null {
  const eventApi = (
    window as Window & {
      __TAURI__?: { event?: Partial<TauriFileDropEventApi> };
    }
  ).__TAURI__?.event;
  return typeof eventApi?.listen === "function"
    ? (eventApi as TauriFileDropEventApi)
    : null;
}

function nativePathFallback(files: File[]): string[] {
  return files
    .map((file) => (file as FileWithNativePath).path?.trim() || file.name.trim())
    .filter(Boolean);
}

function setHighlightedTarget(
  current: HTMLElement | null,
  next: HTMLElement | null,
): HTMLElement | null {
  if (current === next) {
    return current;
  }
  current?.removeAttribute(FILE_DRAG_ATTRIBUTE);
  if (next) {
    next.setAttribute(FILE_DRAG_ATTRIBUTE, "true");
  }
  return next;
}

export function installExplorerFileDrop(
  onPaths: (payload: ExplorerFileDropPayload) => void,
): () => void {
  let highlightedTarget: HTMLElement | null = null;
  let unlistenTauri: (() => void) | null = null;
  let disposed = false;

  const clearHighlight = () => {
    highlightedTarget = setHighlightedTarget(highlightedTarget, null);
  };

  const updateHighlight = (event: DragEvent) => {
    if (!isFileDrag(event)) {
      return null;
    }
    const target = dropTargetForElement(event.target);
    highlightedTarget = setHighlightedTarget(
      highlightedTarget,
      target?.element ?? null,
    );
    return target;
  };

  const onDragOver = (event: DragEvent) => {
    const target = updateHighlight(event);
    if (!isFileDrag(event)) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = target ? "copy" : "none";
    }
  };

  const onDragLeave = (event: DragEvent) => {
    if (
      event.relatedTarget instanceof Node &&
      highlightedTarget?.contains(event.relatedTarget)
    ) {
      return;
    }
    if (!event.relatedTarget) {
      clearHighlight();
    }
  };

  const onDrop = (event: DragEvent) => {
    if (!isFileDrag(event)) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const target = dropTargetForElement(event.target);
    const files = Array.from(event.dataTransfer?.files ?? []);
    clearHighlight();
    if (!target || files.length === 0) {
      return;
    }

    const metadata: ExplorerFileDropMetadata = {
      type: FILE_DROP_MESSAGE_TYPE,
      targetPaneId: target.paneId,
      targetSessionId: target.sessionId,
    };
    const bridge = webView2FileBridge();
    if (bridge) {
      bridge.postMessageWithAdditionalObjects(JSON.stringify(metadata), files);
      return;
    }

    // Browser preview cannot normally expose absolute paths. The optional
    // File.path branch keeps Playwright/Electron-style harnesses useful, while
    // file names are still a sensible fallback in server mode.
    onPaths({
      paths: nativePathFallback(files),
      targetPaneId: target.paneId,
      targetSessionId: target.sessionId,
    });
  };

  window.addEventListener("dragenter", updateHighlight, true);
  window.addEventListener("dragover", onDragOver, true);
  window.addEventListener("dragleave", onDragLeave, true);
  window.addEventListener("drop", onDrop, true);

  const eventApi = tauriEventApi();
  if (eventApi) {
    void eventApi
      .listen(EXPLORER_FILE_DROP_EVENT, (event) => onPaths(event.payload))
      .then((unlisten) => {
        if (disposed) {
          unlisten();
        } else {
          unlistenTauri = unlisten;
        }
      })
      .catch(() => undefined);
  }

  return () => {
    disposed = true;
    clearHighlight();
    unlistenTauri?.();
    window.removeEventListener("dragenter", updateHighlight, true);
    window.removeEventListener("dragover", onDragOver, true);
    window.removeEventListener("dragleave", onDragLeave, true);
    window.removeEventListener("drop", onDrop, true);
  };
}

function windowsPathToWsl(path: string): string {
  const wslUnc = path.match(
    /^\\\\(?:wsl\$|wsl\.localhost)\\[^\\]+(?:\\(.*))?$/i,
  );
  if (wslUnc) {
    return `/${(wslUnc[1] ?? "").replace(/\\/g, "/")}`.replace(/\/$/, "") || "/";
  }
  const drivePath = path.match(/^([a-zA-Z]):[\\/]*(.*)$/);
  if (drivePath) {
    const rest = drivePath[2].replace(/\\/g, "/");
    return `/mnt/${drivePath[1].toLowerCase()}${rest ? `/${rest}` : ""}`;
  }
  return path.replace(/\\/g, "/");
}

function quotePosix(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}

function quotePowerShell(value: string): string {
  return `'${value.replace(/'/g, "''")}'`;
}

function quoteCmd(value: string): string {
  return `"${value.replace(/"/g, '""')}"`;
}

export function formatDroppedPaths(
  paths: string[],
  session: Pick<TerminalSession, "backendKind">,
  surfaceTitle = "",
): string {
  const usablePaths = paths.map((path) => path.trim()).filter(Boolean);
  if (
    session.backendKind === "wsl-direct" ||
    session.backendKind === "wsl-tmux-control"
  ) {
    return usablePaths.map((path) => quotePosix(windowsPathToWsl(path))).join(" ");
  }
  if (session.backendKind === "conpty") {
    const isCmd = /(^|[\\/\s])cmd(?:\.exe)?(?:$|\s)/i.test(surfaceTitle);
    return usablePaths.map(isCmd ? quoteCmd : quotePowerShell).join(" ");
  }
  return usablePaths.map(quotePosix).join(" ");
}
