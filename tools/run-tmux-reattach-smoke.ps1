param(
  [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
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

if (-not (Get-Command wsl.exe -ErrorAction SilentlyContinue)) {
  throw "wsl.exe was not found on PATH."
}

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $machine = if ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { "unknown-machine" }
  $OutputDir = Join-Path $root "docs\implementation\evidence\$stamp-$machine-real-tmux-reattach-smoke"
}

$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Invoke-ProcessCapture {
  param(
    [string]$FilePath,
    [string[]]$ArgumentList,
    [string]$StdoutPath,
    [string]$StderrPath
  )

  $process = Start-Process `
    -FilePath $FilePath `
    -ArgumentList $ArgumentList `
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

function Get-WslDistributions {
  $raw = & wsl.exe --list --quiet 2>$null
  if ($LASTEXITCODE -ne 0) {
    return @()
  }

  return @($raw | ForEach-Object {
    ($_ -replace [char]0, "").Trim()
  } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
}

function Get-TmuxDistribution {
  foreach ($distribution in Get-WslDistributions) {
    & wsl.exe --distribution $distribution --exec sh -lc "command -v tmux >/dev/null 2>&1"
    if ($LASTEXITCODE -eq 0) {
      return $distribution
    }
  }
  return $null
}

$distribution = Get-TmuxDistribution
if (-not $distribution) {
  throw "No WSL distribution with tmux was found."
}

$wslVersionPath = Join-Path $OutputDir "wsl-version.txt"
$wslDistributionsPath = Join-Path $OutputDir "wsl-distributions.txt"
$tmuxVersionPath = Join-Path $OutputDir "tmux-version.txt"

& wsl.exe --version > $wslVersionPath 2>&1
& wsl.exe --list --verbose > $wslDistributionsPath 2>&1
& wsl.exe --distribution $distribution --exec tmux -V > $tmuxVersionPath 2>&1

$stdoutPath = Join-Path $OutputDir "tmux-control-smoke.stdout.txt"
$stderrPath = Join-Path $OutputDir "tmux-control-smoke.stderr.txt"

$previous = @{
  AGENTMUX_RUN_TMUX_SMOKE = $env:AGENTMUX_RUN_TMUX_SMOKE
  AGENTMUX_RUN_TMUX_REATTACH_SMOKE = $env:AGENTMUX_RUN_TMUX_REATTACH_SMOKE
}

Push-Location $root
try {
  $env:AGENTMUX_RUN_TMUX_SMOKE = "1"
  $env:AGENTMUX_RUN_TMUX_REATTACH_SMOKE = "1"

  $exitCode = Invoke-ProcessCapture `
    -FilePath $cargoPath `
    -ArgumentList @("test", "-p", "agentmux-backend-tmux", "--test", "tmux_control_smoke", "--", "--nocapture") `
    -StdoutPath $stdoutPath `
    -StderrPath $stderrPath
} finally {
  Pop-Location
  foreach ($name in $previous.Keys) {
    if ($null -eq $previous[$name]) {
      Remove-Item -Path "env:$name" -ErrorAction SilentlyContinue
    } else {
      Set-Item -Path "env:$name" -Value $previous[$name]
    }
  }
}

$stdout = Get-Content -Raw -LiteralPath $stdoutPath
$stderr = Get-Content -Raw -LiteralPath $stderrPath
$combined = $stdout + "`n" + $stderr

if ($exitCode -ne 0) {
  throw "tmux control smoke failed with exit code $exitCode. See $stderrPath"
}

if ($combined -match "skipping tmux-control") {
  throw "tmux control smoke skipped instead of running. See $stdoutPath and $stderrPath"
}

if ($combined -notmatch "tmux_control_launches_in_wsl_and_round_trips_output \.\.\. ok") {
  throw "launch/input/output smoke did not report success. See $stdoutPath and $stderrPath"
}

if ($combined -notmatch "tmux_control_reattaches_without_duplicating_shell_process \.\.\. ok") {
  throw "reattach/no-duplicate smoke did not report success. See $stdoutPath and $stderrPath"
}

$summary = [ordered]@{
  generated_at = (Get-Date).ToUniversalTime().ToString("o")
  distribution = $distribution
  cargo = $cargoPath
  command = "cargo test -p agentmux-backend-tmux --test tmux_control_smoke -- --nocapture"
  exit_code = $exitCode
  launch_round_trip = "passed"
  reattach_without_duplicate_process = "passed"
  stdout = [System.IO.Path]::GetFileName($stdoutPath)
  stderr = [System.IO.Path]::GetFileName($stderrPath)
  wsl_version = [System.IO.Path]::GetFileName($wslVersionPath)
  wsl_distributions = [System.IO.Path]::GetFileName($wslDistributionsPath)
  tmux_version = [System.IO.Path]::GetFileName($tmuxVersionPath)
}
Write-JsonFile -Value $summary -Path (Join-Path $OutputDir "summary.json")

Write-Host ("Real WSL/tmux reattach smoke artifacts written to " + $OutputDir)
