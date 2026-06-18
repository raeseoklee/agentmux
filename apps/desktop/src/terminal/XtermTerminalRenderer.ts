import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import type { TerminalRenderer, TerminalSnapshot } from "./TerminalRenderer";

export class XtermTerminalRenderer implements TerminalRenderer {
  private terminal?: Terminal;
  private fitAddon?: FitAddon;
  private mountedElement?: HTMLElement;

  mount(element: HTMLElement, initialState: TerminalSnapshot): void {
    this.dispose();

    const terminal = new Terminal({
      convertEol: true,
      cursorBlink: true,
      fontFamily: '"Cascadia Mono", Consolas, "Liberation Mono", monospace',
      fontSize: 13,
      lineHeight: 1.15,
      rows: initialState.rows,
      cols: initialState.columns,
      theme: {
        background: "#0e1116",
        foreground: "#d7dde7",
        cursor: "#f1cf89",
        selectionBackground: "#2d5f73"
      }
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
  }

  unmount(): void {
    this.terminal?.dispose();
    this.terminal = undefined;
    this.fitAddon = undefined;
    this.mountedElement = undefined;
  }

  write(batch: Uint8Array): void {
    this.terminal?.write(batch);
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

  dispose(): void {
    this.unmount();
  }

  element(): HTMLElement | undefined {
    return this.mountedElement;
  }
}
