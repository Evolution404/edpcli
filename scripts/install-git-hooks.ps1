$ErrorActionPreference = "Stop"

$repoRoot = git rev-parse --show-toplevel
if (-not $repoRoot) {
    throw "Not inside the edpcli Git repository."
}

Set-Location $repoRoot
git config core.hooksPath .githooks
Write-Host "Installed edpcli Git hooks: core.hooksPath=.githooks"
