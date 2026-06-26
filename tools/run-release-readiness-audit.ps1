param(
  [string]$OutputDir = "",
  [string]$Distribution = "",
  [switch]$FailOnNeedsAttention
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$allIntegrationKinds = @("claude-teams", "omo", "omx", "omc")

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $machine = if ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { "unknown-machine" }
  $OutputDir = Join-Path $root "docs\implementation\evidence\$stamp-$machine-release-readiness-audit"
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

function ConvertTo-ProcessArgument {
  param([string]$Argument)

  if ($null -eq $Argument) {
    return '""'
  }
  if ($Argument.Length -eq 0) {
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

  return (($Arguments | ForEach-Object { ConvertTo-ProcessArgument $_ }) -join " ")
}

function Invoke-ProcessCapture {
  param(
    [string]$Name,
    [string]$FilePath,
    [string[]]$ArgumentList = @(),
    [int]$TimeoutSeconds = 60
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
    if ($ArgumentList.Count -gt 0) {
      $startInfo.Arguments = Join-ProcessArguments $ArgumentList
    }

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    [void]$process.Start()
    $timedOut = -not $process.WaitForExit($TimeoutSeconds * 1000)
    if ($timedOut) {
      try {
        $process.Kill($true)
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
    Set-Content -Encoding UTF8 -LiteralPath $stdoutPath -Value $stdout
    Set-Content -Encoding UTF8 -LiteralPath $stderrPath -Value $stderr
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
    started_at = $Capture["started_at"]
    exit_code = $Capture["exit_code"]
    timed_out = $Capture["timed_out"]
    stdout = $Capture["stdout"]
    stderr = $Capture["stderr"]
    stdout_preview = $Capture["stdout_preview"]
    stderr_preview = $Capture["stderr_preview"]
  }
}

function ConvertTo-DoctorResultSummary {
  param([object]$Result)

  if ($null -eq $Result -or -not $Result.integrations -or $Result.integrations.Count -eq 0) {
    return $null
  }

  $integration = $Result.integrations[0]
  return [ordered]@{
    kind = $integration.kind
    status = $integration.status
    checks = @(
      $integration.checks | ForEach-Object {
        [ordered]@{
          name = $_.name
          ok = $_.ok
          detail = $_.detail
          fix = $_.fix
        }
      }
    )
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

function Find-LatestInstaller {
  $bundleDir = Join-Path $root "target\release\bundle\nsis"
  $installer = Get-ChildItem -LiteralPath $bundleDir -Filter "*setup.exe" -File -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

  if (-not $installer) {
    return [ordered]@{
      status = "missing"
      bundle_dir = $bundleDir
      fix = "Run npm run installer:build-smoke."
    }
  }

  $hash = Get-FileHash -Algorithm SHA256 -LiteralPath $installer.FullName
  return [ordered]@{
    status = "found"
    path = $installer.FullName
    relative_path = $installer.FullName.Substring($root.Length).TrimStart("\", "/").Replace("\", "/")
    bytes = $installer.Length
    sha256 = $hash.Hash
    last_write_time = $installer.LastWriteTimeUtc.ToString("o")
  }
}

function Find-AgentMuxInstallEntries {
  $registryGlobs = @(
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*",
    "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*",
    "HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*"
  )

  $entries = @()
  foreach ($glob in $registryGlobs) {
    $items = Get-ItemProperty -Path $glob -ErrorAction SilentlyContinue |
      Where-Object { $_.DisplayName -and ($_.DisplayName -match "AgentMux") }
    foreach ($item in $items) {
      $entries += [ordered]@{
        display_name = $item.DisplayName
        display_version = $item.DisplayVersion
        install_location = $item.InstallLocation
        display_icon = $item.DisplayIcon
        uninstall_string = $item.UninstallString
        registry_source = $glob
      }
    }
  }

  return @($entries)
}

function Normalize-RegistryPath {
  param([string]$Value)

  if ([string]::IsNullOrWhiteSpace($Value)) {
    return $null
  }

  $trimmed = $Value.Trim()
  if ($trimmed.StartsWith('"')) {
    $end = $trimmed.IndexOf('"', 1)
    if ($end -gt 1) {
      return $trimmed.Substring(1, $end - 1)
    }
  }

  $withoutSuffix = $trimmed -replace ",\d+$", ""
  $exeIndex = $withoutSuffix.ToLowerInvariant().IndexOf(".exe")
  if ($exeIndex -ge 0) {
    return $withoutSuffix.Substring(0, $exeIndex + 4)
  }
  return $withoutSuffix
}

function Get-AgentMuxInstallLocations {
  param([object[]]$Entries)

  $installLocations = @()
  foreach ($entry in $Entries) {
    $location = Normalize-RegistryPath $entry.install_location
    if ($location) {
      $installLocations += $location
    }
  }
  if ($env:LOCALAPPDATA) {
    $installLocations += (Join-Path $env:LOCALAPPDATA "AgentMux")
  }

  return @($installLocations | Where-Object { $_ } | Select-Object -Unique)
}

function Normalize-PathSegmentForCompare {
  param([string]$Value)

  if ([string]::IsNullOrWhiteSpace($Value)) {
    return $null
  }

  $expanded = [System.Environment]::ExpandEnvironmentVariables($Value.Trim().Trim('"'))
  try {
    return [System.IO.Path]::GetFullPath($expanded).TrimEnd("\", "/")
  } catch {
    return $expanded.TrimEnd("\", "/")
  }
}

function Get-WindowsUserPathValue {
  try {
    return (Get-ItemProperty -Path "HKCU:\Environment" -Name "Path" -ErrorAction Stop).Path
  } catch {
    return ""
  }
}

function Test-PathListContainsAny {
  param([string]$PathValue, [string[]]$Candidates)

  $normalizedCandidates = @(
    $Candidates |
      ForEach-Object { Normalize-PathSegmentForCompare $_ } |
      Where-Object { $_ } |
      Select-Object -Unique
  )
  if ($normalizedCandidates.Count -eq 0 -or [string]::IsNullOrWhiteSpace($PathValue)) {
    return $false
  }

  $normalizedSegments = @(
    $PathValue -split ";" |
      ForEach-Object { Normalize-PathSegmentForCompare $_ } |
      Where-Object { $_ } |
      Select-Object -Unique
  )

  foreach ($candidate in $normalizedCandidates) {
    if ($normalizedSegments -contains $candidate) {
      return $true
    }
  }
  return $false
}

function Inspect-InstalledCliSidecars {
  param([object[]]$Entries)

  $installLocations = Get-AgentMuxInstallLocations -Entries $Entries
  $userPathValue = Get-WindowsUserPathValue
  $installDirOnUserPath = Test-PathListContainsAny -PathValue $userPathValue -Candidates $installLocations

  $binaries = @()
  foreach ($name in @("agentmux", "cmux")) {
    $candidates = @()
    foreach ($location in $installLocations) {
      $candidates += (Join-Path $location "$name.exe")
    }
    $snapshots = @(
      $candidates |
        Where-Object { $_ } |
        Select-Object -Unique |
        ForEach-Object {
          $exists = Test-Path -LiteralPath $_
          [ordered]@{
            name = $name
            path = $_
            exists = $exists
            bytes = if ($exists) { (Get-Item -LiteralPath $_).Length } else { $null }
          }
        }
    )
    $binaries += [ordered]@{
      name = $name
      present = @($snapshots | Where-Object { $_.exists }).Count -gt 0
      candidates = $snapshots
    }
  }

  $missing = @($binaries | Where-Object { -not $_.present } | ForEach-Object { $_.name })
  return [ordered]@{
    status = if ($missing.Count -eq 0) { "ready" } else { "missing" }
    install_locations = $installLocations
    install_directory_on_user_path = $installDirOnUserPath
    user_path = $userPathValue
    binaries = $binaries
    missing = $missing
  }
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

function Inspect-WslState {
  $wslCommand = Get-Command wsl.exe -ErrorAction SilentlyContinue
  if (-not $wslCommand) {
    return [ordered]@{
      status = "wsl_exe_missing"
      command = $null
      version = $null
      list_verbose = $null
      list_quiet = $null
      distributions = @()
      selected_distribution = $null
      matrix = @(
        [ordered]@{ case = "wsl_exe_missing"; observed = $true; status = "observed" },
        [ordered]@{ case = "no_wsl_distribution"; observed = $false; status = "not_observed" },
        [ordered]@{ case = "wsl_without_tmux"; observed = $false; status = "not_observed" },
        [ordered]@{ case = "wsl_with_tmux"; observed = $false; status = "not_observed" }
      )
    }
  }

  $version = Invoke-ProcessCapture -Name "wsl-version" -FilePath $wslCommand.Source -ArgumentList @("--version") -TimeoutSeconds 20
  $listVerbose = Invoke-ProcessCapture -Name "wsl-list-verbose" -FilePath $wslCommand.Source -ArgumentList @("--list", "--verbose") -TimeoutSeconds 20
  $listQuiet = Invoke-ProcessCapture -Name "wsl-list-quiet" -FilePath $wslCommand.Source -ArgumentList @("--list", "--quiet") -TimeoutSeconds 20
  $distributionNames = @(Split-NonEmptyLines $listQuiet.stdout_text)
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
      tmux_version = ($tmux.stdout_text.Trim())
      tmux_probe = ConvertTo-CaptureSummary $tmux
    }
  }

  $withTmux = @($distributionResults | Where-Object { $_.reachable -and $_.tmux_available })
  $withoutTmux = @($distributionResults | Where-Object { $_.reachable -and -not $_.tmux_available })
  $selected = $Distribution.Trim()
  if ([string]::IsNullOrWhiteSpace($selected)) {
    if ($withTmux.Count -gt 0) {
      $selected = $withTmux[0].name
    } elseif ($distributionResults.Count -gt 0) {
      $selected = $distributionResults[0].name
    } else {
      $selected = $null
    }
  }

  $state =
    if ($distributionResults.Count -eq 0) {
      "no_wsl_distribution"
    } elseif ($withTmux.Count -gt 0) {
      "wsl_with_tmux"
    } elseif ($withoutTmux.Count -gt 0) {
      "wsl_without_tmux"
    } else {
      "wsl_distribution_unreachable"
    }

  return [ordered]@{
    status = $state
    command = $wslCommand.Source
    version = ConvertTo-CaptureSummary $version
    list_verbose = ConvertTo-CaptureSummary $listVerbose
    list_quiet = ConvertTo-CaptureSummary $listQuiet
    distributions = $distributionResults
    selected_distribution = $selected
    matrix = @(
      [ordered]@{ case = "wsl_exe_missing"; observed = $false; status = "not_observed" },
      [ordered]@{ case = "no_wsl_distribution"; observed = ($distributionResults.Count -eq 0); status = if ($distributionResults.Count -eq 0) { "observed" } else { "not_observed" } },
      [ordered]@{ case = "wsl_without_tmux"; observed = ($withoutTmux.Count -gt 0); status = if ($withoutTmux.Count -gt 0) { "observed" } else { "not_observed" } },
      [ordered]@{ case = "wsl_with_tmux"; observed = ($withTmux.Count -gt 0); status = if ($withTmux.Count -gt 0) { "observed" } else { "not_observed" } }
    )
  }
}

function Run-IntegrationDoctor {
  param([string]$CmuxExe, [object]$WslState)

  if ([string]::IsNullOrWhiteSpace($CmuxExe)) {
    return [ordered]@{
      status = "missing_cli"
      executable = $null
      runs = @()
      fix = "Build the CLI with cargo build -p agentmux-cli."
    }
  }

  $runs = @()
  foreach ($kind in $allIntegrationKinds) {
    $local = Invoke-ProcessCapture `
      -Name "integration-doctor-$kind-local" `
      -FilePath $CmuxExe `
      -ArgumentList @("integrations", "doctor", $kind, "--json") `
      -TimeoutSeconds 30
    $localJson = Try-ParseJson $local.stdout_text
    $localStatus = if ($localJson -and $localJson.integrations -and $localJson.integrations.Count -gt 0) {
      $localJson.integrations[0].status
    } elseif ($local.exit_code -eq 0) {
      "unknown"
    } else {
      "command_failed"
    }
    $runs += [ordered]@{
      kind = $kind
      scope = "windows"
      distribution = $null
      status = $localStatus
      command = ConvertTo-CaptureSummary $local
      result = ConvertTo-DoctorResultSummary $localJson
    }
  }

  $selectedDistribution = $WslState.selected_distribution
  if (-not [string]::IsNullOrWhiteSpace($selectedDistribution)) {
    foreach ($kind in $allIntegrationKinds) {
      $wsl = Invoke-ProcessCapture `
        -Name "integration-doctor-$kind-wsl-$selectedDistribution" `
        -FilePath $CmuxExe `
        -ArgumentList @("integrations", "doctor", $kind, "--distribution", $selectedDistribution, "--json") `
        -TimeoutSeconds 45
      $wslJson = Try-ParseJson $wsl.stdout_text
      $wslStatus = if ($wslJson -and $wslJson.integrations -and $wslJson.integrations.Count -gt 0) {
        $wslJson.integrations[0].status
      } elseif ($wsl.exit_code -eq 0) {
        "unknown"
      } else {
        "command_failed"
      }
      $runs += [ordered]@{
        kind = $kind
        scope = "wsl"
        distribution = $selectedDistribution
        status = $wslStatus
        command = ConvertTo-CaptureSummary $wsl
        result = ConvertTo-DoctorResultSummary $wslJson
      }
    }
  }

  $needsAttention = @($runs | Where-Object { $_.status -ne "ready" })
  return [ordered]@{
    status = if ($needsAttention.Count -eq 0) { "ready" } else { "needs_attention" }
    executable = $CmuxExe
    selected_distribution = $selectedDistribution
    runs = $runs
  }
}

$installer = Find-LatestInstaller
$installEntries = @(Find-AgentMuxInstallEntries)
$installedCliSidecars = Inspect-InstalledCliSidecars -Entries $installEntries
$wslState = Inspect-WslState
$cmuxExe = Resolve-CmuxExecutable
$integrationDoctor = Run-IntegrationDoctor -CmuxExe $cmuxExe -WslState $wslState

$manualInstallUninstall = [ordered]@{
  status = "manual_required"
  installed_entries_detected = $installEntries.Count
  checklist = @(
    "Run the generated NSIS setup executable.",
    "Launch AgentMux from the installed shortcut or Start menu entry.",
    "Confirm diagnostics export works from the installed app.",
    "Confirm a native shell pane can be created.",
    "Close AgentMux and uninstall it through Windows Apps settings or the generated uninstaller.",
    "Confirm the installed shortcut or Start menu entry no longer launches AgentMux."
  )
}

$needsAttention = @()
if ($installer.status -ne "found") {
  $needsAttention += "installer artifact is missing"
}
if ($manualInstallUninstall.status -ne "passed") {
  $needsAttention += "manual install/uninstall smoke still requires an intentional human pass"
}
if ($installEntries.Count -gt 0 -and $installedCliSidecars.status -ne "ready") {
  $needsAttention += "installed AgentMux CLI sidecars are missing: " + ($installedCliSidecars.missing -join ", ")
}
if ($installEntries.Count -gt 0 -and -not $installedCliSidecars.install_directory_on_user_path) {
  $needsAttention += "installed AgentMux install directory is not on the Windows user PATH"
}
$unobservedMatrix = @($wslState.matrix | Where-Object { -not $_.observed })
if ($unobservedMatrix.Count -gt 0) {
  $needsAttention += "clean-machine WSL matrix has unobserved states: " + (($unobservedMatrix | ForEach-Object { $_.case }) -join ", ")
}
if ($integrationDoctor.status -ne "ready") {
  $needsAttention += "one or more integration doctor checks need attention"
}

$summary = [ordered]@{
  generated_at = (Get-Date).ToUniversalTime().ToString("o")
  script = "tools/run-release-readiness-audit.ps1"
  machine = if ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { "unknown-machine" }
  output_dir = $OutputDir
  installer = $installer
  installed_app = [ordered]@{
    status = if ($installEntries.Count -gt 0) { "detected" } else { "not_detected" }
    entries = $installEntries
    cli_sidecars = $installedCliSidecars
  }
  wsl = $wslState
  integration_doctor = $integrationDoctor
  manual_install_uninstall = $manualInstallUninstall
  readiness = [ordered]@{
    status = if ($needsAttention.Count -eq 0) { "ready" } else { "needs_attention" }
    needs_attention = $needsAttention
  }
}

Write-JsonFile -Value $summary -Path (Join-Path $OutputDir "summary.json")

$readme = @"
# Release Readiness Audit

Generated: $($summary.generated_at)

This audit is non-mutating. It records installer artifact presence, Windows
install detection, the current WSL/tmux state, and cmux integration doctor
results. It does not install or uninstall AgentMux.

- Readiness: $($summary.readiness.status)
- Installer: $($installer.status)
- Installed app: $($summary.installed_app.status)
- Installed CLI sidecars: $($installedCliSidecars.status)
- Installed directory on user PATH: $($installedCliSidecars.install_directory_on_user_path)
- WSL state: $($wslState.status)
- Integration doctor: $($integrationDoctor.status)

Manual install/uninstall remains a human-controlled release gate.
"@
Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDir "README.md") -Value $readme

if ($FailOnNeedsAttention -and $needsAttention.Count -gt 0) {
  throw ("Release readiness audit needs attention: " + ($needsAttention -join "; "))
}

Write-Host ("Release readiness audit artifacts written to " + $OutputDir)
