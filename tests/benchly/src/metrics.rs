use hdrhistogram::Histogram;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyStats {
    pub avg: f64,
    pub min: f64,
    pub max: f64,
    pub p50: u64,
    pub p75: u64,
    pub p90: u64,
    pub p95: u64,
    pub p99: u64,
    pub p999: u64,
    pub p100: u64,
}

pub struct MetricsCollector {
    histogram: Histogram<u64>,
    failures: usize,
    failure_causes: HashMap<String, usize>,
    start_time: Option<Instant>,
    end_time: Option<Instant>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        // Create histogram with max value of 1 hour in microseconds and 3 significant digits.
        let histogram =
            Histogram::<u64>::new_with_max(3_600_000_000, 3).expect("Failed to create histogram");

        Self {
            histogram,
            failures: 0,
            failure_causes: HashMap::new(),
            start_time: None,
            end_time: None,
        }
    }

    pub fn start(&mut self) {
        self.start_time = Some(Instant::now());
    }

    pub fn stop(&mut self) {
        self.end_time = Some(Instant::now());
    }

    pub fn merge(&mut self, other: &Self) {
        self.histogram
            .add(&other.histogram)
            .expect("failed to merge latency histogram");
        self.failures += other.failures;
        for (cause, count) in &other.failure_causes {
            *self.failure_causes.entry(cause.clone()).or_default() += count;
        }
    }

    pub fn record_success(&mut self, latency: Duration) {
        let latency_us = latency.as_micros() as u64;
        // Saturate at max value if latency exceeds histogram range
        self.histogram.saturating_record(latency_us);
    }

    pub fn record_failure(&mut self, cause: impl Into<String>) {
        self.failures += 1;
        *self.failure_causes.entry(cause.into()).or_default() += 1;
    }

    pub fn total_operations(&self) -> usize {
        self.histogram.len() as usize + self.failures
    }

    pub fn successful_operations(&self) -> usize {
        self.histogram.len() as usize
    }

    pub fn failure_causes(&self) -> &HashMap<String, usize> {
        &self.failure_causes
    }

    pub fn total_failures(&self) -> usize {
        self.failures
    }

    pub fn duration(&self) -> Option<Duration> {
        match (self.start_time, self.end_time) {
            (Some(start), Some(end)) => Some(end.duration_since(start)),
            _ => None,
        }
    }

    pub fn calculate_latency_stats(&self) -> LatencyStats {
        if self.histogram.is_empty() {
            return LatencyStats {
                avg: 0.0,
                min: 0.0,
                max: 0.0,
                p50: 0,
                p75: 0,
                p90: 0,
                p95: 0,
                p99: 0,
                p999: 0,
                p100: 0,
            };
        }

        LatencyStats {
            avg: self.histogram.mean(),
            min: self.histogram.min() as f64,
            max: self.histogram.max() as f64,
            p50: self.histogram.value_at_quantile(0.50),
            p75: self.histogram.value_at_quantile(0.75),
            p90: self.histogram.value_at_quantile(0.90),
            p95: self.histogram.value_at_quantile(0.95),
            p99: self.histogram.value_at_quantile(0.99),
            p999: self.histogram.value_at_quantile(0.999),
            p100: self.histogram.max(),
        }
    }

    pub fn operations_per_sec(&self) -> f64 {
        if let Some(duration) = self.duration() {
            let secs = duration.as_secs_f64();
            if secs > 0.0 {
                return self.total_operations() as f64 / secs;
            }
        }
        0.0
    }

    pub fn failures_per_sec(&self) -> f64 {
        if let Some(duration) = self.duration() {
            let secs = duration.as_secs_f64();
            if secs > 0.0 {
                return self.failures as f64 / secs;
            }
        }
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collection() {
        let mut collector = MetricsCollector::new();

        collector.record_success(Duration::from_millis(1));
        collector.record_success(Duration::from_millis(2));
        collector.record_success(Duration::from_millis(5));
        collector.record_failure("connection error");

        assert_eq!(collector.total_operations(), 4);
        assert_eq!(collector.successful_operations(), 3);
        assert_eq!(collector.total_failures(), 1);
        assert_eq!(collector.failure_causes()["connection error"], 1);

        let stats = collector.calculate_latency_stats();
        assert!(stats.min > 0.0);
        assert!(stats.max >= stats.min);
    }
}
