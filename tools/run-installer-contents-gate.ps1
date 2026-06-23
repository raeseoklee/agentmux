param(
  [Parameter(Position = 0)]
  [string]$OutputDir = "",

  [string]$InstallerPath = ""
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $machine = if ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { "unknown-machine" }
  $OutputDir = Join-Path $root "docs\implementation\evidence\$stamp-$machine-installer-contents-gate"
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
    return Get-FileSnapshot $InstallerPath
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
      path = $bundleDir
      relative_path = ConvertTo-RelativePath $bundleDir
      exists = $false
    }
  }
  return Get-FileSnapshot $installer.FullName
}

function Invoke-Capture {
  param(
    [string]$Name,
    [string]$FilePath,
    [string[]]$ArgumentList,
    [string]$WorkingDirectory = $root
  )

  $safeName = ($Name -replace "[^A-Za-z0-9_.-]", "-").Trim("-")
  if ([string]::IsNullOrWhiteSpace($safeName)) {
    $safeName = "command"
  }

  $stdoutPath = Join-Path $OutputDir "$safeName.stdout.txt"
  $stderrPath = Join-Path $OutputDir "$safeName.stderr.txt"
  $process = Start-Process `
    -FilePath $FilePath `
    -ArgumentList $ArgumentList `
    -WorkingDirectory $WorkingDirectory `
    -RedirectStandardOutput $stdoutPath `
    -RedirectStandardError $stderrPath `
    -WindowStyle Hidden `
    -Wait `
    -PassThru

  return [ordered]@{
    exit_code = $process.ExitCode
    stdout = [System.IO.Path]::GetFileName($stdoutPath)
    stderr = [System.IO.Path]::GetFileName($stderrPath)
    stdout_path = $stdoutPath
    stderr_path = $stderrPath
  }
}

$installer = Find-LatestInstaller
$sevenZip = Get-Command 7z.exe -ErrorAction SilentlyContinue
if (-not $sevenZip) {
  $sevenZip = Get-Command 7z -ErrorAction SilentlyContinue
}
if (-not $sevenZip) {
  throw "7z was not found on PATH. Install 7-Zip or Scoop's 7zip package to inspect NSIS installer contents."
}
if (-not $installer.exists) {
  throw "Installer setup executable was not found. Run npm run installer:build-smoke first."
}

$installerScriptPath = Join-Path $root "target\release\nsis\x64\installer.nsi"
$installerScript = Get-FileSnapshot $installerScriptPath
$scriptText = if ($installerScript.exists) { Get-Content -Raw -LiteralPath $installerScriptPath } else { "" }
$hookPath = Join-Path $root "apps\desktop\src-tauri\nsis-hooks.nsh"
$hookScript = Get-FileSnapshot $hookPath
$hookText = if ($hookScript.exists) { Get-Content -Raw -LiteralPath $hookPath } else { "" }
$scriptChecks = [ordered]@{
  script_exists = $installerScript.exists
  agentmux_oname_present = ($scriptText -match '/oname=agentmux\.exe')
  cmux_oname_present = ($scriptText -match '/oname=cmux\.exe')
  hook_file_exists = $hookScript.exists
  hook_included = ($scriptText -match [regex]::Escape($hookPath))
  postinstall_hook_call_present = ($scriptText -match 'NSIS_HOOK_POSTINSTALL')
  preuninstall_hook_call_present = ($scriptText -match 'NSIS_HOOK_PREUNINSTALL')
  path_write_present = ($hookText -match 'WriteRegExpandStr\s+HKCU\s+"Environment"\s+"Path"')
  user_path_read_present = ($hookText -match 'ReadRegStr\s+\$0\s+HKCU\s+"Environment"\s+"Path"')
  environment_broadcast_present = ($hookText -match 'SendMessage\s+0xFFFF\s+0x001A\s+0\s+"STR:Environment"')
  add_path_macro_present = ($hookText -match 'AGENTMUX_ADD_INSTALL_DIR_TO_USER_PATH')
  remove_path_macro_present = ($hookText -match 'AGENTMUX_REMOVE_INSTALL_DIR_FROM_USER_PATH')
}

$listing = Invoke-Capture `
  -Name "installer-listing" `
  -FilePath $sevenZip.Source `
  -ArgumentList @("l", "-slt", $installer.path)
if ($listing.exit_code -ne 0) {
  throw "7z listing failed with exit code $($listing.exit_code). See $(Join-Path $OutputDir $listing.stderr)"
}
$listingText = Get-Content -Raw -LiteralPath $listing.stdout_path
$archivePaths = @(
  [regex]::Matches($listingText, "(?m)^Path = (.+)$") |
    ForEach-Object { $_.Groups[1].Value.Trim() } |
    Where-Object { $_ -and $_ -notmatch "setup\.exe$" }
)

$runtimeDir = Join-Path $OutputDir "runtime\installer-extract"
New-Item -ItemType Directory -Force -Path $runtimeDir | Out-Null
$extract = Invoke-Capture `
  -Name "installer-extract-sidecars" `
  -FilePath $sevenZip.Source `
  -ArgumentList @(
    "x",
    "-y",
    "-o$runtimeDir",
    $installer.path,
    "agentmux-desktop-host.exe",
    "agentmux.exe",
    "cmux.exe"
  )
if ($extract.exit_code -ne 0) {
  throw "7z extraction failed with exit code $($extract.exit_code). See $(Join-Path $OutputDir $extract.stderr)"
}

$expectedSidecars = [ordered]@{
  "agentmux.exe" = Join-Path $root "apps\desktop\src-tauri\binaries\agentmux-x86_64-pc-windows-msvc.exe"
  "cmux.exe" = Join-Path $root "apps\desktop\src-tauri\binaries\cmux-x86_64-pc-windows-msvc.exe"
}

$fileChecks = @()
foreach ($name in @("agentmux-desktop-host.exe", "agentmux.exe", "cmux.exe")) {
  $archivePresent = $archivePaths -contains $name
  $extracted = Get-FileSnapshot (Join-Path $runtimeDir $name)
  $source = if ($expectedSidecars.Contains($name)) {
    Get-FileSnapshot $expectedSidecars[$name]
  } else {
    $null
  }
  $hashMatchesSource =
    if ($source -and $source.exists -and $extracted.exists) {
      $source.sha256 -eq $extracted.sha256
    } elseif ($source) {
      $false
    } else {
      $null
    }

  $fileChecks += [ordered]@{
    name = $name
    archive_present = $archivePresent
    extracted = $extracted
    expected_source = $source
    hash_matches_source = $hashMatchesSource
  }
}

$failedChecks = @()
if (-not $scriptChecks.script_exists) {
  $failedChecks += "generated NSIS installer script is missing"
}
if (-not $scriptChecks.agentmux_oname_present) {
  $failedChecks += "generated NSIS script does not install agentmux.exe"
}
if (-not $scriptChecks.cmux_oname_present) {
  $failedChecks += "generated NSIS script does not install cmux.exe"
}
if (-not $scriptChecks.hook_file_exists) {
  $failedChecks += "NSIS hook source file is missing"
}
if (-not $scriptChecks.hook_included) {
  $failedChecks += "generated NSIS script does not include the AgentMux hook file"
}
if (-not $scriptChecks.postinstall_hook_call_present) {
  $failedChecks += "generated NSIS script does not call the postinstall hook"
}
if (-not $scriptChecks.preuninstall_hook_call_present) {
  $failedChecks += "generated NSIS script does not call the preuninstall hook"
}
if (-not $scriptChecks.path_write_present) {
  $failedChecks += "NSIS hook does not write the user PATH"
}
if (-not $scriptChecks.user_path_read_present) {
  $failedChecks += "NSIS hook does not read the user PATH"
}
if (-not $scriptChecks.environment_broadcast_present) {
  $failedChecks += "NSIS hook does not broadcast Environment changes"
}
if (-not $scriptChecks.add_path_macro_present) {
  $failedChecks += "NSIS hook does not define the add-to-PATH macro"
}
if (-not $scriptChecks.remove_path_macro_present) {
  $failedChecks += "NSIS hook does not define the remove-from-PATH macro"
}
foreach ($check in $fileChecks) {
  if (-not $check.archive_present) {
    $failedChecks += "installer archive does not list $($check.name)"
  }
  if (-not $check.extracted.exists) {
    $failedChecks += "installer archive could not extract $($check.name)"
  }
  if ($check.hash_matches_source -eq $false) {
    $failedChecks += "extracted $($check.name) does not match the prepared sidecar source"
  }
}

$passed = $failedChecks.Count -eq 0
$summary = [ordered]@{
  generated_at = (Get-Date).ToUniversalTime().ToString("o")
  script = "tools/run-installer-contents-gate.ps1"
  result = if ($passed) { "passed" } else { "failed" }
  installer = $installer
  seven_zip = $sevenZip.Source
  generated_installer_script = $installerScript
  nsis_hook_script = $hookScript
  script_checks = $scriptChecks
  archive_paths = $archivePaths
  file_checks = $fileChecks
  failed_checks = $failedChecks
  listing_stdout = $listing.stdout
  listing_stderr = $listing.stderr
  extract_stdout = $extract.stdout
  extract_stderr = $extract.stderr
}
Write-JsonFile -Value $summary -Path (Join-Path $OutputDir "summary.json")

$readme = @"
# Installer Contents Gate

Generated: $($summary.generated_at)

This non-installing gate opens the NSIS setup executable with 7-Zip, verifies
that the installer script installs the CLI sidecars, extracts installer contents
to an ignored runtime directory, and compares extracted CLI sidecar hashes with
the prepared Tauri sidecar inputs.

- Result: $($summary.result)
- Installer: $($installer.relative_path)
- agentmux.exe in archive: $((@($fileChecks | Where-Object { $_.name -eq "agentmux.exe" })[0]).archive_present)
- cmux.exe in archive: $((@($fileChecks | Where-Object { $_.name -eq "cmux.exe" })[0]).archive_present)
- PATH hook included: $($scriptChecks.hook_included)
- PATH hook writes user PATH: $($scriptChecks.path_write_present)
"@
Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDir "README.md") -Value $readme

if (-not $passed) {
  throw "Installer contents gate failed: $($failedChecks -join '; ')"
}

Write-Host ("Installer contents gate artifacts written to " + $OutputDir)
