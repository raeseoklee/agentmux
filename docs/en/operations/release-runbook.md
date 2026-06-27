# Release Runbook

## Purpose

This runbook describes how to publish an AgentMux Windows release through GitHub
Actions with checksums and GitHub Artifact Attestations when repository
visibility and plan support them.

## Preflight

Run these checks on `develop` before tagging:

```powershell
npm run version:check
npm --prefix apps/desktop run build
npm run docs:check
```

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
4. Merge the updater release config from GitHub variables.
5. Build the Windows NSIS installer and Tauri updater archive/signature.
6. Generate a SHA256 file and `latest.json` updater manifest.
7. Generate a GitHub Artifact Attestation when available.
8. Publish the installer, checksum, updater archive, updater signature, and
   `latest.json` to the GitHub Release.

## Verify Published Release

After the workflow completes, download the installer and checksum from the
GitHub Release.

Verify provenance when the release notes say an attestation was generated:

```powershell
gh attestation verify .\AgentMux_0.1.1_x64-setup.exe --repo raeseoklee/agentmux
```

Verify hash:

```powershell
Get-FileHash -Algorithm SHA256 .\AgentMux_0.1.1_x64-setup.exe
Get-Content .\AgentMux_0.1.1_x64-setup.exe.sha256
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
