param(
  [switch]$SkipBuild,

  [ValidateSet("Debug", "Release")]
  [string]$BuildProfile = "Release"
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

function Invoke-AgentMuxCliBuild {
  param(
    [Parameter(Mandatory = $true)]
    [string]$CargoPath,

    [Parameter(Mandatory = $true)]
    [ValidateSet("Debug", "Release")]
    [string]$BuildProfile,

    [int]$MaxAttempts = 3
  )

  $cargoArgs = @("build", "-p", "agentmux-cli")
  if ($BuildProfile -eq "Release") {
    $cargoArgs += "--release"
  }

  for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
    & $CargoPath @cargoArgs
    if ($LASTEXITCODE -eq 0) {
      return
    }

    $exitCode = $LASTEXITCODE
    if ($attempt -ge $MaxAttempts) {
      throw "agentmux-cli $BuildProfile build failed with exit code $exitCode after $MaxAttempts attempts."
    }

    $delaySeconds = 5 * $attempt
    Write-Warning "agentmux-cli $BuildProfile build failed with exit code $exitCode (attempt $attempt/$MaxAttempts); retrying in $delaySeconds seconds."
    Start-Sleep -Seconds $delaySeconds
  }
}

if (-not $SkipBuild) {
  $cargoPath = Resolve-CargoExecutable
  Invoke-AgentMuxCliBuild -CargoPath $cargoPath -BuildProfile $BuildProfile
}

$binaryDir = Join-Path $desktopTauriDir "binaries"
New-Item -ItemType Directory -Force -Path $binaryDir | Out-Null

$targetDir = if ($BuildProfile -eq "Release") { "release" } else { "debug" }
$prepared = @()
foreach ($name in @("agentmux", "cmux")) {
  $source = Join-Path $root "target\$targetDir\$name.exe"
  if (-not (Test-Path -LiteralPath $source)) {
    throw "Required $BuildProfile CLI binary was not found: $source"
  }

  $destination = Join-Path $binaryDir "$name-$targetTriple.exe"
  Copy-Item -LiteralPath $source -Destination $destination -Force
  $prepared += $destination
}

Write-Host "Prepared Tauri sidecar binaries:"
foreach ($path in $prepared) {
  Write-Host "  $path"
}
