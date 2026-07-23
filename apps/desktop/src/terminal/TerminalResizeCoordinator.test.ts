import { afterEach, describe, expect, it, vi } from "vitest";
import { TerminalResizeCoordinator } from "./TerminalResizeCoordinator";

afterEach(() => {
  vi.useRealTimers();
});

describe("TerminalResizeCoordinator", () => {
  it("does not resend a successful terminal size", async () => {
    const send = vi.fn(async () => {});
    const coordinator = new TerminalResizeCoordinator({ delayMs: 80, send });

    coordinator.request({ columns: 120, rows: 30 }, true);
    await Promise.resolve();
    coordinator.request({ columns: 120, rows: 30 }, true);

    expect(send).toHaveBeenCalledTimes(1);
  });

  it("coalesces a resize burst to the latest size", async () => {
    vi.useFakeTimers();
    const send = vi.fn(async () => {});
    const coordinator = new TerminalResizeCoordinator({ delayMs: 160, send });

    coordinator.request({ columns: 100, rows: 20 });
    coordinator.request({ columns: 110, rows: 24 });
    coordinator.request({ columns: 120, rows: 30 });
    await vi.advanceTimersByTimeAsync(159);
    expect(send).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);

    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith({ columns: 120, rows: 30 });
  });

  it("serializes backend requests and keeps only the newest pending size", async () => {
    vi.useFakeTimers();
    let resolveFirst: (() => void) | undefined;
    const send = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveFirst ??= resolve;
        }),
    );
    const coordinator = new TerminalResizeCoordinator({ delayMs: 100, send });

    coordinator.request({ columns: 100, rows: 20 }, true);
    coordinator.request({ columns: 110, rows: 24 });
    coordinator.request({ columns: 120, rows: 30 });
    await vi.advanceTimersByTimeAsync(100);
    expect(send).toHaveBeenCalledTimes(1);

    resolveFirst?.();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(100);

    expect(send).toHaveBeenCalledTimes(2);
    expect(send).toHaveBeenLastCalledWith({ columns: 120, rows: 30 });
  });

  it("flushes the final layout size immediately after an in-flight resize", async () => {
    vi.useFakeTimers();
    let resolveFirst: (() => void) | undefined;
    const send = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveFirst ??= resolve;
        }),
    );
    const coordinator = new TerminalResizeCoordinator({ delayMs: 160, send });

    coordinator.request({ columns: 100, rows: 20 }, true);
    coordinator.request({ columns: 132, rows: 34 }, true);
    resolveFirst?.();
    await vi.advanceTimersByTimeAsync(0);

    expect(send).toHaveBeenCalledTimes(2);
    expect(send).toHaveBeenLastCalledWith({ columns: 132, rows: 34 });
  });

  it("keeps simultaneously visible terminal grids independently coalesced", async () => {
    vi.useFakeTimers();
    const firstSend = vi.fn(async () => {});
    const secondSend = vi.fn(async () => {});
    const first = new TerminalResizeCoordinator({ delayMs: 100, send: firstSend });
    const second = new TerminalResizeCoordinator({ delayMs: 100, send: secondSend });

    first.request({ columns: 88, rows: 22 });
    first.request({ columns: 96, rows: 22 });
    second.request({ columns: 88, rows: 21 });
    second.request({ columns: 96, rows: 21 });
    await vi.advanceTimersByTimeAsync(100);

    expect(firstSend).toHaveBeenCalledTimes(1);
    expect(firstSend).toHaveBeenLastCalledWith({ columns: 96, rows: 22 });
    expect(secondSend).toHaveBeenCalledTimes(1);
    expect(secondSend).toHaveBeenLastCalledWith({ columns: 96, rows: 21 });
  });

  it("drops an intermediate size when the viewport returns to the in-flight size", async () => {
    vi.useFakeTimers();
    let resolveFirst: (() => void) | undefined;
    const send = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          resolveFirst ??= resolve;
        }),
    );
    const coordinator = new TerminalResizeCoordinator({ delayMs: 100, send });

    coordinator.request({ columns: 100, rows: 20 }, true);
    coordinator.request({ columns: 120, rows: 30 });
    coordinator.request({ columns: 100, rows: 20 });
    resolveFirst?.();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(100);

    expect(send).toHaveBeenCalledTimes(1);
  });

  it("returns to the last successful size after a different resize is in flight", async () => {
    vi.useFakeTimers();
    const pendingResolvers: Array<() => void> = [];
    const send = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          pendingResolvers.push(resolve);
        }),
    );
    const coordinator = new TerminalResizeCoordinator({ delayMs: 100, send });

    coordinator.request({ columns: 100, rows: 20 }, true);
    pendingResolvers.shift()?.();
    await vi.advanceTimersByTimeAsync(0);

    coordinator.request({ columns: 120, rows: 30 }, true);
    expect(send).toHaveBeenCalledTimes(2);
    coordinator.request({ columns: 100, rows: 20 });
    pendingResolvers.shift()?.();
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(100);

    expect(send).toHaveBeenCalledTimes(3);
    expect(send).toHaveBeenLastCalledWith({ columns: 100, rows: 20 });
  });

  it("allows a failed size to be retried", async () => {
    const onError = vi.fn();
    const send = vi
      .fn<() => Promise<void>>()
      .mockRejectedValueOnce(new Error("resize failed"))
      .mockResolvedValueOnce();
    const coordinator = new TerminalResizeCoordinator({
      delayMs: 80,
      send,
      onError,
    });

    coordinator.request({ columns: 120, rows: 30 }, true);
    await new Promise((resolve) => setTimeout(resolve, 0));
    coordinator.request({ columns: 120, rows: 30 }, true);
    await Promise.resolve();

    expect(send).toHaveBeenCalledTimes(2);
    expect(onError).toHaveBeenCalledTimes(1);
  });

  it("resends the same grid size after a session reset", async () => {
    const send = vi.fn(async () => {});
    const coordinator = new TerminalResizeCoordinator({ delayMs: 80, send });

    coordinator.request({ columns: 120, rows: 30 }, true);
    await new Promise((resolve) => setTimeout(resolve, 0));
    coordinator.reset();
    coordinator.request({ columns: 120, rows: 30 }, true);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(send).toHaveBeenCalledTimes(2);
  });

  it("does not let an old in-flight resize suppress a reset request", () => {
    const send = vi.fn(
      () =>
        new Promise<void>(() => {}),
    );
    const coordinator = new TerminalResizeCoordinator({ delayMs: 80, send });

    coordinator.request({ columns: 120, rows: 30 }, true);
    coordinator.reset();
    coordinator.request({ columns: 120, rows: 30 }, true);

    expect(send).toHaveBeenCalledTimes(2);
  });

  it("ignores an error from a resize started before reset", async () => {
    let rejectFirst: ((error: Error) => void) | undefined;
    const onError = vi.fn();
    const send = vi
      .fn<() => Promise<void>>()
      .mockImplementationOnce(
        () =>
          new Promise<void>((_resolve, reject) => {
            rejectFirst = reject;
          }),
      )
      .mockResolvedValueOnce();
    const coordinator = new TerminalResizeCoordinator({
      delayMs: 80,
      send,
      onError,
    });

    coordinator.request({ columns: 100, rows: 20 }, true);
    coordinator.reset();
    coordinator.request({ columns: 120, rows: 30 }, true);
    rejectFirst?.(new Error("stale resize failed"));
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(send).toHaveBeenCalledTimes(2);
    expect(onError).not.toHaveBeenCalled();
  });
});
