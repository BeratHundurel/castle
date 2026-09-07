[CmdletBinding()]
param(
    [ValidateSet("Binary", "Cargo")]
    [string]$Launcher = "Binary",
    [ValidateSet("debug", "release")]
    [string]$Profile = "debug",
    [ValidateRange(1000, 120000)]
    [int]$TimeoutMilliseconds = 15000
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$binaryPath = Join-Path $repoRoot (Join-Path (Join-Path "target" $Profile) "castle-mcp.exe")
$started = $false
$process = $null

function Send-JsonMessage {
    param(
        [Parameter(Mandatory)]
        [System.Diagnostics.Process]$Process,
        [Parameter(Mandatory)]
        [object]$Message
    )

    $line = $Message | ConvertTo-Json -Compress -Depth 20
    $Process.StandardInput.WriteLine($line)
    $Process.StandardInput.Flush()
}

function Receive-JsonResponse {
    param(
        [Parameter(Mandatory)]
        [System.Diagnostics.Process]$Process,
        [Parameter(Mandatory)]
        [int]$ExpectedId,
        [Parameter(Mandatory)]
        [int]$TimeoutMilliseconds
    )

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    while ([DateTime]::UtcNow -lt $deadline) {
        $remaining = [Math]::Max(1, [int]($deadline - [DateTime]::UtcNow).TotalMilliseconds)
        $readTask = $Process.StandardOutput.ReadLineAsync()
        if (-not $readTask.Wait($remaining)) {
            throw "Timed out waiting for MCP response id $ExpectedId."
        }

        $line = $readTask.Result
        if ($null -eq $line) {
            $stderr = $Process.StandardError.ReadToEnd()
            throw "MCP process closed stdout before response id $ExpectedId. $stderr"
        }
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }

        try {
            $response = $line | ConvertFrom-Json
        }
        catch {
            throw "MCP emitted non-JSON stdout: $line"
        }

        $idProperty = $response.PSObject.Properties["id"]
        if ($null -ne $idProperty -and "$($idProperty.Value)" -eq "$ExpectedId") {
            return $response
        }
    }

    throw "Timed out waiting for MCP response id $ExpectedId."
}

try {
    if ($Launcher -eq "Binary") {
        if (-not (Test-Path -LiteralPath $binaryPath -PathType Leaf)) {
            throw "Castle MCP binary was not found at '$binaryPath'. Run scripts\agent-bootstrap.ps1 first."
        }
        $executable = $binaryPath
        $arguments = "--database :memory:"
    }
    else {
        $cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
        if ($null -eq $cargoCommand) {
            throw "Cargo was not found on PATH."
        }
        $executable = $cargoCommand.Source
        $arguments = "run --quiet --locked --package castle-mcp --bin castle-mcp -- --database :memory:"
    }

    $startInfo = New-Object System.Diagnostics.ProcessStartInfo
    $startInfo.FileName = $executable
    $startInfo.Arguments = $arguments
    $startInfo.WorkingDirectory = $repoRoot
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardInput = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true

    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        throw "Could not start '$executable'."
    }
    $started = $true

    Send-JsonMessage -Process $process -Message @{
        jsonrpc = "2.0"
        id = 1
        method = "initialize"
        params = @{
            protocolVersion = "2024-11-05"
            capabilities = @{}
            clientInfo = @{
                name = "castle-mcp-smoke"
                version = "0.1.0"
            }
        }
    }

    $initializeResponse = Receive-JsonResponse -Process $process -ExpectedId 1 -TimeoutMilliseconds $TimeoutMilliseconds
    $initializeError = $initializeResponse.PSObject.Properties["error"]
    if ($null -ne $initializeError) {
        throw "MCP initialize failed: $($initializeError.Value | ConvertTo-Json -Compress -Depth 10)"
    }

    Send-JsonMessage -Process $process -Message @{
        jsonrpc = "2.0"
        method = "notifications/initialized"
    }

    Send-JsonMessage -Process $process -Message @{
        jsonrpc = "2.0"
        id = 2
        method = "tools/list"
        params = @{}
    }

    $toolsResponse = Receive-JsonResponse -Process $process -ExpectedId 2 -TimeoutMilliseconds $TimeoutMilliseconds
    $toolsError = $toolsResponse.PSObject.Properties["error"]
    if ($null -ne $toolsError) {
        throw "MCP tools/list failed: $($toolsError.Value | ConvertTo-Json -Compress -Depth 10)"
    }

    $toolsResult = $toolsResponse.PSObject.Properties["result"]
    if ($null -eq $toolsResult) {
        throw "MCP tools/list returned no result."
    }
    $toolsProperty = $toolsResult.Value.PSObject.Properties["tools"]
    if ($null -eq $toolsProperty) {
        throw "MCP tools/list result did not contain tools."
    }
    $tools = @($toolsProperty.Value)
    if ($tools.Count -eq 0) {
        throw "MCP tools/list returned no tools."
    }
    if (-not ($tools.name -contains "list_projects")) {
        throw "MCP tools/list did not contain the expected list_projects tool."
    }

    Write-Host ("MCP stdio smoke passed via {0}: {1} tools discovered." -f $Launcher, $tools.Count)
}
catch {
    Write-Error $_
    exit 1
}
finally {
    if ($null -ne $process) {
        if ($started -and -not $process.HasExited) {
            $process.Kill()
            $process.WaitForExit(3000) | Out-Null
        }
        $process.Dispose()
    }
}
