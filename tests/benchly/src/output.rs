use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::metrics::{LatencyStats, MetricsCollector};
use crate::BenchmarkConfig;

#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkMetadata {
    pub benchmark_name: String,
    pub run_label: String,
    pub workers: usize,
    pub run_time: String,
    pub warmup: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workload_params: Option<HashMap<String, serde_json::Value>>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_seconds: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OperationStats {
    pub total_operations: usize,
    pub successful_operations: usize,
    pub total_documents: usize,
    pub total_failures: usize,
    pub operations_per_sec: f64,
    pub documents_per_sec: f64,
    pub failures_per_sec: f64,
    pub latency_us: LatencyStats,
    pub failure_causes: HashMap<String, usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BenchmarkResults {
    pub metadata: BenchmarkMetadata,
    pub operations: OperationStats,
}

impl BenchmarkResults {
    pub fn from_metrics(
        config: &BenchmarkConfig,
        metrics: &MetricsCollector,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Self {
        let duration_seconds = metrics.duration().map(|d| d.as_secs_f64()).unwrap_or(0.0);

        let total_ops = metrics.total_operations();
        let ops_per_sec = metrics.operations_per_sec();

        Self {
            metadata: BenchmarkMetadata {
                benchmark_name: config.benchmark_name.clone(),
                run_label: config.run_label.clone(),
                workers: config.workers,
                run_time: format!("{}s", config.run_time.as_secs()),
                warmup: format!("{}s", config.warmup.as_secs()),
                workload_params: Some(HashMap::from([
                    ("document_count".to_string(), json!(config.document_count)),
                    ("document_table".to_string(), json!(config.document_table)),
                    ("load_batch_size".to_string(), json!(config.load_batch_size)),
                    (
                        "use_timeout_pool".to_string(),
                        json!(config.use_timeout_pool),
                    ),
                    ("require_tls".to_string(), json!(config.require_tls)),
                    (
                        "workload_mode".to_string(),
                        json!(config.workload_mode.as_str()),
                    ),
                ])),
                start_time,
                end_time,
                duration_seconds,
            },
            operations: OperationStats {
                total_operations: total_ops,
                successful_operations: metrics.successful_operations(),
                total_documents: metrics.successful_operations(),
                total_failures: metrics.total_failures(),
                operations_per_sec: ops_per_sec,
                documents_per_sec: ops_per_sec,
                failures_per_sec: metrics.failures_per_sec(),
                latency_us: metrics.calculate_latency_stats(),
                failure_causes: metrics.failure_causes().clone(),
            },
        }
    }

    pub fn save_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Create directory if it doesn't exist
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}
