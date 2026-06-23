param(
  [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$desktopTauriDir = Join-Path $root "apps\desktop\src-tauri"
$targetTriple = "x86_64-pc-windows-msvc"
$localCargoHome = Join-Path $root ".toolchains\cargo"
$localRustupHome = Join-Path $root ".toolchains\rustup"
$localCargo = Join-Path $localCargoHome "bin\cargo.exe"

function Resolve-CargoExecutable {
  $cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
  if ($cargoCommand) {
    return $cargoCommand.Source
  }

  if (Test-Path -LiteralPath $localCargo) {
    $env:CARGO_HOME = $localCargoHome
    $env:RUSTUP_HOME = $localRustupHome
    if (-not $env:RUSTUP_TOOLCHAIN) {
      $env:RUSTUP_TOOLCHAIN = "stable-x86_64-pc-windows-msvc"
    }
    $env:PATH = (Join-Path $localCargoHome "bin") + ";" + $env:PATH
    return $localCargo
  }

  throw "cargo was not found on PATH or in .toolchains."
}

if (-not $SkipBuild) {
  $cargoPath = Resolve-CargoExecutable
  & $cargoPath build -p agentmux-cli --release
  if ($LASTEXITCODE -ne 0) {
    throw "agentmux-cli release build failed with exit code $LASTEXITCODE."
  }
}

$binaryDir = Join-Path $desktopTauriDir "binaries"
New-Item -ItemType Directory -Force -Path $binaryDir | Out-Null

$prepared = @()
foreach ($name in @("agentmux", "cmux")) {
  $source = Join-Path $root "target\release\$name.exe"
  if (-not (Test-Path -LiteralPath $source)) {
    throw "Required release CLI binary was not found: $source"
  }

  $destination = Join-Path $binaryDir "$name-$targetTriple.exe"
  Copy-Item -LiteralPath $source -Destination $destination -Force
  $prepared += $destination
}

Write-Host "Prepared Tauri sidecar binaries:"
foreach ($path in $prepared) {
  Write-Host "  $path"
}
