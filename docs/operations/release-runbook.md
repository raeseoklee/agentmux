# Release Runbook

## Purpose

This runbook describes how to publish an AgentMux Windows release through GitHub
Actions with GitHub Artifact Attestations.

## Preflight

Run these checks on `develop` before tagging:

```powershell
npm run version:check
npm --prefix apps/desktop run build
npm run docs:check
```

For a full local installer smoke, build the NSIS installer and verify the output
before tagging.

## Bump Version

Set the next SemVer version:

```powershell
npm run version:set -- 0.1.1
npm run version:check -- --tag v0.1.1
```

Commit the version bump:

```powershell
git add package.json apps/desktop/package.json apps/desktop/package-lock.json apps/desktop/src-tauri/tauri.conf.json Cargo.toml
git commit -m "Release 0.1.1"
git push origin develop
```

## Tag Release

Create and push the tag:

```powershell
git tag v0.1.1
git push origin v0.1.1
```

The `release` GitHub Actions workflow will:

1. Check that the tag and source version match.
2. Install desktop dependencies.
3. Build release sidecars.
4. Build the Windows NSIS installer.
5. Generate a SHA256 file.
6. Generate a GitHub Artifact Attestation.
7. Publish the installer and checksum to the GitHub Release.

## Verify Published Release

After the workflow completes, download the installer and checksum from the
GitHub Release.

Verify provenance:

```powershell
gh attestation verify .\AgentMux_0.1.1_x64-setup.exe --repo raeseoklee/agentmux
```

Verify hash:

```powershell
Get-FileHash -Algorithm SHA256 .\AgentMux_0.1.1_x64-setup.exe
Get-Content .\AgentMux_0.1.1_x64-setup.exe.sha256
```

The hashes must match.

## Promote Operational Docs to main

Use [main-merge-policy.md](./main-merge-policy.md). Do not merge
`docs/implementation/**` or evidence folders into `main` unless explicitly
approved for a public release note.
