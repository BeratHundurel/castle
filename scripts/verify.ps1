[CmdletBinding()]
param(
    [ValidateSet("Fast", "Mcp", "NonUi", "Workspace")]
    [string]$Lane = "Fast"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

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
    switch ($Lane) {
        "Fast" {
            Invoke-CheckedCommand -Executable "cargo" -Arguments @("fmt", "--all", "--", "--check")
            Invoke-CheckedCommand -Executable "cargo" -Arguments @("clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings")
            Invoke-CheckedCommand -Executable "cargo" -Arguments @("test", "--locked", "--package", "storage", "--test", "architecture")
            Invoke-CheckedCommand -Executable "cargo" -Arguments @("test", "--locked", "--package", "castle-mcp", "--all-targets")
            & (Join-Path $PSScriptRoot "mcp-smoke.ps1")
            & (Join-Path $PSScriptRoot "mcp-smoke.ps1") -Launcher Cargo
        }
        "Mcp" {
            Invoke-CheckedCommand -Executable "cargo" -Arguments @("test", "--locked", "--package", "castle-mcp", "--all-targets")
            & (Join-Path $PSScriptRoot "mcp-smoke.ps1")
            & (Join-Path $PSScriptRoot "mcp-smoke.ps1") -Launcher Cargo
        }
        "NonUi" {
            Invoke-CheckedCommand -Executable "cargo" -Arguments @("test", "--workspace", "--exclude", "shell", "--all-targets", "--locked", "--no-fail-fast")
        }
        "Workspace" {
            Write-Host "The Workspace lane includes the acknowledged shell UI baseline test."
            Invoke-CheckedCommand -Executable "cargo" -Arguments @("test", "--workspace", "--all-targets", "--locked", "--no-fail-fast")
        }
    }

    Write-Host "Verification lane '$Lane' passed."
}
finally {
    Pop-Location
}
