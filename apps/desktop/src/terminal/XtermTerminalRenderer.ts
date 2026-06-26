import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import type { LigaturesAddon } from "@xterm/addon-ligatures";
import type { WebglAddon } from "@xterm/addon-webgl";
import "@xterm/xterm/css/xterm.css";
import type {
  TerminalRenderer,
  TerminalSnapshot,
  TerminalTypography,
} from "./TerminalRenderer";

export const XTERM_THEME = {
  background: "#0e1116",
  foreground: "#d7dde7",
  cursor: "#f1cf89",
  selectionBackground: "#2d5f73"
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
type WebglAddonModule = typeof import("@xterm/addon-webgl");
type LigaturesAddonModule = typeof import("@xterm/addon-ligatures");
type TauriClipboardModule = typeof import("@tauri-apps/plugin-clipboard-manager");

let webglAddonModulePromise: Promise<WebglAddonModule> | undefined;
let ligaturesAddonModulePromise: Promise<LigaturesAddonModule> | undefined;
let tauriClipboardModulePromise: Promise<TauriClipboardModule> | undefined;

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

async function writeClipboardText(text: string): Promise<void> {
  if (!text) {
    return;
  }
  if (isTauriRuntime()) {
    try {
      const clipboard = await loadTauriClipboardModule();
      await clipboard.writeText(text);
      return;
    } catch {
      // Fall through to browser clipboard paths for preview or plugin errors.
    }
  }
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return;
    }
  } catch {
    // Fall through to the hidden-textarea path used by older WebViews.
  }
  if (!fallbackWriteClipboardText(text)) {
    throw new Error("clipboard copy failed");
  }
}

async function readClipboardText(): Promise<string> {
  if (isTauriRuntime()) {
    try {
      const clipboard = await loadTauriClipboardModule();
      return await clipboard.readText();
    } catch {
      // Fall through to browser clipboard paths for preview or plugin errors.
    }
  }
  try {
    if (navigator.clipboard?.readText) {
      return await navigator.clipboard.readText();
    }
  } catch {
    // Clipboard read can be denied by the host; keep the terminal focused.
  }
  return "";
}

export class XtermTerminalRenderer implements TerminalRenderer {
  private terminal?: Terminal;
  private fitAddon?: FitAddon;
  private unicodeAddon?: Unicode11Addon;
  private ligaturesAddon?: LigaturesAddon;
  private mountedElement?: HTMLElement;
  private inputEventAbort?: AbortController;
  private pasteHandlers = new Set<(text: string) => void>();
  // Active WebGL addon, when GPU rendering has been opted into and succeeded.
  private webglAddon?: WebglAddon;
  // Disposes the onContextLoss subscription tied to the current webglAddon.
  private webglContextLossSub?: { dispose(): void };
  // Guards against overlapping enableWebgl() calls while the addon module is
  // being lazily imported (import() is async, so two calls could race).
  private webglEnablePending = false;
  // Monotonic generation, bumped on every disable. A lazy enableWebgl() import()
  // captures the generation it started in and bails on resolve if a later
  // disable/enable has superseded it. Without this, a rapid
  // enable -> disable -> enable can let two in-flight imports each loadAddon(),
  // leaking a duplicate WebGL context.
  private webglGeneration = 0;
  private fontReadyPromise?: Promise<void>;
  private ligaturesReadyPromise?: Promise<void>;

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
      rows: initialState.rows,
      cols: initialState.columns,
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
    terminal.open(element);
    this.ligaturesReadyPromise = this.enableLigatures(terminal, element);
    terminal.attachCustomKeyEventHandler((event) =>
      this.handleClipboardKey(terminal, event)
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
    fitAddon.fit();

    if (initialState.bytes && initialState.bytes.length > 0) {
      terminal.write(initialState.bytes);
    }

    this.terminal = terminal;
    this.fitAddon = fitAddon;
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
    this.disposeWebglAddon();
    this.inputEventAbort?.abort();
    this.inputEventAbort = undefined;
    this.terminal?.dispose();
    this.terminal = undefined;
    this.fitAddon = undefined;
    this.unicodeAddon = undefined;
    this.ligaturesAddon = undefined;
    this.mountedElement = undefined;
    this.fontReadyPromise = undefined;
    this.ligaturesReadyPromise = undefined;
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
  enableWebgl(): void {
    const terminal = this.terminal;
    if (!terminal) {
      return;
    }
    // Already active, or an enable is mid-flight: do nothing.
    if (this.webglAddon || this.webglEnablePending) {
      return;
    }
    this.webglEnablePending = true;
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
        try {
          const addon = new WebglAddon();
          // If the GPU context is lost (driver reset, tab backgrounded too
          // long, too many live contexts), dispose the addon and drop back to
          // the DOM renderer instead of leaving a blank/frozen terminal.
          this.webglContextLossSub = addon.onContextLoss(() => {
            this.disposeWebglAddon();
          });
          terminal.loadAddon(addon);
          this.webglAddon = addon;
        } catch {
          // WebGL2 unavailable or addon initialization failed — fall back to
          // the default DOM renderer. Clean up any partial subscription.
          this.disposeWebglAddon();
        }
      })
      .catch(() => {
        // Dynamic import itself failed (offline chunk, etc.). Stay on DOM.
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
    this.disposeWebglAddon();
  }

  /**
   * Whether GPU rendering is currently active (an addon is loaded). Pending
   * enables that have not yet resolved report false.
   */
  isWebglEnabled(): boolean {
    return this.webglAddon !== undefined;
  }

  // Disposes the active WebGL addon and its context-loss subscription, leaving
  // the terminal on the DOM renderer. Idempotent.
  private disposeWebglAddon(): void {
    this.webglContextLossSub?.dispose();
    this.webglContextLossSub = undefined;
    this.webglAddon?.dispose();
    this.webglAddon = undefined;
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

  private handleClipboardKey(terminal: Terminal, event: KeyboardEvent): boolean {
    if (event.type !== "keydown" || this.terminal !== terminal) {
      return true;
    }

    const key = event.key.toLowerCase();
    const primaryModifier = event.ctrlKey || event.metaKey;
    if (!event.altKey && primaryModifier && key === "c") {
      const hasSelection = terminal.getSelection().length > 0;
      if (event.shiftKey || hasSelection) {
        event.preventDefault();
        event.stopPropagation();
        this.copySelection(terminal);
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
    void writeClipboardText(selected)
      .catch(() => {})
      .finally(() => {
        if (this.terminal === terminal) {
          terminal.focus();
        }
      });
  }

  private pasteFromClipboard(terminal: Terminal): void {
    void readClipboardText()
      .then((text) => {
        if (this.terminal === terminal && text) {
          this.emitPaste(text);
        }
      })
      .catch(() => {})
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
    for (const handler of this.pasteHandlers) {
      handler(text);
    }
  }
}
