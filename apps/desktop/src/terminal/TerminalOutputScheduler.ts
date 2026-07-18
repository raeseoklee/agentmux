export interface TerminalOutputTarget {
  write(data: Uint8Array, callback?: () => void): void;
}

export type TerminalOutputRecoveryReason =
  | "backlog-overflow"
  | "write-error"
  | "write-stall";

export interface TerminalOutputSchedulerStats {
  queuedBytes: number;
  maxQueuedBytes: number;
  backpressureEvents: number;
  writeInFlight: boolean;
  recovering: boolean;
  writeCount: number;
  parsedBytes: number;
  lastWriteDurationMs: number;
  maxWriteDurationMs: number;
  totalWriteDurationMs: number;
  recoveryCount: number;
}

export interface TerminalOutputWriteOptions {
  foreground: boolean;
  onParsed?: (byteCount: number) => void;
  onPressureChange?: (stats: TerminalOutputSchedulerStats) => void;
  onRecoveryRequired?: (reason: TerminalOutputRecoveryReason) => void;
}

interface OutputChunk {
  bytes: Uint8Array;
  offset: number;
  byteCount: number;
  onParsed?: (byteCount: number) => void;
}

interface ParsedCompletion {
  byteCount: number;
  callback?: (byteCount: number) => void;
}

interface QueueEntry {
  target: TerminalOutputTarget;
  chunks: OutputChunk[];
  queuedBytes: number;
  maxQueuedBytes: number;
  backpressureEvents: number;
  writeInFlight: boolean;
  foreground: boolean;
  recovering: boolean;
  generation: number;
  writeCount: number;
  parsedBytes: number;
  lastWriteDurationMs: number;
  maxWriteDurationMs: number;
  totalWriteDurationMs: number;
  recoveryCount: number;
  stallTimer: ReturnType<typeof setTimeout> | null;
  onPressureChange?: (stats: TerminalOutputSchedulerStats) => void;
  onRecoveryRequired?: (reason: TerminalOutputRecoveryReason) => void;
}

export const TERMINAL_OUTPUT_LIMITS = {
  backgroundFlushDelayMs: 50,
  backgroundDrainIntervalMs: 16,
  foregroundImmediateBytes: 16 * 1024,
  drainChunkBytes: 16 * 1024,
  maxWritesPerDrain: 2,
  drainTimeBudgetMs: 8,
  maxQueuedBytes: 2 * 1024 * 1024,
  writeStallMs: 2_000,
} as const;

const entries = new Map<TerminalOutputTarget, QueueEntry>();
let drainTimer: ReturnType<typeof setTimeout> | null = null;
let drainDeadline = Number.POSITIVE_INFINITY;

function now() {
  return typeof performance === "undefined" ? Date.now() : performance.now();
}

function statsFor(entry: QueueEntry): TerminalOutputSchedulerStats {
  return {
    queuedBytes: entry.queuedBytes,
    maxQueuedBytes: entry.maxQueuedBytes,
    backpressureEvents: entry.backpressureEvents,
    writeInFlight: entry.writeInFlight,
    recovering: entry.recovering,
    writeCount: entry.writeCount,
    parsedBytes: entry.parsedBytes,
    lastWriteDurationMs: entry.lastWriteDurationMs,
    maxWriteDurationMs: entry.maxWriteDurationMs,
    totalWriteDurationMs: entry.totalWriteDurationMs,
    recoveryCount: entry.recoveryCount,
  };
}

function safelyRun(callback: (() => void) | undefined) {
  if (!callback) {
    return;
  }
  try {
    callback();
  } catch {
    // xterm invokes write completions from its parser loop. A consumer callback
    // must never escape and wedge that loop.
  }
}

function notifyPressure(entry: QueueEntry) {
  const snapshot = statsFor(entry);
  safelyRun(() => entry.onPressureChange?.(snapshot));
}

function ensureEntry(
  target: TerminalOutputTarget,
  options: TerminalOutputWriteOptions,
) {
  let entry = entries.get(target);
  if (!entry) {
    entry = {
      target,
      chunks: [],
      queuedBytes: 0,
      maxQueuedBytes: 0,
      backpressureEvents: 0,
      writeInFlight: false,
      foreground: options.foreground,
      recovering: false,
      generation: 0,
      writeCount: 0,
      parsedBytes: 0,
      lastWriteDurationMs: 0,
      maxWriteDurationMs: 0,
      totalWriteDurationMs: 0,
      recoveryCount: 0,
      stallTimer: null,
    };
    entries.set(target, entry);
  }
  entry.foreground = options.foreground;
  entry.onPressureChange = options.onPressureChange;
  entry.onRecoveryRequired = options.onRecoveryRequired;
  return entry;
}

function clearQueuedChunks(entry: QueueEntry) {
  entry.chunks = [];
  entry.queuedBytes = 0;
}

function requestRecovery(
  entry: QueueEntry,
  reason: TerminalOutputRecoveryReason,
) {
  if (entry.recovering) {
    return;
  }
  entry.recovering = true;
  entry.recoveryCount += 1;
  clearQueuedChunks(entry);
  notifyPressure(entry);
  safelyRun(() => entry.onRecoveryRequired?.(reason));
}

function scheduleDrain(delayMs: number) {
  const normalizedDelay = Math.max(0, delayMs);
  const deadline = Date.now() + normalizedDelay;
  if (drainTimer !== null && drainDeadline <= deadline) {
    return;
  }
  if (drainTimer !== null) {
    clearTimeout(drainTimer);
  }
  drainDeadline = deadline;
  drainTimer = setTimeout(() => {
    drainTimer = null;
    drainDeadline = Number.POSITIVE_INFINITY;
    drainQueuedOutput();
  }, normalizedDelay);
}

function takeNextEntry() {
  let background: QueueEntry | null = null;
  for (const entry of entries.values()) {
    if (
      entry.recovering ||
      entry.writeInFlight ||
      entry.queuedBytes === 0
    ) {
      continue;
    }
    if (entry.foreground) {
      return entry;
    }
    background ??= entry;
  }
  return background;
}

function hasDrainableOutput() {
  for (const entry of entries.values()) {
    if (
      !entry.recovering &&
      !entry.writeInFlight &&
      entry.queuedBytes > 0
    ) {
      return true;
    }
  }
  return false;
}

function hasForegroundOutput() {
  for (const entry of entries.values()) {
    if (
      entry.foreground &&
      !entry.recovering &&
      !entry.writeInFlight &&
      entry.queuedBytes > 0
    ) {
      return true;
    }
  }
  return false;
}

function takeBatch(entry: QueueEntry) {
  const byteCount = Math.min(
    entry.queuedBytes,
    TERMINAL_OUTPUT_LIMITS.drainChunkBytes,
  );
  if (byteCount <= 0) {
    return null;
  }

  const first = entry.chunks[0];
  if (first && first.bytes.length - first.offset === byteCount) {
    entry.chunks.shift();
    entry.queuedBytes -= byteCount;
    return {
      bytes: first.bytes.subarray(first.offset),
      completions: [
        { byteCount: first.byteCount, callback: first.onParsed },
      ] satisfies ParsedCompletion[],
    };
  }

  const batch = new Uint8Array(byteCount);
  const completions: ParsedCompletion[] = [];
  let copied = 0;
  while (copied < byteCount && entry.chunks.length > 0) {
    const chunk = entry.chunks[0];
    const available = chunk.bytes.length - chunk.offset;
    const take = Math.min(available, byteCount - copied);
    batch.set(chunk.bytes.subarray(chunk.offset, chunk.offset + take), copied);
    copied += take;
    chunk.offset += take;
    entry.queuedBytes -= take;
    if (chunk.offset === chunk.bytes.length) {
      entry.chunks.shift();
      completions.push({
        byteCount: chunk.byteCount,
        callback: chunk.onParsed,
      });
    }
  }
  return { bytes: batch, completions };
}

function writeBatch(
  entry: QueueEntry,
  bytes: Uint8Array,
  completions: ParsedCompletion[],
) {
  entry.writeInFlight = true;
  const writeStartedAt = now();
  const generation = entry.generation;
  notifyPressure(entry);

  if (entry.stallTimer !== null) {
    clearTimeout(entry.stallTimer);
  }
  entry.stallTimer = setTimeout(() => {
    if (entry.generation !== generation || !entry.writeInFlight) {
      return;
    }
    entry.stallTimer = null;
    entry.generation += 1;
    entry.writeInFlight = false;
    requestRecovery(entry, "write-stall");
  }, TERMINAL_OUTPUT_LIMITS.writeStallMs);

  const complete = () => {
    if (entry.generation !== generation) {
      return;
    }
    if (entry.stallTimer !== null) {
      clearTimeout(entry.stallTimer);
      entry.stallTimer = null;
    }
    entry.writeInFlight = false;
    const writeDurationMs = Math.max(0, now() - writeStartedAt);
    entry.writeCount += 1;
    entry.parsedBytes += bytes.length;
    entry.lastWriteDurationMs = writeDurationMs;
    entry.maxWriteDurationMs = Math.max(
      entry.maxWriteDurationMs,
      writeDurationMs,
    );
    entry.totalWriteDurationMs += writeDurationMs;
    for (const completion of completions) {
      safelyRun(() => completion.callback?.(completion.byteCount));
    }
    notifyPressure(entry);
    if (entry.queuedBytes > 0 && !entry.recovering) {
      scheduleDrain(
        entry.foreground
          ? 0
          : TERMINAL_OUTPUT_LIMITS.backgroundDrainIntervalMs,
      );
    }
  };

  try {
    entry.target.write(bytes, complete);
  } catch {
    if (entry.stallTimer !== null) {
      clearTimeout(entry.stallTimer);
      entry.stallTimer = null;
    }
    entry.generation += 1;
    entry.writeInFlight = false;
    requestRecovery(entry, "write-error");
  }
}

function drainQueuedOutput() {
  const startedAt = now();
  let writes = 0;
  while (writes < TERMINAL_OUTPUT_LIMITS.maxWritesPerDrain) {
    const entry = takeNextEntry();
    if (!entry) {
      break;
    }
    const batch = takeBatch(entry);
    if (!batch) {
      break;
    }
    writeBatch(entry, batch.bytes, batch.completions);
    writes += 1;
    if (
      writes > 0 &&
      now() - startedAt >= TERMINAL_OUTPUT_LIMITS.drainTimeBudgetMs
    ) {
      break;
    }
  }

  if (hasDrainableOutput()) {
    scheduleDrain(
      hasForegroundOutput()
        ? 0
        : TERMINAL_OUTPUT_LIMITS.backgroundDrainIntervalMs,
    );
  }
}

export function writeTerminalOutput(
  target: TerminalOutputTarget,
  bytes: Uint8Array,
  options: TerminalOutputWriteOptions,
) {
  if (bytes.length === 0) {
    return;
  }
  const entry = ensureEntry(target, options);
  if (entry.recovering) {
    return;
  }

  if (
    options.foreground &&
    !entry.writeInFlight &&
    entry.queuedBytes === 0 &&
    bytes.length <= TERMINAL_OUTPUT_LIMITS.foregroundImmediateBytes
  ) {
    writeBatch(entry, bytes, [
      { byteCount: bytes.length, callback: options.onParsed },
    ]);
    return;
  }

  const wasBackpressured = entry.writeInFlight || entry.queuedBytes > 0;
  entry.chunks.push({
    bytes,
    offset: 0,
    byteCount: bytes.length,
    onParsed: options.onParsed,
  });
  entry.queuedBytes += bytes.length;
  entry.maxQueuedBytes = Math.max(entry.maxQueuedBytes, entry.queuedBytes);
  if (wasBackpressured) {
    entry.backpressureEvents += 1;
  }
  notifyPressure(entry);

  if (entry.queuedBytes > TERMINAL_OUTPUT_LIMITS.maxQueuedBytes) {
    requestRecovery(entry, "backlog-overflow");
    return;
  }
  scheduleDrain(
    options.foreground
      ? 0
      : TERMINAL_OUTPUT_LIMITS.backgroundFlushDelayMs,
  );
}

export function setTerminalOutputForeground(
  target: TerminalOutputTarget,
  foreground: boolean,
) {
  const entry = entries.get(target);
  if (!entry) {
    return;
  }
  entry.foreground = foreground;
  if (foreground && entry.queuedBytes > 0 && !entry.writeInFlight) {
    scheduleDrain(0);
  }
}

export function getTerminalOutputStats(
  target: TerminalOutputTarget,
): TerminalOutputSchedulerStats {
  const entry = entries.get(target);
  return entry
    ? statsFor(entry)
    : {
        queuedBytes: 0,
        maxQueuedBytes: 0,
        backpressureEvents: 0,
        writeInFlight: false,
        recovering: false,
        writeCount: 0,
        parsedBytes: 0,
        lastWriteDurationMs: 0,
        maxWriteDurationMs: 0,
        totalWriteDurationMs: 0,
        recoveryCount: 0,
      };
}

export function resetTerminalOutput(target: TerminalOutputTarget) {
  const entry = entries.get(target);
  if (!entry) {
    return;
  }
  entry.generation += 1;
  if (entry.stallTimer !== null) {
    clearTimeout(entry.stallTimer);
    entry.stallTimer = null;
  }
  entry.writeInFlight = false;
  entry.recovering = false;
  clearQueuedChunks(entry);
  notifyPressure(entry);
}

export function discardTerminalOutput(target: TerminalOutputTarget) {
  const entry = entries.get(target);
  if (!entry) {
    return;
  }
  entry.generation += 1;
  if (entry.stallTimer !== null) {
    clearTimeout(entry.stallTimer);
  }
  entries.delete(target);
}

export function resetTerminalOutputSchedulerForTests() {
  if (drainTimer !== null) {
    clearTimeout(drainTimer);
    drainTimer = null;
  }
  drainDeadline = Number.POSITIVE_INFINITY;
  for (const entry of entries.values()) {
    if (entry.stallTimer !== null) {
      clearTimeout(entry.stallTimer);
    }
  }
  entries.clear();
}
