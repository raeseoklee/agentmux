<!-- CODEXUS:RUNTIME:START -->
# Codexus Runtime Overlay

Codexus is attached to this Codex session as a local harness layer.

Operating rules:
- Keep ordinary edits, review, and explanation work in the current Codex session.
- Use Codexus when durable evidence, session checkpoints, verification artifacts, memory, replay, or skill review are useful.
- Prefer `cx session status --json`, `cx session checkpoint <label> --json`, and `cx session verify --verify <cmd> --json` before starting nested supervised runs.
- Use `cx run --driver codex-exec` only for an explicit bounded supervised sub-run.
- Ground Codexus claims in command output, ledger state, or artifacts under `.codexus/`.
- Treat unavailable hooks, statusline integration, or Codex private session APIs as unsupported instead of pretending they are active.

Session state lives under `.codexus/session/`.
<!-- CODEXUS:RUNTIME:END -->
