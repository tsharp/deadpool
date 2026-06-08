use serde::{Deserialize, Serialize};
use std::time::Duration;

mod metrics;
mod output;
mod runner;
use runner::BenchmarkRunner;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadMode {
    PointRead,
    PinnedPointRead,
    PoolOnly,
}

impl WorkloadMode {
    fn from_env() -> Self {
        match std::env::var("WORKLOAD_MODE").as_deref() {
            Ok("pinned_point_read") => Self::PinnedPointRead,
            Ok("pool_only") => Self::PoolOnly,
            _ => Self::PointRead,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PointRead => "point_read",
            Self::PinnedPointRead => "pinned_point_read",
            Self::PoolOnly => "pool_only",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub benchmark_name: String,
    pub run_label: String,
    pub workers: usize,
    pub run_time: Duration,
    pub warmup: Duration,
    pub database_url: String,
    pub pool_size: usize,
    pub postgres_command_timeout: Duration,
    pub connection_pruning_interval: Duration,
    pub connection_idle_lifetime: Duration,
    pub connection_lifetime: Duration,
    pub use_timeout_pool: bool,
    pub require_tls: bool,
    pub document_count: i64,
    pub load_batch_size: i64,
    pub document_table: String,
    pub workload_mode: WorkloadMode,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        let workers = std::env::var("WORKERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8);

        Self {
            benchmark_name: std::env::var("BENCHMARK_NAME")
                .unwrap_or_else(|_| "deadpool_benchmark".to_string()),
            run_label: std::env::var("RUN_LABEL").unwrap_or_else(|_| "default_run".to_string()),
            workers,
            run_time: Duration::from_secs(
                std::env::var("RUN_TIME_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(90),
            ),
            warmup: Duration::from_secs(
                std::env::var("WARMUP_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(5),
            ),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://localhost/test".to_string()),
            pool_size: std::env::var("POOL_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(workers * 2),
            postgres_command_timeout: Duration::from_secs(
                std::env::var("POSTGRES_COMMAND_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(120),
            ),
            connection_pruning_interval: Duration::from_secs(
                std::env::var("CONNECTION_PRUNING_INTERVAL_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(10),
            ),
            connection_idle_lifetime: Duration::from_secs(
                std::env::var("CONNECTION_IDLE_LIFETIME_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(300),
            ),
            connection_lifetime: Duration::from_secs(
                std::env::var("CONNECTION_LIFETIME_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(3600),
            ),
            use_timeout_pool: std::env::var("USE_TIMEOUT_POOL")
                .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "True"))
                .unwrap_or(false),
            require_tls: std::env::var("REQUIRE_TLS")
                .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "True"))
                .unwrap_or(false),
            document_count: std::env::var("DOCUMENT_COUNT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(250_000),
            load_batch_size: std::env::var("LOAD_BATCH_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10_000),
            document_table: std::env::var("DOCUMENT_TABLE")
                .unwrap_or_else(|_| "benchly_documents".to_string()),
            workload_mode: WorkloadMode::from_env(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration from environment or use defaults
    let config = BenchmarkConfig::default();

    println!("Starting benchmark: {}", config.benchmark_name);
    println!(
        "Workers: {}, Runtime: {}s, Warmup: {}s",
        config.workers,
        config.run_time.as_secs(),
        config.warmup.as_secs()
    );
    println!(
        "Pool size: {}, Wait timeout: {}s, Recycling: {}, Connector: {}",
        config.pool_size,
        config.postgres_command_timeout.as_secs(),
        if config.use_timeout_pool {
            "Clean"
        } else {
            "Fast"
        },
        if config.require_tls { "TLS" } else { "NoTls" }
    );
    println!(
        "Workload: {}, {} documents in table {}, load batch size {}",
        config.workload_mode.as_str(),
        config.document_count,
        config.document_table,
        config.load_batch_size
    );

    // Create and run benchmark
    let runner = BenchmarkRunner::new(config.clone());
    let results = runner.run().await?;

    // Output results to JSON file
    let output_path = format!("tests/benchly/results/{}.json", config.run_label);
    results.save_to_file(&output_path)?;

    println!("\nBenchmark complete!");
    println!("Results saved to: {}", output_path);
    println!("\nSummary:");
    println!(
        "  Total operations: {}",
        results.operations.total_operations
    );
    println!(
        "  Successful operations: {}",
        results.operations.successful_operations
    );
    println!(
        "  Operations/sec: {:.2}",
        results.operations.operations_per_sec
    );
    println!("  Failures: {}", results.operations.total_failures);
    println!("  Avg latency: {:.2}us", results.operations.latency_us.avg);
    println!(
        "  p50: {}us, p95: {}us, p99: {}us",
        results.operations.latency_us.p50,
        results.operations.latency_us.p95,
        results.operations.latency_us.p99
    );

    if !results.operations.failure_causes.is_empty() {
        println!("  Failure causes:");
        for (cause, count) in &results.operations.failure_causes {
            println!("    {count}x {cause}");
        }
    }

    if results.operations.total_operations > 0
        && results.operations.successful_operations == 0
        && results.operations.total_failures == results.operations.total_operations
    {
        return Err("all benchmark operations failed".into());
    }

    Ok(())
}
