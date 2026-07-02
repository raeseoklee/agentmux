$ErrorActionPreference = "Stop"

$missing = @()
$root = (Resolve-Path .).Path
$localCargoHome = Join-Path $root ".toolchains\cargo"
$localRustupHome = Join-Path $root ".toolchains\rustup"
$localCargo = Join-Path $localCargoHome "bin\cargo.exe"
$localStableBin = Join-Path $localRustupHome "toolchains\stable-x86_64-pc-windows-msvc\bin"

Write-Host "== AgentMux check =="

$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
$cargoPath = if ($cargoCommand) { $cargoCommand.Source } else { $null }
if (-not $cargoPath -and (Test-Path $localCargo)) {
  $env:CARGO_HOME = $localCargoHome
  $env:RUSTUP_HOME = $localRustupHome
  if (-not $env:RUSTUP_TOOLCHAIN) {
    $env:RUSTUP_TOOLCHAIN = "stable-x86_64-pc-windows-msvc"
  }
  if (Test-Path $localStableBin) {
    $env:PATH = "$localStableBin;$env:PATH"
  }
  $cargoPath = $localCargo
}

# Warn if the active Rust toolchain is not MSVC. The windows-link crate used by
# Tauri generates raw-dylib API Set imports that the GNU/MinGW linker cannot
# resolve at load time (STATUS_ENTRYPOINT_NOT_FOUND / 0xc0000139). The repo
# root rust-toolchain.toml pins to MSVC, but an explicit RUSTUP_TOOLCHAIN env
# var or a user-level default can still override it.
# This check runs after the vendored toolchain override so that developers using
# .toolchains/ (which is always MSVC) do not see a false-positive warning.
$activeToolchain = & rustup show active-toolchain 2>$null
if ($activeToolchain -and $activeToolchain -notmatch "msvc") {
  Write-Warning "Active Rust toolchain '$activeToolchain' is not MSVC. Tests may crash with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139). Set RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-msvc or let rust-toolchain.toml take effect."
}

if ($cargoPath) {
  & $cargoPath fmt --all -- --check
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  & $cargoPath clippy --workspace --all-targets -- -D warnings
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  & $cargoPath test --workspace
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} else {
  Write-Warning "cargo was not found on PATH or in .toolchains. Install Rust before running Rust checks."
  $missing += "cargo"
}

if (Get-Command node -ErrorAction SilentlyContinue) {
  node tools/check-doc-links.mjs
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  node tools/check-repo-hygiene.mjs
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} else {
  Write-Warning "node was not found on PATH. Install Node.js before running docs checks."
  $missing += "node"
}

if ($missing.Count -gt 0) {
  Write-Error ("Missing required tool(s): " + ($missing -join ", "))
}

Write-Host "All available checks passed."
