<#
.SYNOPSIS
Measures Castle's warm Cargo test lanes and optionally compares them with a baseline.

.DESCRIPTION
Runs one warm-up followed by repeated measurements for the selected Cargo test lanes.
Results, raw command logs, executable sizes, and host/toolchain metadata are written
under target/performance-measurements by default, so generated evidence is not tracked.

Peak memory is the peak working set reported for the root Cargo process. Cargo launches
compiler and test child processes, so this value is useful for comparing Cargo overhead
but must not be presented as peak memory for the complete process tree.

.PARAMETER Lane
One or more lanes to measure: app, board, document_editor, app_settings,
storage, or workspace. The feature lanes become available as their crates are
added to the workspace.

.PARAMETER Warmup
Number of unrecorded warm-up runs per lane. Defaults to 1.

.PARAMETER Repetitions
Number of recorded runs per lane. Defaults to 5.

.PARAMETER Label
Short label included in the output directory and result metadata.

.PARAMETER BaselineResult
Path to a result.json produced by an earlier invocation. Matching lane medians are
compared in comparison.json and comparison.csv.

.PARAMETER OutputRoot
Directory that receives timestamped result directories. It must be inside target.

.EXAMPLE
./scripts/measure-cargo-tests.ps1 -Lane app,storage -Repetitions 7 -Label before

.EXAMPLE
./scripts/measure-cargo-tests.ps1 -Label after -BaselineResult target/performance-measurements/20260815-120000-before/result.json
#>

[CmdletBinding()]
param(
    [ValidateSet("app", "board", "document_editor", "app_settings", "storage", "workspace")]
    [string[]] $Lane = @("app", "storage", "workspace"),

    [ValidateRange(0, 100)]
    [int] $Warmup = 1,

    [ValidateRange(1, 100)]
    [int] $Repetitions = 5,

    [ValidatePattern("^[a-zA-Z0-9._-]+$")]
    [string] $Label = "measurement",

    [string] $BaselineResult,

    [string] $OutputRoot,

    [switch] $Help
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($Help) {
    Get-Help $PSCommandPath -Detailed
    exit 0
}

function Invoke-CapturedProcess {
    param(
        [Parameter(Mandatory)]
        [string] $FilePath,

        [Parameter(Mandatory)]
        [string[]] $Arguments,

        [Parameter(Mandatory)]
        [string] $WorkingDirectory
    )

    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $FilePath
    $startInfo.WorkingDirectory = $WorkingDirectory
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.Environment["CARGO_TERM_COLOR"] = "never"
    foreach ($argument in $Arguments) {
        $startInfo.ArgumentList.Add($argument)
    }

    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    if (-not $process.Start()) {
        throw "Failed to start $FilePath."
    }

    $standardOutput = $process.StandardOutput.ReadToEndAsync()
    $standardError = $process.StandardError.ReadToEndAsync()
    $peakWorkingSetBytes = 0L
    while (-not $process.WaitForExit(25)) {
        $process.Refresh()
        $peakWorkingSetBytes = [Math]::Max($peakWorkingSetBytes, $process.PeakWorkingSet64)
    }
    $process.Refresh()
    $peakWorkingSetBytes = [Math]::Max($peakWorkingSetBytes, $process.PeakWorkingSet64)
    $stopwatch.Stop()

    return [ordered]@{
        exit_code = $process.ExitCode
        elapsed_seconds = [Math]::Round($stopwatch.Elapsed.TotalSeconds, 6)
        root_process_peak_working_set_bytes = $peakWorkingSetBytes
        stdout = $standardOutput.GetAwaiter().GetResult()
        stderr = $standardError.GetAwaiter().GetResult()
    }
}

function Invoke-TextCommand {
    param(
        [Parameter(Mandatory)]
        [string] $FilePath,

        [Parameter(Mandatory)]
        [string[]] $Arguments,

        [Parameter(Mandatory)]
        [string] $WorkingDirectory
    )

    $result = Invoke-CapturedProcess -FilePath $FilePath -Arguments $Arguments -WorkingDirectory $WorkingDirectory
    if ($result.exit_code -ne 0) {
        throw "Command failed: $FilePath $($Arguments -join ' ')`n$($result.stderr)"
    }
    return $result.stdout.Trim()
}

function Get-Percentile {
    param(
        [Parameter(Mandatory)]
        [double[]] $Values,

        [Parameter(Mandatory)]
        [ValidateRange(0.0, 1.0)]
        [double] $Percentile
    )

    $sorted = @($Values | Sort-Object)
    if ($sorted.Count -eq 1) {
        return $sorted[0]
    }

    $position = ($sorted.Count - 1) * $Percentile
    $lowerIndex = [Math]::Floor($position)
    $upperIndex = [Math]::Ceiling($position)
    if ($lowerIndex -eq $upperIndex) {
        return $sorted[$lowerIndex]
    }

    $weight = $position - $lowerIndex
    return $sorted[$lowerIndex] + (($sorted[$upperIndex] - $sorted[$lowerIndex]) * $weight)
}

function Get-TestExecutables {
    param(
        [Parameter(Mandatory)]
        [string] $CargoOutput
    )

    $paths = [System.Collections.Generic.HashSet[string]]::new(
        [System.StringComparer]::OrdinalIgnoreCase
    )
    foreach ($line in $CargoOutput -split "`r?`n") {
        if (-not $line.StartsWith("{")) {
            continue
        }
        try {
            $message = $line | ConvertFrom-Json
        }
        catch {
            continue
        }
        if (
            $message.reason -eq "compiler-artifact" -and
            $message.profile.test -eq $true -and
            -not [string]::IsNullOrWhiteSpace($message.executable)
        ) {
            [void] $paths.Add([string] $message.executable)
        }
    }

    return @(
        foreach ($path in $paths) {
            if (Test-Path -LiteralPath $path -PathType Leaf) {
                $file = Get-Item -LiteralPath $path
                [ordered]@{
                    path = $file.FullName
                    size_bytes = $file.Length
                }
            }
        }
    )
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$targetRoot = (Join-Path $repoRoot "target")
if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
    $OutputRoot = Join-Path $targetRoot "performance-measurements"
}
elseif (-not [System.IO.Path]::IsPathRooted($OutputRoot)) {
    $OutputRoot = Join-Path $repoRoot $OutputRoot
}

$resolvedOutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
$resolvedTargetRoot = [System.IO.Path]::GetFullPath($targetRoot)
$targetPrefix = $resolvedTargetRoot.TrimEnd([System.IO.Path]::DirectorySeparatorChar) + [System.IO.Path]::DirectorySeparatorChar
if (-not $resolvedOutputRoot.StartsWith($targetPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "OutputRoot must be inside $resolvedTargetRoot so generated measurements remain untracked."
}

$timestamp = [DateTimeOffset]::UtcNow.ToString("yyyyMMdd-HHmmssfff")
$resultDirectory = Join-Path $resolvedOutputRoot "$timestamp-$Label"
New-Item -ItemType Directory -Path $resultDirectory -Force | Out-Null

$laneDefinitions = [ordered]@{
    app = @("test", "-p", "app", "--lib", "--message-format=json")
    board = @("test", "-p", "board", "--lib", "--message-format=json")
    document_editor = @("test", "-p", "document_editor", "--lib", "--message-format=json")
    app_settings = @("test", "-p", "app_settings", "--lib", "--message-format=json")
    storage = @("test", "-p", "storage", "--lib", "--message-format=json")
    workspace = @("test", "--workspace", "--all-targets", "--message-format=json")
}

$gitStatus = Invoke-TextCommand -FilePath "git" -Arguments @("status", "--short") -WorkingDirectory $repoRoot
$metadata = [ordered]@{
    recorded_at_utc = [DateTimeOffset]::UtcNow.ToString("O")
    label = $Label
    repository_root = $repoRoot
    git_commit = Invoke-TextCommand -FilePath "git" -Arguments @("rev-parse", "HEAD") -WorkingDirectory $repoRoot
    git_dirty = -not [string]::IsNullOrWhiteSpace($gitStatus)
    rustc = Invoke-TextCommand -FilePath "rustc" -Arguments @("-Vv") -WorkingDirectory $repoRoot
    cargo = Invoke-TextCommand -FilePath "cargo" -Arguments @("-V") -WorkingDirectory $repoRoot
    operating_system = [System.Runtime.InteropServices.RuntimeInformation]::OSDescription
    architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    host_name = [System.Net.Dns]::GetHostName()
    processor = $env:PROCESSOR_IDENTIFIER
    logical_processor_count = [Environment]::ProcessorCount
    warmup_runs = $Warmup
    measured_runs = $Repetitions
    memory_metric = "Root Cargo process PeakWorkingSet64; excludes compiler and test child processes."
}

$laneResults = @()
foreach ($laneName in $Lane) {
    $arguments = $laneDefinitions[$laneName]
    Write-Host "Warming $laneName ($Warmup run(s))..."
    for ($index = 1; $index -le $Warmup; $index++) {
        $warmupResult = Invoke-CapturedProcess -FilePath "cargo" -Arguments $arguments -WorkingDirectory $repoRoot
        if ($warmupResult.exit_code -ne 0) {
            throw "Warm-up failed for $laneName.`n$($warmupResult.stderr)"
        }
    }

    $runs = @()
    $executablesByPath = [ordered]@{}
    for ($index = 1; $index -le $Repetitions; $index++) {
        Write-Host "Measuring $laneName ($index/$Repetitions)..."
        $run = Invoke-CapturedProcess -FilePath "cargo" -Arguments $arguments -WorkingDirectory $repoRoot
        $stdoutPath = Join-Path $resultDirectory "$laneName-run-$index.stdout.log"
        $stderrPath = Join-Path $resultDirectory "$laneName-run-$index.stderr.log"
        Set-Content -LiteralPath $stdoutPath -Value $run.stdout -Encoding utf8NoBOM
        Set-Content -LiteralPath $stderrPath -Value $run.stderr -Encoding utf8NoBOM
        if ($run.exit_code -ne 0) {
            throw "Measured run failed for $laneName. See $stdoutPath and $stderrPath."
        }

        foreach ($executable in (Get-TestExecutables -CargoOutput $run.stdout)) {
            $executablesByPath[$executable.path] = $executable
        }
        $runs += [ordered]@{
            repetition = $index
            elapsed_seconds = $run.elapsed_seconds
            root_process_peak_working_set_bytes = $run.root_process_peak_working_set_bytes
            stdout_log = [System.IO.Path]::GetRelativePath($resultDirectory, $stdoutPath)
            stderr_log = [System.IO.Path]::GetRelativePath($resultDirectory, $stderrPath)
        }
    }

    [double[]] $elapsed = @($runs | ForEach-Object { $_.elapsed_seconds })
    [double[]] $rootPeakMemory = @($runs | ForEach-Object { $_.root_process_peak_working_set_bytes })
    $executables = @($executablesByPath.Values)
    $executableTotalBytes = 0L
    foreach ($executable in $executables) {
        $executableTotalBytes += [long] $executable["size_bytes"]
    }
    $laneResults += [ordered]@{
        name = $laneName
        cargo_arguments = $arguments
        runs = $runs
        summary = [ordered]@{
            elapsed_seconds_median = [Math]::Round((Get-Percentile -Values $elapsed -Percentile 0.5), 6)
            elapsed_seconds_min = [Math]::Round(($elapsed | Measure-Object -Minimum).Minimum, 6)
            elapsed_seconds_max = [Math]::Round(($elapsed | Measure-Object -Maximum).Maximum, 6)
            elapsed_seconds_p25 = [Math]::Round((Get-Percentile -Values $elapsed -Percentile 0.25), 6)
            elapsed_seconds_p75 = [Math]::Round((Get-Percentile -Values $elapsed -Percentile 0.75), 6)
            root_process_peak_working_set_bytes_median = [long](Get-Percentile -Values $rootPeakMemory -Percentile 0.5)
            test_executable_count = $executables.Count
            test_executable_total_bytes = $executableTotalBytes
        }
        test_executables = $executables
    }
}

$result = [ordered]@{
    schema_version = 1
    metadata = $metadata
    lanes = $laneResults
}
$resultPath = Join-Path $resultDirectory "result.json"
$result | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $resultPath -Encoding utf8NoBOM

$summaryRows = foreach ($laneResult in $laneResults) {
    [pscustomobject]@{
        label = $Label
        lane = $laneResult.name
        repetitions = $Repetitions
        elapsed_seconds_median = $laneResult.summary.elapsed_seconds_median
        elapsed_seconds_min = $laneResult.summary.elapsed_seconds_min
        elapsed_seconds_max = $laneResult.summary.elapsed_seconds_max
        root_cargo_peak_working_set_bytes_median = $laneResult.summary.root_process_peak_working_set_bytes_median
        test_executable_count = $laneResult.summary.test_executable_count
        test_executable_total_bytes = $laneResult.summary.test_executable_total_bytes
    }
}
$summaryRows | Export-Csv -LiteralPath (Join-Path $resultDirectory "summary.csv") -NoTypeInformation -Encoding utf8

if (-not [string]::IsNullOrWhiteSpace($BaselineResult)) {
    $baselinePath = if ([System.IO.Path]::IsPathRooted($BaselineResult)) {
        $BaselineResult
    }
    else {
        Join-Path $repoRoot $BaselineResult
    }
    $baseline = Get-Content -LiteralPath $baselinePath -Raw | ConvertFrom-Json
    $comparisons = foreach ($laneResult in $laneResults) {
        $baselineLane = @($baseline.lanes | Where-Object { $_.name -eq $laneResult.name })
        if ($baselineLane.Count -ne 1) {
            continue
        }
        $before = [double] $baselineLane[0].summary.elapsed_seconds_median
        $after = [double] $laneResult.summary.elapsed_seconds_median
        $percentChange = if ($before -eq 0.0) { $null } else { (($after - $before) / $before) * 100.0 }
        $baselineExecutableBytes = [double] $baselineLane[0].summary.test_executable_total_bytes
        $currentExecutableBytes = [double] $laneResult.summary.test_executable_total_bytes
        $executablePercentChange = if ($baselineExecutableBytes -eq 0.0) {
            $null
        }
        else {
            (($currentExecutableBytes - $baselineExecutableBytes) / $baselineExecutableBytes) * 100.0
        }
        [pscustomobject]@{
            lane = $laneResult.name
            baseline_label = $baseline.metadata.label
            current_label = $Label
            baseline_elapsed_seconds_median = $before
            current_elapsed_seconds_median = $after
            elapsed_percent_change = if ($null -eq $percentChange) { $null } else { [Math]::Round($percentChange, 3) }
            baseline_test_executable_total_bytes = [long] $baselineExecutableBytes
            current_test_executable_total_bytes = [long] $currentExecutableBytes
            test_executable_size_percent_change = if ($null -eq $executablePercentChange) {
                $null
            }
            else {
                [Math]::Round($executablePercentChange, 3)
            }
        }
    }
    $comparisonResult = [ordered]@{
        baseline_result = (Resolve-Path -LiteralPath $baselinePath).Path
        current_result = $resultPath
        lanes = @($comparisons)
    }
    $comparisonResult | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $resultDirectory "comparison.json") -Encoding utf8NoBOM
    $comparisons | Export-Csv -LiteralPath (Join-Path $resultDirectory "comparison.csv") -NoTypeInformation -Encoding utf8
}

Write-Host "Measurement complete: $resultPath"
