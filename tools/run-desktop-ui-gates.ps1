param(
  [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
  throw "npm was not found on PATH."
}

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $stamp = Get-Date -Format "yyyyMMdd-HHmmss"
  $machine = if ($env:COMPUTERNAME) { $env:COMPUTERNAME } else { "unknown-machine" }
  $OutputDir = Join-Path $root "docs\implementation\evidence\$stamp-$machine-desktop-ui-gates"
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

Push-Location $root
try {
  $buildStdout = Join-Path $OutputDir "desktop-build.stdout.txt"
  $buildStderr = Join-Path $OutputDir "desktop-build.stderr.txt"
  $buildExit = Invoke-ProcessCapture `
    -FilePath "cmd.exe" `
    -ArgumentList @("/d", "/c", "npm --prefix apps/desktop run build") `
    -StdoutPath $buildStdout `
    -StderrPath $buildStderr
  if ($buildExit -ne 0) {
    throw "desktop build failed with exit code $buildExit. See $buildStderr"
  }

  $distDir = Join-Path $root "apps\desktop\dist"
  $distIndex = Join-Path $distDir "index.html"
  if (-not (Test-Path $distIndex)) {
    throw "desktop build did not produce apps/desktop/dist/index.html"
  }

  $archivedDist = Join-Path $OutputDir "dist"
  if (Test-Path $archivedDist) {
    Remove-Item -LiteralPath $archivedDist -Recurse -Force
  }
  Copy-Item -LiteralPath $distDir -Destination $archivedDist -Recurse -Force

  $uiStdout = Join-Path $OutputDir "ui-smoke.stdout.txt"
  $uiStderr = Join-Path $OutputDir "ui-smoke.stderr.txt"
  $uiExit = Invoke-ProcessCapture `
    -FilePath "cmd.exe" `
    -ArgumentList @("/d", "/c", "npm --prefix apps/desktop run test:ui") `
    -StdoutPath $uiStdout `
    -StderrPath $uiStderr
  if ($uiExit -ne 0) {
    throw "UI smoke failed with exit code $uiExit. See $uiStderr"
  }

  $uiOutput = (Get-Content -Raw -LiteralPath $uiStdout) + "`n" + (Get-Content -Raw -LiteralPath $uiStderr)
  $uiPassedCount = $null
  if ($uiOutput -match "([1-9][0-9]*) passed") {
    $uiPassedCount = [int]$Matches[1]
  }
  if ($null -eq $uiPassedCount) {
    throw "UI smoke did not report passing tests. See $uiStdout and $uiStderr"
  }

  $distFiles = @(Get-ChildItem -LiteralPath $distDir -Recurse -File | ForEach-Object {
    $relativePath = $_.FullName.Substring($distDir.Length).TrimStart("\", "/").Replace("\", "/")
    [ordered]@{
      path = $relativePath
      bytes = $_.Length
    }
  })

  $summary = [ordered]@{
    generated_at = (Get-Date).ToUniversalTime().ToString("o")
    build_command = "npm --prefix apps/desktop run build"
    build_exit_code = $buildExit
    ui_smoke_command = "npm --prefix apps/desktop run test:ui"
    ui_smoke_exit_code = $uiExit
    ui_smoke_result = "$uiPassedCount passed"
    ui_smoke_passed_count = $uiPassedCount
    dist_index = "apps/desktop/dist/index.html"
    archived_dist = "dist"
    dist_files = $distFiles
    build_stdout = [System.IO.Path]::GetFileName($buildStdout)
    build_stderr = [System.IO.Path]::GetFileName($buildStderr)
    ui_stdout = [System.IO.Path]::GetFileName($uiStdout)
    ui_stderr = [System.IO.Path]::GetFileName($uiStderr)
  }
  Write-JsonFile -Value $summary -Path (Join-Path $OutputDir "summary.json")
} finally {
  Pop-Location
}

Write-Host ("Desktop build and UI smoke artifacts written to " + $OutputDir)
