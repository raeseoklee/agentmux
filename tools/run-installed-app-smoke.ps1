param(
  [string]$OutputDir = "",
  [switch]$SkipBuild,
  [string]$CliExe = ""
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$localCargoHome = Join-Path $root ".toolchains\cargo"
$localRustupHome = Join-Path $root ".toolchains\rustup"
$localCargo = Join-Path $localCargoHome "bin\cargo.exe"

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $machine = if ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { "unknown-machine" }
  $OutputDir = Join-Path $root "docs\implementation\evidence\$stamp-$machine-installed-app-smoke"
}

$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

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

function Invoke-ProcessCapture {
  param(
    [string]$Name,
    [string]$FilePath,
    [string[]]$ArgumentList = @(),
    [int]$TimeoutSeconds = 60
  )

  $safeName = ($Name -replace "[^A-Za-z0-9_.-]", "-").Trim("-")
  if ([string]::IsNullOrWhiteSpace($safeName)) {
    $safeName = "command"
  }
  $stdoutPath = Join-Path $OutputDir "$safeName.stdout.txt"
  $stderrPath = Join-Path $OutputDir "$safeName.stderr.txt"
  $commandPath = Join-Path $OutputDir "$safeName.cmd"

  $quotedFilePath = ConvertTo-SingleString -Value (ConvertTo-ProcessArgument -Argument $FilePath)
  $quotedRoot = ConvertTo-SingleString -Value (ConvertTo-ProcessArgument -Argument $root)
  $quotedStdout = ConvertTo-SingleString -Value (ConvertTo-ProcessArgument -Argument $stdoutPath)
  $quotedStderr = ConvertTo-SingleString -Value (ConvertTo-ProcessArgument -Argument $stderrPath)
  $quotedCommandPath = ConvertTo-SingleString -Value (ConvertTo-ProcessArgument -Argument $commandPath)
  $commandLine = $quotedFilePath
  if ($ArgumentList.Count -gt 0) {
    $commandLine = $commandLine + " " + (Join-ProcessArguments $ArgumentList)
  }
  $cmdText = @(
    "@echo off",
    "cd /d $quotedRoot",
    "$commandLine > $quotedStdout 2> $quotedStderr",
    "exit /b %ERRORLEVEL%"
  )
  Set-Content -Encoding ASCII -LiteralPath $commandPath -Value $cmdText

  $process = Start-Process `
    -FilePath "cmd.exe" `
    -ArgumentList "/d /c $quotedCommandPath" `
    -WindowStyle Hidden `
    -Wait `
    -PassThru

  $exitCode = $process.ExitCode
  if ($null -eq $exitCode) {
    $exitCode = 0
  }

  return [ordered]@{
    exit_code = $exitCode
    stdout = $stdoutPath
    stderr = $stderrPath
    command = $commandPath
  }
}

function Write-JsonFile {
  param([object]$Value, [string]$Path)

  $Value | ConvertTo-Json -Depth 16 | Set-Content -Encoding UTF8 -LiteralPath $Path
}

function Read-ControlResultJson {
  param([string]$Path)

  $envelope = Get-Content -Raw -LiteralPath $Path | ConvertFrom-Json
  if ($envelope.schema -ne "agentmux.control.v1") {
    throw "unexpected control envelope schema: $($envelope.schema)"
  }
  if ($null -eq $envelope.outcome.Ok) {
    throw "control command returned a non-Ok envelope: $Path"
  }
  $result = $envelope.outcome.Ok.result_json
  if ($result -is [string]) {
    return ($result | ConvertFrom-Json)
  }
  return $result
}

function Invoke-CliJson {
  param(
    [string]$CliExe,
    [string[]]$Arguments,
    [string]$Name,
    [int]$TimeoutSeconds = 60
  )

  $capture = Invoke-ProcessCapture `
    -Name $Name `
    -FilePath $CliExe `
    -ArgumentList $Arguments `
    -TimeoutSeconds $TimeoutSeconds
  if ($capture.exit_code -ne 0) {
    throw "CLI command '$Name' failed with exit code $($capture.exit_code). See $($capture.stderr)"
  }
  return [ordered]@{
    result = Read-ControlResultJson -Path $capture.stdout
    stdout = [System.IO.Path]::GetFileName($capture.stdout)
    stderr = [System.IO.Path]::GetFileName($capture.stderr)
    command = [System.IO.Path]::GetFileName($capture.command)
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

function Resolve-CliExecutable {
  param([string]$RequestedPath, [string]$InstalledDesktopExe)

  if (-not [string]::IsNullOrWhiteSpace($RequestedPath)) {
    if (-not (Test-Path -LiteralPath $RequestedPath)) {
      throw "Requested agentmux CLI executable was not found at $RequestedPath"
    }
    return [System.IO.Path]::GetFullPath($RequestedPath)
  }

  if (-not [string]::IsNullOrWhiteSpace($InstalledDesktopExe)) {
    $installedCli = Join-Path (Split-Path -Parent $InstalledDesktopExe) "agentmux.exe"
    if (Test-Path -LiteralPath $installedCli) {
      return $installedCli
    }
  }

  $debug = Join-Path $root "target\debug\agentmux.exe"
  if (Test-Path -LiteralPath $debug) {
    return $debug
  }
  $release = Join-Path $root "target\release\agentmux.exe"
  if (Test-Path -LiteralPath $release) {
    return $release
  }
  throw "agentmux.exe was not found. Run without -SkipBuild first."
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
  return ($trimmed -replace ",\d+$", "")
}

function Find-AgentMuxInstall {
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

  $candidates = @()
  foreach ($entry in $entries) {
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

  $exe = $candidates | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -First 1
  if (-not $exe) {
    throw "Installed AgentMux executable was not found in registry entries or LOCALAPPDATA."
  }

  return [ordered]@{
    executable = $exe
    entries = $entries
  }
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

$install = Find-AgentMuxInstall
$desktopExe = $install.executable
$cliExe = Resolve-CliExecutable -RequestedPath $CliExe -InstalledDesktopExe $desktopExe

$runtimeDir = Join-Path $OutputDir "runtime"
New-Item -ItemType Directory -Force -Path $runtimeDir | Out-Null
$pipeName = "\\.\pipe\agentmux-installed-smoke-" + [Guid]::NewGuid().ToString("N")
$storePath = Join-Path $runtimeDir "agentmux-installed-smoke.sqlite3"
$tokenPath = Join-Path $runtimeDir "control.token"
$marker = "AGENTMUX_INSTALLED_SMOKE_" + [Guid]::NewGuid().ToString("N")

$previous = @{
  AGENTMUX_STORE_PATH = $env:AGENTMUX_STORE_PATH
  AGENTMUX_CONTROL_TOKEN_PATH = $env:AGENTMUX_CONTROL_TOKEN_PATH
  AGENTMUX_CONTROL_PIPE = $env:AGENTMUX_CONTROL_PIPE
  AGENTMUX_BROWSER_AUTOMATION = $env:AGENTMUX_BROWSER_AUTOMATION
}

$appProcess = $null
try {
  $env:AGENTMUX_STORE_PATH = $storePath
  $env:AGENTMUX_CONTROL_TOKEN_PATH = $tokenPath
  $env:AGENTMUX_CONTROL_PIPE = $pipeName
  $env:AGENTMUX_BROWSER_AUTOMATION = "memory"

  $appProcess = Start-Process `
    -FilePath $desktopExe `
    -WindowStyle Hidden `
    -PassThru

  $commonCliArgs = @("--json", "--pipe", $pipeName, "--token-path", $tokenPath)
  $diagnostics = $null
  $deadline = (Get-Date).AddSeconds(30)
  do {
    if (Test-Path -LiteralPath $tokenPath) {
      try {
        $diagnostics = Invoke-CliJson `
          -CliExe $cliExe `
          -Arguments (@("diagnostics", "export") + $commonCliArgs) `
          -Name "diagnostics-export"
        break
      } catch {
        Start-Sleep -Milliseconds 500
      }
    } else {
      Start-Sleep -Milliseconds 500
    }
  } while ((Get-Date) -lt $deadline)

  if ($null -eq $diagnostics) {
    throw "Installed app diagnostics export did not succeed before timeout."
  }

  if ($diagnostics.result.format_version -ne "agentmux.diagnostics.v1") {
    throw "unexpected diagnostics format_version: $($diagnostics.result.format_version)"
  }

  $workspace = Invoke-CliJson `
    -CliExe $cliExe `
    -Arguments (@("workspace", "create", "InstalledSmoke") + $commonCliArgs) `
    -Name "workspace-create"
  $workspaceId = $workspace.result.workspace_id
  if ([string]::IsNullOrWhiteSpace($workspaceId)) {
    throw "Installed app workspace create did not return a workspace id."
  }

  $spawn = Invoke-CliJson `
    -CliExe $cliExe `
    -Arguments (@(
      "session", "spawn",
      "--workspace", $workspaceId,
      "--backend", "conpty",
      "--cwd", $root,
      "--durability", "ephemeral"
    ) + $commonCliArgs + @("--", "cmd.exe", "/d", "/q", "/c", "echo $marker")) `
    -Name "session-spawn"
  $sessionId = $spawn.result.session_id
  if ([string]::IsNullOrWhiteSpace($sessionId)) {
    throw "Installed app session spawn did not return a session id."
  }

  $recent = $null
  $deadline = (Get-Date).AddSeconds(15)
  do {
    $recent = Invoke-CliJson `
      -CliExe $cliExe `
      -Arguments (@("session", "read-recent", $sessionId, "--max-bytes", "4096") + $commonCliArgs) `
      -Name "session-read-recent"
    if (($recent.result.text -as [string]) -match [regex]::Escape($marker)) {
      break
    }
    Start-Sleep -Milliseconds 500
  } while ((Get-Date) -lt $deadline)

  if (($recent.result.text -as [string]) -notmatch [regex]::Escape($marker)) {
    throw "Installed app ConPTY session output did not include smoke marker '$marker'."
  }

  $summary = [ordered]@{
    generated_at = (Get-Date).ToUniversalTime().ToString("o")
    script = "tools/run-installed-app-smoke.ps1"
    installed_executable = $desktopExe
    installed_entries = $install.entries
    cli_executable = $cliExe
    pipe_name = $pipeName
    store_path = $storePath
    token_path = $tokenPath
    app_process_id = $appProcess.Id
    diagnostics_format_version = $diagnostics.result.format_version
    workspace_id = $workspaceId
    session_id = $sessionId
    output_marker = $marker
    result = "passed"
    diagnostics_stdout = $diagnostics.stdout
    workspace_create_stdout = $workspace.stdout
    session_spawn_stdout = $spawn.stdout
    session_read_recent_stdout = $recent.stdout
  }
  Write-JsonFile -Value $summary -Path (Join-Path $OutputDir "summary.json")

  $readme = @"
# Installed App Smoke

Generated: $($summary.generated_at)

This smoke launches the installed AgentMux desktop executable with isolated
store, token, and control pipe paths, then verifies diagnostics export,
workspace creation, native ConPTY session spawn, and terminal output capture.

- Result: passed
- Installed executable: $desktopExe
- Workspace: $workspaceId
- Session: $sessionId
- Output marker: $marker
"@
  Set-Content -Encoding UTF8 -LiteralPath (Join-Path $OutputDir "README.md") -Value $readme
} finally {
  if ($appProcess -and -not $appProcess.HasExited) {
    Stop-Process -Id $appProcess.Id -Force
    $appProcess.WaitForExit()
  }
  foreach ($name in $previous.Keys) {
    if ($null -eq $previous[$name]) {
      Remove-Item -Path "env:$name" -ErrorAction SilentlyContinue
    } else {
      Set-Item -Path "env:$name" -Value $previous[$name]
    }
  }
}

Write-Host ("Installed app smoke artifacts written to " + $OutputDir)
