param(
  [switch]$DebugBuild,
  [switch]$NoBundle,
  [switch]$Ci,

  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$TauriArgs = @()
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$desktopDir = Join-Path $root "apps\desktop"
$prepareScript = Join-Path $root "tools\prepare-desktop-bundle-binaries.ps1"
$tauriCmd = Join-Path $desktopDir "node_modules\.bin\tauri.cmd"
$localCargoHome = Join-Path $root ".toolchains\cargo"
$localRustupHome = Join-Path $root ".toolchains\rustup"
$localCargo = Join-Path $localCargoHome "bin\cargo.exe"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue) -and (Test-Path -LiteralPath $localCargo)) {
  $env:CARGO_HOME = $localCargoHome
  $env:RUSTUP_HOME = $localRustupHome
  if (-not $env:RUSTUP_TOOLCHAIN) {
    $env:RUSTUP_TOOLCHAIN = "stable-x86_64-pc-windows-msvc"
  }
  $env:PATH = (Join-Path $localCargoHome "bin") + ";" + $env:PATH
}

if (-not (Test-Path -LiteralPath $tauriCmd)) {
  $tauriCommand = Get-Command tauri -ErrorAction SilentlyContinue
  if (-not $tauriCommand) {
    throw "tauri was not found in apps/desktop/node_modules/.bin or on PATH."
  }
  $tauriCmd = $tauriCommand.Source
}

& powershell -NoProfile -ExecutionPolicy Bypass -File $prepareScript
if ($LASTEXITCODE -ne 0) {
  throw "desktop bundle sidecar preparation failed with exit code $LASTEXITCODE."
}

Push-Location $desktopDir
try {
  $buildArgs = @()
  if ($DebugBuild) {
    $buildArgs += "--debug"
  }
  if ($NoBundle) {
    $buildArgs += "--no-bundle"
  }
  if ($Ci) {
    $buildArgs += "--ci"
  }
  $buildArgs += $TauriArgs

  Write-Host ("Running tauri build " + ($buildArgs -join " "))
  & $tauriCmd build @buildArgs
  if ($LASTEXITCODE -ne 0) {
    throw "tauri build failed with exit code $LASTEXITCODE."
  }
} finally {
  Pop-Location
}
