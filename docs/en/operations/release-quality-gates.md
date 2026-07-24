# Release Quality Gates

## Purpose

This policy prevents regressions that pass mocks or browser previews but fail in
the Windows desktop application. It applies to pull requests, hotfixes, release
candidates, and agent-generated changes.

## Core Rule

Evidence must match the runtime boundary being changed. Browser Preview proves
React behavior against a mock client. It does not prove Tauri command
serialization, Rust IPC validation, named-pipe behavior, WebView2 integration,
PTY behavior, or packaged application behavior.

A change is not complete merely because a nearby test passes. It is complete
when the protected user workflow passes through every affected production
boundary.

## Required Test Matrix

| Changed surface | Minimum automated evidence | Additional release evidence |
| --- | --- | --- |
| React-only presentation | Unit test and relevant Playwright workflow | Visual inspection when layout or rendering changes |
| `ControlClient` request mapping | TypeScript contract test with boundary values | Isolated Tauri smoke for desktop methods; server smoke for server methods |
| IPC schema or validation | Rust round-trip and validation tests | Invoke the method through its production caller |
| Tauri host or native plugin | Rust/Tauri test and desktop build | Isolated real Tauri workflow |
| Server transport | Server contract test and server smoke | Browser/server workflow against the running server |
| CLI or MCP | Parser/delegation test and MCP Workbench where applicable | Invoke the packaged command or tool |
| Terminal, WSL, tmux, restore, or input | Backend tests and Playwright coverage | Real Tauri session using the affected backend |
| Installer, updater, notification, or OS integration | Build and dedicated smoke script | Installed-app verification |

When one change affects multiple rows, satisfy every applicable row.

## Boundary-Value Contract

For optional strings and serialized filters, test all meaningful states:

1. omitted or `undefined`;
2. explicit `null`;
3. empty string;
4. whitespace-only string;
5. a valid non-empty value;
6. the maximum accepted size and an oversized value.

Opaque tokens such as cursors must not be trimmed or rewritten. Normalize only
fields whose contract explicitly defines normalization. The caller and receiver
must agree on whether an empty value means "not provided" or "invalid".

## Regression-Fix Requirements

Every regression fix must include:

- a root-cause statement naming the failed boundary;
- an invariant that describes the expected behavior;
- a test that fails against the regressed implementation;
- defense at the narrowest caller boundary and, when safe, receiver-side
  compatibility handling;
- verification through the production runtime surface that exposed the bug.

For example, a Git filter with no text means "no filter." The desktop and server
clients must serialize it as absent, and IPC must not turn that harmless state
into a request-wide failure.

## Desktop Smoke Isolation

Do not reuse a developer's active AgentMux state. Launch the Tauri application
with isolated store, configuration, token, and control-pipe paths. Exercise the
smallest workflow that reaches the changed boundary, then stop the isolated
instance and retain only redacted evidence.

The smoke must verify visible behavior, not only process startup. For a source
control change this includes opening a repository pane, opening Source Control,
checking branch and file data, switching to another pane or workspace, and
switching back without stale state or transport errors.

## Pull Request and Release Rules

- Pull requests must complete the repository template and name every affected
  runtime surface.
- `npm run check` and `npm run desktop:gates` are the default local gates for
  user-visible desktop changes.
- CI success is necessary but does not replace an applicable native smoke.
- A skipped applicable gate must have a concrete technical reason and blocks a
  public release until the missing evidence is produced.
- Release managers must review the protected invariant and native evidence for
  every user-visible regression fix included in the release.
- A hotfix follows the same gates as a regular release. Urgency may reduce scope,
  but it must not remove the test that protects the failed workflow.

## Evidence Record

Record the following in the pull request or release review:

- affected feature and production boundary;
- commands and test counts;
- Tauri or installed-app scenario performed;
- result and any intentionally untested surface;
- Codexus verification or equivalent durable evidence when available.
