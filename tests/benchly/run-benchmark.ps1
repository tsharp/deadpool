#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Run deadpool benchmarks with configurable database connection

.DESCRIPTION
    Simple script to run benchly benchmarks. Update the connection settings
    at the top of this file and run.

.PARAMETER RunLabel
    Label for this benchmark run (default: timestamp)

.PARAMETER Workers
    Number of concurrent workers (default: 8)

.PARAMETER RunTime
    Benchmark duration in seconds (default: 90)

.PARAMETER Warmup
    Warmup duration in seconds (default: 5)

.EXAMPLE
    .\run-benchmark.ps1
    
.EXAMPLE
    .\run-benchmark.ps1 -RunLabel "my_test" -Workers 16 -RunTime 120
#>

param(
    [string]$RunLabel = "",
    [int]$Workers = 8,
    [int]$RunTime = 90,
    [int]$Warmup = 5,
    [string]$BenchmarkName = "deadpool_benchmark"
)

# ============================================================================
# DATABASE CONNECTION SETTINGS - UPDATE THESE
# ============================================================================

$DbHost = "localhost"
$DbPort = 5432
$DbName = "test"
$DbUser = "postgres"           # <-- UPDATE THIS
$DbPassword = "password"       # <-- UPDATE THIS

# ============================================================================

# Build connection string
$env:DATABASE_URL = "postgresql://${DbUser}:${DbPassword}@${DbHost}:${DbPort}/${DbName}"

# Set benchmark configuration
$env:BENCHMARK_NAME = $BenchmarkName
$env:WORKERS = $Workers
$env:RUN_TIME_SECS = $RunTime
$env:WARMUP_SECS = $Warmup

# Generate run label if not provided
if ([string]::IsNullOrEmpty($RunLabel)) {
    $timestamp = Get-Date -Format "yyyy-MM-dd_HH-mm-ss"
    $RunLabel = "${BenchmarkName}_${timestamp}"
}
$env:RUN_LABEL = $RunLabel

Write-Host "=====================================" -ForegroundColor Cyan
Write-Host "Deadpool Benchmark Runner" -ForegroundColor Cyan
Write-Host "=====================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Configuration:" -ForegroundColor Yellow
Write-Host "  Database:      $DbHost:$DbPort/$DbName" -ForegroundColor Gray
Write-Host "  User:          $DbUser" -ForegroundColor Gray
Write-Host "  Benchmark:     $BenchmarkName" -ForegroundColor Gray
Write-Host "  Run Label:     $RunLabel" -ForegroundColor Gray
Write-Host "  Workers:       $Workers" -ForegroundColor Gray
Write-Host "  Runtime:       ${RunTime}s" -ForegroundColor Gray
Write-Host "  Warmup:        ${Warmup}s" -ForegroundColor Gray
Write-Host ""

# Change to benchly directory
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $scriptDir

try {
    # Build in release mode for accurate benchmarks
    Write-Host "Building benchmark (release mode)..." -ForegroundColor Yellow
    cargo build --release
    
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Build failed!" -ForegroundColor Red
        exit 1
    }
    
    Write-Host ""
    Write-Host "Starting benchmark..." -ForegroundColor Green
    Write-Host ""
    
    # Run the benchmark
    cargo run --release
    
    if ($LASTEXITCODE -eq 0) {
        Write-Host ""
        Write-Host "=====================================" -ForegroundColor Green
        Write-Host "Benchmark Complete!" -ForegroundColor Green
        Write-Host "=====================================" -ForegroundColor Green
        
        $resultFile = "results/${RunLabel}.json"
        if (Test-Path $resultFile) {
            Write-Host ""
            Write-Host "Results saved to: $resultFile" -ForegroundColor Cyan
            
            # Display quick summary
            $results = Get-Content $resultFile | ConvertFrom-Json
            Write-Host ""
            Write-Host "Quick Summary:" -ForegroundColor Yellow
            Write-Host "  Total Operations:  $($results.operations.total_operations)" -ForegroundColor Gray
            Write-Host "  Operations/sec:    $([math]::Round($results.operations.operations_per_sec, 2))" -ForegroundColor Gray
            Write-Host "  Failures:          $($results.operations.total_failures)" -ForegroundColor Gray
            Write-Host "  Avg Latency:       $([math]::Round($results.operations.latency_ms.avg, 2))ms" -ForegroundColor Gray
            Write-Host "  p50:               $($results.operations.latency_ms.p50)ms" -ForegroundColor Gray
            Write-Host "  p95:               $($results.operations.latency_ms.p95)ms" -ForegroundColor Gray
            Write-Host "  p99:               $($results.operations.latency_ms.p99)ms" -ForegroundColor Gray
        }
    } else {
        Write-Host ""
        Write-Host "Benchmark failed!" -ForegroundColor Red
        exit 1
    }
}
finally {
    Pop-Location
}
