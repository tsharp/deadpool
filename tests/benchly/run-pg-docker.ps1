#!/usr/bin/env pwsh
<#
.SYNOPSIS
	Run the deadpool benchmark against a local PostgreSQL Docker container over UDS.

.DESCRIPTION
	Starts a generic local PostgreSQL container, exposes its Unix domain socket
	through a bind-mounted directory, then runs run-benchmark.ps1 using that
	socket instead of Azure/network connectivity.
#>

param(
	[string]$ContainerName = "benchly-postgres",
	[string]$Image = "postgres:16-alpine",
	[string]$DbName = "benchly",
	[string]$DbUser = "postgres",
	[string]$DbPassword = "postgres",
	[string]$SocketDir = "",
	[int]$Workers = 8,
	[int]$RunTime = 15,
	[int]$Warmup = 5,
	[int]$PoolSize = $Workers * 2,
	[int]$PostgresMaxConnections = [Math]::Max($PoolSize + 16, 100),
	[int]$PostgresCommandTimeout = 120,
	[switch]$UseTimeoutPool,
	[int]$DocumentCount = 250000,
	[int]$LoadBatchSize = 10000,
	[string]$DocumentTable = "benchly_documents",
	[ValidateSet("point_read", "pinned_point_read", "pool_only")]
	[string]$WorkloadMode = "point_read",
	[string]$BenchmarkName = "deadpool_benchmark",
	[string]$RunLabel = "",
	[switch]$Recreate,
	[switch]$StopContainer
)

$ErrorActionPreference = "Stop"

function Invoke-CheckedCommand {
	param(
		[Parameter(Mandatory = $true)]
		[scriptblock]$Command,
		[Parameter(Mandatory = $true)]
		[string]$ErrorMessage
	)

	& $Command
	if ($LASTEXITCODE -ne 0) {
		throw $ErrorMessage
	}
}

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
	throw "Docker is not available on PATH."
}

if ($PostgresMaxConnections -lt $PoolSize) {
	throw "PostgresMaxConnections ($PostgresMaxConnections) must be at least PoolSize ($PoolSize)."
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrEmpty($SocketDir)) {
	$SocketDir = Join-Path $scriptDir ".pg-socket"
}

$SocketDir = [System.IO.Path]::GetFullPath($SocketDir)
New-Item -ItemType Directory -Force -Path $SocketDir | Out-Null
if ($IsLinux -or $IsMacOS) {
	chmod 777 $SocketDir 2>$null
	if ($LASTEXITCODE -ne 0) {
		Write-Host "Socket directory permission change was skipped; continuing with existing permissions." -ForegroundColor DarkYellow
	}
}

function Remove-PostgresContainer {
	Write-Host "Removing existing PostgreSQL container: $ContainerName" -ForegroundColor Yellow
	Invoke-CheckedCommand -Command { docker rm -f $ContainerName | Out-Null } -ErrorMessage "Failed to remove existing container."
}

function Start-PostgresContainer {
	Write-Host "Starting PostgreSQL container: $ContainerName" -ForegroundColor Yellow
	Invoke-CheckedCommand -Command {
		docker run -d `
			--name $ContainerName `
			-e POSTGRES_USER=$DbUser `
			-e POSTGRES_PASSWORD=$DbPassword `
			-e POSTGRES_DB=$DbName `
			-v "${SocketDir}:/var/run/postgresql" `
			$Image `
			-c unix_socket_directories=/var/run/postgresql `
			-c max_connections=$PostgresMaxConnections `
			| Out-Null
	} -ErrorMessage "Failed to start PostgreSQL container."
}

function Wait-PostgresReady {
	Write-Host "Waiting for PostgreSQL to accept UDS connections..." -ForegroundColor Yellow
	for ($attempt = 1; $attempt -le 60; $attempt++) {
		docker exec $ContainerName pg_isready -h /var/run/postgresql -U $DbUser -d $DbName | Out-Null
		if ($LASTEXITCODE -eq 0) {
			return
		}
		Start-Sleep -Seconds 1
	}

	docker logs $ContainerName --tail 100
	throw "PostgreSQL did not become ready in time."
}

function Get-PostgresMaxConnections {
	$currentMaxConnections = docker exec $ContainerName psql -U $DbUser -d $DbName -Atc "SHOW max_connections;"
	if ($LASTEXITCODE -ne 0) {
		throw "Failed to read PostgreSQL max_connections."
	}
	return [int]$currentMaxConnections
}

$existingContainer = docker ps -a --filter "name=^/$ContainerName$" --format "{{.Names}}"
if ($existingContainer -and $Recreate) {
	Remove-PostgresContainer
	$existingContainer = ""
}

if (-not $existingContainer) {
	Start-PostgresContainer
} else {
	$runningContainer = docker ps --filter "name=^/$ContainerName$" --format "{{.Names}}"
	if (-not $runningContainer) {
		Write-Host "Starting existing PostgreSQL container: $ContainerName" -ForegroundColor Yellow
		Invoke-CheckedCommand -Command { docker start $ContainerName | Out-Null } -ErrorMessage "Failed to start existing container."
	} else {
		Write-Host "Using running PostgreSQL container: $ContainerName" -ForegroundColor Yellow
	}
}

Wait-PostgresReady

$currentMaxConnections = Get-PostgresMaxConnections
if ($currentMaxConnections -lt $PostgresMaxConnections) {
	Write-Host "Existing PostgreSQL max_connections is $currentMaxConnections, but this run needs $PostgresMaxConnections." -ForegroundColor Yellow
	Write-Host "Recreating local benchmark container with higher max_connections." -ForegroundColor Yellow
	Remove-PostgresContainer
	Start-PostgresContainer
	Wait-PostgresReady
	$currentMaxConnections = Get-PostgresMaxConnections
}

$encodedSocketDir = [System.Uri]::EscapeDataString($SocketDir)
$env:DATABASE_URL = "postgresql://$($DbUser):$($DbPassword)@$($encodedSocketDir)/$($DbName)"

Write-Host "=====================================" -ForegroundColor Cyan
Write-Host "Local PostgreSQL Docker Benchmark" -ForegroundColor Cyan
Write-Host "=====================================" -ForegroundColor Cyan
Write-Host "  Container:     $ContainerName" -ForegroundColor Gray
Write-Host "  Image:         $Image" -ForegroundColor Gray
Write-Host "  Database:      $DbName" -ForegroundColor Gray
Write-Host "  User:          $DbUser" -ForegroundColor Gray
Write-Host "  Socket Dir:    $SocketDir" -ForegroundColor Gray
Write-Host "  Connector:     UDS / NoTls" -ForegroundColor Gray
Write-Host "  PG Max Conn:   $currentMaxConnections (requested $PostgresMaxConnections)" -ForegroundColor Gray
Write-Host ""

$benchmarkArgs = @{
	Workers                = $Workers
	RunTime                = $RunTime
	Warmup                 = $Warmup
	PoolSize               = $PoolSize
	PostgresCommandTimeout = $PostgresCommandTimeout
	DocumentCount          = $DocumentCount
	LoadBatchSize          = $LoadBatchSize
	DocumentTable          = $DocumentTable
	WorkloadMode           = $WorkloadMode
	BenchmarkName          = $BenchmarkName
}

if (-not [string]::IsNullOrEmpty($RunLabel)) {
	$benchmarkArgs.RunLabel = $RunLabel
}

if ($UseTimeoutPool) {
	$benchmarkArgs.UseTimeoutPool = $true
}

try {
	& (Join-Path $scriptDir "run-benchmark.ps1") @benchmarkArgs
	if ($LASTEXITCODE -ne 0) {
		exit $LASTEXITCODE
	}
}
finally {
	if ($StopContainer) {
		Write-Host "Stopping PostgreSQL container: $ContainerName" -ForegroundColor Yellow
		docker stop $ContainerName | Out-Null
	} else {
		Write-Host "Leaving container running for reuse: $ContainerName" -ForegroundColor Yellow
		Write-Host "Use './run-pg-docker.ps1 -Recreate' to reset it, or 'docker rm -f $ContainerName' to remove it." -ForegroundColor Yellow
	}
}
