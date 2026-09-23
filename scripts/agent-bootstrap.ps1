[CmdletBinding()]
param(
    [switch]$SkipWorkspaceCheck,
    [ValidateSet("debug", "release")]
    [string]$Profile = "debug"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$targetDataDirectory = Join-Path $repoRoot "target\agent-data"
$mcpTargetDirectory = Join-Path $targetDataDirectory "mcp-build"

function Invoke-CheckedCommand {
    param(
        [Parameter(Mandatory)]
        [string]$Executable,
        [Parameter(Mandatory)]
        [string[]]$Arguments
    )

    Write-Host ("> {0} {1}" -f $Executable, ($Arguments -join " "))
    & $Executable @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code ${LASTEXITCODE}: $Executable"
    }
}

Push-Location $repoRoot
try {
    foreach ($tool in @("cargo", "rustc", "pwsh")) {
        if ($null -eq (Get-Command $tool -ErrorAction SilentlyContinue)) {
            throw "Required tool '$tool' was not found on PATH."
        }
    }

    if (-not $SkipWorkspaceCheck) {
        Invoke-CheckedCommand -Executable "cargo" -Arguments @("check", "--workspace", "--locked")
    }

    $buildArguments = @("build", "--locked", "--target-dir", $mcpTargetDirectory, "--package", "castle-mcp", "--bin", "castle-mcp")
    if ($Profile -eq "release") {
        $buildArguments += "--release"
    }
    Invoke-CheckedCommand -Executable "cargo" -Arguments $buildArguments

    New-Item -ItemType Directory -Force -Path $targetDataDirectory | Out-Null

    $binaryDirectory = Join-Path $mcpTargetDirectory $Profile
    $binaryPath = Join-Path $binaryDirectory "castle-mcp.exe"
    if (-not (Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
        throw "Castle MCP binary was not produced at '$binaryPath'."
    }

    Write-Host "Agent bootstrap complete."
    Write-Host "Project MCP data directory: $targetDataDirectory"
    Write-Host "Next: powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify.ps1 -Lane Fast"
}
finally {
    Pop-Location
}
