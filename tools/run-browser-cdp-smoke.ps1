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

Push-Location $root
try {
  $stdoutPath = [System.IO.Path]::GetTempFileName()
  $stderrPath = [System.IO.Path]::GetTempFileName()
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
  Remove-Item -LiteralPath $stdoutPath -Force
  Remove-Item -LiteralPath $stderrPath -Force
  if ($exitCode -ne 0) {
    exit $exitCode
  }
} finally {
  Pop-Location
}
