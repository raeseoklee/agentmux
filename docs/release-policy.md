# AgentMux Release Policy

English documentation is the canonical documentation set for AgentMux releases.
The Korean translation is maintained at [docs/ko/release-policy.md](./ko/release-policy.md).

## Release Cadence

AgentMux uses small implementation commits and larger themed releases. A normal
patch release should bundle one coherent user-visible theme, such as terminal
restore polish, release automation, or public documentation readiness.

Normal releases should include at least two substantive slices, with three to
five slices preferred when the work is not urgent.

## Patch, Minor, And Prerelease Boundaries

- Patch releases may add stable behavior additively, fix regressions, improve
  documentation, or harden release automation.
- Minor releases are used for promoted stable contracts or breaking behavior.
- Prereleases are opt-in only and should not replace the stable release channel.

## Hotfix Exceptions

Small patch releases are allowed when they fix:

- Security issues.
- Broken installation or publishing.
- CI, release, updater, or attestation blockers.
- Severe regressions in the current stable release.

## Evidence Requirements

Before pushing a release tag, run the local release preflight:

```powershell
npm run version:check
npm --prefix apps/desktop run build
npm run docs:check
npm run repo:hygiene
npm run check
```

After the tag workflow publishes a release, verify:

- The GitHub Release exists for the expected tag.
- The Windows installer, checksum, updater artifact, updater signature, and
  `latest.json` are uploaded.
- GitHub Artifact Attestation verifies for the release assets.
- The SHA256 file matches the downloaded installer.

See [release-runbook.md](./en/operations/release-runbook.md) and
[versioning.md](./en/release/versioning.md) for the operational commands.
