# Main Merge Policy

## Goal

Keep `main` focused on code, release automation, user documentation, and
operations documentation. Keep development notes, goal logs, and evidence
captures on `develop`.

## Include in main

Use this allowlist when promoting documentation:

- `README.md`
- `docs/README.md`
- `docs/user/**`
- `docs/operations/**`
- `docs/release/versioning.md`
- `docs/schemas/agentmux.config.schema.json`

Release automation and version tooling may also be merged when needed:

- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- `tools/check-version.mjs`
- `tools/set-version.mjs`
- `package.json`
- `apps/desktop/package.json`
- `apps/desktop/package-lock.json`
- `apps/desktop/src-tauri/tauri.conf.json`
- `Cargo.toml`

## Exclude from main by default

- `docs/implementation/**`
- `docs/implementation/evidence/**`
- `docs/development/**`
- `docs/ieee-*.md`
- `docs/features.md`
- `.codexus/**`
- `.vs/**`
- local MCP/config files such as `.mcp.json`

## Recommended Promotion Command

From a clean working tree:

```powershell
git checkout main
git pull origin main
git checkout develop -- README.md docs/README.md docs/user docs/operations docs/release/versioning.md docs/schemas/agentmux.config.schema.json
npm run docs:check
git status --short
git commit -m "Publish AgentMux operations documentation"
git push origin main
```

If release automation is part of the same promotion:

```powershell
git checkout develop -- .github/workflows/ci.yml .github/workflows/release.yml tools/check-version.mjs tools/set-version.mjs package.json apps/desktop/package.json apps/desktop/package-lock.json apps/desktop/src-tauri/tauri.conf.json Cargo.toml
npm run version:check
```

## If main Already Contains Development Docs

Remove them in a separate cleanup commit:

```powershell
git rm -r docs/implementation docs/development
git rm docs/features.md docs/ieee-29148-desktop-performance-optimization.md docs/ieee-29148-system-design.md
git commit -m "Remove development-only docs from main"
```

Do not delete the same paths from `develop`; they remain useful for engineering
history and evidence.
