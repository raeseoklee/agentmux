$ErrorActionPreference = "Stop"

Write-Host "AgentMux Windows bootstrap check"
Write-Host ""

$tools = @(
  @{ Name = "git"; Hint = "Install Git for Windows." },
  @{ Name = "cargo"; Hint = "Install Rust from https://rustup.rs/." },
  @{ Name = "node"; Hint = "Install Node.js 22 LTS or newer." },
  @{ Name = "npm"; Hint = "Install npm with Node.js." }
)

foreach ($tool in $tools) {
  $command = Get-Command $tool.Name -ErrorAction SilentlyContinue
  if ($command) {
    Write-Host ("[ok] " + $tool.Name + " -> " + $command.Source)
  } elseif ($tool.Name -eq "cargo" -and (Test-Path ".toolchains\cargo\bin\cargo.exe")) {
    Write-Host "[ok] cargo -> .toolchains\cargo\bin\cargo.exe"
  } else {
    Write-Warning ("[missing] " + $tool.Name + ". " + $tool.Hint)
  }
}

Write-Host ""
Write-Host "Next commands:"
Write-Host "  npm install"
Write-Host "  Push-Location apps/desktop; npm install; Pop-Location"
Write-Host "  npm run docs:check"
Write-Host "  npm run check"
