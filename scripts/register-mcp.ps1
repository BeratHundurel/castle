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
$launcherPath = Join-Path $PSScriptRoot "launch-mcp.ps1"
$dataDirectory = Join-Path $repoRoot "target\agent-data"
$databasePath = Join-Path $dataDirectory "castle.db"

if ($null -eq (Get-Command codex -ErrorAction SilentlyContinue)) {
    throw "The Codex CLI was not found on PATH."
}
if ($null -eq (Get-Command pwsh -ErrorAction SilentlyContinue)) {
    throw "PowerShell 7 was not found on PATH."
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
    "pwsh",
    "-NoProfile",
    "-File",
    $launcherPath,
    "-Database",
    $databasePath,
    "-Profile",
    $Profile
)
& codex @arguments
if ($LASTEXITCODE -ne 0) {
    throw "Could not register MCP server '$Name'."
}

Write-Host "Registered '$Name' with an isolated project database: $databasePath"
Write-Host "Restart Codex clients that were already open."
