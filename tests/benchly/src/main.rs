use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};

mod metrics;
mod runner;
mod output;

use metrics::MetricsCollector;
use runner::BenchmarkRunner;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub benchmark_name: String,
    pub run_label: String,
    pub workers: usize,
    pub run_time: Duration,
    pub warmup: Duration,
    pub database_url: String,
    pub pool_size: Option<usize>,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            benchmark_name: std::env::var("BENCHMARK_NAME")
                .unwrap_or_else(|_| "deadpool_benchmark".to_string()),
            run_label: std::env::var("RUN_LABEL")
                .unwrap_or_else(|_| "default_run".to_string()),
            workers: std::env::var("WORKERS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8),
            run_time: Duration::from_secs(
                std::env::var("RUN_TIME_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(90)
            ),
            warmup: Duration::from_secs(
                std::env::var("WARMUP_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(5)
            ),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://localhost/test".to_string()),
            pool_size: std::env::var("POOL_SIZE")
                .ok()
                .and_then(|s| s.parse().ok()),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration from environment or use defaults
    let config = BenchmarkConfig::default();
    
    println!("Starting benchmark: {}", config.benchmark_name);
    println!("Workers: {}, Runtime: {}s, Warmup: {}s", 
        config.workers, 
        config.run_time.as_secs(),
        config.warmup.as_secs()
    );

    // Create metrics collector
    let metrics = Arc::new(Mutex::new(MetricsCollector::new()));
    
    // Create and run benchmark
    let runner = BenchmarkRunner::new(config.clone(), metrics.clone());
    let results = runner.run().await?;
    
    // Output results to JSON file
    let output_path = format!("tests/benchly/results/{}.json", config.run_label);
    results.save_to_file(&output_path)?;
    
    println!("\nBenchmark complete!");
    println!("Results saved to: {}", output_path);
    println!("\nSummary:");
    println!("  Total operations: {}", results.operations.total_operations);
    println!("  Operations/sec: {:.2}", results.operations.operations_per_sec);
    println!("  Failures: {}", results.operations.total_failures);
    println!("  Avg latency: {:.2}ms", results.operations.latency_ms.avg);
    println!("  p50: {}ms, p95: {}ms, p99: {}ms", 
        results.operations.latency_ms.p50,
        results.operations.latency_ms.p95,
        results.operations.latency_ms.p99
    );
    
    Ok(())
}
