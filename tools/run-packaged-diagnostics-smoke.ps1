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

function Resolve-DesktopExecutable {
  $release = Join-Path $root "target\release\agentmux-desktop-host.exe"
  $debug = Join-Path $root "target\debug\agentmux-desktop-host.exe"
  if (Test-Path $release) {
    return $release
  }
  if (Test-Path $debug) {
    return $debug
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
      -ArgumentList @("/d", "/c", "npm --prefix apps/desktop run tauri:build -- --debug --no-bundle --ci") `
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
      diagnostics_stdout = [System.IO.Path]::GetFileName($diagnosticsStdout)
      diagnostics_stderr = [System.IO.Path]::GetFileName($diagnosticsStderr)
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
