param(
  [string]$OutputDir = "",
  [string]$Distribution = "",
  [switch]$SkipBuild,
  [switch]$RequireUnderlyingAgents
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$localCargoHome = Join-Path $root ".toolchains\cargo"
$localRustupHome = Join-Path $root ".toolchains\rustup"
$localCargo = Join-Path $localCargoHome "bin\cargo.exe"

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $machine = if ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { "unknown-machine" }
  $OutputDir = Join-Path $root "docs\implementation\evidence\$stamp-$machine-integration-live-smoke"
}

$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function ConvertTo-SafeName {
  param([string]$Value)

  $safe = $Value -replace "[^A-Za-z0-9_.-]", "-"
  $safe = $safe.Trim("-")
  if ([string]::IsNullOrWhiteSpace($safe)) {
    return "command"
  }
  return $safe
}

function ConvertTo-ProcessArgument {
  param([string]$Argument)

  if ($null -eq $Argument -or $Argument.Length -eq 0) {
    return '""'
  }
  if ($Argument -notmatch '[\s"]') {
    return $Argument
  }

  $escaped = $Argument -replace '(\\*)"', '$1$1\"'
  $escaped = $escaped -replace '(\\+)$', '$1$1'
  return '"' + $escaped + '"'
}

function Join-ProcessArguments {
  param([string[]]$Arguments)

  $quoted = @()
  foreach ($argument in $Arguments) {
    $quoted += ConvertTo-SingleString -Value (ConvertTo-ProcessArgument -Argument $argument)
  }
  return ($quoted -join " ")
}

function ConvertTo-SingleString {
  param([object]$Value)

  return (@($Value) -join "")
}

function Limit-Text {
  param([string]$Value, [int]$MaxLength = 2000)

  if ($null -eq $Value) {
    return ""
  }
  if ($Value.Length -le $MaxLength) {
    return $Value
  }
  return $Value.Substring(0, $MaxLength) + "`n...[truncated]"
}

function Split-NonEmptyLines {
  param([string]$Text)

  if ([string]::IsNullOrWhiteSpace($Text)) {
    return @()
  }

  return @(
    ($Text -replace "`0", "") -split "\r?\n" |
      ForEach-Object { $_.Trim() } |
      Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
  )
}

function Write-JsonFile {
  param([object]$Value, [string]$Path)

  $Value | ConvertTo-Json -Depth 16 | Set-Content -Encoding UTF8 -LiteralPath $Path
}

function Invoke-ProcessCapture {
  param(
    [string]$Name,
    [string]$FilePath,
    [string[]]$ArgumentList = @(),
    [string]$PathPrefix = "",
    [int]$TimeoutSeconds = 60
  )

  $safeName = ConvertTo-SafeName $Name
  $stdoutPath = Join-Path $OutputDir "$safeName.stdout.txt"
  $stderrPath = Join-Path $OutputDir "$safeName.stderr.txt"
  $commandPath = Join-Path $OutputDir "$safeName.cmd"
  $startedAt = (Get-Date).ToUniversalTime().ToString("o")

  try {
    $quotedFilePath = ConvertTo-SingleString -Value (ConvertTo-ProcessArgument -Argument $FilePath)
    $quotedRoot = ConvertTo-SingleString -Value (ConvertTo-ProcessArgument -Argument $root)
    $quotedStdout = ConvertTo-SingleString -Value (ConvertTo-ProcessArgument -Argument $stdoutPath)
    $quotedStderr = ConvertTo-SingleString -Value (ConvertTo-ProcessArgument -Argument $stderrPath)
    $quotedCommandPath = ConvertTo-SingleString -Value (ConvertTo-ProcessArgument -Argument $commandPath)
    $commandLine = $quotedFilePath
    if ($ArgumentList.Count -gt 0) {
      $commandLine = $commandLine + " " + (Join-ProcessArguments $ArgumentList)
    }
    $pathLine = if ([string]::IsNullOrWhiteSpace($PathPrefix)) {
      ""
    } else {
      "set `"PATH=$PathPrefix;%PATH%`""
    }
    $cmdText = @(
      "@echo off",
      "cd /d $quotedRoot",
      $pathLine,
      "$commandLine > $quotedStdout 2> $quotedStderr",
      "exit /b %ERRORLEVEL%"
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    Set-Content -Encoding ASCII -LiteralPath $commandPath -Value $cmdText

    $startArgs = @{
      FilePath = "cmd.exe"
      WorkingDirectory = $root
      WindowStyle = "Hidden"
      PassThru = $true
      ArgumentList = "/d /c $quotedCommandPath"
    }
    $process = Start-Process @startArgs
    $timedOut = -not $process.WaitForExit($TimeoutSeconds * 1000)
    if ($timedOut) {
      try {
        $process.Kill()
      } catch {
        Add-Content -Encoding UTF8 -LiteralPath $stderrPath -Value "`nFailed to kill timed out process: $($_.Exception.Message)"
      }
      $exitCode = $null
    } else {
      $process.Refresh()
      $exitCode = $process.ExitCode
    }
    $stdout = if (Test-Path -LiteralPath $stdoutPath) { Get-Content -Raw -LiteralPath $stdoutPath } else { "" }
    $stderr = if (Test-Path -LiteralPath $stderrPath) { Get-Content -Raw -LiteralPath $stderrPath } else { "" }
    $stdout = $stdout -replace "`0", ""
    $stderr = $stderr -replace "`0", ""
  } catch {
    $stdout = ""
    $stderr = $_.Exception.Message
    $timedOut = $false
    $exitCode = $null
    Set-Content -Encoding UTF8 -LiteralPath $stdoutPath -Value $stdout
    Set-Content -Encoding UTF8 -LiteralPath $stderrPath -Value $stderr
  }

  return [ordered]@{
    name = $Name
    file = $FilePath
    arguments = $ArgumentList
    command = [System.IO.Path]::GetFileName($commandPath)
    started_at = $startedAt
    exit_code = $exitCode
    timed_out = $timedOut
    stdout = [System.IO.Path]::GetFileName($stdoutPath)
    stderr = [System.IO.Path]::GetFileName($stderrPath)
    stdout_preview = Limit-Text $stdout
    stderr_preview = Limit-Text $stderr
    stdout_text = $stdout
    stderr_text = $stderr
  }
}

function ConvertTo-CaptureSummary {
  param([object]$Capture)

  if ($null -eq $Capture) {
    return $null
  }

  return [ordered]@{
    name = $Capture["name"]
    file = $Capture["file"]
    arguments = $Capture["arguments"]
    command = $Capture["command"]
    started_at = $Capture["started_at"]
    exit_code = $Capture["exit_code"]
    timed_out = $Capture["timed_out"]
    stdout = $Capture["stdout"]
    stderr = $Capture["stderr"]
    stdout_preview = $Capture["stdout_preview"]
    stderr_preview = $Capture["stderr_preview"]
  }
}

function Try-ParseJson {
  param([string]$Text)

  if ([string]::IsNullOrWhiteSpace($Text)) {
    return $null
  }

  try {
    return ($Text | ConvertFrom-Json)
  } catch {
    return $null
  }
}

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

function Resolve-CmuxExecutable {
  $candidates = @(
    (Join-Path $root "target\debug\cmux.exe"),
    (Join-Path $root "target\debug\agentmux.exe"),
    (Join-Path $root "target\release\cmux.exe"),
    (Join-Path $root "target\release\agentmux.exe")
  )

  foreach ($candidate in $candidates) {
    if (Test-Path -LiteralPath $candidate) {
      return $candidate
    }
  }
  return $null
}

function Resolve-WslDistributionWithTmux {
  param([string]$PreferredDistribution)

  $wslCommand = Get-Command wsl.exe -ErrorAction SilentlyContinue
  if (-not $wslCommand) {
    return $null
  }

  $candidates = @()
  if (-not [string]::IsNullOrWhiteSpace($PreferredDistribution)) {
    $candidates += $PreferredDistribution
  }

  $list = Invoke-ProcessCapture `
    -Name "wsl-list-quiet" `
    -FilePath $wslCommand.Source `
    -ArgumentList @("--list", "--quiet") `
    -TimeoutSeconds 20
  $candidates += @(Split-NonEmptyLines $list.stdout_text)

  foreach ($candidate in ($candidates | Select-Object -Unique)) {
    if ([string]::IsNullOrWhiteSpace($candidate)) {
      continue
    }
    $probe = Invoke-ProcessCapture `
      -Name "wsl-$candidate-tmux-probe" `
      -FilePath $wslCommand.Source `
      -ArgumentList @("--distribution", $candidate, "--exec", "sh", "-lc", "command -v tmux >/dev/null 2>&1 && tmux -V") `
      -TimeoutSeconds 30
    if ($probe.exit_code -eq 0) {
      return $candidate
    }
  }

  return $null
}

function Get-DoctorChecks {
  param([object]$DoctorJson)

  $checks = @()
  if ($null -eq $DoctorJson -or -not $DoctorJson.integrations) {
    return $checks
  }

  foreach ($integration in $DoctorJson.integrations) {
    foreach ($check in $integration.checks) {
      $checks += [ordered]@{
        integration = $integration.integration
        status = $integration.status
        name = $check.name
        ok = [bool]$check.ok
        detail = $check.detail
        fix = $check.fix
      }
    }
  }
  return $checks
}

function Test-FoundationReady {
  param([object[]]$Checks)

  $foundationNames = @(
    "persistent-wrapper",
    "tmux-shim",
    "wrapper-on-path",
    "omo-shadow-config",
    "omo-opencode-plugin",
    "omo-tmux-enabled",
    "omo-node-modules-isolated",
    "omc-node-options-restore",
    "wsl-distribution",
    "wsl-tmux-shim",
    "wsl-omo-shadow-config",
    "wsl-omc-node-options-restore"
  )

  $foundationChecks = @($Checks | Where-Object { $foundationNames -contains $_.name })
  if ($foundationChecks.Count -eq 0) {
    return $false
  }
  return -not [bool](@($foundationChecks | Where-Object { -not $_.ok }).Count)
}

$cargoPath = Resolve-CargoExecutable
if (-not $SkipBuild) {
  $build = Invoke-ProcessCapture `
    -Name "cargo-build-agentmux-cli" `
    -FilePath $cargoPath `
    -ArgumentList @("build", "-p", "agentmux-cli") `
    -TimeoutSeconds 120
  if ($build.exit_code -ne 0) {
    throw "CLI build failed with exit code $($build.exit_code). See $($build.stderr)"
  }
}

$cmuxExe = Resolve-CmuxExecutable
if (-not $cmuxExe) {
  throw "cmux.exe or agentmux.exe was not found after build."
}

$runtimeDir = Join-Path $OutputDir "runtime"
$baseDir = Join-Path $runtimeDir "cmuxterm"
$binDir = Join-Path $baseDir "bin"
New-Item -ItemType Directory -Force -Path $runtimeDir | Out-Null

$install = Invoke-ProcessCapture `
  -Name "integration-install-shims" `
  -FilePath $cmuxExe `
  -ArgumentList @("integrations", "install-shims", "--json", "--base-dir", $baseDir, "--bin-dir", $binDir) `
  -TimeoutSeconds 60
if ($install.exit_code -ne 0) {
  throw "Integration shim install failed with exit code $($install.exit_code). See $($install.stderr)"
}

$windowsDoctor = Invoke-ProcessCapture `
  -Name "integration-doctor-windows" `
  -FilePath $cmuxExe `
  -ArgumentList @("integrations", "doctor", "--json", "--base-dir", $baseDir, "--bin-dir", $binDir) `
  -PathPrefix $binDir `
  -TimeoutSeconds 60
if ($windowsDoctor.exit_code -ne 0) {
  throw "Windows integration doctor failed with exit code $($windowsDoctor.exit_code). See $($windowsDoctor.stderr)"
}

$distribution = Resolve-WslDistributionWithTmux -PreferredDistribution $Distribution
$wslDoctor = $null
if ($distribution) {
  $wslDoctor = Invoke-ProcessCapture `
    -Name "integration-doctor-wsl-$distribution" `
    -FilePath $cmuxExe `
    -ArgumentList @("integrations", "doctor", "--json", "--base-dir", $baseDir, "--bin-dir", $binDir, "--distribution", $distribution) `
    -PathPrefix $binDir `
    -TimeoutSeconds 90
  if ($wslDoctor.exit_code -ne 0) {
    throw "WSL integration doctor failed with exit code $($wslDoctor.exit_code). See $($wslDoctor.stderr)"
  }
}

$installJson = Try-ParseJson $install.stdout_text
$windowsDoctorJson = Try-ParseJson $windowsDoctor.stdout_text
$wslDoctorJson = if ($wslDoctor) { Try-ParseJson $wslDoctor.stdout_text } else { $null }
$windowsChecks = @(Get-DoctorChecks $windowsDoctorJson)
$wslChecks = @(Get-DoctorChecks $wslDoctorJson)
$allChecks = @($windowsChecks + $wslChecks)
$foundationReady = Test-FoundationReady $allChecks
$underlyingMissing = @($allChecks | Where-Object { $_.name -eq "underlying-executable" -and -not $_.ok })
$underlyingReady = $underlyingMissing.Count -eq 0
$status = if ($foundationReady -and ($underlyingReady -or -not $RequireUnderlyingAgents)) {
  "passed"
} else {
  "needs_attention"
}

$summary = [ordered]@{
  generated_at = (Get-Date).ToUniversalTime().ToString("o")
  script = "tools/run-integration-live-smoke.ps1"
  output_dir = $OutputDir
  cmux_executable = $cmuxExe
  base_dir = $baseDir
  bin_dir = $binDir
  require_underlying_agents = [bool]$RequireUnderlyingAgents
  selected_distribution = $distribution
  install = [ordered]@{
    command = ConvertTo-CaptureSummary $install
    result = $installJson
  }
  windows_doctor = [ordered]@{
    command = ConvertTo-CaptureSummary $windowsDoctor
    checks = $windowsChecks
  }
  wsl_doctor = if ($wslDoctor) {
    [ordered]@{
      command = ConvertTo-CaptureSummary $wslDoctor
      checks = $wslChecks
    }
  } else {
    [ordered]@{
      command = $null
      checks = @()
      status = "skipped"
      reason = "No WSL distribution with tmux was found."
    }
  }
  result = [ordered]@{
    status = $status
    foundation_ready = $foundationReady
    underlying_agents_ready = $underlyingReady
    underlying_missing = $underlyingMissing
  }
}

Write-JsonFile -Value $summary -Path (Join-Path $OutputDir "summary.json")

$readme = @"
# Integration Live Smoke

Generated: $($summary.generated_at)

This smoke installs AgentMux/cmux integration wrappers and tmux shims into an
isolated evidence runtime directory, temporarily prepends that bin directory to
PATH only for doctor subprocesses, and records Windows plus WSL doctor results.

- Result: $status
- Foundation ready: $foundationReady
- Underlying agents ready: $underlyingReady
- Base dir: $baseDir
- Bin dir: $binDir
- WSL distribution: $distribution

Use `-RequireUnderlyingAgents` to make missing `claude`, `opencode`, `omx`, or
`omc` executables fail the smoke.
"@
Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDir "README.md") -Value $readme

if ($status -ne "passed") {
  throw "Integration live smoke needs attention. See $(Join-Path $OutputDir "summary.json")"
}

Write-Host ("Integration live smoke artifacts written to " + $OutputDir)
