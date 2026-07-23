export interface TerminalGridSize {
  columns: number;
  rows: number;
}

interface TerminalResizeCoordinatorOptions {
  delayMs: number;
  send(size: TerminalGridSize): Promise<void>;
  onError?(): void;
}

function sameSize(left: TerminalGridSize | null, right: TerminalGridSize): boolean {
  return left?.columns === right.columns && left.rows === right.rows;
}

interface ResizeOperation {
  generation: number;
  size: TerminalGridSize;
}

/** Coalesces resize storms and keeps at most one backend resize in flight. */
export class TerminalResizeCoordinator {
  private readonly delayMs: number;
  private readonly send: (size: TerminalGridSize) => Promise<void>;
  private readonly onError?: () => void;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private pending: TerminalGridSize | null = null;
  private inFlight: ResizeOperation | null = null;
  private lastSuccessful: TerminalGridSize | null = null;
  private generation = 0;
  private disposed = false;
  private flushPendingWhenReady = false;

  constructor(options: TerminalResizeCoordinatorOptions) {
    this.delayMs = options.delayMs;
    this.send = options.send;
    this.onError = options.onError;
  }

  request(size: TerminalGridSize, immediate = false): void {
    if (this.disposed || size.columns <= 0 || size.rows <= 0) {
      return;
    }
    if (this.inFlight !== null) {
      // Keep the latest observed viewport while the backend applies the current
      // request. When it matches the in-flight size the follow-up becomes a
      // cheap no-op after that request succeeds, while still preserving event
      // order if another size arrives before completion.
      this.pending = size;
      this.flushPendingWhenReady ||= immediate;
      if (this.timer !== null) {
        clearTimeout(this.timer);
        this.timer = null;
      }
      return;
    }
    if (sameSize(this.lastSuccessful, size)) {
      this.pending = null;
      this.flushPendingWhenReady = false;
      if (this.timer !== null) {
        clearTimeout(this.timer);
        this.timer = null;
      }
      return;
    }

    this.pending = size;
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    if (immediate && this.inFlight === null) {
      this.flush();
      return;
    }
    this.schedule();
  }

  dispose(): void {
    this.disposed = true;
    this.generation++;
    this.pending = null;
    this.flushPendingWhenReady = false;
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }

  /** Starts a fresh resize sequence, for example after switching sessions. */
  reset(): void {
    if (this.disposed) {
      return;
    }
    this.generation++;
    this.pending = null;
    this.inFlight = null;
    this.lastSuccessful = null;
    this.flushPendingWhenReady = false;
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
  }

  private schedule(): void {
    if (this.disposed || this.timer !== null) {
      return;
    }
    this.timer = setTimeout(() => {
      this.timer = null;
      this.flush();
    }, this.delayMs);
  }

  private flush(): void {
    if (this.disposed || this.inFlight !== null) {
      return;
    }
    const next = this.pending;
    this.pending = null;
    if (!next || sameSize(this.lastSuccessful, next)) {
      this.flushPendingWhenReady = false;
      return;
    }

    const generation = this.generation;
    const operation: ResizeOperation = { generation, size: next };
    this.inFlight = operation;
    void this.send(next)
      .then(() => {
        if (!this.disposed && this.generation === generation) {
          this.lastSuccessful = next;
        }
      })
      .catch(() => {
        if (!this.disposed && this.generation === generation) {
          this.onError?.();
        }
      })
      .finally(() => {
        if (this.inFlight === operation) {
          this.inFlight = null;
          if (this.pending && !this.disposed) {
            if (this.flushPendingWhenReady) {
              this.flushPendingWhenReady = false;
              this.flush();
            } else {
              this.schedule();
            }
          }
        }
      });
  }
}
