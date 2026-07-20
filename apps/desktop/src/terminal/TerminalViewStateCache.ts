export interface TerminalViewState {
  serialized: string;
  outputOffset: number;
  updatedAt: number;
}

/**
 * Keeps xterm framebuffer state alive across React remounts caused by tab,
 * workspace, and pane moves. This is intentionally memory-only: process
 * restart recovery remains owned by the backend output snapshot. Entries are
 * removed only when their session lifecycle ends; silently evicting a live
 * terminal would reduce its recoverable history to the backend's bounded ring.
 */
export class TerminalViewStateCache {
  private readonly entries = new Map<string, TerminalViewState>();

  read(sessionId: string): TerminalViewState | null {
    const entry = this.entries.get(sessionId);
    return entry ?? null;
  }

  write(sessionId: string, state: TerminalViewState): boolean {
    if (
      !sessionId ||
      !state.serialized ||
      !Number.isSafeInteger(state.outputOffset) ||
      state.outputOffset < 0
    ) {
      return false;
    }
    this.entries.set(sessionId, state);
    return true;
  }

  delete(sessionId: string): void {
    this.entries.delete(sessionId);
  }

  deleteMany(sessionIds: Iterable<string>): void {
    for (const sessionId of sessionIds) {
      this.entries.delete(sessionId);
    }
  }
}

export const terminalViewStateCache = new TerminalViewStateCache();
