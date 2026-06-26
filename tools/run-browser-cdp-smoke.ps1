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
  $cargoPath = $localCargo
}

if (-not $cargoPath) {
  throw "cargo was not found on PATH or in .toolchains."
}

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $machine = if ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { "unknown-machine" }
  $OutputDir = Join-Path $root "docs\implementation\evidence\$stamp-$machine-browser-cdp-smoke"
}

$OutputDir = [System.IO.Path]::GetFullPath($OutputDir)
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
$stdoutPath = Join-Path $OutputDir "browser-cdp-smoke.stdout.txt"
$stderrPath = Join-Path $OutputDir "browser-cdp-smoke.stderr.txt"
$summaryPath = Join-Path $OutputDir "summary.json"

Push-Location $root
try {
  $process = Start-Process `
    -FilePath $cargoPath `
    -ArgumentList @("test", "-p", "agentmux-browser", "cdp_browser_launches_real_browser_smoke", "--", "--ignored", "--nocapture") `
    -RedirectStandardOutput $stdoutPath `
    -RedirectStandardError $stderrPath `
    -WindowStyle Hidden `
    -Wait `
    -PassThru
  $exitCode = $process.ExitCode
  Get-Content -LiteralPath $stdoutPath
  Get-Content -LiteralPath $stderrPath
  if ($exitCode -ne 0) {
    exit $exitCode
  }

  $output = (Get-Content -Raw -LiteralPath $stdoutPath) + "`n" + (Get-Content -Raw -LiteralPath $stderrPath)
  if ($output -notmatch "cdp_browser_launches_real_browser_smoke \.\.\. ok") {
    throw "Browser CDP smoke did not report the expected passing test. See $stdoutPath and $stderrPath"
  }

  [ordered]@{
    generated_at = (Get-Date).ToUniversalTime().ToString("o")
    command = "cargo test -p agentmux-browser cdp_browser_launches_real_browser_smoke -- --ignored --nocapture"
    exit_code = $exitCode
    result = "passed"
    stdout = [System.IO.Path]::GetFileName($stdoutPath)
    stderr = [System.IO.Path]::GetFileName($stderrPath)
  } | ConvertTo-Json -Depth 6 | Set-Content -Encoding UTF8 -Path $summaryPath
} finally {
  Pop-Location
}

Write-Host ("Browser CDP smoke artifacts written to " + $OutputDir)
