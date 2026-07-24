# Release Runbook

## Purpose

This runbook describes how to publish the AgentMux Windows-only release through
GitHub Actions with checksums and GitHub Artifact Attestations.

## Preflight

Run these checks on the release branch before tagging:

```powershell
npm run check
npm run desktop:gates
npm run version:check
```

Apply [Release quality gates](./release-quality-gates.md) to every user-visible
change in the release. Browser Preview evidence alone is insufficient for a
workflow that crosses Tauri IPC, WebView2, PTY, filesystem, updater, or another
Windows integration. Perform and record an isolated real-Tauri smoke for each
affected production boundary before tagging.

For a full local installer smoke, build the NSIS installer and verify the output
before tagging.

## Updater Signing Setup

AgentMux uses the Tauri updater with GitHub Releases as the static update
endpoint. No separate update server is required for the default release channel.

Generate a Tauri updater keypair once and store the private key outside the
repository:

```powershell
npm --prefix apps/desktop exec -- tauri signer generate -- -w "$env:USERPROFILE\.tauri\agentmux.key"
```

Configure GitHub before publishing a release:

- Repository variable `TAURI_UPDATER_PUBLIC_KEY`: the public key printed by the
  signer command. This value is embedded in the app and is safe to share.
- Repository secret `TAURI_SIGNING_PRIVATE_KEY`: the private key content or path
  used by CI to sign updater artifacts. Never commit this value.
- Repository secret `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: optional password for
  the private key.

## Bump Version

Set the next SemVer version:

```powershell
npm run version:set -- 0.1.9
npm run version:check -- --tag v0.1.9
```

Commit the version bump:

```powershell
git add package.json apps/desktop/package.json apps/desktop/package-lock.json apps/desktop/src-tauri/tauri.conf.json Cargo.toml Cargo.lock
git commit -m "Release 0.1.9"
git push origin <release-branch>
```

Open a pull request, wait for every required CI check, and merge it to `main`.
Do not tag a release-branch commit. The release tag must point to the exact
commit already verified on `main`.

## Tag Release

Create and push the tag:

```powershell
git switch main
git pull --ff-only origin main
git tag v0.1.9
git push origin v0.1.9
```

The `release` GitHub Actions workflow will:

1. Check that the tag points exactly to the workflow SHA on `main`.
2. Wait for the CI workflow for that exact SHA to succeed.
3. Refuse to modify an existing release or replace its assets.
4. Install desktop dependencies and build release sidecars.
5. Merge the updater release config from GitHub variables.
6. Build the Windows NSIS installer and Tauri updater archive/signature.
7. Silently install the NSIS package into an isolated CI directory and verify
   that the packaged `agentmux.exe` exposes MCP help, doctor help, and
   non-mutating Codex/Claude setup previews without starting the desktop.
8. Generate a SHA256 file and `latest.json` updater manifest.
9. Generate and verify GitHub Artifact Attestations for the release assets.
10. Generate release notes and publish the installer, checksum, updater archive,
   updater signature, and
   `latest.json` to the GitHub Release.

## Verify Published Release

After the workflow completes, download the installer and checksum from the
GitHub Release.

Verify provenance:

```powershell
gh attestation verify .\AgentMux_0.1.9_x64-setup.exe --repo raeseoklee/agentmux --signer-workflow raeseoklee/agentmux/.github/workflows/release.yml
```

Verify hash:

```powershell
Get-FileHash -Algorithm SHA256 .\AgentMux_0.1.9_x64-setup.exe
Get-Content .\AgentMux_0.1.9_x64-setup.exe.sha256
```

The hashes must match.

The packaged app checks:

```text
https://github.com/raeseoklee/agentmux/releases/latest/download/latest.json
```

Users can disable startup update checks from Settings > General > Updates.

## Promote Operational Docs to main

Use [main-merge-policy.md](./main-merge-policy.md). Do not merge
`docs/ko/implementation/**` or `docs/implementation/evidence/**` into `main`
unless explicitly approved for a public release note.
