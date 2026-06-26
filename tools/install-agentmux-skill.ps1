param(
  [string]$SkillName = "agentmux-control",
  [string]$DestinationRoot = ""
)

$ErrorActionPreference = "Stop"

$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$source = Join-Path $root "skills\$SkillName"

if (-not (Test-Path (Join-Path $source "SKILL.md"))) {
  throw "Skill '$SkillName' was not found at $source."
}

if ([string]::IsNullOrWhiteSpace($DestinationRoot)) {
  $codexHome = if ($env:CODEX_HOME) {
    $env:CODEX_HOME
  } elseif ($env:USERPROFILE) {
    Join-Path $env:USERPROFILE ".codex"
  } else {
    throw "Could not resolve CODEX_HOME or USERPROFILE for skill installation."
  }
  $DestinationRoot = Join-Path $codexHome "skills"
}

$destinationRootFull = [System.IO.Path]::GetFullPath($DestinationRoot)
$destination = Join-Path $destinationRootFull $SkillName

New-Item -ItemType Directory -Force -Path $destination | Out-Null
Copy-Item -Path (Join-Path $source "*") -Destination $destination -Recurse -Force

$installedSkill = Join-Path $destination "SKILL.md"
if (-not (Test-Path $installedSkill)) {
  throw "Skill install failed: $installedSkill was not created."
}

Write-Host "Installed AgentMux skill '$SkillName' to $destination"
