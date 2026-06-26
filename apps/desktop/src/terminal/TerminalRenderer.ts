export interface TerminalSnapshot {
  bytes?: Uint8Array;
  columns: number;
  rows: number;
}

export interface TerminalTypography {
  fontSize: number;
  lineHeight: number;
}

export interface TerminalRenderer {
  mount(
    element: HTMLElement,
    initialState: TerminalSnapshot,
    typography?: Partial<TerminalTypography>,
  ): void;
  unmount(): void;
  write(batch: Uint8Array, callback?: () => void): void;
  resize(columns: number, rows: number): void;
  size(): { columns: number; rows: number } | null;
  setTypography(typography: Partial<TerminalTypography>): void;
  onData(handler: (data: string) => void): () => void;
  onPaste(handler: (text: string) => void): () => void;
  onResize(handler: (columns: number, rows: number) => void): () => void;
  focus(): void;
  dispose(): void;
}

export type TerminalInputEvent =
  | { type: "text"; text: string }
  | { type: "paste"; text: string; bracketed?: boolean }
  | { type: "key"; key: string };
