[CmdletBinding()]
param(
    [string]$Name = "castle-dev",
    [ValidateSet("debug", "release")]
    [string]$Profile = "debug",
    [switch]$Replace
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$binaryPath = Join-Path $repoRoot (Join-Path (Join-Path "target" $Profile) "castle-mcp.exe")
$dataDirectory = Join-Path $repoRoot "target\agent-data"
$databasePath = Join-Path $dataDirectory "castle.db"

if ($null -eq (Get-Command codex -ErrorAction SilentlyContinue)) {
    throw "The Codex CLI was not found on PATH."
}
if (-not (Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
    throw "Castle MCP binary was not found at '$binaryPath'. Run scripts\agent-bootstrap.ps1 first."
}

New-Item -ItemType Directory -Force -Path $dataDirectory | Out-Null

& codex mcp get $Name 2>$null | Out-Null
$exists = $LASTEXITCODE -eq 0
if ($exists -and -not $Replace) {
    throw "MCP server '$Name' already exists. Re-run with -Replace to update only that entry."
}
if ($exists) {
    & codex mcp remove $Name
    if ($LASTEXITCODE -ne 0) {
        throw "Could not remove existing MCP server '$Name'."
    }
}

$arguments = @(
    "mcp",
    "add",
    $Name,
    "--",
    $binaryPath,
    "--database",
    $databasePath
)
& codex @arguments
if ($LASTEXITCODE -ne 0) {
    throw "Could not register MCP server '$Name'."
}

Write-Host "Registered '$Name' with an isolated project database: $databasePath"
Write-Host "Restart Codex clients that were already open."
