import { describe, expect, it, vi } from "vitest";
import { TerminalInputScheduler } from "./TerminalInputScheduler";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe("TerminalInputScheduler", () => {
  it("serializes writes and coalesces adjacent keys without crossing paste", async () => {
    const firstWrite = deferred();
    const calls: string[] = [];
    let textWrites = 0;
    const scheduler = new TerminalInputScheduler({
      sendText: async (text) => {
        calls.push(`text:${text}`);
        textWrites += 1;
        if (textWrites === 1) {
          await firstWrite.promise;
        }
      },
      sendPaste: async (text) => {
        calls.push(`paste:${text}`);
      },
    });

    scheduler.enqueueText("a");
    await Promise.resolve();
    scheduler.enqueueText("b");
    scheduler.enqueueText("c");
    scheduler.enqueuePaste("PASTE");
    scheduler.enqueueText("d");
    scheduler.enqueueText("e");
    firstWrite.resolve();
    await scheduler.waitForIdle();

    expect(calls).toEqual(["text:a", "text:bc", "paste:PASTE", "text:de"]);
  });

  it("continues draining after an input failure", async () => {
    const onError = vi.fn();
    const delivered = vi.fn();
    const calls: string[] = [];
    const scheduler = new TerminalInputScheduler(
      {
        sendText: async (text) => {
          calls.push(text);
          if (text === "a") {
            throw new Error("write failed");
          }
        },
        sendPaste: async () => {},
      },
      { onDelivered: delivered, onError },
    );

    scheduler.enqueueText("a");
    await Promise.resolve();
    scheduler.enqueueText("b");
    await scheduler.waitForIdle();

    expect(calls).toEqual(["a", "b"]);
    expect(onError).toHaveBeenCalledOnce();
    expect(delivered).toHaveBeenCalledOnce();
  });

  it("stops accepting new input after close but flushes queued bytes", async () => {
    const firstWrite = deferred();
    const calls: string[] = [];
    const scheduler = new TerminalInputScheduler({
      sendText: async (text) => {
        calls.push(text);
        await firstWrite.promise;
      },
      sendPaste: async () => {},
    });

    scheduler.enqueueText("before-close");
    await Promise.resolve();
    scheduler.close();
    scheduler.enqueueText("after-close");
    firstWrite.resolve();
    await scheduler.waitForIdle();

    expect(calls).toEqual(["before-close"]);
  });
});
