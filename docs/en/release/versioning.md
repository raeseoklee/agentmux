# AgentMux Versioning and Release Flow

AgentMux uses one SemVer version across the desktop package, Tauri bundle, Rust
workspace, and desktop package lock.

## Version Commands

Check that all version sources agree:

```powershell
npm run version:check
```

Set the next version:

```powershell
npm run version:set -- 0.1.3
npm run version:check -- --tag v0.1.3
```

The version script updates:

- `package.json`
- `apps/desktop/package.json`
- `apps/desktop/package-lock.json`
- `apps/desktop/src-tauri/tauri.conf.json`
- `Cargo.toml` `[workspace.package]`

## Release Trigger

Push a SemVer tag to trigger the signed release workflow:

```powershell
git tag v0.1.3
git push origin v0.1.3
```

The release workflow builds the Windows NSIS installer, writes a SHA256
checksum, generates Tauri updater artifacts and `latest.json`, generates and
verifies GitHub Artifact Attestations for the release assets, and publishes
them to the GitHub Release.

## Provenance Verification

After downloading the installer from a GitHub Release:

```powershell
gh attestation verify .\AgentMux_0.1.3_x64-setup.exe --repo raeseoklee/agentmux --signer-workflow raeseoklee/agentmux/.github/workflows/release.yml
```

The release notes include the exact command and installer hash for each release.
