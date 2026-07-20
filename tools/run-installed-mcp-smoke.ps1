param(
  [Parameter(Mandatory = $true)]
  [string]$InstallerPath,

  [string]$InstallDir,

  [switch]$AllowLocal,

  [switch]$KeepInstalled
)

$ErrorActionPreference = "Stop"

if (-not $env:CI -and -not $AllowLocal) {
  throw "This smoke installs and uninstalls AgentMux. Run it in CI or pass -AllowLocal explicitly."
}

$installer = [System.IO.Path]::GetFullPath($InstallerPath)
if (-not (Test-Path -LiteralPath $installer -PathType Leaf)) {
  throw "AgentMux NSIS installer was not found: $installer"
}

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
  $InstallDir = Join-Path ([System.IO.Path]::GetTempPath()) ("agentmux-mcp-smoke-" + [Guid]::NewGuid().ToString("N"))
}
$installRoot = [System.IO.Path]::GetFullPath($InstallDir)
if (Test-Path -LiteralPath $installRoot) {
  throw "Refusing to reuse an existing MCP smoke install directory: $installRoot"
}

$cli = Join-Path $installRoot "agentmux.exe"
$uninstaller = Join-Path $installRoot "uninstall.exe"
$previewRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("agentmux-mcp-preview-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $previewRoot | Out-Null

function Invoke-CliCheck {
  param(
    [Parameter(Mandatory = $true)]
    [string[]]$Arguments,

    [Parameter(Mandatory = $true)]
    [string[]]$ExpectedText
  )

  $output = @(& $cli @Arguments 2>&1)
  if ($LASTEXITCODE -ne 0) {
    throw "Installed CLI failed: agentmux $($Arguments -join ' ')`n$($output -join [Environment]::NewLine)"
  }
  $text = $output -join [Environment]::NewLine
  foreach ($expected in $ExpectedText) {
    if (-not $text.Contains($expected)) {
      throw "Installed CLI output did not contain '$expected': agentmux $($Arguments -join ' ')"
    }
  }
  return $text
}

function Invoke-SetupPreview {
  param(
    [Parameter(Mandatory = $true)]
    [ValidateSet("codex", "claude")]
    [string]$Client,

    [Parameter(Mandatory = $true)]
    [string]$ConfigPath
  )

  $output = @(& $cli mcp setup --client $Client --profile read --config $ConfigPath --executable $cli --json 2>&1)
  if ($LASTEXITCODE -ne 0) {
    throw "Installed CLI setup preview failed for $Client.`n$($output -join [Environment]::NewLine)"
  }
  $result = ($output -join [Environment]::NewLine) | ConvertFrom-Json
  if ($result.status -ne "preview" -or $result.profile -ne "read" -or $result.client -ne $Client) {
    throw "Unexpected $Client MCP setup preview result."
  }
  if (Test-Path -LiteralPath $ConfigPath) {
    throw "$Client MCP setup preview modified the configuration file."
  }
}

$installed = $false
try {
  $installProcess = Start-Process `
    -FilePath $installer `
    -ArgumentList @("/S", "/NS", "/D=$installRoot") `
    -Wait `
    -PassThru
  if ($installProcess.ExitCode -ne 0) {
    throw "NSIS installer exited with code $($installProcess.ExitCode)."
  }
  $installed = $true

  $deadline = (Get-Date).AddSeconds(30)
  while (-not (Test-Path -LiteralPath $cli -PathType Leaf) -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 250
  }
  if (-not (Test-Path -LiteralPath $cli -PathType Leaf)) {
    throw "The installed package did not contain agentmux.exe at $cli"
  }

  Invoke-CliCheck -Arguments @("mcp", "help") -ExpectedText @("serve|doctor|setup") | Out-Null
  Invoke-CliCheck -Arguments @("mcp", "serve", "--help") -ExpectedText @("read|standard|full") | Out-Null
  Invoke-CliCheck -Arguments @("mcp", "doctor", "--help") -ExpectedText @("--json", "read|standard|full") | Out-Null
  Invoke-CliCheck -Arguments @("mcp", "setup", "--help") -ExpectedText @("codex|claude", "--install") | Out-Null
  Invoke-CliCheck -Arguments @("server", "--help") -ExpectedText @("--mcp-http", "--mcp-profile read|standard|full", "--desktop-control") | Out-Null

  Invoke-SetupPreview -Client codex -ConfigPath (Join-Path $previewRoot "config.toml")
  Invoke-SetupPreview -Client claude -ConfigPath (Join-Path $previewRoot "claude.json")

  Write-Host "Installed AgentMux MCP smoke passed: $cli"
} finally {
  if ($installed -and -not $KeepInstalled -and (Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
    $uninstallProcess = Start-Process -FilePath $uninstaller -ArgumentList @("/S") -Wait -PassThru
    if ($uninstallProcess.ExitCode -ne 0) {
      Write-Warning "AgentMux uninstaller exited with code $($uninstallProcess.ExitCode)."
    }
  }
  if (Test-Path -LiteralPath $previewRoot) {
    $resolvedPreview = [System.IO.Path]::GetFullPath($previewRoot)
    $resolvedTemp = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
    if ($resolvedPreview.StartsWith($resolvedTemp, [System.StringComparison]::OrdinalIgnoreCase)) {
      Remove-Item -LiteralPath $resolvedPreview -Recurse -Force
    }
  }
}
