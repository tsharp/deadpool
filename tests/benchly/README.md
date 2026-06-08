# Benchly - Deadpool Benchmark Framework

A benchmark framework for testing deadpool connection pools with Locust-compatible JSON output format.

## Overview

Benchly provides a skeleton for running performance benchmarks against deadpool connection pools. It collects detailed latency statistics and outputs results in a format compatible with Locust benchmark results.

## Structure

- `main.rs` - Entry point and configuration
- `metrics.rs` - Metrics collection and latency statistics calculation
- `runner.rs` - Benchmark execution engine with worker pool
- `output.rs` - Result formatting and JSON serialization
- `results/` - Output directory for benchmark results

## Quick Start

### Using PowerShell Script (Recommended)

1. Edit `run-benchmark.ps1` and update the database credentials:
   ```powershell
   $DbHost = "localhost"
   $DbPort = 5432
   $DbName = "test"
   $DbUser = "postgres"      # <-- UPDATE THIS
   $DbPassword = "password"  # <-- UPDATE THIS
   ```

2. Run the benchmark:
   ```powershell
   .\run-benchmark.ps1
   ```

3. Optional parameters:
   ```powershell
   .\run-benchmark.ps1 -RunLabel "my_test" -Workers 16 -RunTime 120 -Warmup 10
   ```

### Manual Method

```bash
# Set database URL (if needed)
export DATABASE_URL="postgresql://user:pass@localhost/test"

# Run the benchmark
cargo run --release
```

Results will be saved to `tests/benchly/results/<run_label>.json`.

## Customizing the Workload

The default implementation includes a simulated workload in `runner.rs`. To benchmark actual deadpool operations:

1. **Initialize the pool** in `BenchmarkRunner::new()` or `run()`:
   ```rust
   let pool_config = deadpool_postgres::Config {
       // your config
       ..Default::default()
   };
   let pool = pool_config.create_pool(/* runtime */).unwrap();
   ```

2. **Replace `simulate_operation()`** with your actual workload:
   ```rust
   async fn perform_database_operation(pool: &Pool) -> Result<(), Error> {
       let client = pool.get().await?;
       let row = client.query_one("SELECT 1", &[]).await?;
       Ok(())
   }
   ```

3. **Update worker_loop** to use your operation:
   ```rust
   let result = perform_database_operation(&pool).await;
   ```

## Configuration

Modify the `BenchmarkConfig::default()` in `main.rs` or load from environment:

```rust
BenchmarkConfig {
    benchmark_name: "my_benchmark",
    run_label: "run_001",
    workers: 8,              // Number of concurrent workers
    run_time: Duration::from_secs(90),   // Benchmark duration
    warmup: Duration::from_secs(5),      // Warmup duration
    database_url: "postgresql://...",
    pool_size: Some(10),
}
```

## Output Format

Results are saved as JSON with the following structure:

```json
{
  "metadata": {
    "benchmark_name": "string",
    "run_label": "string",
    "workers": number,
    "run_time": "90s",
    "warmup": "5s",
    "start_time": "ISO8601",
    "end_time": "ISO8601",
    "duration_seconds": number
  },
  "operations": {
    "total_operations": number,
    "total_documents": number,
    "total_failures": number,
    "operations_per_sec": number,
    "documents_per_sec": number,
    "failures_per_sec": number,
    "latency_ms": {
      "avg": number,
      "min": number,
      "max": number,
      "p50": number,
      "p75": number,
      "p90": number,
      "p95": number,
      "p99": number,
      "p999": number,
      "p100": number
    }
  }
}
```

## Example Workloads

### PostgreSQL Connection Pool
```rust
use deadpool_postgres::{Config, Pool, Runtime};

// In BenchmarkRunner::run()
let mut cfg = Config::new();
cfg.dbname = Some("testdb".to_string());
cfg.host = Some("localhost".to_string());
let pool = cfg.create_pool(Some(Runtime::Tokio1), tokio_postgres::NoTls)?;

// In worker_loop
let client = pool.get().await?;
client.query_one("SELECT 1", &[]).await?;
```

### Redis Connection Pool
```rust
use deadpool_redis::{Config, Pool};

let cfg = Config::from_url("redis://localhost");
let pool = cfg.create_pool(Some(Runtime::Tokio1))?;

// In worker
let mut conn = pool.get().await?;
redis::cmd("PING").query_async(&mut conn).await?;
```

## Tips

- Use `--release` mode for accurate performance measurements
- Adjust `workers` to match your system's capabilities
- Run warmup to stabilize connection pools and caches
- Monitor system resources during benchmarks
- Save results with descriptive run labels for comparison

## License

Same as the parent deadpool project (MIT OR Apache-2.0).
