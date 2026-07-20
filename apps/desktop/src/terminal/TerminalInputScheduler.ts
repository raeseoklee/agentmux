export interface TerminalInputTransport {
  sendText(text: string): Promise<void>;
  sendPaste(text: string): Promise<void>;
}

export interface TerminalInputSchedulerOptions {
  onDelivered?: () => void;
  onError?: (error: unknown) => void;
}

type TerminalInputOperation =
  | { kind: "text"; text: string }
  | { kind: "paste"; text: string };

/**
 * Preserves PTY input order while coalescing adjacent keystrokes that arrive
 * during an in-flight IPC write. No input is dropped and paste boundaries stay
 * explicit so bracketed-paste behavior remains correct.
 */
export class TerminalInputScheduler {
  private readonly pending: TerminalInputOperation[] = [];
  private readonly idleWaiters = new Set<() => void>();
  private scheduled = false;
  private draining = false;
  private accepting = true;

  constructor(
    private readonly transport: TerminalInputTransport,
    private readonly options: TerminalInputSchedulerOptions = {},
  ) {}

  enqueueText(text: string): void {
    if (!this.accepting || text.length === 0) {
      return;
    }
    const last = this.pending.at(-1);
    if (last?.kind === "text") {
      last.text += text;
    } else {
      this.pending.push({ kind: "text", text });
    }
    this.schedule();
  }

  enqueuePaste(text: string): void {
    if (!this.accepting || text.length === 0) {
      return;
    }
    this.pending.push({ kind: "paste", text });
    this.schedule();
  }

  /** Stop accepting input while allowing already queued bytes to finish. */
  close(): void {
    this.accepting = false;
    if (!this.draining && !this.scheduled && this.pending.length === 0) {
      this.resolveIdleWaiters();
    }
  }

  waitForIdle(): Promise<void> {
    if (!this.draining && !this.scheduled && this.pending.length === 0) {
      return Promise.resolve();
    }
    return new Promise((resolve) => this.idleWaiters.add(resolve));
  }

  private schedule(): void {
    if (this.scheduled || this.draining) {
      return;
    }
    this.scheduled = true;
    queueMicrotask(() => {
      this.scheduled = false;
      void this.drain();
    });
  }

  private async drain(): Promise<void> {
    if (this.draining) {
      return;
    }
    this.draining = true;
    try {
      while (this.pending.length > 0) {
        const operation = this.pending.shift();
        if (!operation) {
          continue;
        }
        try {
          if (operation.kind === "paste") {
            await this.transport.sendPaste(operation.text);
          } else {
            await this.transport.sendText(operation.text);
          }
          this.options.onDelivered?.();
        } catch (error) {
          this.options.onError?.(error);
        }
      }
    } finally {
      this.draining = false;
      if (this.pending.length > 0) {
        this.schedule();
      } else {
        this.resolveIdleWaiters();
      }
    }
  }

  private resolveIdleWaiters(): void {
    for (const resolve of this.idleWaiters) {
      resolve();
    }
    this.idleWaiters.clear();
  }
}
