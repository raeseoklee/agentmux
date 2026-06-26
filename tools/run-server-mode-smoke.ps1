param(
  [int]$Port = 18765,
  [switch]$SkipBuild,
  [string]$AgentMuxExe = "",
  [string]$ServerWorkingDirectory = ""
)

$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")

function Resolve-Cargo {
  $cargo = Get-Command cargo -ErrorAction SilentlyContinue
  if ($cargo) {
    return $cargo.Source
  }

  $userCargo = Join-Path $env:USERPROFILE ".rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe"
  if (Test-Path $userCargo) {
    return $userCargo
  }

  $repoCargo = Join-Path $RepoRoot ".toolchains\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\cargo.exe"
  if (Test-Path $repoCargo) {
    return $repoCargo
  }

  throw "cargo.exe was not found."
}

function Resolve-Npm {
  $npm = Get-Command npm.cmd -ErrorAction SilentlyContinue
  if ($npm) {
    return $npm.Source
  }

  $npm = Get-Command npm -ErrorAction SilentlyContinue
  if ($npm) {
    return $npm.Source
  }

  throw "npm was not found."
}

if ([string]::IsNullOrWhiteSpace($AgentMuxExe)) {
  $AgentMuxExe = Join-Path $RepoRoot "target\debug\agentmux.exe"
  if ([string]::IsNullOrWhiteSpace($ServerWorkingDirectory)) {
    $ServerWorkingDirectory = $RepoRoot.Path
  }
} elseif ([string]::IsNullOrWhiteSpace($ServerWorkingDirectory)) {
  $ServerWorkingDirectory = Split-Path -Parent ([System.IO.Path]::GetFullPath($AgentMuxExe))
}

if (-not $SkipBuild) {
  $npm = Resolve-Npm
  & $npm --prefix (Join-Path $RepoRoot "apps\desktop") run build

  $cargo = Resolve-Cargo
  & $cargo build -p agentmux-cli
}

if (-not (Test-Path $AgentMuxExe)) {
  throw "agentmux.exe was not found at $AgentMuxExe"
}
if (-not (Test-Path -LiteralPath $ServerWorkingDirectory)) {
  throw "server working directory was not found at $ServerWorkingDirectory"
}

$stdout = Join-Path $RepoRoot ".codexus\server-smoke.stdout.log"
$stderr = Join-Path $RepoRoot ".codexus\server-smoke.stderr.log"
Remove-Item -LiteralPath $stdout, $stderr -ErrorAction SilentlyContinue

$proc = Start-Process `
  -FilePath $AgentMuxExe `
  -ArgumentList @("server", "--port", "$Port", "--backend", "conpty", "--json", "--", "cmd.exe", "/d", "/q") `
  -WorkingDirectory $ServerWorkingDirectory `
  -PassThru `
  -WindowStyle Hidden `
  -RedirectStandardOutput $stdout `
  -RedirectStandardError $stderr

try {
  $baseUrl = "http://127.0.0.1:$Port"
  $root = $null
  for ($attempt = 0; $attempt -lt 20; $attempt++) {
    try {
      $root = Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/" -TimeoutSec 2
      break
    } catch {
      Start-Sleep -Milliseconds 250
    }
  }
  if (-not $root) {
    throw "server did not respond on $baseUrl"
  }
  if ($root.Content -notlike "*__AGENTMUX_SERVER__*") {
    throw "server did not serve the desktop UI bundle bootstrap."
  }
  if ($root.Content -like "*AgentMux Web Terminal*") {
    throw "server served the legacy standalone web-terminal UI."
  }
  $assetMatches = [regex]::Matches($root.Content, '(?:src|href)="(?<asset>/assets/[^"]+)"')
  if ($assetMatches.Count -eq 0) {
    throw "server desktop UI response did not reference built assets."
  }
  $tokenMatch = [regex]::Match($root.Content, '"token"\s*:\s*"(?<token>[^"]+)"')
  if (-not $tokenMatch.Success) {
    throw "server desktop UI bootstrap did not include a local auth token."
  }
  $apiHeaders = @{ "X-AgentMux-Server-Token" = $tokenMatch.Groups["token"].Value }
  $assetPath = $assetMatches[0].Groups["asset"].Value
  $asset = Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl$assetPath" -TimeoutSec 5

  $unauthorized = $null
  try {
    $unauthorized = Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/api/state" -TimeoutSec 5
  } catch {
    $unauthorized = $_.Exception.Response
  }
  if (-not $unauthorized -or [int]$unauthorized.StatusCode -ne 401) {
    throw "server API did not reject an unauthenticated request."
  }

  $state = Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/api/state" -Headers $apiHeaders -TimeoutSec 5
  $expectedWslDistributions = @()
  if (Get-Command wsl.exe -ErrorAction SilentlyContinue) {
    $expectedWslOutput = (& wsl.exe --list --quiet 2>$null) -join "`n"
    $expectedWslDistributions = $expectedWslOutput.Replace("`0", "").Split("`n") |
      ForEach-Object { $_.Trim().TrimStart([char]0xfeff).TrimStart([char]'*').Trim() } |
      Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
  }
  $wslDistributionCount = $null
  $wslSessionId = $null
  $wslRecentContainsEcho = $null
  if ($expectedWslDistributions.Count -gt 0) {
    $wsl = Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/api/wsl/distributions" -Headers $apiHeaders -TimeoutSec 5
    $wslDistributions = (($wsl.Content | ConvertFrom-Json).result.distributions)
    $wslDistributionCount = $wslDistributions.Count
    if ($wslDistributionCount -eq 0) {
      throw "server WSL distribution API returned no distributions even though WSL lists installed distributions."
    }
    $distribution = $wslDistributions[0].name
    $wslSpawnBody = @{
      workspace_id = "ws_server"
      backend = "wsl-direct"
      backend_profile = $distribution
      command = @("bash", "-lc", "printf agentmux-wsl-smoke")
      cwd = $RepoRoot.Path
    } | ConvertTo-Json -Compress
    $wslSpawn = Invoke-WebRequest `
      -UseBasicParsing `
      -Method Post `
      -Uri "$baseUrl/api/spawn" `
      -Headers $apiHeaders `
      -ContentType "application/json" `
      -Body $wslSpawnBody `
      -TimeoutSec 20
    $wslSessionId = ($wslSpawn.Content | ConvertFrom-Json).result.session_id
    Start-Sleep -Milliseconds 900
    $wslRecent = Invoke-WebRequest `
      -UseBasicParsing `
      -Uri "$baseUrl/api/session/$wslSessionId/recent?max_bytes=65536" `
      -Headers $apiHeaders `
      -TimeoutSec 5
    $wslRecentText = ($wslRecent.Content | ConvertFrom-Json).result.text
    if ($wslRecentText -notlike "*agentmux-wsl-smoke*") {
      throw "server WSL terminal did not emit the smoke marker."
    }
    $wslRecentContainsEcho = $true
    try {
      $null = Invoke-WebRequest `
        -UseBasicParsing `
        -Method Post `
        -Uri "$baseUrl/api/session/$wslSessionId/terminate" `
        -Headers $apiHeaders `
        -ContentType "application/json" `
        -Body "{}" `
        -TimeoutSec 5
    } catch {
      # The WSL smoke command can exit before cleanup reaches terminate.
    }
  }
  $spawnBody = @{
    workspace_id = "ws_server"
    backend = "conpty"
    command_line = "cmd.exe /d /q"
  } | ConvertTo-Json -Compress
  $spawn = Invoke-WebRequest `
    -UseBasicParsing `
    -Method Post `
    -Uri "$baseUrl/api/spawn" `
    -Headers $apiHeaders `
    -ContentType "application/json" `
    -Body $spawnBody `
    -TimeoutSec 5
  $sessionId = ($spawn.Content | ConvertFrom-Json).result.session_id

  $sendBody = @{ text = "echo agentmux-web`r" } | ConvertTo-Json -Compress
  $null = Invoke-WebRequest `
    -UseBasicParsing `
    -Method Post `
    -Uri "$baseUrl/api/session/$sessionId/send" `
    -Headers $apiHeaders `
    -ContentType "application/json" `
    -Body $sendBody `
    -TimeoutSec 5

  Start-Sleep -Milliseconds 900
  $recent = Invoke-WebRequest `
    -UseBasicParsing `
    -Uri "$baseUrl/api/session/$sessionId/recent?max_bytes=65536" `
    -Headers $apiHeaders `
    -TimeoutSec 5
  $recentText = ($recent.Content | ConvertFrom-Json).result.text
  if ($recentText -notlike "*agentmux-web*") {
    throw "server terminal did not echo the smoke marker."
  }

  $null = Invoke-WebRequest `
    -UseBasicParsing `
    -Method Post `
    -Uri "$baseUrl/api/session/$sessionId/terminate" `
    -Headers $apiHeaders `
    -ContentType "application/json" `
    -Body "{}" `
    -TimeoutSec 5

  [pscustomobject]@{
    rootStatus = $root.StatusCode
    assetStatus = $asset.StatusCode
    stateStatus = $state.StatusCode
    unauthenticatedApiStatus = [int]$unauthorized.StatusCode
    wslDistributionCount = $wslDistributionCount
    wslSessionId = $wslSessionId
    wslRecentContainsEcho = $wslRecentContainsEcho
    spawnStatus = $spawn.StatusCode
    sessionId = $sessionId
    recentContainsEcho = $true
    agentMuxExe = ([System.IO.Path]::GetFullPath($AgentMuxExe))
    serverWorkingDirectory = ([System.IO.Path]::GetFullPath($ServerWorkingDirectory))
    url = "$baseUrl/"
  } | ConvertTo-Json -Compress
} finally {
  if ($proc -and -not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force
  }
}
