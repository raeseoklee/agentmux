export interface TerminalSnapshot {
  bytes?: Uint8Array;
  columns: number;
  rows: number;
}

export interface TerminalTypography {
  fontSize: number;
  lineHeight: number;
}

export type AlternateWheelMode = "auto" | "page";

export interface TerminalRenderer {
  mount(
    element: HTMLElement,
    initialState: TerminalSnapshot,
    typography?: Partial<TerminalTypography>,
  ): Promise<void>;
  unmount(): void;
  write(batch: Uint8Array, callback?: () => void): void;
  resize(columns: number, rows: number): void;
  size(): { columns: number; rows: number } | null;
  setTypography(typography: Partial<TerminalTypography>): void;
  setAlternateWheelMode?(mode: AlternateWheelMode): void;
  onData(handler: (data: string) => void): () => void;
  onPaste(handler: (text: string) => void): () => void;
  onPastePaths?(handler: (paths: string[]) => void): () => void;
  onResize(handler: (columns: number, rows: number) => void): () => void;
  focus(): void;
  dispose(): void;
  /** Clear the terminal scrollback and visible buffer (TS-4). Never sends to PTY. */
  clearBuffer?(): void;
  /** Select all content in the terminal buffer (TS-6). */
  selectAll?(): void;
  /** Find the next match for term in the buffer; returns true when found (TS-9). */
  findNext?(term: string): boolean;
  /** Find the previous match for term in the buffer; returns true when found (TS-9). */
  findPrevious?(term: string): boolean;
  /** Scroll to the bottom of the terminal buffer (used by command registry). */
  scrollToBottom?(): void;
}

export type TerminalInputEvent =
  | { type: "text"; text: string }
  | { type: "paste"; text: string; bracketed?: boolean }
  | { type: "key"; key: string };
