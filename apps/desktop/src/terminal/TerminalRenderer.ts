export interface TerminalSnapshot {
  bytes?: Uint8Array;
  columns: number;
  rows: number;
}

export interface TerminalRenderer {
  mount(element: HTMLElement, initialState: TerminalSnapshot): void;
  unmount(): void;
  write(batch: Uint8Array): void;
  resize(columns: number, rows: number): void;
  onData(handler: (data: string) => void): () => void;
  onResize(handler: (columns: number, rows: number) => void): () => void;
  focus(): void;
  dispose(): void;
}

export type TerminalInputEvent =
  | { type: "text"; text: string }
  | { type: "paste"; text: string; bracketed?: boolean }
  | { type: "key"; key: string };
