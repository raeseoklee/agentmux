import {
  Terminal,
  type ILink,
  type ILinkHandler,
  type ILinkProvider,
} from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon, type ISearchOptions } from "@xterm/addon-search";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import type { LigaturesAddon } from "@xterm/addon-ligatures";
import type { WebglAddon } from "@xterm/addon-webgl";
import type {
  AlternateWheelMode,
  TerminalRenderer,
  TerminalSnapshot,
  TerminalTypography,
} from "./TerminalRenderer";
import {
  TERMINAL_WEBGL_MAX_ATTACH_FAILURES,
  decideTerminalWebglPolicy,
  decideTerminalWebglPreflight,
  detectTerminalWebglPlatform,
  probeTerminalWebglCapability,
  terminalWebglAttachBackoffMs,
  type TerminalWebglMode,
  type TerminalWebglPlatform,
  type TerminalWebglPolicyReason,
  type TerminalWebglProbeResult,
  type TerminalWebglRendererKind,
} from "./TerminalWebglPolicy";
import "@xterm/xterm/css/xterm.css";

// TS-11: module-level escape hatch — set to false to disable multi-line paste
// confirmation (e.g. when wiring a settings toggle later).
let _multilinePasteGuardEnabled = true;
export function setMultilinePasteGuard(enabled: boolean): void {
  _multilinePasteGuardEnabled = enabled;
}

export const XTERM_THEME = {
  background: "#0e1116",
  foreground: "#d7dde7",
  cursor: "#f1cf89",
  selectionBackground: "#2d5f73",
  scrollbarSliderBackground: "rgba(170, 183, 201, 0.28)",
  scrollbarSliderHoverBackground: "rgba(190, 202, 220, 0.46)",
  scrollbarSliderActiveBackground: "rgba(210, 220, 235, 0.64)"
} as const;

const TERMINAL_PRIMARY_FONT = "Cascadia Code";
const TERMINAL_WINDOWS_FALLBACK_FONT = "Cascadia Mono";
const TERMINAL_BUNDLED_FALLBACK_FONT = "D2Coding Nerd";
const TERMINAL_SYMBOL_FONT = "Symbols Nerd Font Mono";
const TERMINAL_FONT_FEATURE_SETTINGS = '"calt" on, "liga" on';
const TERMINAL_FONT_FAMILY = [
  // Cascadia Code keeps Windows Terminal-like metrics while enabling
  // programming ligatures; fallbacks cover symbols, Hangul, and Nerd icons.
  '"Cascadia Code"',
  '"Fira Code"',
  '"JetBrains Mono"',
  '"Cascadia Mono"',
  '"CaskaydiaCove Nerd Font Mono"',
  '"CaskaydiaCove Nerd Font"',
  '"Symbols Nerd Font Mono"',
  '"D2Coding Nerd"',
  '"MesloLGS NF"',
  '"JetBrainsMono Nerd Font Mono"',
  '"JetBrainsMono Nerd Font"',
  '"FiraCode Nerd Font Mono"',
  '"FiraCode Nerd Font"',
  "Consolas",
  '"Liberation Mono"',
  "monospace"
].join(", ");
const TERMINAL_FONT_SIZE = 12.5;
const TERMINAL_LINE_HEIGHT = 1.0;
const WHEEL_PIXEL_LINE_HEIGHT = 24;
const WHEEL_MIN_LINES = 1;
const WHEEL_MAX_LINES = 12;
const TRANSIENT_SCROLLBAR_MS = 800;
const TERMINAL_CLIPBOARD_FALLBACK_MS = 30_000;
const PAGE_UP_SEQUENCE = "\x1b[5~";
const PAGE_DOWN_SEQUENCE = "\x1b[6~";
// Screen-interactive heuristic: treat the terminal as running a full-screen TUI
// when at least this many absolute cursor-repositioning sequences (CUP/HVP/CUU)
// are observed within SCREEN_INTERACTIVE_WINDOW_MS.  A single `cls`/clear
// emits one CUP(1;1); requiring three events already filters that out.
const SCREEN_INTERACTIVE_MIN_EVENTS = 3;
const SCREEN_INTERACTIVE_WINDOW_MS = 2000;
const SCREEN_INTERACTIVE_RING_SIZE = 8; // power-of-two, keeps allocation fixed
type WebglAddonModule = typeof import("@xterm/addon-webgl");
type LigaturesAddonModule = typeof import("@xterm/addon-ligatures");
type TauriClipboardModule = typeof import("@tauri-apps/plugin-clipboard-manager");
type TerminalLinkOpenHandler = (url: string, event: MouseEvent) => void;

export type TerminalWebglRuntimeState =
  | "idle"
  | "loading"
  | "enabled"
  | "blocked"
  | "fallback"
  | "disabled";

export type TerminalWebglRuntimeReason =
  | TerminalWebglPolicyReason
  | "not-mounted"
  | "attach-failed"
  | "context-lost"
  | "disabled"
  | "unmounted"
  | "recovery-reset";

export interface TerminalWebglDiagnostics {
  state: TerminalWebglRuntimeState;
  reason: TerminalWebglRuntimeReason;
  mode: TerminalWebglMode;
  platform: TerminalWebglPlatform;
  webgl2: boolean | null;
  renderer: string | null;
  rendererKind: TerminalWebglRendererKind;
  attachFailures: number;
  attachRetryAt: number;
  contextLossLatched: boolean;
  canvasAttached: boolean;
  updatedAt: number;
}

export type TerminalWebglStateListener = (
  diagnostics: Readonly<TerminalWebglDiagnostics>,
) => void;

interface WebglLoseContextExtension {
  loseContext(): void;
}

let webglAddonModulePromise: Promise<WebglAddonModule> | undefined;
let ligaturesAddonModulePromise: Promise<LigaturesAddonModule> | undefined;
let tauriClipboardModulePromise: Promise<TauriClipboardModule> | undefined;
let lastTerminalClipboardText = "";
let lastTerminalClipboardAt = 0;

const TERMINAL_URL_PATTERN = /\bhttps?:\/\/[^\s<>"'`]+/gi;
const TERMINAL_URL_TRAILING_PUNCTUATION = /[),.;:!?\]}]+$/;

function loadWebglAddonModule(): Promise<WebglAddonModule> {
  if (!webglAddonModulePromise) {
    webglAddonModulePromise = import("@xterm/addon-webgl").catch((error) => {
      webglAddonModulePromise = undefined;
      throw error;
    });
  }
  return webglAddonModulePromise;
}

function loadLigaturesAddonModule(): Promise<LigaturesAddonModule> {
  if (!ligaturesAddonModulePromise) {
    ligaturesAddonModulePromise = import("@xterm/addon-ligatures").catch((error) => {
      ligaturesAddonModulePromise = undefined;
      throw error;
    });
  }
  return ligaturesAddonModulePromise;
}

function loadTauriClipboardModule(): Promise<TauriClipboardModule> {
  if (!tauriClipboardModulePromise) {
    tauriClipboardModulePromise = import("@tauri-apps/plugin-clipboard-manager").catch((error) => {
      tauriClipboardModulePromise = undefined;
      throw error;
    });
  }
  return tauriClipboardModulePromise;
}

function isTauriRuntime(): boolean {
  const runtime = window as Window & { __TAURI_INTERNALS__?: unknown };
  return Boolean(window.__TAURI__?.core?.invoke || runtime.__TAURI_INTERNALS__);
}

function normalizeTerminalUrl(value: string): string | null {
  const trimmed = value.trim().replace(TERMINAL_URL_TRAILING_PUNCTUATION, "");
  try {
    const parsed = new URL(trimmed);
    return parsed.protocol === "http:" || parsed.protocol === "https:"
      ? parsed.href
      : null;
  } catch {
    return null;
  }
}

function extractTerminalLinks(lineText: string): Array<{
  url: string;
  startIndex: number;
  endIndex: number;
}> {
  const links: Array<{ url: string; startIndex: number; endIndex: number }> = [];
  TERMINAL_URL_PATTERN.lastIndex = 0;
  let match: RegExpExecArray | null = null;
  while ((match = TERMINAL_URL_PATTERN.exec(lineText))) {
    const raw = match[0];
    const url = normalizeTerminalUrl(raw);
    if (!url) {
      continue;
    }
    const endIndex = match.index + raw.replace(
      TERMINAL_URL_TRAILING_PUNCTUATION,
      "",
    ).length;
    links.push({
      url,
      startIndex: match.index,
      endIndex,
    });
  }
  return links;
}

function normalizeFontSize(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.min(18, Math.max(10, value))
    : TERMINAL_FONT_SIZE;
}

function normalizeLineHeight(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.min(1.4, Math.max(1.0, value))
    : TERMINAL_LINE_HEIGHT;
}

function wheelDeltaLines(event: WheelEvent, rows: number): number {
  const delta = Math.abs(event.deltaY);
  if (delta === 0) {
    return 0;
  }
  let lines: number;
  if (event.deltaMode === WheelEvent.DOM_DELTA_PAGE) {
    lines = Math.max(1, rows - 1);
  } else if (event.deltaMode === WheelEvent.DOM_DELTA_LINE) {
    lines = delta;
  } else {
    lines = delta / WHEEL_PIXEL_LINE_HEIGHT;
  }
  return Math.min(
    WHEEL_MAX_LINES,
    Math.max(WHEEL_MIN_LINES, Math.ceil(lines)),
  );
}

function fallbackWriteClipboardText(text: string): boolean {
  const textarea = document.createElement("textarea");
  const active = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  textarea.value = text;
  textarea.setAttribute("readonly", "true");
  textarea.style.position = "fixed";
  textarea.style.left = "-10000px";
  textarea.style.top = "0";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  textarea.focus();
  textarea.select();
  let copied = false;
  try {
    copied = document.execCommand("copy");
  } finally {
    textarea.remove();
    active?.focus({ preventScroll: true });
  }
  return copied;
}

function rememberTerminalClipboardText(text: string): void {
  lastTerminalClipboardText = text;
  lastTerminalClipboardAt = Date.now();
}

function recentTerminalClipboardText(): string {
  if (
    lastTerminalClipboardText &&
    Date.now() - lastTerminalClipboardAt <= TERMINAL_CLIPBOARD_FALLBACK_MS
  ) {
    return lastTerminalClipboardText;
  }
  return "";
}

async function writeClipboardText(text: string): Promise<void> {
  if (!text) {
    return;
  }
  if (isTauriRuntime()) {
    try {
      const clipboard = await loadTauriClipboardModule();
      await clipboard.writeText(text);
      return;
    } catch (error) {
      console.warn("[agentmux] tauri clipboard write failed", { error });
      // Fall through to browser clipboard paths for preview or plugin errors.
    }
  }
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return;
    }
  } catch (error) {
    console.warn("[agentmux] navigator clipboard write failed", { error });
    // Fall through to the hidden-textarea path used by older WebViews.
  }
  if (!fallbackWriteClipboardText(text)) {
    throw new Error("clipboard copy failed");
  }
}

interface ClipboardReadResult {
  /** true when a clipboard read succeeded (even if the clipboard is empty). */
  ok: boolean;
  text: string;
}

/**
 * Read text from the clipboard. Returns { ok: true, text } when any read path
 * succeeds — text may be an empty string when the clipboard holds non-text
 * content (image, file, etc.). Returns { ok: false, text: "" } only when every
 * read path throws (permission denied, plugin failure, etc.) so the caller can
 * distinguish a legitimate empty clipboard from a read error.
 */
async function readClipboardText(): Promise<ClipboardReadResult> {
  if (isTauriRuntime()) {
    try {
      const clipboard = await loadTauriClipboardModule();
      const text = await clipboard.readText();
      return { ok: true, text: text ?? "" };
    } catch (error) {
      console.warn("[agentmux] tauri clipboard read failed", { error });
      // Fall through to browser clipboard paths for preview or plugin errors.
    }
  }
  try {
    if (navigator.clipboard?.readText) {
      const text = await navigator.clipboard.readText();
      return { ok: true, text: text ?? "" };
    }
  } catch (error) {
    console.warn("[agentmux] navigator clipboard read failed", { error });
    // Clipboard read can be denied by the host; keep the terminal focused.
  }
  return { ok: false, text: "" };
}

export class XtermTerminalRenderer implements TerminalRenderer {
  private terminal?: Terminal;
  private fitAddon?: FitAddon;
  private searchAddon?: SearchAddon;
  private unicodeAddon?: Unicode11Addon;
  private ligaturesAddon?: LigaturesAddon;
  private mountedElement?: HTMLElement;
  private inputEventAbort?: AbortController;
  // TS-9: last search term so F3 / Shift+F3 repeat without needing a UI.
  private _lastSearchTerm = "";
  private pasteHandlers = new Set<(text: string) => void>();
  private openLinkHandler?: TerminalLinkOpenHandler;
  private linkProviderDisposable?: { dispose(): void };
  // Active WebGL addon, when GPU rendering has been opted into and succeeded.
  private webglAddon?: WebglAddon;
  // Disposes the onContextLoss subscription tied to the current webglAddon.
  private webglContextLossSub?: { dispose(): void };
  private webglCanvas?: HTMLCanvasElement;
  private webglContext?: WebGL2RenderingContext;
  private webglFallbackFrame?: number;
  // Guards against overlapping enableWebgl() calls while the addon module is
  // being lazily imported (import() is async, so two calls could race).
  private webglEnablePending = false;
  // Monotonic generation, bumped on every disable. A lazy enableWebgl() import()
  // captures the generation it started in and bails on resolve if a later
  // disable/enable has superseded it. Without this, a rapid
  // enable -> disable -> enable can let two in-flight imports each loadAddon(),
  // leaking a duplicate WebGL context.
  private webglGeneration = 0;
  private webglMode: TerminalWebglMode = "off";
  private webglAttachFailures = 0;
  private webglAttachRetryAt = 0;
  private webglContextLossLatched = false;
  private readonly webglStateListeners = new Set<TerminalWebglStateListener>();
  private webglDiagnostics: TerminalWebglDiagnostics = {
    state: "idle",
    reason: "mode-off",
    mode: "off",
    platform: detectTerminalWebglPlatform(),
    webgl2: null,
    renderer: null,
    rendererKind: "unknown",
    attachFailures: 0,
    attachRetryAt: 0,
    contextLossLatched: false,
    canvasAttached: false,
    updatedAt: Date.now(),
  };
  private fontReadyPromise?: Promise<void>;
  private ligaturesReadyPromise?: Promise<void>;
  private scrollbarHideTimer?: number;
  private alternateWheelMode: AlternateWheelMode = "auto";
  // Copy-on-select: debounce timer and last-copied value to avoid churn.
  private _cosTimer?: number;
  private _cosLast = "";
  // Disposable for the onSelectionChange subscription.
  private _cosSub?: { dispose(): void };
  // Fixed-size ring buffer of timestamps (ms) for recent absolute
  // cursor-repositioning sequences.  Used by screenInteractiveActive().
  private readonly _siRing: number[] = new Array<number>(SCREEN_INTERACTIVE_RING_SIZE).fill(0);
  private _siHead = 0; // next write index (wraps mod SCREEN_INTERACTIVE_RING_SIZE)

  mount(
    element: HTMLElement,
    initialState: TerminalSnapshot,
    typography?: Partial<TerminalTypography>,
  ): void {
    this.dispose();
    const fontSize = normalizeFontSize(typography?.fontSize);
    const lineHeight = normalizeLineHeight(typography?.lineHeight);

    const terminal = new Terminal({
      allowProposedApi: true,
      convertEol: false,
      customGlyphs: false,
      cursorBlink: true,
      fontFamily: TERMINAL_FONT_FAMILY,
      fontSize,
      letterSpacing: 0,
      lineHeight,
      linkHandler: this.createOscLinkHandler(),
      overviewRuler: { width: 9 },
      rows: initialState.rows,
      cols: initialState.columns,
      scrollback: 10_000,
      theme: XTERM_THEME
    });
    const fitAddon = new FitAddon();
    const unicodeAddon = new Unicode11Addon();

    terminal.loadAddon(unicodeAddon);
    terminal.unicode.activeVersion = "11";
    element.dataset.agentmuxTerminalUnicodeVersion = terminal.unicode.activeVersion;
    element.dataset.agentmuxTerminalCustomGlyphs = String(terminal.options.customGlyphs);
    element.dataset.agentmuxTerminalFontFamily = TERMINAL_FONT_FAMILY;
    element.dataset.agentmuxTerminalLigatures = "loading";
    element.dataset.agentmuxTerminalFontFeatureSettings = TERMINAL_FONT_FEATURE_SETTINGS;
    terminal.loadAddon(fitAddon);
    // TS-9: in-buffer search addon — loaded before open() so the addon is ready
    // when the terminal begins rendering.
    const searchAddon = new SearchAddon();
    terminal.loadAddon(searchAddon);
    terminal.open(element);

    // Reset repaint-observation state for this fresh terminal.
    this._siRing.fill(0);
    this._siHead = 0;

    // Register non-consuming CSI observers for absolute cursor-repositioning
    // sequences emitted heavily by full-screen TUIs (CUP ESC[row;colH,
    // HVP ESC[row;colf, and CUU ESC[nA).  Returning false keeps the handler
    // chain running so xterm still processes the sequence normally.
    // These disposables live until terminal.dispose() — no manual cleanup
    // needed because the terminal itself owns them after registration.
    terminal.parser.registerCsiHandler({ final: "H" }, (params) => {
      // CUP — filter trivial cls/clear (row=1, col=1 or no params) to reduce
      // false positives, but the >=3 threshold is the primary gate.
      const row = Array.isArray(params[0]) ? params[0][0] : (params[0] ?? 1);
      const col = Array.isArray(params[1]) ? params[1][0] : (params[1] ?? 1);
      if (row !== 1 || col !== 1) {
        this._siRecordEvent();
      }
      return false;
    });
    terminal.parser.registerCsiHandler({ final: "f" }, (_params) => {
      // HVP — same semantics as CUP, used by some TUIs.
      this._siRecordEvent();
      return false;
    });
    terminal.parser.registerCsiHandler({ final: "A" }, (_params) => {
      // CUU (cursor up) — emitted by TUI differential repaints.
      this._siRecordEvent();
      return false;
    });

    this.ligaturesReadyPromise = this.enableLigatures(terminal, element);
    terminal.attachCustomKeyEventHandler((event) =>
      this.handleClipboardKey(terminal, event)
    );
    terminal.attachCustomWheelEventHandler((event) =>
      this.handleWheelEvent(terminal, element, event)
    );
    const inputEventAbort = new AbortController();
    element.addEventListener(
      "copy",
      (event) => {
        this.handleCopyEvent(terminal, event);
      },
      { capture: true, signal: inputEventAbort.signal }
    );
    element.addEventListener(
      "paste",
      (event) => {
        this.handlePasteEvent(terminal, event);
      },
      { capture: true, signal: inputEventAbort.signal }
    );
    element.addEventListener(
      "contextmenu",
      (event) => {
        this.handleContextMenuPasteOrCopy(terminal, event);
      },
      { signal: inputEventAbort.signal }
    );
    // TS-10: middle-click paste. Use primary-selection semantics: paste the
    // current terminal selection if non-empty, else fall back to system clipboard.
    // When xterm is in mouse-tracking mode the middle click belongs to the app.
    element.addEventListener(
      "mousedown",
      (event: MouseEvent) => {
        if (event.button !== 1 || this.terminal !== terminal) {
          return;
        }
        if (terminal.modes.mouseTrackingMode !== "none") {
          // App is tracking mouse — forward the event normally.
          return;
        }
        event.preventDefault();
        const sel = terminal.getSelection();
        if (sel) {
          this.emitPaste(sel);
          terminal.focus();
        } else {
          this.pasteFromClipboard(terminal);
        }
      },
      { capture: true, signal: inputEventAbort.signal }
    );
    // Copy-on-select: when the selection becomes non-empty, debounce 150 ms
    // (drag emits many change events) then write to the clipboard and remember
    // it for the stale-cache fallback.  Does not steal focus.  Skipped when
    // the selection is unchanged from the last copied value to avoid churn.
    this._cosLast = "";
    if (this._cosTimer !== undefined) {
      window.clearTimeout(this._cosTimer);
      this._cosTimer = undefined;
    }
    this._cosSub?.dispose();
    this._cosSub = terminal.onSelectionChange(() => {
      if (this.terminal !== terminal) {
        return;
      }
      const sel = terminal.getSelection();
      if (!sel || sel === this._cosLast) {
        return;
      }
      if (this._cosTimer !== undefined) {
        window.clearTimeout(this._cosTimer);
      }
      this._cosTimer = window.setTimeout(() => {
        this._cosTimer = undefined;
        if (this.terminal !== terminal) {
          return;
        }
        const current = terminal.getSelection();
        if (!current || current === this._cosLast) {
          return;
        }
        this._cosLast = current;
        rememberTerminalClipboardText(current);
        void writeClipboardText(current).catch((error) => {
          console.warn("[agentmux] copy-on-select write failed", { error });
        });
      }, 150);
    });

    this.installPlainUrlLinkProvider(terminal);
    fitAddon.fit();

    if (initialState.bytes && initialState.bytes.length > 0) {
      terminal.write(initialState.bytes);
    }

    this.terminal = terminal;
    this.fitAddon = fitAddon;
    this.searchAddon = searchAddon;
    this.unicodeAddon = unicodeAddon;
    this.mountedElement = element;
    this.inputEventAbort = inputEventAbort;

    // The Nerd font (@font-face) loads lazily, so xterm's first glyph
    // measurement can use fallback metrics — leaving icons/powerline glyphs
    // blank or misaligned. Once the face is ready, re-measure: drop the WebGL
    // texture atlas, re-apply the family to force a glyph re-measure, refit.
    this.fontReadyPromise = this.ensureFontsThenRemeasure(terminal, fontSize);
  }

  private ensureFontsThenRemeasure(
    terminal: Terminal,
    fontSize = normalizeFontSize(this.terminal?.options.fontSize),
  ): Promise<void> {
    const fonts = (document as Document & { fonts?: FontFaceSet }).fonts;
    const fontLoads = fonts?.load
      ? Promise.allSettled([
          fonts.load(`${fontSize}px "${TERMINAL_PRIMARY_FONT}"`),
          fonts.load(`${fontSize}px "${TERMINAL_WINDOWS_FALLBACK_FONT}"`),
          fonts.load(`${fontSize}px "${TERMINAL_BUNDLED_FALLBACK_FONT}"`),
          fonts.load(`${fontSize}px "${TERMINAL_SYMBOL_FONT}"`)
        ]).then(() => {})
      : Promise.resolve();
    return fontLoads
      .catch(() => {})
      .then(
        () =>
          new Promise<void>((resolve) => {
            window.setTimeout(resolve, 80);
          })
      )
      .then(() => {
        if (this.terminal !== terminal) {
          return;
        }
        this.webglAddon?.clearTextureAtlas();
        terminal.options.fontFamily = TERMINAL_FONT_FAMILY;
        this.fitAddon?.fit();
        terminal.refresh(0, terminal.rows - 1);
      })
      .catch(() => {
        /* font failed to load — keep the monospace fallback */
      });
  }

  unmount(): void {
    // WebGL addon must be disposed BEFORE the terminal: disposing the terminal
    // first leaves the addon holding a dangling reference / leaked GL context.
    this.webglGeneration++;
    this.webglEnablePending = false;
    this.cancelWebglFallbackRefresh();
    this.disposeWebglAddon(false);
    this.linkProviderDisposable?.dispose();
    this.linkProviderDisposable = undefined;
    this.clearTransientScrollbar();
    // Dispose copy-on-select subscription and any pending debounce timer.
    this._cosSub?.dispose();
    this._cosSub = undefined;
    if (this._cosTimer !== undefined) {
      window.clearTimeout(this._cosTimer);
      this._cosTimer = undefined;
    }
    this._cosLast = "";
    this.inputEventAbort?.abort();
    this.inputEventAbort = undefined;
    this.terminal?.dispose();
    this.terminal = undefined;
    this.fitAddon = undefined;
    this.searchAddon = undefined;
    this.unicodeAddon = undefined;
    this.ligaturesAddon = undefined;
    this.mountedElement = undefined;
    this.fontReadyPromise = undefined;
    this.ligaturesReadyPromise = undefined;
    this._lastSearchTerm = "";
    this.webglMode = "off";
    this.updateWebglDiagnostics({
      state: "disabled",
      reason: "unmounted",
      mode: "off",
    });
  }

  write(batch: Uint8Array, callback?: () => void): void {
    if (!this.terminal) {
      callback?.();
      return;
    }
    this.terminal.write(batch, callback);
  }

  reset(): void {
    this.terminal?.reset();
  }

  resize(columns: number, rows: number): void {
    this.terminal?.resize(columns, rows);
  }

  size(): { columns: number; rows: number } | null {
    const terminal = this.terminal;
    return terminal ? { columns: terminal.cols, rows: terminal.rows } : null;
  }

  setTypography(typography: Partial<TerminalTypography>): void {
    const terminal = this.terminal;
    if (!terminal) {
      return;
    }
    const nextFontSize = normalizeFontSize(
      typography.fontSize ?? terminal.options.fontSize,
    );
    const nextLineHeight = normalizeLineHeight(
      typography.lineHeight ?? terminal.options.lineHeight,
    );
    const changed =
      terminal.options.fontSize !== nextFontSize ||
      terminal.options.lineHeight !== nextLineHeight ||
      terminal.options.fontFamily !== TERMINAL_FONT_FAMILY;
    if (!changed) {
      return;
    }
    terminal.options.fontSize = nextFontSize;
    terminal.options.lineHeight = nextLineHeight;
    terminal.options.letterSpacing = 0;
    terminal.options.fontFamily = TERMINAL_FONT_FAMILY;
    this.webglAddon?.clearTextureAtlas();
    this.fitAddon?.fit();
    terminal.refresh(0, terminal.rows - 1);
    this.fontReadyPromise = this.ensureFontsThenRemeasure(terminal, nextFontSize);
  }

  setAlternateWheelMode(mode: AlternateWheelMode): void {
    this.alternateWheelMode = mode;
  }

  onData(handler: (data: string) => void): () => void {
    const disposable = this.terminal?.onData(handler);
    return () => disposable?.dispose();
  }

  onPaste(handler: (text: string) => void): () => void {
    this.pasteHandlers.add(handler);
    return () => {
      this.pasteHandlers.delete(handler);
    };
  }

  onOpenLink(handler: TerminalLinkOpenHandler): () => void {
    this.openLinkHandler = handler;
    return () => {
      if (this.openLinkHandler === handler) {
        this.openLinkHandler = undefined;
      }
    };
  }

  onResize(handler: (columns: number, rows: number) => void): () => void {
    const disposable = this.terminal?.onResize((size) => handler(size.cols, size.rows));
    return () => disposable?.dispose();
  }

  focus(): void {
    this.terminal?.focus();
  }

  fit(): void {
    this.fitAddon?.fit();
  }

  /**
   * Opt-in GPU rendering. Lazily loads the WebGL addon and attaches it to the
   * terminal. This is intentionally NOT called from mount(): the default
   * renderer remains the DOM renderer so existing callers are unaffected.
   *
   * Safe to call repeatedly (double-enable guarded). If WebGL is unavailable
   * (no terminal mounted, no GPU/WebGL2 context, or the addon throws) it
   * silently falls back to the default DOM renderer — the terminal keeps
   * working, just without hardware acceleration.
   */
  enableWebgl(mode: TerminalWebglMode = "on"): void {
    this.webglMode = mode;
    if (mode === "off") {
      this.disableWebgl();
      return;
    }
    const terminal = this.terminal;
    if (!terminal) {
      this.updateWebglDiagnostics({
        state: "blocked",
        reason: "not-mounted",
        mode,
      });
      return;
    }
    // A repeated request in the same mode does not need another probe.
    if (this.webglAddon && this.webglDiagnostics.mode === mode) {
      if (this.hasAttachedWebglCanvas()) {
        this.updateWebglDiagnostics({
          state: "enabled",
          reason: mode === "auto" ? "enabled-auto" : "enabled-on",
          mode,
        });
        return;
      }
      // The addon can outlive a renderer swap performed inside xterm. Treat a
      // detached/zeroed canvas as stale even if the addon object still exists.
      this.webglGeneration++;
      this.disposeWebglAddon(true);
    }
    if (this.webglEnablePending) {
      return;
    }

    const platform = detectTerminalWebglPlatform();
    const now = Date.now();
    const recovery = {
      attachRetryAt: this.webglAttachRetryAt,
      contextLossLatched: this.webglContextLossLatched,
    };
    const preflightDecision = decideTerminalWebglPreflight({
      mode,
      platform,
      recovery,
      now,
    });
    if (preflightDecision) {
      this.disposeWebglAddon(true);
      this.updateWebglDiagnostics({
        state:
          preflightDecision.reason === "mode-off" ? "disabled" : "blocked",
        reason: preflightDecision.reason,
        mode,
        platform,
      });
      return;
    }
    const probe = probeTerminalWebglCapability();
    const decision = decideTerminalWebglPolicy({
      mode,
      platform,
      probe,
      recovery,
      now,
    });
    if (!decision.enable) {
      this.disposeWebglAddon(true);
      this.updateWebglDiagnostics({
        state: decision.reason === "mode-off" ? "disabled" : "blocked",
        reason: decision.reason,
        mode,
        platform,
        webgl2: probe.webgl2,
        renderer: probe.renderer,
        rendererKind: probe.rendererKind,
      });
      return;
    }
    if (this.webglAddon) {
      this.updateWebglDiagnostics({
        state: "enabled",
        reason: decision.reason,
        mode,
        platform,
        webgl2: probe.webgl2,
        renderer: probe.renderer,
        rendererKind: probe.rendererKind,
      });
      return;
    }

    this.webglEnablePending = true;
    this.updateWebglDiagnostics({
      state: "loading",
      reason: decision.reason,
      mode,
      platform,
      webgl2: probe.webgl2,
      renderer: probe.renderer,
      rendererKind: probe.rendererKind,
    });
    // Claim a generation for this enable. Any later disable bumps the counter,
    // which invalidates this in-flight import when it resolves.
    const generation = ++this.webglGeneration;
    void Promise.all([
      this.fontReadyPromise ?? Promise.resolve(),
      this.ligaturesReadyPromise ?? Promise.resolve(),
    ])
      .catch(() => {})
      .then(() => loadWebglAddonModule())
      .then(({ WebglAddon }) => {
        // Bail if this enable was superseded (disable/re-enable) or the terminal
        // was swapped/unmounted while the import was in flight, or an addon is
        // already attached. Any of these means attaching here would leak a
        // duplicate GL context.
        if (
          generation !== this.webglGeneration ||
          this.terminal !== terminal ||
          this.webglAddon
        ) {
          return;
        }
        const existingCanvases = new Set(
          this.mountedElement?.querySelectorAll("canvas") ?? [],
        );
        try {
          const addon = new WebglAddon();
          this.webglAddon = addon;
          // If the GPU context is lost (driver reset, tab backgrounded too
          // long, too many live contexts), dispose the addon and drop back to
          // the DOM renderer instead of leaving a blank/frozen terminal.
          this.webglContextLossSub = addon.onContextLoss(() => {
            if (this.webglAddon !== addon) {
              return;
            }
            this.webglContextLossLatched = true;
            this.webglGeneration++;
            this.webglEnablePending = false;
            this.updateWebglDiagnostics({
              state: "fallback",
              reason: "context-lost",
              contextLossLatched: true,
            });
            this.disposeWebglAddon(true);
          });
          terminal.loadAddon(addon);
          if (this.webglAddon !== addon) {
            return;
          }
          this.captureWebglContext(existingCanvases);
          this.webglAttachFailures = 0;
          this.webglAttachRetryAt = 0;
          this.updateWebglDiagnostics({
            state: "enabled",
            reason: decision.reason,
            attachFailures: 0,
            attachRetryAt: 0,
          });
        } catch {
          // WebGL2 unavailable or addon initialization failed — fall back to
          // the default DOM renderer. Clean up any partial subscription.
          this.captureWebglContext(existingCanvases);
          this.disposeWebglAddon(true);
          if (
            generation === this.webglGeneration &&
            this.terminal === terminal
          ) {
            this.recordWebglAttachFailure(mode, platform, probe);
          }
        }
      })
      .catch(() => {
        if (
          generation === this.webglGeneration &&
          this.terminal === terminal
        ) {
          this.recordWebglAttachFailure(mode, platform, probe);
        }
      })
      .finally(() => {
        // Only clear the pending flag if we still own the latest generation; a
        // superseding enable owns it otherwise.
        if (generation === this.webglGeneration) {
          this.webglEnablePending = false;
        }
      });
  }

  /**
   * Disable GPU rendering and return to the DOM renderer. Disposes the WebGL
   * addon (if any) and clears internal refs so enableWebgl() can re-attach a
   * fresh addon later. Also cancels an in-flight enableWebgl() import.
   */
  disableWebgl(): void {
    // Bump the generation so any in-flight enableWebgl() import() bails on
    // resolve instead of attaching after the caller asked to disable, then
    // clear the pending flag and dispose any live addon.
    this.webglGeneration++;
    this.webglEnablePending = false;
    this.webglMode = "off";
    this.disposeWebglAddon(true);
    this.updateWebglDiagnostics({
      state: "disabled",
      reason: "disabled",
      mode: "off",
    });
  }

  /**
   * Whether GPU rendering is currently active (an addon is loaded). Pending
   * enables that have not yet resolved report false.
   */
  isWebglEnabled(): boolean {
    return this.webglAddon !== undefined;
  }

  getWebglDiagnostics(): Readonly<TerminalWebglDiagnostics> {
    return {
      ...this.webglDiagnostics,
      canvasAttached: this.hasAttachedWebglCanvas(),
    };
  }

  onWebglStateChange(listener: TerminalWebglStateListener): () => void {
    this.webglStateListeners.add(listener);
    listener(this.getWebglDiagnostics());
    return () => {
      this.webglStateListeners.delete(listener);
    };
  }

  resetWebglRecovery(): void {
    if (this.webglEnablePending) {
      this.webglGeneration++;
      this.webglEnablePending = false;
    }
    this.webglAttachFailures = 0;
    this.webglAttachRetryAt = 0;
    this.webglContextLossLatched = false;
    this.updateWebglDiagnostics({
      state: this.webglAddon
        ? "enabled"
        : this.webglMode === "off"
          ? "disabled"
          : "idle",
      reason: "recovery-reset",
      attachFailures: 0,
      attachRetryAt: 0,
      contextLossLatched: false,
    });
  }

  // Releases the active context before addon disposal, leaving xterm's default
  // renderer ready for the scheduled fit and full refresh.
  private disposeWebglAddon(scheduleFallbackRefresh: boolean): void {
    const addon = this.webglAddon;
    const terminal = this.terminal;
    this.webglContextLossSub?.dispose();
    this.webglContextLossSub = undefined;
    if (addon && !this.webglContext) {
      this.captureWebglContext(new Set());
    }
    this.releaseWebglContext();
    this.webglAddon = undefined;
    try {
      addon?.dispose();
    } catch (error) {
      console.warn("[agentmux] terminal WebGL addon dispose failed", {
        error,
      });
    }
    if (addon && scheduleFallbackRefresh && terminal) {
      this.scheduleWebglFallbackRefresh(terminal);
    }
  }

  private captureWebglContext(
    existingCanvases: ReadonlySet<HTMLCanvasElement>,
  ): void {
    const element = this.mountedElement;
    if (!element) {
      return;
    }
    const canvases = Array.from(element.querySelectorAll("canvas"));
    const candidates = [
      ...canvases.filter((canvas) => !existingCanvases.has(canvas)).reverse(),
      ...canvases.reverse(),
    ];
    const seen = new Set<HTMLCanvasElement>();
    for (const canvas of candidates) {
      if (seen.has(canvas)) {
        continue;
      }
      seen.add(canvas);
      try {
        const context = canvas.getContext("webgl2");
        if (context) {
          this.webglCanvas = canvas;
          this.webglContext = context;
          return;
        }
      } catch {
        // A canvas owned by another renderer may reject a second context type.
      }
    }
  }

  private releaseWebglContext(): void {
    const canvas = this.webglCanvas;
    let context = this.webglContext;
    if (!context && canvas) {
      try {
        const canvasContext = canvas.getContext("webgl2");
        if (canvasContext) {
          context = canvasContext;
        }
      } catch {
        // The canvas is still zeroed below even when the driver is unavailable.
      }
    }
    if (context) {
      try {
        const loseContext = context.getExtension(
          "WEBGL_lose_context",
        ) as WebglLoseContextExtension | null;
        loseContext?.loseContext();
      } catch {
        // Context loss itself can make extension access throw on some drivers.
      }
    }
    if (canvas) {
      canvas.width = 0;
      canvas.height = 0;
    }
    this.webglCanvas = undefined;
    this.webglContext = undefined;
  }

  private hasAttachedWebglCanvas(): boolean {
    const canvas = this.webglCanvas;
    return Boolean(
      this.webglAddon &&
        canvas?.isConnected &&
        canvas.width > 0 &&
        canvas.height > 0,
    );
  }

  private scheduleWebglFallbackRefresh(terminal: Terminal): void {
    this.cancelWebglFallbackRefresh();
    this.webglFallbackFrame = window.requestAnimationFrame(() => {
      this.webglFallbackFrame = undefined;
      if (this.terminal !== terminal) {
        return;
      }
      this.fitAddon?.fit();
      terminal.refresh(0, terminal.rows - 1);
    });
  }

  private cancelWebglFallbackRefresh(): void {
    if (this.webglFallbackFrame === undefined) {
      return;
    }
    window.cancelAnimationFrame(this.webglFallbackFrame);
    this.webglFallbackFrame = undefined;
  }

  private recordWebglAttachFailure(
    mode: TerminalWebglMode,
    platform: TerminalWebglPlatform,
    probe: TerminalWebglProbeResult,
  ): void {
    this.webglAttachFailures = Math.min(
      TERMINAL_WEBGL_MAX_ATTACH_FAILURES,
      this.webglAttachFailures + 1,
    );
    this.webglAttachRetryAt =
      Date.now() + terminalWebglAttachBackoffMs(this.webglAttachFailures);
    this.updateWebglDiagnostics({
      state: "fallback",
      reason: "attach-failed",
      mode,
      platform,
      webgl2: probe.webgl2,
      renderer: probe.renderer,
      rendererKind: probe.rendererKind,
    });
  }

  private updateWebglDiagnostics(
    update: Partial<TerminalWebglDiagnostics>,
  ): void {
    this.webglDiagnostics = {
      ...this.webglDiagnostics,
      ...update,
      mode: update.mode ?? this.webglMode,
      attachFailures: this.webglAttachFailures,
      attachRetryAt: this.webglAttachRetryAt,
      contextLossLatched: this.webglContextLossLatched,
      canvasAttached: this.hasAttachedWebglCanvas(),
      updatedAt: Date.now(),
    };
    const snapshot = this.getWebglDiagnostics();
    for (const listener of this.webglStateListeners) {
      try {
        listener(snapshot);
      } catch (error) {
        console.warn("[agentmux] terminal WebGL state listener failed", {
          error,
        });
      }
    }
  }

  dispose(): void {
    this.unmount();
  }

  element(): HTMLElement | undefined {
    return this.mountedElement;
  }

  private enableLigatures(
    terminal: Terminal,
    element: HTMLElement,
  ): Promise<void> {
    return loadLigaturesAddonModule()
      .then(({ LigaturesAddon }) => {
        if (this.terminal !== terminal) {
          return;
        }
        const addon = new LigaturesAddon({
          fontFeatureSettings: TERMINAL_FONT_FEATURE_SETTINGS,
        });
        terminal.loadAddon(addon);
        this.ligaturesAddon = addon;
        element.dataset.agentmuxTerminalLigatures = "true";
        this.webglAddon?.clearTextureAtlas();
        terminal.refresh(0, terminal.rows - 1);
      })
      .catch(() => {
        if (this.terminal === terminal) {
          element.dataset.agentmuxTerminalLigatures = "false";
        }
      });
  }

  private createOscLinkHandler(): ILinkHandler {
    return {
      allowNonHttpProtocols: false,
      activate: (event, text) => {
        this.openTerminalLink(text, event);
      },
    };
  }

  private installPlainUrlLinkProvider(terminal: Terminal): void {
    this.linkProviderDisposable?.dispose();
    const provider: ILinkProvider = {
      provideLinks: (bufferLineNumber, callback) => {
        const line = terminal.buffer.active.getLine(bufferLineNumber - 1);
        if (!line) {
          callback(undefined);
          return;
        }
        const lineText = line.translateToString(true);
        const links: ILink[] = extractTerminalLinks(lineText).map(
          ({ url, startIndex, endIndex }) => ({
            text: url,
            range: {
              start: { x: startIndex + 1, y: bufferLineNumber },
              end: { x: endIndex, y: bufferLineNumber },
            },
            decorations: {
              pointerCursor: true,
              underline: true,
            },
            activate: (event, text) => {
              this.openTerminalLink(text, event);
            },
          }),
        );
        callback(links.length > 0 ? links : undefined);
      },
    };
    this.linkProviderDisposable = terminal.registerLinkProvider(provider);
  }

  private openTerminalLink(text: string, event: MouseEvent): void {
    const url = normalizeTerminalUrl(text);
    const handler = this.openLinkHandler;
    if (!url || !handler) {
      return;
    }
    // Keep terminal clicks safe for focus/selection/TUI mouse input. Windows
    // users get Ctrl-click; macOS/server-preview users get Cmd-click.
    if (!event.ctrlKey && !event.metaKey) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    handler(url, event);
  }

  // Record a cursor-repositioning event into the ring buffer.
  private _siRecordEvent(): void {
    this._siRing[this._siHead] = Date.now();
    this._siHead = (this._siHead + 1) % SCREEN_INTERACTIVE_RING_SIZE;
  }

  // Returns true when a full-screen TUI appears to be active in the terminal.
  // Heuristic: >= SCREEN_INTERACTIVE_MIN_EVENTS absolute cursor-repositioning
  // sequences were observed within the last SCREEN_INTERACTIVE_WINDOW_MS ms.
  // When the TUI exits, repaints stop, and this decays to false within 2 s.
  private screenInteractiveActive(): boolean {
    const cutoff = Date.now() - SCREEN_INTERACTIVE_WINDOW_MS;
    let count = 0;
    for (let i = 0; i < SCREEN_INTERACTIVE_RING_SIZE; i++) {
      if (this._siRing[i] > cutoff) {
        count++;
        if (count >= SCREEN_INTERACTIVE_MIN_EVENTS) {
          return true;
        }
      }
    }
    return false;
  }

  private handleWheelEvent(
    terminal: Terminal,
    element: HTMLElement,
    event: WheelEvent,
  ): boolean {
    if (this.terminal !== terminal || event.ctrlKey || event.metaKey) {
      return true;
    }

    const lines = wheelDeltaLines(event, terminal.rows);
    if (lines === 0) {
      return true;
    }

    const direction = event.deltaY > 0 ? 1 : -1;
    const buffer = terminal.buffer.active;
    if (buffer.type === "normal") {
      // Screen-interactive heuristic: while a full-screen TUI (e.g. codex) is
      // live under ConPTY, xterm's buffer type stays "normal" because ConPTY
      // swallows all DECSET mode changes (1049/1047 alt-screen, 1007 alt-scroll)
      // before they reach us.  The xterm scrollback in this state contains only
      // stale pre-TUI frames, so scrolling it is useless.  Instead, detect an
      // active TUI by counting absolute cursor-repositioning sequences (CUP/HVP/
      // CUU) that TUIs emit on every repaint, and synthesize DECCKM-aware cursor
      // keys — the same input that actually moves the TUI's selection.  Users
      // who chose the "page" wheel mode get paging keys here instead, matching
      // the alt-buffer override.  When the TUI exits, repaints stop and the
      // heuristic decays within 2 s, restoring normal scrollback behaviour
      // (including for "page" mode, which never repurposes shell scrollback).
      if (this.screenInteractiveActive()) {
        if (this.alternateWheelMode === "page") {
          terminal.input(direction < 0 ? PAGE_UP_SEQUENCE : PAGE_DOWN_SEQUENCE, true);
        } else {
          const app = terminal.modes.applicationCursorKeysMode;
          const key =
            direction < 0 ? (app ? "\x1bOA" : "\x1b[A") : (app ? "\x1bOB" : "\x1b[B");
          terminal.input(key.repeat(lines), true);
        }
        // No scrollbar: nothing moves in the viewport; showing it would mislead.
        event.preventDefault();
        event.stopPropagation();
        return false;
      }

      const hasScrollback = buffer.baseY > 0;
      const canScrollUp = hasScrollback && direction < 0 && buffer.viewportY > 0;
      const canScrollDown =
        hasScrollback && direction > 0 && buffer.viewportY < buffer.baseY;
      const canScroll = canScrollUp || canScrollDown;
      if (canScroll) {
        terminal.scrollLines(direction * lines);
        this.showTransientScrollbar(element);
      }
      event.preventDefault();
      event.stopPropagation();
      return false;
    }

    if (
      this.alternateWheelMode !== "page" &&
      terminal.modes.mouseTrackingMode !== "none"
    ) {
      return true;
    }

    if (this.alternateWheelMode === "page") {
      terminal.input(direction < 0 ? PAGE_UP_SEQUENCE : PAGE_DOWN_SEQUENCE, true);
    } else {
      // Alternate-scroll semantics: synthesize cursor keys per wheel line
      // (honoring DECCKM), matching what conhost does for alt-screen apps.
      // Note: ConPTY strips DECSET mode changes (including 1007 alt-scroll and
      // 1049 alt-screen) before they reach us, so for native PowerShell/cmd
      // sessions the buffer type stays "normal" even inside a TUI — that case
      // is handled above by screenInteractiveActive().  This branch fires only
      // for tmux-control sessions (raw bytes pass through, alt-screen is
      // visible) and similar transparent backends.  PageUp/PageDown is ignored
      // by most TUIs, so cursor keys are the right synthesised input here too.
      const app = terminal.modes.applicationCursorKeysMode;
      const key =
        direction < 0 ? (app ? "\x1bOA" : "\x1b[A") : (app ? "\x1bOB" : "\x1b[B");
      terminal.input(key.repeat(lines), true);
    }
    this.showTransientScrollbar(element);
    event.preventDefault();
    event.stopPropagation();
    return false;
  }

  private showTransientScrollbar(element: HTMLElement): void {
    if (this.scrollbarHideTimer !== undefined) {
      window.clearTimeout(this.scrollbarHideTimer);
      this.scrollbarHideTimer = undefined;
    }
    element.dataset.agentmuxTerminalScrolling = "true";
    this.scrollbarHideTimer = window.setTimeout(() => {
      if (this.mountedElement === element) {
        element.dataset.agentmuxTerminalScrolling = "false";
      }
      this.scrollbarHideTimer = undefined;
    }, TRANSIENT_SCROLLBAR_MS);
  }

  private clearTransientScrollbar(): void {
    if (this.scrollbarHideTimer !== undefined) {
      window.clearTimeout(this.scrollbarHideTimer);
      this.scrollbarHideTimer = undefined;
    }
    if (this.mountedElement) {
      this.mountedElement.dataset.agentmuxTerminalScrolling = "false";
    }
  }

  private handleClipboardKey(terminal: Terminal, event: KeyboardEvent): boolean {
    if (event.type !== "keydown" || this.terminal !== terminal) {
      return true;
    }

    // Ctrl+Tab / Ctrl+Shift+Tab are app-level tab-cycling shortcuts.
    // Return false without preventDefault/stopPropagation so xterm skips PTY
    // input (\t) while the event still bubbles to the window-level listener.
    if (event.ctrlKey && !event.altKey && !event.metaKey && event.key === "Tab") {
      return false;
    }

    const key = event.key.toLowerCase();
    const primaryModifier = event.ctrlKey || event.metaKey;
    if (!event.altKey && primaryModifier && key === "c") {
      const hasSelection = terminal.getSelection().length > 0;
      // Explicit copy: Ctrl+Shift+C always, and Cmd+C with a selection on
      // macOS-style layouts (metaKey never doubles as SIGINT).
      if (event.shiftKey || (event.metaKey && !event.ctrlKey && hasSelection)) {
        event.preventDefault();
        event.stopPropagation();
        this.copySelection(terminal);
        return false;
      }
      // Windows Terminal semantics: with a visible selection Ctrl+C copies and
      // DISMISSES it — no SIGINT alongside the copy. The selection is cleared
      // so the very next Ctrl+C reaches the shell; without a selection Ctrl+C
      // interrupts as usual. (Copy-on-select already put the text on the
      // clipboard, so the copy here is a no-op refresh of the same content.)
      if (hasSelection) {
        event.preventDefault();
        event.stopPropagation();
        this.copySelection(terminal);
        terminal.clearSelection();
        return false;
      }
      return true;
    }

    if (!event.altKey && primaryModifier && key === "v") {
      event.preventDefault();
      event.stopPropagation();
      this.pasteFromClipboard(terminal);
      return false;
    }

    if (!event.altKey && event.shiftKey && key === "insert") {
      event.preventDefault();
      event.stopPropagation();
      this.pasteFromClipboard(terminal);
      return false;
    }

    if (!event.altKey && event.ctrlKey && key === "insert") {
      event.preventDefault();
      event.stopPropagation();
      this.copySelection(terminal);
      return false;
    }

    // TS-1: scrollback paging — normal buffer only; pass through in alt buffer
    // or while a full-screen TUI is active (screenInteractiveActive).
    if (
      !event.altKey &&
      !event.ctrlKey &&
      event.shiftKey &&
      (event.key === "PageUp" || event.key === "PageDown")
    ) {
      if (terminal.buffer.active.type === "normal" && !this.screenInteractiveActive()) {
        event.preventDefault();
        event.stopPropagation();
        terminal.scrollPages(event.key === "PageUp" ? -1 : 1);
        if (this.mountedElement) {
          this.showTransientScrollbar(this.mountedElement);
        }
        return false;
      }
      // Alt buffer or TUI active — let xterm forward the key to the PTY.
      return true;
    }

    if (
      !event.altKey &&
      event.ctrlKey &&
      !event.shiftKey &&
      (event.key === "Home" || event.key === "End")
    ) {
      if (terminal.buffer.active.type === "normal" && !this.screenInteractiveActive()) {
        event.preventDefault();
        event.stopPropagation();
        if (event.key === "Home") {
          terminal.scrollToTop();
        } else {
          terminal.scrollToBottom();
        }
        if (this.mountedElement) {
          this.showTransientScrollbar(this.mountedElement);
        }
        return false;
      }
      return true;
    }

    // TS-4: Ctrl+Shift+K — clear terminal buffer; never sends to PTY.
    if (!event.altKey && event.ctrlKey && event.shiftKey && key === "k") {
      event.preventDefault();
      event.stopPropagation();
      terminal.clear();
      return false;
    }

    // TS-6: Ctrl+Shift+A — select all; copy-on-select handles the copy.
    if (!event.altKey && event.ctrlKey && event.shiftKey && key === "a") {
      event.preventDefault();
      event.stopPropagation();
      terminal.selectAll();
      return false;
    }

    // TS-9: F3 / Shift+F3 — find next / previous using last search term.
    if (!event.altKey && !event.ctrlKey && !event.shiftKey && event.key === "F3") {
      event.preventDefault();
      event.stopPropagation();
      if (this._lastSearchTerm) {
        this.findNext(this._lastSearchTerm);
      }
      return false;
    }

    if (!event.altKey && !event.ctrlKey && event.shiftKey && event.key === "F3") {
      event.preventDefault();
      event.stopPropagation();
      if (this._lastSearchTerm) {
        this.findPrevious(this._lastSearchTerm);
      }
      return false;
    }

    return true;
  }

  private handleContextMenuPasteOrCopy(
    terminal: Terminal,
    event: MouseEvent
  ): void {
    if (this.terminal !== terminal) {
      return;
    }
    const selected = terminal.getSelection();
    event.preventDefault();
    if (selected) {
      this.copySelection(terminal);
      return;
    }
    this.pasteFromClipboard(terminal);
  }

  private handleCopyEvent(terminal: Terminal, event: ClipboardEvent): void {
    if (this.terminal !== terminal) {
      return;
    }
    const selected = terminal.getSelection();
    if (!selected) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    rememberTerminalClipboardText(selected);
    event.clipboardData?.setData("text/plain", selected);
    terminal.focus();
  }

  private handlePasteEvent(terminal: Terminal, event: ClipboardEvent): void {
    if (this.terminal !== terminal) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const text = event.clipboardData?.getData("text/plain") ?? "";
    if (text) {
      this.emitPaste(text);
      terminal.focus();
      return;
    }
    this.pasteFromClipboard(terminal);
  }

  private copySelection(terminal: Terminal): void {
    const selected = terminal.getSelection();
    if (!selected) {
      terminal.focus();
      return;
    }
    rememberTerminalClipboardText(selected);
    void writeClipboardText(selected)
      .catch((error) => {
        console.warn("[agentmux] terminal copy failed", { error });
      })
      .finally(() => {
        if (this.terminal === terminal) {
          terminal.focus();
        }
      });
  }

  private pasteFromClipboard(terminal: Terminal): void {
    void readClipboardText()
      .then(({ ok, text }) => {
        // Only fall back to the 30-s cache when every read path errored.
        // A successful read with empty text means the clipboard holds
        // non-text content (image, file…) — pasting nothing is correct.
        const pasteText = ok ? text : recentTerminalClipboardText();
        if (this.terminal === terminal && pasteText) {
          this.emitPaste(pasteText);
        }
      })
      .catch((error) => {
        console.warn("[agentmux] terminal paste failed", { error });
      })
      .finally(() => {
        if (this.terminal === terminal) {
          terminal.focus();
        }
      });
  }

  private emitPaste(text: string): void {
    if (!text) {
      return;
    }
    // TS-12: normalize newlines before handlers see the text.
    // \r\n → \r, lone \n → \r (matching xterm.paste semantics for PTY input).
    const normalized = text.replace(/\r\n/g, "\r").replace(/\n/g, "\r");

    // TS-11: multi-line paste guard — confirm before sending >1 line.
    // Trigger when the normalized text has a \r that is not the sole trailing
    // character — i.e. there is content on more than one line.  A single
    // trailing \r (pressing Enter at end) does not count.
    const hasMultipleLines = /\r[\s\S]/.test(normalized);
    if (_multilinePasteGuardEnabled && hasMultipleLines) {
      const preview = text.slice(0, 120);
      const message = `여러 줄을 붙여넣습니다. 계속할까요?\n\n${preview}`;
      if (!window.confirm(message)) {
        return;
      }
    }

    for (const handler of this.pasteHandlers) {
      handler(normalized);
    }
  }

  // TS-4: clear the terminal buffer. Never sends anything to the PTY.
  clearBuffer(): void {
    this.terminal?.clear();
  }

  // TS-6: select all content in the terminal buffer.
  selectAll(): void {
    this.terminal?.selectAll();
  }

  // TS-9: in-buffer search via SearchAddon.
  private static readonly _searchOptions: ISearchOptions = {
    decorations: {
      matchBackground: "#ffff0040",
      matchBorder: "#ffff00",
      matchOverviewRuler: "#ffff00",
      activeMatchBackground: "#ff800080",
      activeMatchBorder: "#ff8000",
      activeMatchColorOverviewRuler: "#ff8000",
    },
  };

  findNext(term: string): boolean {
    if (!term || !this.searchAddon) {
      return false;
    }
    this._lastSearchTerm = term;
    return this.searchAddon.findNext(term, XtermTerminalRenderer._searchOptions);
  }

  findPrevious(term: string): boolean {
    if (!term || !this.searchAddon) {
      return false;
    }
    this._lastSearchTerm = term;
    return this.searchAddon.findPrevious(term, XtermTerminalRenderer._searchOptions);
  }

  // Used by the command registry in LiveTerminal.
  scrollToBottom(): void {
    this.terminal?.scrollToBottom();
  }
}
