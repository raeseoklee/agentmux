# Main Merge Policy

## Goal

Keep `main` focused on code, release automation, user documentation, and
operations documentation. Keep development notes, goal logs, and evidence
captures on `develop`.

## Include in main

Use this allowlist when promoting documentation:

- `README.md`
- `LICENSE`
- `SECURITY.md`
- `CONTRIBUTING.md`
- `THIRD_PARTY_NOTICES.md`
- `docs/README.md`
- `docs/en/README.md`
- `docs/en/features.md`
- `docs/en/user/**`
- `docs/en/operations/**`
- `docs/en/release/versioning.md`
- `docs/en/schemas/agentmux.config.schema.json`

Localized documents may be included only when they intentionally mirror or
supplement the published user documentation:

- `docs/ko/README.md`
- `docs/ko/features.md`
- `docs/ko/user/**`

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

- `docs/ko/implementation/**`
- `docs/implementation/evidence/**`
- `docs/en/development/**`
- `docs/en/ieee-*.md`
- `.codexus/**`
- `.vs/**`
- `.claude/**`
- `AGENTS.md`
- local MCP/config files such as `.mcp.json`

## Recommended Promotion Command

From a clean working tree:

```powershell
git checkout main
git pull origin main
git checkout develop -- README.md LICENSE SECURITY.md CONTRIBUTING.md THIRD_PARTY_NOTICES.md docs/README.md docs/en/README.md docs/en/features.md docs/en/user docs/en/operations docs/en/release/versioning.md docs/en/schemas/agentmux.config.schema.json docs/ko/README.md
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
git rm -r docs/ko/implementation docs/implementation/evidence docs/en/development
git rm --cached --ignore-unmatch AGENTS.md .mcp.json
git rm docs/en/ieee-29148-desktop-performance-optimization.md docs/en/ieee-29148-system-design.md
git commit -m "Remove development-only docs from main"
```

Do not delete the same paths from `develop`; they remain useful for engineering
history and evidence.
