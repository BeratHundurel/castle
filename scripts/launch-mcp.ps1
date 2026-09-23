#requires -Version 7.0

[CmdletBinding()]
param(
    [string]$Database = "target/agent-data/castle.db",
    [ValidateSet("debug", "release")]
    [string]$Profile = "debug"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$mcpTargetDirectory = Join-Path $repoRoot "target/agent-data/mcp-build"
$binaryPath = Join-Path $mcpTargetDirectory (Join-Path $Profile "castle-mcp.exe")
$runRoot = Join-Path $repoRoot "target/agent-data/mcp-runs"
$runDirectory = $null

$buildArguments = @(
    "build", "--quiet", "--locked", "--manifest-path", (Join-Path $repoRoot "Cargo.toml"),
    "--target-dir", $mcpTargetDirectory, "--package", "castle-mcp", "--bin", "castle-mcp"
)
if ($Profile -eq "release") {
    $buildArguments += "--release"
}
& cargo @buildArguments | ForEach-Object { [Console]::Error.WriteLine($_) }
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
    throw "Castle MCP build failed. Run make bootstrap for diagnostics."
}

if ($Database -ne ":memory:" -and -not [System.IO.Path]::IsPathRooted($Database)) {
    $Database = Join-Path $repoRoot $Database
}

try {
    New-Item -ItemType Directory -Force -Path $runRoot | Out-Null
    $runDirectory = Join-Path $runRoot ([guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $runDirectory | Out-Null
    $runBinary = Join-Path $runDirectory "castle-mcp.exe"
    Copy-Item -LiteralPath $binaryPath -Destination $runBinary

    & $runBinary --database $Database
    $serverExitCode = $LASTEXITCODE
}
finally {
    if ($null -ne $runDirectory -and (Test-Path -LiteralPath $runDirectory)) {
        $runBinary = Join-Path $runDirectory "castle-mcp.exe"
        if (Test-Path -LiteralPath $runBinary) {
            Remove-Item -LiteralPath $runBinary
        }
        Remove-Item -LiteralPath $runDirectory
    }
}

exit $serverExitCode
