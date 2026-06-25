import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import type { WebglAddon } from "@xterm/addon-webgl";
import "@xterm/xterm/css/xterm.css";
import type { TerminalRenderer, TerminalSnapshot } from "./TerminalRenderer";

export const XTERM_THEME = {
  background: "#0e1116",
  foreground: "#d7dde7",
  cursor: "#f1cf89",
  selectionBackground: "#2d5f73"
} as const;

const TERMINAL_PRIMARY_FONT = "D2Coding Nerd";
const TERMINAL_SYMBOL_FONT = "Symbols Nerd Font Mono";
const TERMINAL_FONT_FAMILY = [
  '"CaskaydiaCove Nerd Font Mono"',
  '"CaskaydiaCove Nerd Font"',
  '"MesloLGS NF"',
  '"JetBrainsMono Nerd Font Mono"',
  '"JetBrainsMono Nerd Font"',
  '"FiraCode Nerd Font Mono"',
  '"Symbols Nerd Font Mono"',
  '"D2Coding Nerd"',
  '"Cascadia Mono"',
  "Consolas",
  '"Liberation Mono"',
  "monospace"
].join(", ");
const TERMINAL_FONT_SIZE = 13;
type WebglAddonModule = typeof import("@xterm/addon-webgl");

let webglAddonModulePromise: Promise<WebglAddonModule> | undefined;

function loadWebglAddonModule(): Promise<WebglAddonModule> {
  if (!webglAddonModulePromise) {
    webglAddonModulePromise = import("@xterm/addon-webgl").catch((error) => {
      webglAddonModulePromise = undefined;
      throw error;
    });
  }
  return webglAddonModulePromise;
}

export class XtermTerminalRenderer implements TerminalRenderer {
  private terminal?: Terminal;
  private fitAddon?: FitAddon;
  private mountedElement?: HTMLElement;
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

  mount(element: HTMLElement, initialState: TerminalSnapshot): void {
    this.dispose();

    const terminal = new Terminal({
      convertEol: true,
      cursorBlink: true,
      fontFamily: TERMINAL_FONT_FAMILY,
      fontSize: TERMINAL_FONT_SIZE,
      lineHeight: 1.15,
      rows: initialState.rows,
      cols: initialState.columns,
      theme: XTERM_THEME
    });
    const fitAddon = new FitAddon();

    terminal.loadAddon(fitAddon);
    terminal.open(element);
    fitAddon.fit();

    if (initialState.bytes && initialState.bytes.length > 0) {
      terminal.write(initialState.bytes);
    }

    this.terminal = terminal;
    this.fitAddon = fitAddon;
    this.mountedElement = element;

    // The Nerd font (@font-face) loads lazily, so xterm's first glyph
    // measurement can use fallback metrics — leaving icons/powerline glyphs
    // blank or misaligned. Once the face is ready, re-measure: drop the WebGL
    // texture atlas, re-apply the family to force a glyph re-measure, refit.
    this.fontReadyPromise = this.ensureFontsThenRemeasure(terminal);
  }

  private ensureFontsThenRemeasure(terminal: Terminal): Promise<void> {
    const fonts = (document as Document & { fonts?: FontFaceSet }).fonts;
    const fontLoads = fonts?.load
      ? Promise.allSettled([
          fonts.load(`${TERMINAL_FONT_SIZE}px "${TERMINAL_PRIMARY_FONT}"`),
          fonts.load(`${TERMINAL_FONT_SIZE}px "${TERMINAL_SYMBOL_FONT}"`)
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
    this.terminal?.dispose();
    this.terminal = undefined;
    this.fitAddon = undefined;
    this.mountedElement = undefined;
    this.fontReadyPromise = undefined;
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

  onData(handler: (data: string) => void): () => void {
    const disposable = this.terminal?.onData(handler);
    return () => disposable?.dispose();
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
    void (this.fontReadyPromise ?? Promise.resolve())
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
}
