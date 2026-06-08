# Quick Start Example

This example demonstrates running a simple benchmark with the benchly framework.

## Basic Usage

```bash
cd tests/benchly
cargo run --release
```

## Customizing for Deadpool

Replace the `simulate_operation()` in `src/runner.rs` with actual pool operations:

```rust
use deadpool_postgres::{Config, Pool, Runtime};
use tokio_postgres::NoTls;

pub struct BenchmarkRunner {
    config: BenchmarkConfig,
    metrics: Arc<Mutex<MetricsCollector>>,
    pool: Pool,  // Add pool field
}

impl BenchmarkRunner {
    pub fn new(
        config: BenchmarkConfig, 
        metrics: Arc<Mutex<MetricsCollector>>
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Create the pool
        let mut pg_config = Config::new();
        pg_config.dbname = Some("test".to_string());
        pg_config.host = Some("localhost".to_string());
        
        let pool = pg_config.create_pool(Some(Runtime::Tokio1), NoTls)?;
        
        Ok(Self { config, metrics, pool })
    }
    
    // Update worker_loop to accept pool
    async fn worker_loop(
        pool: Pool,
        metrics: Arc<Mutex<MetricsCollector>>,
        end_time: Instant,
        record: bool,
    ) {
        while Instant::now() < end_time {
            let op_start = Instant::now();
            
            // Actual database operation
            let result = async {
                let client = pool.get().await?;
                client.query_one("SELECT 1", &[]).await?;
                Ok::<_, Box<dyn std::error::Error>>(())
            }.await;
            
            let latency = op_start.elapsed();

            if record {
                let mut metrics = metrics.lock().await;
                match result {
                    Ok(_) => metrics.record_success(latency),
                    Err(_) => metrics.record_failure(),
                }
            }
        }
    }
}
```

## Expected Output

```
Starting benchmark: deadpool_benchmark
Workers: 8, Runtime: 90s, Warmup: 5s
Warming up for 5s...
Running benchmark for 90s...

Benchmark complete!
Results saved to: tests/benchly/results/default_run.json

Summary:
  Total operations: 282342
  Operations/sec: 3136.97
  Failures: 0
  Avg latency: 2.05ms
  p50: 2ms, p95: 4ms, p99: 6ms
```
