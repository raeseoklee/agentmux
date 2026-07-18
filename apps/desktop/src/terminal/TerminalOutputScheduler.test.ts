import { afterEach, describe, expect, it, vi } from "vitest";
import {
  getTerminalOutputStats,
  resetTerminalOutput,
  resetTerminalOutputSchedulerForTests,
  setTerminalOutputForeground,
  TERMINAL_OUTPUT_LIMITS,
  writeTerminalOutput,
  type TerminalOutputTarget,
} from "./TerminalOutputScheduler";

class FakeTerminal implements TerminalOutputTarget {
  readonly writes: Uint8Array[] = [];
  readonly completions: Array<() => void> = [];
  throwOnWrite = false;

  constructor(private readonly onWrite?: () => void) {}

  write(data: Uint8Array, callback?: () => void) {
    if (this.throwOnWrite) {
      throw new Error("write failed");
    }
    this.onWrite?.();
    this.writes.push(data);
    if (callback) {
      this.completions.push(callback);
    }
  }

  completeNext() {
    this.completions.shift()?.();
  }
}

afterEach(() => {
  resetTerminalOutputSchedulerForTests();
  vi.useRealTimers();
});

describe("TerminalOutputScheduler", () => {
  it("writes small foreground output immediately", () => {
    vi.useFakeTimers();
    const terminal = new FakeTerminal();
    const parsed = vi.fn();

    writeTerminalOutput(terminal, new Uint8Array([1, 2, 3]), {
      foreground: true,
      onParsed: parsed,
    });

    expect(terminal.writes).toHaveLength(1);
    expect(parsed).not.toHaveBeenCalled();
    terminal.completeNext();
    expect(parsed).toHaveBeenCalledWith(3);
    expect(getTerminalOutputStats(terminal)).toMatchObject({
      writeCount: 1,
      parsedBytes: 3,
      recoveryCount: 0,
    });
  });

  it("coalesces background output before draining", () => {
    vi.useFakeTimers();
    const terminal = new FakeTerminal();

    writeTerminalOutput(terminal, new Uint8Array([1, 2]), {
      foreground: false,
    });
    writeTerminalOutput(terminal, new Uint8Array([3, 4]), {
      foreground: false,
    });

    vi.advanceTimersByTime(
      TERMINAL_OUTPUT_LIMITS.backgroundFlushDelayMs - 1,
    );
    expect(terminal.writes).toHaveLength(0);
    vi.advanceTimersByTime(1);
    expect(terminal.writes).toHaveLength(1);
    expect([...terminal.writes[0]]).toEqual([1, 2, 3, 4]);
  });

  it("prioritizes foreground output over an older background queue", () => {
    vi.useFakeTimers();
    const order: string[] = [];
    const background = new FakeTerminal(() => order.push("background"));
    const foreground = new FakeTerminal(() => order.push("foreground"));

    writeTerminalOutput(background, new Uint8Array([1]), {
      foreground: false,
    });
    writeTerminalOutput(
      foreground,
      new Uint8Array(TERMINAL_OUTPUT_LIMITS.foregroundImmediateBytes + 1),
      { foreground: true },
    );

    vi.advanceTimersByTime(0);
    expect(foreground.writes).toHaveLength(1);
    expect(background.writes).toHaveLength(1);
    expect(order).toEqual(["foreground", "background"]);
  });

  it("flushes a queued pane as soon as it becomes foreground", () => {
    vi.useFakeTimers();
    const terminal = new FakeTerminal();

    writeTerminalOutput(terminal, new Uint8Array([1]), {
      foreground: false,
    });
    setTerminalOutputForeground(terminal, true);
    vi.advanceTimersByTime(0);

    expect(terminal.writes).toHaveLength(1);
  });

  it("cancels delayed output before a snapshot reset", () => {
    vi.useFakeTimers();
    const terminal = new FakeTerminal();

    writeTerminalOutput(terminal, new Uint8Array([1]), {
      foreground: false,
    });
    resetTerminalOutput(terminal);
    vi.advanceTimersByTime(TERMINAL_OUTPUT_LIMITS.backgroundFlushDelayMs);

    expect(terminal.writes).toHaveLength(0);
  });

  it("bounds queued output and requests snapshot recovery", () => {
    vi.useFakeTimers();
    const terminal = new FakeTerminal();
    const recover = vi.fn();

    writeTerminalOutput(
      terminal,
      new Uint8Array(TERMINAL_OUTPUT_LIMITS.maxQueuedBytes + 1),
      { foreground: false, onRecoveryRequired: recover },
    );

    expect(recover).toHaveBeenCalledWith("backlog-overflow");
    expect(getTerminalOutputStats(terminal)).toMatchObject({
      queuedBytes: 0,
      recovering: true,
    });
    resetTerminalOutput(terminal);
    expect(getTerminalOutputStats(terminal).recovering).toBe(false);
  });

  it("requests recovery when xterm never completes a write", () => {
    vi.useFakeTimers();
    const terminal = new FakeTerminal();
    const recover = vi.fn();

    writeTerminalOutput(terminal, new Uint8Array([1]), {
      foreground: true,
      onRecoveryRequired: recover,
    });
    vi.advanceTimersByTime(TERMINAL_OUTPUT_LIMITS.writeStallMs);

    expect(recover).toHaveBeenCalledWith("write-stall");
    expect(getTerminalOutputStats(terminal)).toMatchObject({
      writeInFlight: false,
      recovering: true,
      recoveryCount: 1,
    });
  });

  it("contains parser callback failures", () => {
    vi.useFakeTimers();
    const terminal = new FakeTerminal();

    writeTerminalOutput(terminal, new Uint8Array([1]), {
      foreground: true,
      onParsed: () => {
        throw new Error("consumer callback failed");
      },
    });

    expect(() => terminal.completeNext()).not.toThrow();
  });

  it("turns synchronous xterm write errors into recovery", () => {
    vi.useFakeTimers();
    const terminal = new FakeTerminal();
    terminal.throwOnWrite = true;
    const recover = vi.fn();

    writeTerminalOutput(terminal, new Uint8Array([1]), {
      foreground: true,
      onRecoveryRequired: recover,
    });

    expect(recover).toHaveBeenCalledWith("write-error");
    expect(getTerminalOutputStats(terminal).recovering).toBe(true);
  });
});
