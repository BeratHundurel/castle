[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$binaryPath = Join-Path $repoRoot "target/agent-data/mcp-build/debug/castle-mcp.exe"
$backupPath = "$binaryPath.$([guid]::NewGuid().ToString('N')).backup"
$hadBinary = Test-Path -LiteralPath $binaryPath -PathType Leaf

if ($hadBinary) {
    Move-Item -LiteralPath $binaryPath -Destination $backupPath
}

try {
    & (Join-Path $PSScriptRoot "mcp-smoke.ps1") -Launcher Configured -StartupTimeoutMilliseconds 600000
    if (-not (Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
        throw "The configured MCP launcher did not build its missing binary."
    }
}
finally {
    if ($hadBinary -and (Test-Path -LiteralPath $backupPath -PathType Leaf)) {
        if (Test-Path -LiteralPath $binaryPath -PathType Leaf) {
            Remove-Item -LiteralPath $backupPath
        }
        else {
            Move-Item -LiteralPath $backupPath -Destination $binaryPath
        }
    }
}
