$ErrorActionPreference = "Stop"

$missing = @()
$root = (Resolve-Path .).Path
$localCargoHome = Join-Path $root ".toolchains\cargo"
$localRustupHome = Join-Path $root ".toolchains\rustup"
$localCargo = Join-Path $localCargoHome "bin\cargo.exe"

Write-Host "== AgentMux check =="

$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
$cargoPath = if ($cargoCommand) { $cargoCommand.Source } else { $null }
if (-not $cargoPath -and (Test-Path $localCargo)) {
  $env:CARGO_HOME = $localCargoHome
  $env:RUSTUP_HOME = $localRustupHome
  $cargoPath = $localCargo
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
} else {
  Write-Warning "node was not found on PATH. Install Node.js before running docs checks."
  $missing += "node"
}

if ($missing.Count -gt 0) {
  Write-Error ("Missing required tool(s): " + ($missing -join ", "))
}

Write-Host "All available checks passed."
