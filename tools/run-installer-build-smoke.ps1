param(
  [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$desktopDir = Join-Path $root "apps\desktop"
$prepareSidecarsScript = Join-Path $root "tools\prepare-desktop-bundle-binaries.ps1"
$localCargoHome = Join-Path $root ".toolchains\cargo"
$localRustupHome = Join-Path $root ".toolchains\rustup"
$localCargo = Join-Path $localCargoHome "bin\cargo.exe"

$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
$cargoPath = if ($cargoCommand) { $cargoCommand.Source } else { $null }
if (-not $cargoPath -and (Test-Path $localCargo)) {
  $env:CARGO_HOME = $localCargoHome
  $env:RUSTUP_HOME = $localRustupHome
  $env:PATH = (Join-Path $localCargoHome "bin") + ";" + $env:PATH
  $cargoPath = $localCargo
}

if (-not $cargoPath) {
  throw "cargo was not found on PATH or in .toolchains."
}

if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
  throw "npm was not found on PATH."
}

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $machine = if ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { "unknown-machine" }
  $OutputDir = Join-Path $root "docs\implementation\evidence\$stamp-$machine-installer-build-smoke"
}

$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Invoke-ProcessCapture {
  param(
    [string]$FilePath,
    [string[]]$ArgumentList,
    [string]$StdoutPath,
    [string]$StderrPath,
    [string]$WorkingDirectory = $root
  )

  $process = Start-Process `
    -FilePath $FilePath `
    -ArgumentList $ArgumentList `
    -WorkingDirectory $WorkingDirectory `
    -RedirectStandardOutput $StdoutPath `
    -RedirectStandardError $StderrPath `
    -WindowStyle Hidden `
    -Wait `
    -PassThru
  return $process.ExitCode
}

function Write-JsonFile {
  param([object]$Value, [string]$Path)

  $Value | ConvertTo-Json -Depth 12 | Set-Content -Encoding UTF8 -Path $Path
}

function Get-FileSnapshot {
  param([string]$Path)

  if (-not (Test-Path -LiteralPath $Path)) {
    return [ordered]@{
      path = $Path
      exists = $false
    }
  }

  $item = Get-Item -LiteralPath $Path
  $hash = Get-FileHash -Algorithm SHA256 -LiteralPath $Path
  return [ordered]@{
    path = $Path.Substring($root.Length).TrimStart("\", "/").Replace("\", "/")
    exists = $true
    bytes = $item.Length
    sha256 = $hash.Hash
    last_write_time = $item.LastWriteTimeUtc.ToString("o")
  }
}

$bundleDir = Join-Path $root "target\release\bundle\nsis"
$resolvedBundleDir = Resolve-Path $bundleDir -ErrorAction SilentlyContinue
if ($resolvedBundleDir) {
  if (-not $resolvedBundleDir.Path.StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to remove outside workspace: $($resolvedBundleDir.Path)"
  }
  Remove-Item -LiteralPath $resolvedBundleDir.Path -Recurse -Force
}

$stdoutPath = Join-Path $OutputDir "installer-build.stdout.txt"
$stderrPath = Join-Path $OutputDir "installer-build.stderr.txt"
$sidecarStdoutPath = Join-Path $OutputDir "prepare-sidecars.stdout.txt"
$sidecarStderrPath = Join-Path $OutputDir "prepare-sidecars.stderr.txt"

$sidecarExitCode = Invoke-ProcessCapture `
  -FilePath "powershell" `
  -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $prepareSidecarsScript) `
  -StdoutPath $sidecarStdoutPath `
  -StderrPath $sidecarStderrPath `
  -WorkingDirectory $root

if ($sidecarExitCode -ne 0) {
  throw "sidecar preparation failed with exit code $sidecarExitCode. See $sidecarStderrPath"
}

$exitCode = Invoke-ProcessCapture `
  -FilePath "cmd.exe" `
  -ArgumentList @("/d", "/c", ".\node_modules\.bin\tauri.cmd build --ci --no-sign -b nsis") `
  -StdoutPath $stdoutPath `
  -StderrPath $stderrPath `
  -WorkingDirectory $desktopDir

if ($exitCode -ne 0) {
  throw "installer build failed with exit code $exitCode. See $stderrPath"
}

$installer = Get-ChildItem -LiteralPath $bundleDir -Filter "*setup.exe" -File -ErrorAction SilentlyContinue |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 1

if (-not $installer) {
  throw "installer build did not produce a NSIS setup executable in $bundleDir"
}

$hash = Get-FileHash -Algorithm SHA256 -LiteralPath $installer.FullName
$archivedInstaller = Join-Path $OutputDir $installer.Name
Copy-Item -LiteralPath $installer.FullName -Destination $archivedInstaller -Force
$releaseExe = Join-Path $root "target\release\agentmux-desktop-host.exe"
if (-not (Test-Path $releaseExe)) {
  throw "installer build did not produce target/release/agentmux-desktop-host.exe"
}

$sidecars = @(
  (Get-FileSnapshot -Path (Join-Path $root "apps\desktop\src-tauri\binaries\agentmux-x86_64-pc-windows-msvc.exe")),
  (Get-FileSnapshot -Path (Join-Path $root "apps\desktop\src-tauri\binaries\cmux-x86_64-pc-windows-msvc.exe"))
)
foreach ($sidecar in $sidecars) {
  if (-not $sidecar.exists) {
    throw "installer build sidecar preparation did not produce $($sidecar.path)"
  }
}

$summary = [ordered]@{
  generated_at = (Get-Date).ToUniversalTime().ToString("o")
  command = "tauri build --ci --no-sign -b nsis"
  sidecar_prepare_exit_code = $sidecarExitCode
  sidecars = $sidecars
  exit_code = $exitCode
  installer_path = $installer.FullName.Substring($root.Length).TrimStart("\", "/").Replace("\", "/")
  archived_installer = [System.IO.Path]::GetFileName($archivedInstaller)
  installer_bytes = $installer.Length
  installer_sha256 = $hash.Hash
  release_executable = "target/release/agentmux-desktop-host.exe"
  release_executable_bytes = (Get-Item -LiteralPath $releaseExe).Length
  sidecar_stdout = [System.IO.Path]::GetFileName($sidecarStdoutPath)
  sidecar_stderr = [System.IO.Path]::GetFileName($sidecarStderrPath)
  stdout = [System.IO.Path]::GetFileName($stdoutPath)
  stderr = [System.IO.Path]::GetFileName($stderrPath)
}
Write-JsonFile -Value $summary -Path (Join-Path $OutputDir "summary.json")

Write-Host ("Installer build smoke artifacts written to " + $OutputDir)
