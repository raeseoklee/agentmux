# Desktop Build and UI Smoke Evidence: IRAE-DESKTOP

Date: 2026-06-18
Machine: IRAE-DESKTOP

## Scope

This gate verifies the desktop frontend build and the browser-based UI smoke
suite:

1. Run the desktop production build.
2. Verify `apps/desktop/dist/index.html` is produced.
3. Archive the generated `dist` files.
4. Run the Playwright UI smoke suite against the Vite dev server.
5. Archive command output and build artifact metadata.

## Result

The gate passed.

- Build command: `npm --prefix apps/desktop run build`
- Build exit code: 0
- UI smoke command: `npm --prefix apps/desktop run test:ui`
- UI smoke result: 2 passed
- Built files: `index.html`, `assets/index-CaBCnzcG.css`,
  `assets/index-Ccfx0nVT.js`

The build emitted the known Vite chunk-size warning for the main JavaScript
bundle but completed successfully.

Artifacts:

- `dist/`
- `summary.json`
- `desktop-build.stdout.txt`
- `desktop-build.stderr.txt`
- `ui-smoke.stdout.txt`
- `ui-smoke.stderr.txt`
