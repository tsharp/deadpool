use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

use crate::metrics::MetricsCollector;
use crate::output::BenchmarkResults;
use crate::BenchmarkConfig;

pub struct BenchmarkRunner {
    config: BenchmarkConfig,
    metrics: Arc<Mutex<MetricsCollector>>,
}

impl BenchmarkRunner {
    pub fn new(config: BenchmarkConfig, metrics: Arc<Mutex<MetricsCollector>>) -> Self {
        Self { config, metrics }
    }

    pub async fn run(self) -> Result<BenchmarkResults, Box<dyn std::error::Error>> {
        let start_time = chrono::Utc::now();

        // Warmup phase
        if self.config.warmup.as_secs() > 0 {
            println!("Warming up for {}s...", self.config.warmup.as_secs());
            self.run_phase(self.config.warmup, false).await?;
        }

        // Actual benchmark phase
        println!("Running benchmark for {}s...", self.config.run_time.as_secs());
        {
            let mut metrics = self.metrics.lock().await;
            metrics.start();
        }
        
        self.run_phase(self.config.run_time, true).await?;
        
        {
            let mut metrics = self.metrics.lock().await;
            metrics.stop();
        }

        let end_time = chrono::Utc::now();

        // Collect and format results
        let metrics = self.metrics.lock().await;
        let results = BenchmarkResults::from_metrics(
            &self.config,
            &metrics,
            start_time,
            end_time,
        );

        Ok(results)
    }

    async fn run_phase(
        &self,
        duration: Duration,
        record: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let start = Instant::now();
        let mut handles = vec![];

        // Spawn worker tasks
        for worker_id in 0..self.config.workers {
            let metrics = self.metrics.clone();
            let end_time = start + duration;

            let handle = tokio::spawn(async move {
                Self::worker_loop(worker_id, metrics, end_time, record).await;
            });

            handles.push(handle);
        }

        // Wait for all workers to complete
        for handle in handles {
            handle.await?;
        }

        Ok(())
    }

    async fn worker_loop(
        _worker_id: usize,
        metrics: Arc<Mutex<MetricsCollector>>,
        end_time: Instant,
        record: bool,
    ) {
        while Instant::now() < end_time {
            let op_start = Instant::now();
            
            // TODO: Replace this with your actual workload
            // Example: pool.get().await, perform database operation, etc.
            let result = Self::simulate_operation().await;
            
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

    /// Simulate an operation - replace this with your actual benchmark workload
    async fn simulate_operation() -> Result<(), ()> {
        // Example: simulate some async work
        sleep(Duration::from_millis(1)).await;
        Ok(())
    }
}
