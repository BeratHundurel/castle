[CmdletBinding()]
param(
    [ValidateSet("Fast", "Mcp", "McpLauncher", "NonUi", "Workspace", "Package")]
    [string]$Lane = "Fast",
    [string]$Package
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

if ($Lane -eq "Package" -and [string]::IsNullOrWhiteSpace($Package)) {
    throw "The Package lane requires -Package with a Cargo package name."
}
if ($Lane -ne "Package" -and -not [string]::IsNullOrWhiteSpace($Package)) {
    throw "-Package can only be used with the Package lane."
}

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
        "Package" {
            Invoke-CheckedCommand -Executable "cargo" -Arguments @("fmt", "--package", $Package, "--", "--check")
            Invoke-CheckedCommand -Executable "cargo" -Arguments @("clippy", "--package", $Package, "--all-targets", "--locked", "--", "-D", "warnings")
            Invoke-CheckedCommand -Executable "cargo" -Arguments @("test", "--package", $Package, "--all-targets", "--locked")
        }
        "Fast" {
            Invoke-CheckedCommand -Executable "cargo" -Arguments @("fmt", "--all", "--", "--check")
            Invoke-CheckedCommand -Executable "cargo" -Arguments @("clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings")
            Invoke-CheckedCommand -Executable "cargo" -Arguments @("test", "--locked", "--package", "storage", "--test", "architecture")
        }
        "Mcp" {
            Invoke-CheckedCommand -Executable "cargo" -Arguments @("test", "--locked", "--package", "castle-mcp", "--all-targets")
            & (Join-Path $PSScriptRoot "mcp-smoke.ps1") -Launcher Configured
        }
        "McpLauncher" {
            & (Join-Path $PSScriptRoot "test-mcp-launcher.ps1")
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
