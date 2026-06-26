param(
  [Parameter(Position = 0)]
  [ValidateSet("any", "wsl_exe_missing", "no_wsl_distribution", "wsl_without_tmux", "wsl_with_tmux", "wsl_distribution_unreachable")]
  [string]$ExpectedState = "any",

  [Parameter(Position = 1)]
  [string]$OutputDir = "",

  [string]$Distribution = ""
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $machine = if ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { "unknown-machine" }
  $OutputDir = Join-Path $root "docs\implementation\evidence\$stamp-$machine-wsl-state-gate"
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
    [int]$TimeoutSeconds = 30
  )

  $safeName = ConvertTo-SafeName $Name
  $stdoutPath = Join-Path $OutputDir "$safeName.stdout.txt"
  $stderrPath = Join-Path $OutputDir "$safeName.stderr.txt"
  $startedAt = (Get-Date).ToUniversalTime().ToString("o")

  try {
    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $FilePath
    $startInfo.WorkingDirectory = $root
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.CreateNoWindow = $true
    foreach ($argument in $ArgumentList) {
      if ($startInfo.ArgumentList) {
        [void]$startInfo.ArgumentList.Add($argument)
      } else {
        if ($startInfo.Arguments.Length -gt 0) {
          $startInfo.Arguments += " "
        }
        $startInfo.Arguments += ConvertTo-ProcessArgument $argument
      }
    }

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    [void]$process.Start()
    $timedOut = -not $process.WaitForExit($TimeoutSeconds * 1000)
    if ($timedOut) {
      try {
        $process.Kill()
      } catch {
        $stderr = "Failed to kill timed out process: $($_.Exception.Message)"
      }
      $exitCode = $null
      $stdout = $process.StandardOutput.ReadToEnd()
      $stderr = ($stderr + "`n" + $process.StandardError.ReadToEnd()).Trim()
    } else {
      $stdout = $process.StandardOutput.ReadToEnd()
      $stderr = $process.StandardError.ReadToEnd()
      $exitCode = $process.ExitCode
    }
    $stdout = $stdout -replace "`0", ""
    $stderr = $stderr -replace "`0", ""
  } catch {
    $stdout = ""
    $stderr = $_.Exception.Message
    $timedOut = $false
    $exitCode = $null
  }

  Set-Content -Encoding UTF8 -LiteralPath $stdoutPath -Value $stdout
  Set-Content -Encoding UTF8 -LiteralPath $stderrPath -Value $stderr

  return [ordered]@{
    name = $Name
    file = $FilePath
    arguments = $ArgumentList
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

function ConvertTo-CaptureSummary {
  param([object]$Capture)

  if ($null -eq $Capture) {
    return $null
  }

  return [ordered]@{
    name = $Capture["name"]
    file = $Capture["file"]
    arguments = $Capture["arguments"]
    started_at = $Capture["started_at"]
    exit_code = $Capture["exit_code"]
    timed_out = $Capture["timed_out"]
    stdout = $Capture["stdout"]
    stderr = $Capture["stderr"]
    stdout_preview = $Capture["stdout_preview"]
    stderr_preview = $Capture["stderr_preview"]
  }
}

$wslCommand = Get-Command wsl.exe -ErrorAction SilentlyContinue
if (-not $wslCommand) {
  $state = [ordered]@{
    status = "wsl_exe_missing"
    command = $null
    version = $null
    list_quiet = $null
    list_verbose = $null
    distributions = @()
    selected_distribution = $null
  }
} else {
  $version = Invoke-ProcessCapture -Name "wsl-version" -FilePath $wslCommand.Source -ArgumentList @("--version") -TimeoutSeconds 20
  $listQuiet = Invoke-ProcessCapture -Name "wsl-list-quiet" -FilePath $wslCommand.Source -ArgumentList @("--list", "--quiet") -TimeoutSeconds 20
  $listVerbose = Invoke-ProcessCapture -Name "wsl-list-verbose" -FilePath $wslCommand.Source -ArgumentList @("--list", "--verbose") -TimeoutSeconds 20
  $distributionNames = @(Split-NonEmptyLines $listQuiet.stdout_text)
  if (-not [string]::IsNullOrWhiteSpace($Distribution)) {
    $distributionNames = @($Distribution)
  }

  $distributionResults = @()
  foreach ($name in $distributionNames) {
    $safe = ConvertTo-SafeName $name
    $reach = Invoke-ProcessCapture `
      -Name "wsl-$safe-reachability" `
      -FilePath $wslCommand.Source `
      -ArgumentList @("--distribution", $name, "--exec", "sh", "-lc", "printf '%s' ok") `
      -TimeoutSeconds 30
    $tmux = Invoke-ProcessCapture `
      -Name "wsl-$safe-tmux-version" `
      -FilePath $wslCommand.Source `
      -ArgumentList @("--distribution", $name, "--exec", "sh", "-lc", "if command -v tmux >/dev/null 2>&1; then tmux -V; else exit 127; fi") `
      -TimeoutSeconds 30

    $distributionResults += [ordered]@{
      name = $name
      reachable = ($reach.exit_code -eq 0)
      reachability = ConvertTo-CaptureSummary $reach
      tmux_available = ($tmux.exit_code -eq 0)
      tmux_version = $tmux.stdout_text.Trim()
      tmux_probe = ConvertTo-CaptureSummary $tmux
    }
  }

  $withTmux = @($distributionResults | Where-Object { $_.reachable -and $_.tmux_available })
  $withoutTmux = @($distributionResults | Where-Object { $_.reachable -and -not $_.tmux_available })
  $selected =
    if ($withTmux.Count -gt 0) {
      $withTmux[0].name
    } elseif ($distributionResults.Count -gt 0) {
      $distributionResults[0].name
    } else {
      $null
    }
  $status =
    if ($distributionResults.Count -eq 0) {
      "no_wsl_distribution"
    } elseif ($withTmux.Count -gt 0) {
      "wsl_with_tmux"
    } elseif ($withoutTmux.Count -gt 0) {
      "wsl_without_tmux"
    } else {
      "wsl_distribution_unreachable"
    }

  $state = [ordered]@{
    status = $status
    command = $wslCommand.Source
    version = ConvertTo-CaptureSummary $version
    list_quiet = ConvertTo-CaptureSummary $listQuiet
    list_verbose = ConvertTo-CaptureSummary $listVerbose
    distributions = $distributionResults
    selected_distribution = $selected
  }
}

$passed = ($ExpectedState -eq "any" -or $state.status -eq $ExpectedState)
$summary = [ordered]@{
  generated_at = (Get-Date).ToUniversalTime().ToString("o")
  script = "tools/run-wsl-state-gate.ps1"
  expected_state = $ExpectedState
  observed_state = $state.status
  passed = $passed
  state = $state
}
Write-JsonFile -Value $summary -Path (Join-Path $OutputDir "summary.json")

$readme = @"
# WSL State Gate

Generated: $($summary.generated_at)

This gate classifies the current Windows WSL/tmux state and optionally fails
when it does not match `-ExpectedState`.

- Expected: $ExpectedState
- Observed: $($state.status)
- Passed: $passed
- Selected distribution: $($state.selected_distribution)
"@
Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDir "README.md") -Value $readme

if (-not $passed) {
  throw "WSL state gate expected '$ExpectedState' but observed '$($state.status)'. See $(Join-Path $OutputDir "summary.json")"
}

Write-Host ("WSL state gate artifacts written to " + $OutputDir)
