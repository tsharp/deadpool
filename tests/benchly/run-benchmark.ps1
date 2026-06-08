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
    [int]$RunTime = 15,
    [int]$Warmup = 5,
    [int]$PoolSize = $Workers * 2,
    [int]$PostgresCommandTimeout = 120,
    [switch]$UseTimeoutPool,
    [switch]$RequireTls,
    [int]$DocumentCount = 250000,
    [int]$LoadBatchSize = 10000,
    [string]$DocumentTable = "benchly_documents",
    [ValidateSet("point_read", "pinned_point_read", "pool_only")]
    [string]$WorkloadMode = "point_read",
    [string]$BenchmarkName = "deadpool_benchmark"
)

# ============================================================================
# DATABASE CONNECTION SETTINGS - UPDATE THESE
# ============================================================================

$DbHost = "localhost"
$DbPort = 5432
$DbName = "benchly"
$DbUser = "postgres"           # <-- UPDATE THIS
$DbPassword = "postgres"       # <-- UPDATE THIS

# ============================================================================

# Build connection string unless the caller already supplied one.
if ([string]::IsNullOrEmpty($env:DATABASE_URL)) {
    $env:DATABASE_URL = "postgresql://$($DbUser):$($DbPassword)@$($DbHost):$($DbPort)/$($DbName)"
}

# Set benchmark configuration
$env:BENCHMARK_NAME = $BenchmarkName
$env:WORKERS = $Workers
$env:RUN_TIME_SECS = $RunTime
$env:WARMUP_SECS = $Warmup
$env:POOL_SIZE = $PoolSize
$env:POSTGRES_COMMAND_TIMEOUT_SECS = $PostgresCommandTimeout
$env:USE_TIMEOUT_POOL = if ($UseTimeoutPool) { "true" } else { "false" }
$env:REQUIRE_TLS = if ($RequireTls) { "true" } else { "false" }
$env:DOCUMENT_COUNT = $DocumentCount
$env:LOAD_BATCH_SIZE = $LoadBatchSize
$env:DOCUMENT_TABLE = $DocumentTable
$env:WORKLOAD_MODE = $WorkloadMode

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
$displayDatabaseUrl = $env:DATABASE_URL -replace '://([^:/@]+):([^@]+)@', '://$1:***@'
Write-Host "  Database URL:  $displayDatabaseUrl" -ForegroundColor Gray
Write-Host "  User:          $DbUser" -ForegroundColor Gray
Write-Host "  Benchmark:     $BenchmarkName" -ForegroundColor Gray
Write-Host "  Run Label:     $RunLabel" -ForegroundColor Gray
Write-Host "  Workers:       $Workers" -ForegroundColor Gray
Write-Host "  Pool Size:     $PoolSize" -ForegroundColor Gray
Write-Host "  Wait Timeout:  ${PostgresCommandTimeout}s" -ForegroundColor Gray
Write-Host "  Pool Mode:     $(if ($UseTimeoutPool) { 'Clean' } else { 'Fast' })" -ForegroundColor Gray
Write-Host "  Connector:     $(if ($RequireTls) { 'TLS' } else { 'NoTls' })" -ForegroundColor Gray
Write-Host "  Documents:     $DocumentCount" -ForegroundColor Gray
Write-Host "  Load Batch:    $LoadBatchSize" -ForegroundColor Gray
Write-Host "  Table:         $DocumentTable" -ForegroundColor Gray
Write-Host "  Workload Mode: $WorkloadMode" -ForegroundColor Gray
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
            Write-Host "  Successful Ops:    $($results.operations.successful_operations)" -ForegroundColor Gray
            Write-Host "  Operations/sec:    $([math]::Round($results.operations.operations_per_sec, 2))" -ForegroundColor Gray
            Write-Host "  Failures:          $($results.operations.total_failures)" -ForegroundColor Gray
            Write-Host "  Avg Latency:       $([math]::Round($results.operations.latency_ms.avg, 2))ms" -ForegroundColor Gray
            Write-Host "  Avg Latency:       $([math]::Round($results.operations.latency_us.avg, 2))us" -ForegroundColor Gray
            Write-Host "  p50:               $($results.operations.latency_us.p50)us" -ForegroundColor Gray
            Write-Host "  p95:               $($results.operations.latency_us.p95)us" -ForegroundColor Gray
            Write-Host "  p99:               $($results.operations.latency_us.p99)us" -ForegroundColor Gray
            if ($results.operations.failure_causes) {
                Write-Host "  Failure Causes:" -ForegroundColor Gray
                $results.operations.failure_causes.PSObject.Properties | ForEach-Object {
                    Write-Host "    $($_.Value)x $($_.Name)" -ForegroundColor Gray
                }
            }
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
