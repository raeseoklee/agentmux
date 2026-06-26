# Goal 17 — Terminal Live Byte Stream

Status: Implemented baseline; follow-up hardening remains

Implementation update, 2026-06-25:

- `session.snapshot` is implemented as a separate raw-byte/base64 snapshot API,
  leaving `session.read_recent` as the lossy-text compatibility path for CLI and
  fallback clients.
- Desktop Tauri uses a per-session output stream via `tauri::ipc::Channel`,
  with raw output bytes base64-encoded at the host boundary and decoded to
  `Uint8Array` before xterm ingestion.
- Server mode exposes the same output-stream contract through a per-session
  WebSocket at `/api/session/<session-id>/stream`.
- `LiveTerminal` is stream-first when a stream transport is available, with
  snapshot polling and legacy `readRecent` polling retained only as fallback
  paths.
- Host diagnostics now expose output-stream counters, renderer queued bytes,
  and renderer backpressure reports.
- The 2026-06-25 performance follow-up implements PR-7 through PR-10: action
  descriptor source-list splitting, byte-level agent-signal prefiltering plus
  heuristic scan throttling, read-only runtime pre-dispatch `collect_events`
  reduction, and an amortized `VecDeque` recent-output ring.
- The next 2026-06-25 follow-up implements PR-11 and PR-12 from the desktop
  performance SRS: `renderPane` now uses DFS backtracking instead of cloning the
  `visited` set per recursive level, and WebGL rendering now caches the addon
  module import while debouncing deactivation teardown to survive rapid pane
  switching.
- The P0/P1 completion pass refreshes the preview UI E2E fixture so product
  startup remains no-default-workspace while tests can explicitly seed a
  workspace; the full Playwright UI suite passes again.
- Backend read-pause is wired for local terminal output pressure:
  renderer pressure reports flow through desktop/server control clients to
  `session.report_output_pressure`, core forwards pause/resume to the backend,
  ConPTY checks a pause flag before issuing the next blocking read, and
  wsl-direct forwards the same pressure signal to its inner backend.
- Local server mode now injects a per-process auth token into the shared desktop
  UI bootstrap. `/api/*` requests require the token in
  `X-AgentMux-Server-Token`, and the per-session output WebSocket requires the
  token query parameter.

Remaining follow-up:

- Keep performance gates and Tauri UI smoke current as the stream path evolves.
- Add packaged/remote deployment hardening before treating non-loopback server
  mode as a regular production path: stronger operator docs, token presentation
  UX, and optional packaged-server discovery.

Related: [Goal 1 status](09-goal-1-native-terminal-slice-status.md) ("Not Yet Implemented: live backend event stream; output batching/backpressure"), [Goal 16 server mode](26-goal-16-server-mode-web-terminal-status.md), [Goal groups](08-goal-groups.md).

Current reading note: sections below preserve the original design rationale.
When they describe future work or pending choices, prefer the implementation
update above and the decision summary in section 9 for the current state.

This document specifies — implementation-ready, before any code is written — the replacement of the terminal output **polling** model with a **live byte stream**. The design below is converged with the user; it is to be specified precisely, not re-litigated. Step #1 (WebGL renderer, visible-only) is being implemented separately and is referenced where it intersects this work.

---

## 1. Problem — why poll + `read_recent` + string-diff is wrong

The desktop terminal does not stream. Each pane runs an independent client-side poll loop that re-reads a bounded recent buffer and reconstructs deltas by string prefix comparison. Four concrete defects follow, each traced to source:

### 1.1 Added latency (batch quantization + input echo round-trips)

- The renderer polls on a fixed interval: `POLL_INTERVAL_MS = 120` (`apps/desktop/src/agentmux/LiveTerminal.tsx:9`), driven by `window.setInterval(() => void poll(), POLL_INTERVAL_MS)` (`LiveTerminal.tsx:147`). Output produced just after a poll completes is not shown until the next tick — up to ~120 ms of added display latency on top of transport, independent of how fast the backend emitted it.
- Input echo does not stream back; it is *chased* by extra polls. On keystroke, `schedulePoll(INPUT_POLL_DELAYS_MS[0])` fires, then after the send resolves the remaining delays are scheduled: `INPUT_POLL_DELAYS_MS.slice(1).forEach(schedulePoll)` (`LiveTerminal.tsx:110-118`). `INPUT_POLL_DELAYS_MS = [16, 0, 40, 100]` (`LiveTerminal.tsx:10`) is a hand-tuned ladder of speculative re-reads whose sole purpose is to make echo *feel* prompt. It is a workaround for the absence of a push channel: the cost is up to four extra `read_recent` round-trips per keystroke and echo latency bounded by whichever speculative poll happens to catch the output.

### 1.2 Periodic `reset()` + full rewrite flicker at 64 KB ring rotation

- The recent buffer is bounded to 64 KB on the core side: `recent_output_limit: 64 * 1024` (`crates/agentmux-core/src/lib.rs:365`), enforced by front-draining the oldest bytes: `buffer.drain(..overflow)` in `append_recent_output` (`crates/agentmux-core/src/lib.rs:1826-1828`).
- The renderer's delta logic is *prefix-based*: it keeps `renderedText` and, on each poll, fetches the whole 64 KB window via `client.readRecent(sessionId, 65536)` (`LiveTerminal.tsx:64`). If the new text still starts with `renderedText` it appends the suffix (`LiveTerminal.tsx:79-83`); otherwise it assumes a discontinuity, calls `renderer.reset()` and rewrites the entire buffer (`LiveTerminal.tsx:71-77`).
- Once a session has emitted more than 64 KB total, every rotation shifts the window's start, so the freshly fetched text no longer has the previous `renderedText` as a prefix. The non-prefix branch fires, triggering a clear-and-full-rewrite. The user sees periodic flicker / scrollback churn that recurs roughly every time ~64 KB of new output accumulates.

### 1.3 Broken full-screen TUIs (vim / htop / less)

- Full-screen apps drive the display with cursor-addressed VT sequences (absolute cursor moves, line erases, scroll-region writes) that are only correct when fed to a terminal emulator **in order, exactly once**. The current model instead reconstructs the screen by *re-slicing a text buffer*: the renderer diffs two snapshots of the last 64 KB and feeds whatever suffix it computed.
- This is wrong for cursor-addressed output. A `reset()` + full rewrite (1.2) replays raw bytes captured mid-frame from an arbitrary 64 KB boundary that may bisect an escape sequence or a frame, so vim/htop/less repaint incorrectly or corrupt. The buffer was never designed to be a faithful VT transcript; it is a bounded capture tail.

### 1.4 Lossy UTF-8 string round-trip

- The control plane converts recent bytes to a string with **lossy** decoding before returning them: `let text = String::from_utf8_lossy(&output).to_string();` and returns it as `SessionReadRecentResult { text, .. }` (`crates/agentmux-core/src/lib.rs:1023-1031`; struct at `crates/agentmux-ipc/src/lib.rs:836-841`). Any byte sequence that is not valid UTF-8 *at the 64 KB window boundary* — e.g. a multi-byte rune split by the ring cut, or a raw non-UTF-8 byte from a TUI — is replaced by U+FFFD.
- The renderer then re-encodes that lossy string back to bytes before writing to xterm: `renderer.write(encoder.encode(next))` with `const encoder = new TextEncoder()` (`LiveTerminal.tsx:8, 82`). The byte → (lossy) string → byte round-trip is irreversible: corrupted bytes can never be recovered, and a rune split across two polls is destroyed rather than buffered.

**Net:** the polling model adds up to ~120 ms display latency, multiplies keystroke round-trips, flickers on every 64 KB rotation, cannot faithfully render cursor-addressed TUIs, and corrupts non-UTF-8/boundary-split bytes. The fix is to deliver **raw bytes, in order, exactly once, via push**, and write **bytes** (not strings) into xterm.

---

## 2. Offset-based recent handshake (the core contract)

The single primitive that makes streaming correct is an **absolute monotonic byte offset** per session, plus a snapshot that reports the byte range the bounded ring currently covers. This one primitive serves three distinct uses — cold start, gap resync, and renderer swap — so it must be specified once, precisely.

### 2.1 Definitions

- **Absolute offset** — a per-session `u64` equal to the total number of raw output bytes ever emitted by that session since it started. It is **not** an index into the bounded ring; it only ever increases and is never rebased when the ring rotates. Offsets are counted on **raw bytes, before any base64 encoding**.
- **`base_offset`** — the absolute offset of the **first** byte currently retained in the bounded recent ring.
- **`end_offset`** — the absolute offset **one past the last** byte currently retained (i.e. the total bytes emitted so far). The ring covers exactly the half-open range `[base_offset, end_offset)`; its length is `end_offset - base_offset` and is bounded by `recent_output_limit` (today 64 KB).

Invariant: `base_offset <= end_offset`, and `end_offset - base_offset <= recent_output_limit`. After more than `recent_output_limit` bytes have been emitted, `base_offset > 0` and advances every time the ring front-drains.

### 2.2 Snapshot API

A snapshot returns the atomically-captured triple:

```
snapshot(session_id) -> { base_offset: u64, end_offset: u64, bytes: <raw recent ring contents> }
```

`bytes` is exactly the current ring contents, i.e. the raw bytes for `[base_offset, end_offset)`. The capture must be **atomic** with respect to the live stream: the snapshot's `end_offset` and the live stream's first delivered `from_offset` must align with no gap and no overlap at the seam (see 2.3). Concretely, the snapshot and the subscription cursor are taken under the same lock acquisition so that no `BackendEvent::Output` can be appended (advancing `end_offset`) between reading the ring and registering the stream cursor.

### 2.3 Live stream framing and seam attachment

Each streamed frame carries:

```
{ from_offset: u64, bytes: <raw delta bytes> }
```

where `from_offset` is the absolute offset of the first byte in `bytes`. Frames are delivered **in order**; the next frame's `from_offset` equals the previous frame's `from_offset + bytes.len()`.

**Attachment at the seam (cold start / remount):**
1. Renderer calls `snapshot(session_id)` → `(base_offset, end_offset, bytes)`.
2. Renderer `reset()`s xterm and writes `bytes` (the ring contents) as the initial screen.
3. Renderer sets `expected_offset = end_offset` and attaches the live stream, accepting frames where `from_offset == expected_offset`.
4. Because snapshot+cursor are atomic, the first live frame's `from_offset` is exactly `end_offset` — no byte is dropped or duplicated at the seam.

### 2.4 Gap handling

Two distinct failure modes, two distinct responses:

- **Stream ahead of expected** (`from_offset > expected_offset`): a frame was lost between host and renderer, or the renderer paused. Response: **re-snapshot, then `reset()` + replay** the snapshot's `[base_offset, end_offset)` bytes, and resume from the new `end_offset`. This recovers a consistent (if approximate) screen.
- **Renderer-needed offset older than the ring** (`expected_offset < base_offset`): the renderer fell so far behind that the bytes it still needs have already been front-drained from the ring. The gap **cannot** be filled. Response: `reset()` and restart from `base_offset` (the oldest byte still retained), and **accept partial loss**. Do **not** silently pretend to fill the gap (no zero-fill, no fabricated bytes). The lost span `[expected_offset, base_offset)` is gone; surfacing a reset is the honest behavior.

Frames where `from_offset < expected_offset` (overlap / duplicate) are de-duplicated: write only the suffix `bytes[(expected_offset - from_offset)..]` if any, else drop the frame.

### 2.5 Resync fidelity note

Resync replays **raw recent bytes**, not a reconstructed VT screen. For a full-screen app this is approximate: the replayed window may begin mid-frame, so the first repaint after a resync can be momentarily imperfect until the app issues its next full redraw. This is a deliberate trade-off and is **strictly better** than today's behavior — today's 64 KB-rotation `reset()` (1.2) does exactly the same raw-window replay but does it *routinely on every rotation*, whereas resync here happens only on an actual gap. No new failure mode is introduced; an existing one is made rare.

### 2.6 The one primitive, three uses

| Use | Trigger | Action |
|-----|---------|--------|
| Cold start | LiveTerminal first mount for a session | `snapshot` → write ring → attach at `end_offset` |
| Gap resync | `from_offset != expected_offset` | re-`snapshot` → reset+replay → resume |
| Renderer remount | LiveTerminal renderer rebuilt (session swap / unmount→remount) | re-`snapshot` into the new xterm/renderer → attach at `end_offset` |

The renderer-remount use is why the handshake must be cheap and idempotent: whenever the xterm instance is actually rebuilt (e.g. the LiveTerminal mount effect re-runs on a session swap), the new instance must cold-start from the snapshot exactly like a first mount.

**WebGL visible-only toggling is _not_ a remount.** As implemented in step #1 (`apps/desktop/src/agentmux/LiveTerminal.tsx`, `XtermTerminalRenderer.enableWebgl`/`disableWebgl`), enabling/disabling WebGL loads/unloads the addon on the **same** `Terminal` instance; the xterm buffer and the live stream are preserved across the toggle, so **no re-handshake is performed on a WebGL toggle**. A re-handshake is needed only on an actual renderer rebuild. (A future change that fully tears down a hidden pane's xterm instance would turn that teardown into a remount, at which point the cold-start handshake applies — but plain WebGL on/off must stay handshake-free to avoid reset+snapshot churn on every focus change.)

---

## 3. Transport — Tauri `ipc::Channel`

### 3.1 Availability

`tauri::ipc::Channel<TSend>` is available at the pinned **tauri 2.11.3** (`Cargo.lock:4472-4473`). Evidence in the vendored crate: `pub struct Channel<TSend = InvokeResponseBody>` (`tauri-2.11.3/src/ipc/channel.rs:49`), `pub fn send(&self, data: TSend) -> crate::Result<()>` (`channel.rs:292`), and a raw-bytes fast path `InvokeResponseBody::Raw(bytes)` (`channel.rs:163, 256`). A `Channel` is `Serialize` (`channel.rs:88`) so it can be passed in as a command argument and is JSON-encoded as its numeric id. Note: the host crate currently enables **no** tauri features (`apps/desktop/src-tauri/Cargo.toml:21`, `tauri = { version = "2", features = [] }`); `ipc::Channel` is in the core API and needs no extra feature flag, but this must be reconfirmed at build time.

### 3.2 Per-session Channel vs global `emit`

The desktop host today uses **neither** `emit` nor `Channel`; the React UI calls the in-process Tauri command `agentmux_control` (`apps/desktop/src-tauri/src/lib.rs:4722`) and polls. We add a stream; we do not change the request/response path.

Recommendation: **one `Channel` per session**, point-to-point.

- `app_handle.emit("session.output", ..)` **broadcasts** to every webview/listener. With N panes open, every pane's listener receives every session's bytes and must filter by `session_id` — an N×fan-out plus per-frame filtering cost in JS, exactly the kind of bulk-bytes-to-everyone traffic to avoid.
- A `Channel<TSend>` is point-to-point and **ordered**: the renderer creates the channel, passes it into a `session.subscribe_output`-style command, and the host holds it for that one session and calls `channel.send(frame)` per delta. No fan-out, no filtering, ordering guaranteed by the channel.

### 3.3 Payload encoding — SPIKE QUESTION (resolve before build)

**Open question:** does `Vec<u8>` sent over a `Channel` arrive in JS as a JSON `number[]`? If so it is badly bloated — a JSON array serializes each byte as decimal digits + comma, roughly **3–4 bytes of JSON per raw byte**, so a single 64 KB burst becomes ~200–256 KB of JSON to parse. (Tauri 2.11.3 does have an `InvokeResponseBody::Raw` path at `channel.rs:163/256`, but whether `channel.send(vec_u8)` selects it or falls back to JSON-array encoding for the `onmessage` delivery must be measured, not assumed.)

**Recommended default:** encode each frame's `bytes` as a **base64 string** on the Rust side and `decode → Uint8Array` in JS. Base64 is ~1.33× the raw size (predictable, far smaller than number[]'s ~3–4×) and trivially decodes to the `Uint8Array` xterm wants (§5).

**Required A/B spike (manual, before committing the encoding):**
- Fixture: one **64 KB** burst of representative terminal bytes (mix of ASCII text and VT escape sequences).
- Variant A: send as `Vec<u8>` (number[]). Variant B: send as base64 `String`.
- Measure, in the renderer: (a) wall-clock ms from `channel.send` on the host to `onmessage` decode-complete in JS, and (b) the on-wire / received payload byte size.
- Decision rule: pick base64 unless number[] is both smaller and faster in this test (not expected). Record the numbers in the evidence folder and in this doc before implementing the stream-first renderer.

### 3.4 Frontend wiring and lifecycle

- Frontend uses `Channel` and `onmessage` from `@tauri-apps/api/core`: construct `const ch = new Channel<...>()`, set `ch.onmessage = frame => { ... }`, pass `ch` into the subscribe command invocation.
- **Lifecycle:**
  - Drop / close the channel when the session closes or the LiveTerminal unmounts (host releases its handle; no further `send`).
  - **Re-handshake the offset** (§2.3) whenever LiveTerminal **remounts** or **pane focus changes** such that the renderer is rebuilt (including WebGL on/off swaps, §2.6). A remount with a stale `expected_offset` must not blindly resume; it must re-`snapshot`.
- `recent_output` stays **unchanged** and authoritative for capture, recovery, agent-state detection, and the CLI. The stream is purely **additive** — it does not replace `read_recent`; it removes the renderer's *dependence* on polling `read_recent` for live display.

---

## 4. Backpressure (phased)

### Phase 1 — host-side bounded queue + coalescing (ship with the stream)

- Per-session **bounded queue** of pending output deltas on the host, fed by `BackendEvent::Output` and drained to `channel.send`.
- **Coalesce** on an 8–16 ms timer (one frame budget): accumulate bytes arriving within the window into a single frame before sending, so a chatty backend produces ~60 fps of frames rather than one frame per `ReadFile` chunk. (ConPTY reads in 8192-byte chunks — `spawn_output_reader` buffer `[0u8; 8192]`, `crates/agentmux-backend-conpty/src/lib.rs:562, 594-597` — so coalescing also merges many small chunks into one frame.)
- **Offset-gap resync** (§2.4) is the correctness backstop for anything the queue drops.
- **On queue-full:** drop **oldest**, jump to **newest**, and emit a **reset signal** to the renderer (which then re-handshakes via §2.4's "ahead of expected" path). Rationale: xterm's own internal write buffer is the **first** absorber of bursts; this host queue is the **safety valve** behind it, sized to bound host memory, not to be lossless.

### Phase 2 — real PTY read-pause (defer; backend-specific)

- True flow control uses xterm's `write(data, callback)`: the renderer only acks (via callback) when it has drained its buffer, and the host **pauses the backend reader** until acked.
- This requires backend changes: the ConPTY reader is a tight blocking `ReadFile` → `Vec<u8>` → push loop (`crates/agentmux-backend-conpty/src/lib.rs:564-599`) with no pause point. Adding genuine pause means a backend-trait/reader-loop change (e.g. a pause flag the loop checks, or stop issuing `ReadFile`).
- **Do not over-abstract early.** conpty, wsl-direct, tmux-control, and ssh have **different** pause semantics (a local pipe read vs. a tmux control-mode flow vs. an ssh channel window). A premature common "backpressure trait" would model the wrong thing. Phase 2 is scoped per backend, after Phase 1 ships and real saturation is observed.

---

## 5. Xterm ingestion — write bytes, not strings

Write **`Uint8Array`** into xterm, never strings. xterm's byte-oriented `write(Uint8Array)` maintains an internal UTF-8 and escape-sequence decoder that **buffers partial sequences across writes**: a multi-byte rune or an escape sequence split across two frames is held until completed, then decoded correctly. This directly fixes the §1.4 corruption — there is no `String::from_utf8_lossy` and no `TextEncoder` round-trip in the live path; raw bytes from the snapshot and from each frame go straight into xterm. (The renderer already accepts bytes — `renderer.mount(host, { ..., bytes })` and `renderer.write(encoder.encode(...))` at `LiveTerminal.tsx:43, 82` — so the change is to feed it the decoded base64 `Uint8Array` instead of re-encoded lossy text.)

---

## 6. WebGL visible-only integration (Goal step #1, referenced)

Step #1 (implemented separately) enables the WebGL renderer **only on visible/active panes**, because the browser caps live WebGL contexts (~16). As implemented, activating a pane **loads the WebGL addon onto the existing `Terminal`** and deactivating **unloads it** — the xterm instance is **not** rebuilt, so the buffer and the live stream survive the toggle and **no re-handshake is needed** for a WebGL on/off. The snapshot+offset handshake (§2.3) applies only if/when the renderer is genuinely rebuilt (session swap, or a future teardown of hidden panes' xterm instances). Keeping addon toggles handshake-free avoids needless reset+snapshot churn on every focus change.

---

## 7. Server mode — define `session.output` once

Define the **`session.output` byte-stream abstraction once** at the core/host boundary, with a sink chosen per deployment:

- **Desktop sink:** a Tauri `Channel` per session (§3).
- **Server-mode sink:** a **WebSocket** — a single WS per session carrying input + output + resize. WS gives **TCP backpressure for free** (the kernel send buffer fills, `send` blocks/▸errors, the host applies §4's bounded-queue policy). The plan is to **replace the server's HTTP `/recent` polling** with this WS stream later; the offset handshake (§2) is the same on both sinks — the WS just sends `{from_offset, bytes(base64)}` frames and answers `snapshot` over the same socket. Cross-reference [Goal 16 server mode](26-goal-16-server-mode-web-terminal-status.md).

The point of defining the abstraction once is that the core emits an ordered offset-tagged byte stream regardless of sink; only the transport adapter differs (Channel vs WS).

---

## 8. Phased rollout / sequencing + rough effort sizing

| # | Step | Layer | Current status |
|---|------|-------|----------------|
| 1 | WebGL renderer, **visible-only** | UI | Implemented |
| 2 | **Offset snapshot API** — absolute counter + `snapshot(base,end,bytes)` | core + ipc | Implemented |
| 3 | **Channel encoding** | host + UI | Base64 chosen and implemented |
| 4 | **Stream-first LiveTerminal** — subscribe via Channel/WS, write bytes | UI + host | Implemented with fallback polling retained |
| 5 | **Bounded queue + coalescing** (Phase 1 backpressure) | host | Implemented baseline; renderer pressure diagnostics remain additive |
| 6 | **Server WS** sink for `session.output` | server (Goal 16) | Implemented baseline |

Sequencing rationale: the snapshot API (2) is the prerequisite for both the stream renderer (4) and the WebGL swap (1); the spike (3) gates the wire format before (4) is written; backpressure (5) hardens (4); the WS sink (6) reuses the same abstraction last.

---

## 9. Decisions

1. **IPC shape for snapshot** — implemented as a new `session.snapshot` method,
   leaving `session.read_recent` untouched for compatibility.
2. **Frame encoding** — base64 string, decoded to `Uint8Array` in the renderer.
3. **Where the bytes live** — separate `session.output` stream channels rather
   than `EventFrame.data_json`; event history keeps only lightweight byte-count
   signals.
4. **Where the absolute offset counter lives** — core `TerminalRuntime`, per
   session, incremented with each `BackendEvent::Output` before the recent ring
   is updated.

Secondary note for (1)/(3): the IPC `ErrorCode` enum (`crates/agentmux-ipc/src/lib.rs:104-119`) has **no** `Gone` / `OutOfRange` / `ResourceExhausted` variant. A "renderer fell behind the ring" condition (§2.4) is handled entirely renderer-side by reset+restart-from-`base_offset` and needs no new error code; if a server-mode error is ever required, reuse `Conflict` or `InvalidRequest`, or add a variant deliberately (enum is a wire contract with fixtures).

---

## Appendix — source anchors

| Fact | Location |
|------|----------|
| Bytes dropped, only `byte_count` kept | `crates/agentmux-core/src/lib.rs:1468-1480` |
| Recent ring map + 64 KB limit | `crates/agentmux-core/src/lib.rs:352-353, 365` |
| Ring front-drain on overflow | `crates/agentmux-core/src/lib.rs:1813-1830` |
| `read_recent` lossy UTF-8 → text | `crates/agentmux-core/src/lib.rs:500-510, 1023-1031` |
| ConPTY reader loop (`ReadFile`→8192→Output) | `crates/agentmux-backend-conpty/src/lib.rs:554-605` |
| Output append + `SessionOutputBatch` emit | `crates/agentmux-core/src/lib.rs:538-548` |
| 100 ms subscribe cursor-poll loop | `apps/desktop/src-tauri/src/lib.rs:497-512` |
| `agentmux_control` Tauri command (no emit/Channel today) | `apps/desktop/src-tauri/src/lib.rs:4722` |
| Host tauri features = [] | `apps/desktop/src-tauri/Cargo.toml:21` |
| `SessionReadRecentResult` (text + byte_count) | `crates/agentmux-ipc/src/lib.rs:836-841` |
| `EventFrame` (data_json carrier) | `crates/agentmux-ipc/src/lib.rs:164-187` |
| `ErrorCode` enum (no Gone/OutOfRange) | `crates/agentmux-ipc/src/lib.rs:104-119` |
| `POLL_INTERVAL_MS`, `INPUT_POLL_DELAYS_MS` | `apps/desktop/src/agentmux/LiveTerminal.tsx:9-10` |
| `readRecent(.,65536)` + prefix-diff + `reset()` | `apps/desktop/src/agentmux/LiveTerminal.tsx:64, 68-83` |
| tauri 2.11.3 pin | `Cargo.lock:4472-4473` |
| `Channel<TSend>` + `send` + raw path | `tauri-2.11.3/src/ipc/channel.rs:49, 292, 163, 256` |
