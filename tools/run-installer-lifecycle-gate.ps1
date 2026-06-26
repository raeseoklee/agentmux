param(
  [Parameter(Position = 0)]
  [ValidateSet("audit", "preinstall", "installed", "uninstalled")]
  [string]$ExpectedPhase = "audit",

  [Parameter(Position = 1)]
  [string]$OutputDir = "",

  [string]$InstallerPath = "",

  [switch]$RequireCli,

  [switch]$RequireUserPath
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $machine = if ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { "unknown-machine" }
  $OutputDir = Join-Path $root "docs\implementation\evidence\$stamp-$machine-installer-lifecycle-gate"
}

$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function Write-JsonFile {
  param([object]$Value, [string]$Path)

  $Value | ConvertTo-Json -Depth 16 | Set-Content -Encoding UTF8 -LiteralPath $Path
}

function ConvertTo-RelativePath {
  param([string]$Path)

  if ([string]::IsNullOrWhiteSpace($Path)) {
    return $null
  }

  $full = [System.IO.Path]::GetFullPath($Path)
  if ($full.StartsWith($root, [System.StringComparison]::OrdinalIgnoreCase)) {
    return $full.Substring($root.Length).TrimStart("\", "/").Replace("\", "/")
  }
  return $full
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

function Get-FileSnapshot {
  param([string]$Path)

  if ([string]::IsNullOrWhiteSpace($Path)) {
    return $null
  }

  $full = [System.IO.Path]::GetFullPath($Path)
  if (-not (Test-Path -LiteralPath $full)) {
    return [ordered]@{
      path = $full
      relative_path = ConvertTo-RelativePath $full
      exists = $false
    }
  }

  $item = Get-Item -LiteralPath $full
  $hash = Get-FileHash -Algorithm SHA256 -LiteralPath $full
  return [ordered]@{
    path = $full
    relative_path = ConvertTo-RelativePath $full
    exists = $true
    bytes = $item.Length
    sha256 = $hash.Hash
    last_write_time = $item.LastWriteTimeUtc.ToString("o")
  }
}

function Find-LatestInstaller {
  if (-not [string]::IsNullOrWhiteSpace($InstallerPath)) {
    if (-not (Test-Path -LiteralPath $InstallerPath)) {
      return [ordered]@{
        status = "missing"
        requested_path = $InstallerPath
        fix = "Pass an existing AgentMux setup executable with -InstallerPath."
      }
    }
    $snapshot = Get-FileSnapshot $InstallerPath
    return [ordered]@{
      status = "found"
      source = "parameter"
      file = $snapshot
    }
  }

  $candidates = @()
  $bundleDir = Join-Path $root "target\release\bundle\nsis"
  if (Test-Path -LiteralPath $bundleDir) {
    $candidates += Get-ChildItem -LiteralPath $bundleDir -Filter "*setup.exe" -File -ErrorAction SilentlyContinue
  }

  $evidenceDir = Join-Path $root "docs\implementation\evidence"
  if (Test-Path -LiteralPath $evidenceDir) {
    $candidates += Get-ChildItem -LiteralPath $evidenceDir -Recurse -Filter "*setup.exe" -File -ErrorAction SilentlyContinue |
      Where-Object { $_.DirectoryName -match "installer-build-smoke" }
  }

  $installer = $candidates | Sort-Object LastWriteTime -Descending | Select-Object -First 1
  if (-not $installer) {
    return [ordered]@{
      status = "missing"
      searched = @(
        ConvertTo-RelativePath $bundleDir,
        "docs/implementation/evidence/*-installer-build-smoke/*.exe"
      )
      fix = "Run npm run installer:build-smoke."
    }
  }

  return [ordered]@{
    status = "found"
    source = "latest"
    file = Get-FileSnapshot $installer.FullName
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
        publisher = $item.Publisher
        install_location = $item.InstallLocation
        display_icon = $item.DisplayIcon
        uninstall_string = $item.UninstallString
        quiet_uninstall_string = $item.QuietUninstallString
        registry_source = $glob
      }
    }
  }

  return @($entries)
}

function Find-AgentMuxExecutableCandidates {
  param([object[]]$Entries)

  $candidates = @()
  foreach ($entry in $Entries) {
    $icon = Normalize-RegistryPath $entry.display_icon
    if ($icon) {
      $candidates += $icon
    }

    $location = Normalize-RegistryPath $entry.install_location
    if ($location) {
      $candidates += (Join-Path $location "agentmux-desktop-host.exe")
    }
  }

  if ($env:LOCALAPPDATA) {
    $candidates += (Join-Path $env:LOCALAPPDATA "AgentMux\agentmux-desktop-host.exe")
  }

  return @(
    $candidates |
      Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
      Select-Object -Unique |
      ForEach-Object { Get-FileSnapshot $_ }
  )
}

function Find-AgentMuxCliCandidates {
  param([object[]]$Entries, [string]$Name)

  $candidates = @()
  foreach ($entry in $Entries) {
    $location = Normalize-RegistryPath $entry.install_location
    if ($location) {
      $candidates += (Join-Path $location "$Name.exe")
    }
  }

  if ($env:LOCALAPPDATA) {
    $candidates += (Join-Path $env:LOCALAPPDATA "AgentMux\$Name.exe")
  }

  return @(
    $candidates |
      Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
      Select-Object -Unique |
      ForEach-Object { Get-FileSnapshot $_ }
  )
}

function Get-AgentMuxInstallLocations {
  param([object[]]$Entries)

  $locations = @()
  foreach ($entry in $Entries) {
    $location = Normalize-RegistryPath $entry.install_location
    if ($location) {
      $locations += $location
    }
  }
  if ($env:LOCALAPPDATA) {
    $locations += (Join-Path $env:LOCALAPPDATA "AgentMux")
  }

  return @($locations | Where-Object { $_ } | Select-Object -Unique)
}

function Normalize-PathSegmentForCompare {
  param([string]$Value)

  if ([string]::IsNullOrWhiteSpace($Value)) {
    return ""
  }

  $expanded = [Environment]::ExpandEnvironmentVariables($Value.Trim().Trim('"'))
  $normalized = $expanded.Replace("/", "\")
  while ($normalized.EndsWith("\") -and $normalized.Length -gt 3) {
    $normalized = $normalized.Substring(0, $normalized.Length - 1)
  }
  return $normalized.ToLowerInvariant()
}

function Get-WindowsUserPathValue {
  try {
    $item = Get-ItemProperty -Path "HKCU:\Environment" -Name "Path" -ErrorAction Stop
    return ($item.Path -as [string])
  } catch {
    return ""
  }
}

function Test-PathListContainsAny {
  param([string]$PathValue, [string[]]$Candidates)

  $candidateSet = @{}
  foreach ($candidate in $Candidates) {
    $normalized = Normalize-PathSegmentForCompare $candidate
    if (-not [string]::IsNullOrWhiteSpace($normalized)) {
      $candidateSet[$normalized] = $true
    }
  }
  if ($candidateSet.Count -eq 0) {
    return $false
  }

  foreach ($segment in (($PathValue -as [string]) -split ";")) {
    $normalized = Normalize-PathSegmentForCompare $segment
    if ($candidateSet.ContainsKey($normalized)) {
      return $true
    }
  }
  return $false
}

function Find-AgentMuxShortcuts {
  $roots = @()
  if ($env:APPDATA) {
    $roots += (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs")
  }
  if ($env:ProgramData) {
    $roots += (Join-Path $env:ProgramData "Microsoft\Windows\Start Menu\Programs")
  }
  if ($env:USERPROFILE) {
    $roots += (Join-Path $env:USERPROFILE "Desktop")
  }
  if ($env:PUBLIC) {
    $roots += (Join-Path $env:PUBLIC "Desktop")
  }

  $shortcuts = @()
  foreach ($shortcutRoot in ($roots | Select-Object -Unique)) {
    if (-not (Test-Path -LiteralPath $shortcutRoot)) {
      continue
    }

    $shortcuts += Get-ChildItem -LiteralPath $shortcutRoot -Recurse -Filter "*AgentMux*.lnk" -File -ErrorAction SilentlyContinue |
      ForEach-Object {
        [ordered]@{
          path = $_.FullName
          relative_path = ConvertTo-RelativePath $_.FullName
          bytes = $_.Length
          last_write_time = $_.LastWriteTimeUtc.ToString("o")
        }
      }
  }

  return @($shortcuts)
}

$installer = Find-LatestInstaller
$entries = @(Find-AgentMuxInstallEntries)
$installLocations = @(Get-AgentMuxInstallLocations -Entries $entries)
$executables = @(Find-AgentMuxExecutableCandidates -Entries $entries)
$existingExecutables = @($executables | Where-Object { $_.exists })
$agentmuxCliCandidates = @(Find-AgentMuxCliCandidates -Entries $entries -Name "agentmux")
$cmuxCliCandidates = @(Find-AgentMuxCliCandidates -Entries $entries -Name "cmux")
$existingAgentmuxCli = @($agentmuxCliCandidates | Where-Object { $_.exists })
$existingCmuxCli = @($cmuxCliCandidates | Where-Object { $_.exists })
$shortcuts = @(Find-AgentMuxShortcuts)
$userPathValue = Get-WindowsUserPathValue
$installDirOnUserPath = Test-PathListContainsAny -PathValue $userPathValue -Candidates $installLocations
$hasUninstallString = @($entries | Where-Object {
    -not [string]::IsNullOrWhiteSpace($_.uninstall_string) -or
    -not [string]::IsNullOrWhiteSpace($_.quiet_uninstall_string)
  }).Count -gt 0

$phaseChecks = [ordered]@{
  installer_found = ($installer.status -eq "found")
  registry_entry_present = ($entries.Count -gt 0)
  installed_executable_present = ($existingExecutables.Count -gt 0)
  installed_agentmux_cli_present = ($existingAgentmuxCli.Count -gt 0)
  installed_cmux_cli_present = ($existingCmuxCli.Count -gt 0)
  install_directory_on_user_path = $installDirOnUserPath
  uninstall_command_present = $hasUninstallString
  shortcuts_present = ($shortcuts.Count -gt 0)
}

$passed = $true
$requirements = @()
switch ($ExpectedPhase) {
  "audit" {
    $passed = $true
    $requirements = @("record current installer and Windows install state")
  }
  "preinstall" {
    $requirements = @(
      "installer artifact is present",
      "AgentMux registry entry is absent",
      "installed AgentMux executable is absent",
      "installed agentmux.exe is absent",
      "installed cmux.exe is absent",
      "AgentMux install directory is absent from the user PATH"
    )
    $passed =
      $phaseChecks.installer_found -and
      (-not $phaseChecks.registry_entry_present) -and
      (-not $phaseChecks.installed_executable_present) -and
      (-not $phaseChecks.installed_agentmux_cli_present) -and
      (-not $phaseChecks.installed_cmux_cli_present) -and
      (-not $phaseChecks.install_directory_on_user_path)
  }
  "installed" {
    $requirements = @(
      "installer artifact is present",
      "AgentMux registry entry is present",
      "installed AgentMux executable is present",
      "uninstall command is present"
    )
    if ($RequireCli) {
      $requirements += @(
        "installed agentmux.exe CLI is present",
        "installed cmux.exe CLI is present"
      )
    }
    if ($RequireUserPath) {
      $requirements += "AgentMux install directory is present on the user PATH"
    }
    $passed =
      $phaseChecks.installer_found -and
      $phaseChecks.registry_entry_present -and
      $phaseChecks.installed_executable_present -and
      $phaseChecks.uninstall_command_present
    if ($RequireCli) {
      $passed =
        $passed -and
        $phaseChecks.installed_agentmux_cli_present -and
        $phaseChecks.installed_cmux_cli_present
    }
    if ($RequireUserPath) {
      $passed = $passed -and $phaseChecks.install_directory_on_user_path
    }
  }
  "uninstalled" {
    $requirements = @(
      "installer artifact is present for traceability",
      "AgentMux registry entry is absent",
      "installed AgentMux executable is absent",
      "installed agentmux.exe is absent",
      "installed cmux.exe is absent",
      "AgentMux install directory is absent from the user PATH"
    )
    $passed =
      $phaseChecks.installer_found -and
      (-not $phaseChecks.registry_entry_present) -and
      (-not $phaseChecks.installed_executable_present) -and
      (-not $phaseChecks.installed_agentmux_cli_present) -and
      (-not $phaseChecks.installed_cmux_cli_present) -and
      (-not $phaseChecks.install_directory_on_user_path)
  }
}

$summary = [ordered]@{
  generated_at = (Get-Date).ToUniversalTime().ToString("o")
  script = "tools/run-installer-lifecycle-gate.ps1"
  expected_phase = $ExpectedPhase
  require_cli = [bool]$RequireCli
  require_user_path = [bool]$RequireUserPath
  result = if ($passed) { "passed" } else { "needs_attention" }
  requirements = $requirements
  phase_checks = $phaseChecks
  installer = $installer
  installed_state = [ordered]@{
    registry_entries = $entries
    install_locations = $installLocations
    user_path = [ordered]@{
      value = $userPathValue
      install_directory_on_user_path = $installDirOnUserPath
    }
    executable_candidates = $executables
    agentmux_cli_candidates = $agentmuxCliCandidates
    cmux_cli_candidates = $cmuxCliCandidates
    shortcuts = $shortcuts
  }
}
Write-JsonFile -Value $summary -Path (Join-Path $OutputDir "summary.json")

$readme = @"
# Installer Lifecycle Gate

Generated: $($summary.generated_at)

This gate is non-mutating. It records the current Windows installer lifecycle
state so manual install/uninstall passes can leave auditable evidence.

- Expected phase: $ExpectedPhase
- Require CLI: $([bool]$RequireCli)
- Require user PATH: $([bool]$RequireUserPath)
- Result: $($summary.result)
- Installer found: $($phaseChecks.installer_found)
- Registry entry present: $($phaseChecks.registry_entry_present)
- Installed executable present: $($phaseChecks.installed_executable_present)
- Installed agentmux.exe present: $($phaseChecks.installed_agentmux_cli_present)
- Installed cmux.exe present: $($phaseChecks.installed_cmux_cli_present)
- Install directory on user PATH: $($phaseChecks.install_directory_on_user_path)
- Uninstall command present: $($phaseChecks.uninstall_command_present)
- Shortcuts present: $($phaseChecks.shortcuts_present)
"@
Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDir "README.md") -Value $readme

if (-not $passed) {
  throw "Installer lifecycle gate expected '$ExpectedPhase' but observed checks that need attention. See $(Join-Path $OutputDir "summary.json")"
}

Write-Host ("Installer lifecycle gate artifacts written to " + $OutputDir)
