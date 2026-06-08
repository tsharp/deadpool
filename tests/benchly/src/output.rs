use std::collections::HashMap;
use std::fs;
use std::path::Path;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub total_documents: usize,
    pub total_failures: usize,
    pub operations_per_sec: f64,
    pub documents_per_sec: f64,
    pub failures_per_sec: f64,
    pub latency_ms: LatencyStats,
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
        let duration_seconds = metrics.duration()
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);

        let total_ops = metrics.total_operations();
        let ops_per_sec = metrics.operations_per_sec();

        Self {
            metadata: BenchmarkMetadata {
                benchmark_name: config.benchmark_name.clone(),
                run_label: config.run_label.clone(),
                workers: config.workers,
                run_time: format!("{}s", config.run_time.as_secs()),
                warmup: format!("{}s", config.warmup.as_secs()),
                workload_params: None, // Can be extended for specific workloads
                start_time,
                end_time,
                duration_seconds,
            },
            operations: OperationStats {
                total_operations: total_ops,
                total_documents: total_ops, // Assuming 1 operation = 1 document
                total_failures: metrics.total_failures(),
                operations_per_sec: ops_per_sec,
                documents_per_sec: ops_per_sec,
                failures_per_sec: metrics.failures_per_sec(),
                latency_ms: metrics.calculate_latency_stats(),
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
