param(
  [string]$OutputDir = "",
  [switch]$Smoke
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
  $cargoPath = $localCargo
}

if (-not $cargoPath) {
  throw "cargo was not found on PATH or in .toolchains."
}

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $machine = if ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { "unknown-machine" }
  $OutputDir = Join-Path $root "docs\implementation\evidence\$stamp-$machine-performance-gates"
}

$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Invoke-OptionalCommand {
  param([string]$Command, [string[]]$Arguments = @())

  $resolved = Get-Command $Command -ErrorAction SilentlyContinue
  if (-not $resolved) {
    return $null
  }

  try {
    $output = & $resolved.Source @Arguments 2>&1
    return Normalize-CommandOutput (($output | Out-String).Trim())
  } catch {
    return Normalize-CommandOutput $_.Exception.Message
  }
}

function Normalize-CommandOutput {
  param([AllowNull()][string]$Value)

  if ($null -eq $Value) {
    return $null
  }

  return ($Value -replace "`0", "").Trim()
}

function Split-NonEmptyLines {
  param([AllowNull()][string]$Value)

  if ([string]::IsNullOrWhiteSpace($Value)) {
    return @()
  }

  return @($Value -split "\r?\n" | ForEach-Object { $_.Trim() } | Where-Object { $_ })
}

function Get-VersionValues {
  param([AllowNull()][string]$Value)

  if ([string]::IsNullOrWhiteSpace($Value)) {
    return @()
  }

  return @([regex]::Matches($Value, "\d+(?:\.\d+)+(?:-\d+)?") | ForEach-Object { $_.Value })
}

function Write-JsonFile {
  param([object]$Value, [string]$Path)

  $Value | ConvertTo-Json -Depth 8 | Set-Content -Encoding UTF8 -Path $Path
}

function Get-ReferenceProfile {
  $os = Get-CimInstance Win32_OperatingSystem
  $computer = Get-CimInstance Win32_ComputerSystem
  $processors = @(Get-CimInstance Win32_Processor)
  $powerScheme = Invoke-OptionalCommand -Command "powercfg.exe" -Arguments @("/getactivescheme")
  $wslVersion = Invoke-OptionalCommand -Command "wsl.exe" -Arguments @("--version")
  $wslDistributions = Invoke-OptionalCommand -Command "wsl.exe" -Arguments @("--list", "--quiet")

  return [ordered]@{
    generated_at = (Get-Date).ToUniversalTime().ToString("o")
    machine_name = $env:COMPUTERNAME
    user_domain = $env:USERDOMAIN
    os = [ordered]@{
      caption = $os.Caption
      version = $os.Version
      build_number = $os.BuildNumber
      architecture = $os.OSArchitecture
    }
    computer = [ordered]@{
      manufacturer = $computer.Manufacturer
      model = $computer.Model
      total_physical_memory_bytes = [int64]$computer.TotalPhysicalMemory
    }
    cpu = @($processors | ForEach-Object {
      [ordered]@{
        name = $_.Name
        cores = [int]$_.NumberOfCores
        logical_processors = [int]$_.NumberOfLogicalProcessors
        max_clock_mhz = [int]$_.MaxClockSpeed
      }
    })
    power_scheme = $powerScheme
    wsl_version = $wslVersion
    wsl_version_values = Get-VersionValues $wslVersion
    wsl_distributions = $wslDistributions
    wsl_distribution_names = Split-NonEmptyLines $wslDistributions
    display_scale = $null
    notes = "Set display_scale and release-lab notes manually if Windows does not expose them consistently."
  }
}

function Get-Benchmarks {
  if ($Smoke) {
    return @(
      @{ Name = "single-terminal-latency"; Args = @("run", "-p", "agentmux-bench-single-terminal-latency") },
      @{ Name = "many-idle-sessions"; Args = @("run", "-p", "agentmux-bench-many-idle-sessions", "--", "--sessions", "1", "--observe-ms", "250") },
      @{ Name = "high-output"; Args = @("run", "-p", "agentmux-bench-high-output", "--", "--lines", "100", "--visible-probes", "1") },
      @{ Name = "resize-storm"; Args = @("run", "-p", "agentmux-bench-resize-storm", "--", "--iterations", "5") },
      @{ Name = "restart-recovery"; Args = @("run", "-p", "agentmux-bench-restart-recovery", "--", "--sessions", "2") }
    )
  }

  return @(
    @{ Name = "single-terminal-latency"; Args = @("run", "-p", "agentmux-bench-single-terminal-latency") },
    @{ Name = "many-idle-sessions"; Args = @("run", "-p", "agentmux-bench-many-idle-sessions") },
    @{ Name = "high-output"; Args = @("run", "-p", "agentmux-bench-high-output") },
    @{ Name = "resize-storm"; Args = @("run", "-p", "agentmux-bench-resize-storm") },
    @{ Name = "restart-recovery"; Args = @("run", "-p", "agentmux-bench-restart-recovery") }
  )
}

Push-Location $root
try {
  $profilePath = Join-Path $OutputDir "reference-profile.json"
  Write-JsonFile -Value (Get-ReferenceProfile) -Path $profilePath

  $results = @()
  foreach ($benchmark in Get-Benchmarks) {
    $stdoutPath = Join-Path $OutputDir ($benchmark.Name + ".json")
    $stderrPath = Join-Path $OutputDir ($benchmark.Name + ".stderr.txt")
    $started = Get-Date
    $timer = [System.Diagnostics.Stopwatch]::StartNew()

    $process = Start-Process `
      -FilePath $cargoPath `
      -ArgumentList @($benchmark.Args) `
      -RedirectStandardOutput $stdoutPath `
      -RedirectStandardError $stderrPath `
      -WindowStyle Hidden `
      -Wait `
      -PassThru
    $exitCode = $process.ExitCode
    $timer.Stop()

    $results += [ordered]@{
      name = $benchmark.Name
      command = "cargo " + ($benchmark.Args -join " ")
      exit_code = $exitCode
      started_at = $started.ToUniversalTime().ToString("o")
      elapsed_ms = [Math]::Round($timer.Elapsed.TotalMilliseconds, 3)
      stdout = [System.IO.Path]::GetFileName($stdoutPath)
      stderr = [System.IO.Path]::GetFileName($stderrPath)
    }

    if ($exitCode -ne 0) {
      throw "Benchmark '$($benchmark.Name)' failed with exit code $exitCode. See $stderrPath"
    }
  }

  $manifest = [ordered]@{
    generated_at = (Get-Date).ToUniversalTime().ToString("o")
    smoke = [bool]$Smoke
    output_dir = $OutputDir
    cargo = $cargoPath
    reference_profile = [System.IO.Path]::GetFileName($profilePath)
    benchmarks = $results
  }
  Write-JsonFile -Value $manifest -Path (Join-Path $OutputDir "manifest.json")
} finally {
  Pop-Location
}

Write-Host ("Performance gate artifacts written to " + $OutputDir)
