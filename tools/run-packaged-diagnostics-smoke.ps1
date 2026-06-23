param(
  [string]$OutputDir = "",
  [switch]$SkipBuild
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

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $machine = if ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { "unknown-machine" }
  $OutputDir = Join-Path $root "docs\implementation\evidence\$stamp-$machine-packaged-diagnostics-smoke"
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
    [string]$StdoutPath,
    [string]$StderrPath
  )

  $exit = Invoke-ProcessCapture `
    -FilePath $CliExe `
    -ArgumentList $Arguments `
    -StdoutPath $StdoutPath `
    -StderrPath $StderrPath
  if ($exit -ne 0) {
    throw "CLI command failed with exit code $exit. See $StderrPath"
  }
  return Read-ControlResultJson -Path $StdoutPath
}

function Resolve-DesktopExecutable {
  $release = Join-Path $root "target\release\agentmux-desktop-host.exe"
  $debug = Join-Path $root "target\debug\agentmux-desktop-host.exe"
  if (Test-Path $debug) {
    return $debug
  }
  if (Test-Path $release) {
    return $release
  }
  throw "agentmux-desktop-host.exe was not found. Run without -SkipBuild first."
}

function Resolve-CliExecutable {
  $debug = Join-Path $root "target\debug\agentmux.exe"
  if (Test-Path $debug) {
    return $debug
  }
  throw "agentmux.exe was not found. Run without -SkipBuild first."
}

Push-Location $root
try {
  if (-not $SkipBuild) {
    $tauriStdout = Join-Path $OutputDir "tauri-build.stdout.txt"
    $tauriStderr = Join-Path $OutputDir "tauri-build.stderr.txt"
    $tauriExit = Invoke-ProcessCapture `
      -FilePath "cmd.exe" `
      -ArgumentList @("/d", "/c", "npm --prefix apps/desktop run tauri:build:debug") `
      -StdoutPath $tauriStdout `
      -StderrPath $tauriStderr
    if ($tauriExit -ne 0) {
      throw "Tauri debug build failed with exit code $tauriExit. See $tauriStderr"
    }

    $cliBuildStdout = Join-Path $OutputDir "cli-build.stdout.txt"
    $cliBuildStderr = Join-Path $OutputDir "cli-build.stderr.txt"
    $cliExit = Invoke-ProcessCapture `
      -FilePath $cargoPath `
      -ArgumentList @("build", "-p", "agentmux-cli") `
      -StdoutPath $cliBuildStdout `
      -StderrPath $cliBuildStderr
    if ($cliExit -ne 0) {
      throw "CLI build failed with exit code $cliExit. See $cliBuildStderr"
    }
  }

  $desktopExe = Resolve-DesktopExecutable
  $cliExe = Resolve-CliExecutable
  $runtimeDir = Join-Path $OutputDir "runtime"
  New-Item -ItemType Directory -Force -Path $runtimeDir | Out-Null

  $pipeName = "\\.\pipe\agentmux-control-smoke-" + [Guid]::NewGuid().ToString("N")
  $storePath = Join-Path $runtimeDir "agentmux-smoke.sqlite3"
  $tokenPath = Join-Path $runtimeDir "control.token"

  $previous = @{
    AGENTMUX_STORE_PATH = $env:AGENTMUX_STORE_PATH
    AGENTMUX_CONTROL_TOKEN_PATH = $env:AGENTMUX_CONTROL_TOKEN_PATH
    AGENTMUX_CONTROL_PIPE = $env:AGENTMUX_CONTROL_PIPE
    AGENTMUX_BROWSER_AUTOMATION = $env:AGENTMUX_BROWSER_AUTOMATION
  }

  $env:AGENTMUX_STORE_PATH = $storePath
  $env:AGENTMUX_CONTROL_TOKEN_PATH = $tokenPath
  $env:AGENTMUX_CONTROL_PIPE = $pipeName
  $env:AGENTMUX_BROWSER_AUTOMATION = "memory"

  $appProcess = $null
  try {
    $appProcess = Start-Process `
      -FilePath $desktopExe `
      -WindowStyle Hidden `
      -PassThru

    $diagnosticsStdout = Join-Path $OutputDir "diagnostics-export.json"
    $diagnosticsStderr = Join-Path $OutputDir "diagnostics-export.stderr.txt"
    $workspaceAStdout = Join-Path $OutputDir "workspace-alpha-create.json"
    $workspaceAStderr = Join-Path $OutputDir "workspace-alpha-create.stderr.txt"
    $workspaceBStdout = Join-Path $OutputDir "workspace-beta-create.json"
    $workspaceBStderr = Join-Path $OutputDir "workspace-beta-create.stderr.txt"
    $groupAStdout = Join-Path $OutputDir "workspace-group-alpha-create.json"
    $groupAStderr = Join-Path $OutputDir "workspace-group-alpha-create.stderr.txt"
    $groupBStdout = Join-Path $OutputDir "workspace-group-beta-create.json"
    $groupBStderr = Join-Path $OutputDir "workspace-group-beta-create.stderr.txt"
    $groupAddBStdout = Join-Path $OutputDir "workspace-group-alpha-add-beta.json"
    $groupAddBStderr = Join-Path $OutputDir "workspace-group-alpha-add-beta.stderr.txt"
    $groupAddAStdout = Join-Path $OutputDir "workspace-group-alpha-add-alpha.json"
    $groupAddAStderr = Join-Path $OutputDir "workspace-group-alpha-add-alpha.stderr.txt"
    $groupAUpdateStdout = Join-Path $OutputDir "workspace-group-alpha-update.json"
    $groupAUpdateStderr = Join-Path $OutputDir "workspace-group-alpha-update.stderr.txt"
    $groupBUpdateStdout = Join-Path $OutputDir "workspace-group-beta-update.json"
    $groupBUpdateStderr = Join-Path $OutputDir "workspace-group-beta-update.stderr.txt"
    $groupListBeforeRestartStdout = Join-Path $OutputDir "workspace-groups-before-restart.json"
    $groupListBeforeRestartStderr = Join-Path $OutputDir "workspace-groups-before-restart.stderr.txt"
    $groupListAfterRestartStdout = Join-Path $OutputDir "workspace-groups-after-restart.json"
    $groupListAfterRestartStderr = Join-Path $OutputDir "workspace-groups-after-restart.stderr.txt"
    $deadline = (Get-Date).AddSeconds(30)
    $lastExit = $null

    do {
      if (Test-Path $tokenPath) {
        $lastExit = Invoke-ProcessCapture `
          -FilePath $cliExe `
          -ArgumentList @("diagnostics", "export", "--json", "--pipe", $pipeName, "--token-path", $tokenPath) `
          -StdoutPath $diagnosticsStdout `
          -StderrPath $diagnosticsStderr
        if ($lastExit -eq 0) {
          break
        }
      }
      Start-Sleep -Milliseconds 500
    } while ((Get-Date) -lt $deadline)

    if ($lastExit -ne 0) {
      throw "diagnostics export did not succeed before timeout. Last exit code: $lastExit. See $diagnosticsStderr"
    }

    $envelope = Get-Content -Raw -LiteralPath $diagnosticsStdout | ConvertFrom-Json
    if ($envelope.schema -ne "agentmux.control.v1") {
      throw "unexpected diagnostics envelope schema: $($envelope.schema)"
    }
    if ($null -eq $envelope.outcome.Ok) {
      throw "diagnostics export returned a non-Ok envelope"
    }
    $diagnostics = $envelope.outcome.Ok.result_json | ConvertFrom-Json
    if ($diagnostics.format_version -ne "agentmux.diagnostics.v1") {
      throw "unexpected diagnostics format_version: $($diagnostics.format_version)"
    }
    if ($null -eq $diagnostics.backend_health) {
      throw "diagnostics export omitted backend_health"
    }
    if ($null -eq $diagnostics.queue_pressure -or $diagnostics.queue_pressure.Count -eq 0) {
      throw "diagnostics export omitted queue_pressure"
    }

    $commonCliArgs = @("--json", "--pipe", $pipeName, "--token-path", $tokenPath)
    $workspaceA = Invoke-CliJson `
      -CliExe $cliExe `
      -Arguments (@("workspace", "create", "SmokeAlpha") + $commonCliArgs) `
      -StdoutPath $workspaceAStdout `
      -StderrPath $workspaceAStderr
    $workspaceB = Invoke-CliJson `
      -CliExe $cliExe `
      -Arguments (@("workspace", "create", "SmokeBeta") + $commonCliArgs) `
      -StdoutPath $workspaceBStdout `
      -StderrPath $workspaceBStderr

    $workspaceAId = $workspaceA.workspace_id
    $workspaceBId = $workspaceB.workspace_id
    if ([string]::IsNullOrWhiteSpace($workspaceAId) -or [string]::IsNullOrWhiteSpace($workspaceBId)) {
      throw "workspace create smoke did not return workspace ids"
    }

    $groupA = Invoke-CliJson `
      -CliExe $cliExe `
      -Arguments (@("workspace", "group", "create", "SmokeAlphaGroup", "--workspace", $workspaceAId) + $commonCliArgs) `
      -StdoutPath $groupAStdout `
      -StderrPath $groupAStderr
    $groupB = Invoke-CliJson `
      -CliExe $cliExe `
      -Arguments (@("workspace", "group", "create", "SmokeBetaGroup", "--workspace", $workspaceBId) + $commonCliArgs) `
      -StdoutPath $groupBStdout `
      -StderrPath $groupBStderr

    $groupAId = $groupA.group_id
    $groupBId = $groupB.group_id
    if ([string]::IsNullOrWhiteSpace($groupAId) -or [string]::IsNullOrWhiteSpace($groupBId)) {
      throw "workspace group create smoke did not return group ids"
    }

    Invoke-CliJson `
      -CliExe $cliExe `
      -Arguments (@("workspace", "group", "add", $groupAId, $workspaceBId, "--position", "0") + $commonCliArgs) `
      -StdoutPath $groupAddBStdout `
      -StderrPath $groupAddBStderr | Out-Null
    Invoke-CliJson `
      -CliExe $cliExe `
      -Arguments (@("workspace", "group", "add", $groupAId, $workspaceAId, "--position", "1") + $commonCliArgs) `
      -StdoutPath $groupAddAStdout `
      -StderrPath $groupAddAStderr | Out-Null
    Invoke-CliJson `
      -CliExe $cliExe `
      -Arguments (@("workspace", "group", "update", $groupAId, "--sort-order", "5") + $commonCliArgs) `
      -StdoutPath $groupAUpdateStdout `
      -StderrPath $groupAUpdateStderr | Out-Null
    Invoke-CliJson `
      -CliExe $cliExe `
      -Arguments (@("workspace", "group", "update", $groupBId, "--sort-order", "1") + $commonCliArgs) `
      -StdoutPath $groupBUpdateStdout `
      -StderrPath $groupBUpdateStderr | Out-Null

    $groupsBeforeRestart = Invoke-CliJson `
      -CliExe $cliExe `
      -Arguments (@("workspace", "group", "list") + $commonCliArgs) `
      -StdoutPath $groupListBeforeRestartStdout `
      -StderrPath $groupListBeforeRestartStderr

    if (@($groupsBeforeRestart.groups).Count -lt 2) {
      throw "workspace group smoke expected at least two groups before restart"
    }

    if ($appProcess -and -not $appProcess.HasExited) {
      Stop-Process -Id $appProcess.Id -Force
      $appProcess.WaitForExit()
    }

    $appProcess = Start-Process `
      -FilePath $desktopExe `
      -WindowStyle Hidden `
      -PassThru

    $groupsAfterRestart = $null
    $deadline = (Get-Date).AddSeconds(30)
    do {
      try {
        $groupsAfterRestart = Invoke-CliJson `
          -CliExe $cliExe `
          -Arguments (@("workspace", "group", "list") + $commonCliArgs) `
          -StdoutPath $groupListAfterRestartStdout `
          -StderrPath $groupListAfterRestartStderr
        break
      } catch {
        Start-Sleep -Milliseconds 500
      }
    } while ((Get-Date) -lt $deadline)

    if ($null -eq $groupsAfterRestart) {
      throw "workspace group list did not succeed after packaged-app restart"
    }

    $afterGroups = @($groupsAfterRestart.groups)
    $afterGroupA = $afterGroups | Where-Object { $_.group_id -eq $groupAId } | Select-Object -First 1
    $afterGroupB = $afterGroups | Where-Object { $_.group_id -eq $groupBId } | Select-Object -First 1
    if ($null -eq $afterGroupA -or $null -eq $afterGroupB) {
      throw "workspace group restart smoke could not find created groups after restart"
    }
    if ($afterGroupB.sort_order -ne 1 -or $afterGroupA.sort_order -ne 5) {
      throw "workspace group sort_order did not survive restart"
    }
    if (@($afterGroupA.members).Count -ne 2) {
      throw "workspace group member count did not survive restart"
    }
    $orderedMembers = @($afterGroupA.members | Sort-Object position)
    if ($orderedMembers[0].workspace_id -ne $workspaceBId -or $orderedMembers[0].position -ne 0) {
      throw "workspace group first member position did not survive restart"
    }
    if ($orderedMembers[1].workspace_id -ne $workspaceAId -or $orderedMembers[1].position -ne 1) {
      throw "workspace group second member position did not survive restart"
    }

    $summary = [ordered]@{
      generated_at = (Get-Date).ToUniversalTime().ToString("o")
      desktop_executable = $desktopExe
      cli_executable = $cliExe
      pipe_name = $pipeName
      store_path = $storePath
      token_path = $tokenPath
      app_process_id = $appProcess.Id
      diagnostics_schema = $envelope.schema
      diagnostics_format_version = $diagnostics.format_version
      backend_health_count = @($diagnostics.backend_health).Count
      queue_pressure_count = @($diagnostics.queue_pressure).Count
      notification_count = @($diagnostics.notifications).Count
      browser_failure_count = @($diagnostics.browser.failures).Count
      workspace_group_restart_smoke = "passed"
      workspace_group_alpha_id = $groupAId
      workspace_group_beta_id = $groupBId
      workspace_group_alpha_sort_order = $afterGroupA.sort_order
      workspace_group_beta_sort_order = $afterGroupB.sort_order
      workspace_group_alpha_member_order = @($orderedMembers | ForEach-Object {
        [ordered]@{
          workspace_id = $_.workspace_id
          position = $_.position
        }
      })
      diagnostics_stdout = [System.IO.Path]::GetFileName($diagnosticsStdout)
      diagnostics_stderr = [System.IO.Path]::GetFileName($diagnosticsStderr)
      workspace_groups_before_restart_stdout = [System.IO.Path]::GetFileName($groupListBeforeRestartStdout)
      workspace_groups_after_restart_stdout = [System.IO.Path]::GetFileName($groupListAfterRestartStdout)
    }
    Write-JsonFile -Value $summary -Path (Join-Path $OutputDir "summary.json")
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
} finally {
  Pop-Location
}

Write-Host ("Packaged diagnostics smoke artifacts written to " + $OutputDir)
