use std::error::Error;
use std::str::FromStr;
use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc,
};
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::Duration;

use deadpool::Runtime;
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use postgres_openssl::MakeTlsConnector;
use tokio_postgres::{Config as PgConfig, NoTls};

use crate::metrics::MetricsCollector;
use crate::output::BenchmarkResults;
use crate::{BenchmarkConfig, WorkloadMode};

#[derive(Debug, thiserror::Error)]
enum WorkloadError {
    #[error(transparent)]
    Pool(#[from] deadpool_postgres::PoolError),
    #[error(transparent)]
    Postgres(#[from] tokio_postgres::Error),
    #[error("document {0} was not found")]
    DocumentNotFound(i64),
    #[error("document table name must contain only ASCII letters, numbers, and underscores: {0}")]
    InvalidTableName(String),
}

pub struct BenchmarkRunner {
    config: BenchmarkConfig,
    metrics: Arc<Mutex<MetricsCollector>>,
}

struct BenchmarkPools {
    primary: Pool,
    timeout: Pool,
    prune_task: JoinHandle<()>,
}

impl BenchmarkPools {
    fn new(config: &BenchmarkConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let pg_config = PgConfig::from_str(&config.database_url)?;

        let primary = Self::build_pool(pg_config.clone(), RecyclingMethod::Fast, config)?;
        let timeout = Self::build_pool(pg_config, RecyclingMethod::Clean, config)?;

        let primary_copy = primary.clone();
        let timeout_copy = timeout.clone();
        let prune_interval = config.connection_pruning_interval;
        let idle_lifetime = config.connection_idle_lifetime;
        let lifetime = config.connection_lifetime;
        let timeout_idle_lifetime = config.postgres_command_timeout;

        let prune_task = tokio::spawn(async move {
            let mut prune_interval = tokio::time::interval(prune_interval);

            loop {
                prune_interval.tick().await;

                primary_copy.retain(|_, conn_metrics| {
                    conn_metrics.last_used() < idle_lifetime && conn_metrics.age() < lifetime
                });

                timeout_copy.retain(|_, conn_metrics| {
                    conn_metrics.last_used() < timeout_idle_lifetime
                        && conn_metrics.age() < lifetime
                });
            }
        });

        Ok(Self {
            primary,
            timeout,
            prune_task,
        })
    }

    fn build_pool(
        pg_config: PgConfig,
        recycling_method: RecyclingMethod,
        config: &BenchmarkConfig,
    ) -> Result<Pool, Box<dyn std::error::Error>> {
        let manager_config = ManagerConfig { recycling_method };
        let manager = if config.require_tls {
            let mut tls_builder = SslConnector::builder(SslMethod::tls())?;
            tls_builder.set_verify(SslVerifyMode::NONE);
            let tls = MakeTlsConnector::new(tls_builder.build());
            Manager::from_config(pg_config, tls, manager_config)
        } else {
            Manager::from_config(pg_config, NoTls, manager_config)
        };

        Ok(Pool::builder(manager)
            .runtime(Runtime::Tokio1)
            .max_size(config.pool_size)
            .wait_timeout(Some(config.postgres_command_timeout))
            .build()?)
    }

    fn benchmark_pool(&self, use_timeout_pool: bool) -> Pool {
        if use_timeout_pool {
            self.timeout.clone()
        } else {
            self.primary.clone()
        }
    }
}

impl Drop for BenchmarkPools {
    fn drop(&mut self) {
        self.prune_task.abort();
    }
}

impl BenchmarkRunner {
    pub fn new(config: BenchmarkConfig, metrics: Arc<Mutex<MetricsCollector>>) -> Self {
        Self { config, metrics }
    }

    pub async fn run(self) -> Result<BenchmarkResults, Box<dyn std::error::Error>> {
        let start_time = chrono::Utc::now();
        let pools = BenchmarkPools::new(&self.config)?;
        let table_name = self.quoted_table_name()?;

        if self.config.workload_mode != WorkloadMode::PoolOnly {
            self.prepare_workload(&pools, &table_name).await?;
        }

        // Warmup phase
        if self.config.warmup.as_secs() > 0 {
            println!("Warming up for {}s...", self.config.warmup.as_secs());
            self.run_phase(&pools, &table_name, self.config.warmup, false)
                .await?;
        }

        // Actual benchmark phase
        println!(
            "Running benchmark for {}s...",
            self.config.run_time.as_secs()
        );
        {
            let mut metrics = self.metrics.lock().await;
            metrics.start();
        }

        self.run_phase(&pools, &table_name, self.config.run_time, true)
            .await?;

        {
            let mut metrics = self.metrics.lock().await;
            metrics.stop();
        }

        let end_time = chrono::Utc::now();

        // Collect and format results
        let metrics = self.metrics.lock().await;
        let results = BenchmarkResults::from_metrics(&self.config, &metrics, start_time, end_time);

        Ok(results)
    }

    async fn run_phase(
        &self,
        pools: &BenchmarkPools,
        table_name: &str,
        duration: Duration,
        record: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let start = Instant::now();
        let mut handles = vec![];
        let next_document_id = Arc::new(AtomicI64::new(1));
        let read_sql = Arc::new(format!("SELECT document FROM {table_name} WHERE id = $1"));

        // Spawn worker tasks
        for worker_id in 0..self.config.workers {
            let metrics = self.metrics.clone();
            let pool = pools.benchmark_pool(self.config.use_timeout_pool);
            let end_time = start + duration;
            let document_count = self.config.document_count;
            let workload_mode = self.config.workload_mode;
            let next_document_id = Arc::clone(&next_document_id);
            let read_sql = Arc::clone(&read_sql);

            let handle = tokio::spawn(async move {
                Self::worker_loop(
                    worker_id,
                    metrics,
                    pool,
                    end_time,
                    record,
                    document_count,
                    workload_mode,
                    next_document_id,
                    read_sql,
                )
                .await;
            });

            handles.push(handle);
        }

        // Wait for all workers to complete
        for handle in handles {
            handle.await?;
        }

        Ok(())
    }

    async fn prepare_workload(
        &self,
        pools: &BenchmarkPools,
        table_name: &str,
    ) -> Result<(), WorkloadError> {
        let client = pools.benchmark_pool(false).get().await?;
        let create_table_sql = format!(
            "CREATE TABLE IF NOT EXISTS {table_name} (id BIGINT PRIMARY KEY, document JSONB NOT NULL)"
        );
        client.batch_execute(&create_table_sql).await?;

        let max_id_sql = format!("SELECT COALESCE(MAX(id), 0)::BIGINT FROM {table_name}");
        let row = client.query_one(&max_id_sql, &[]).await?;
        let mut max_id = row.get::<_, i64>(0);

        if max_id >= self.config.document_count {
            println!(
                "Using existing {} documents from {}",
                self.config.document_count, self.config.document_table
            );
        } else {
            println!(
                "Loading documents into {}: existing max id {}, target {}",
                self.config.document_table, max_id, self.config.document_count
            );

            while max_id < self.config.document_count {
                let start_id = max_id + 1;
                let end_id =
                    (start_id + self.config.load_batch_size - 1).min(self.config.document_count);
                let insert_sql = format!(
                    r#"INSERT INTO {table_name} (id, document)
SELECT series.id,
       jsonb_build_object(
           'id', series.id,
           'partition_key', 'pk-' || (series.id % 1024),
           'name', 'document-' || series.id,
           'payload', repeat(md5(series.id::TEXT), 8)
       )
FROM generate_series($1::BIGINT, $2::BIGINT) AS series(id)
ON CONFLICT (id) DO NOTHING"#
                );

                client.execute(&insert_sql, &[&start_id, &end_id]).await?;
                max_id = end_id;
                println!("Loaded documents through id {max_id}");
            }
        }

        let analyze_sql = format!("ANALYZE {table_name}");
        client.batch_execute(&analyze_sql).await?;
        Ok(())
    }

    async fn worker_loop(
        _worker_id: usize,
        metrics: Arc<Mutex<MetricsCollector>>,
        pool: Pool,
        end_time: Instant,
        record: bool,
        document_count: i64,
        workload_mode: WorkloadMode,
        next_document_id: Arc<AtomicI64>,
        read_sql: Arc<String>,
    ) {
        let pinned_connection = match workload_mode {
            WorkloadMode::PinnedPointRead => match pool.get().await {
                Ok(connection) => Some(connection),
                Err(error) => {
                    if record {
                        let mut metrics = metrics.lock().await;
                        metrics.record_failure(Self::format_workload_error(&error.into()));
                    }
                    return;
                }
            },
            WorkloadMode::PointRead | WorkloadMode::PoolOnly => None,
        };

        while Instant::now() < end_time {
            let op_start = Instant::now();

            let document_id = Self::next_document_id(&next_document_id, document_count);
            let result = match workload_mode {
                WorkloadMode::PointRead => {
                    Self::point_read_operation(&pool, &read_sql, document_id).await
                }
                WorkloadMode::PinnedPointRead => {
                    Self::point_read_on_connection(
                        pinned_connection
                            .as_ref()
                            .expect("pinned connection is initialized"),
                        &read_sql,
                        document_id,
                    )
                    .await
                }
                WorkloadMode::PoolOnly => Self::pool_only_operation(&pool).await,
            };

            let latency = op_start.elapsed();

            if record {
                let mut metrics = metrics.lock().await;
                match result {
                    Ok(_) => metrics.record_success(latency),
                    Err(error) => metrics.record_failure(Self::format_workload_error(&error)),
                }
            }
        }
    }

    async fn point_read_operation(
        pool: &Pool,
        read_sql: &str,
        document_id: i64,
    ) -> Result<(), WorkloadError> {
        let connection = pool.get().await?;
        Self::point_read_on_connection(&connection, read_sql, document_id).await
    }

    async fn point_read_on_connection(
        connection: &deadpool_postgres::Client,
        read_sql: &str,
        document_id: i64,
    ) -> Result<(), WorkloadError> {
        let statement = connection.prepare_cached(read_sql).await?;
        let row = connection.query_opt(&statement, &[&document_id]).await?;

        match row {
            Some(row) => {
                let _document = row.get::<_, serde_json::Value>(0);
                Ok(())
            }
            None => Err(WorkloadError::DocumentNotFound(document_id)),
        }
    }

    async fn pool_only_operation(pool: &Pool) -> Result<(), WorkloadError> {
        let _connection = pool.get().await?;
        Ok(())
    }

    fn next_document_id(next_document_id: &AtomicI64, document_count: i64) -> i64 {
        let ordinal = next_document_id.fetch_add(1, Ordering::Relaxed);
        ((ordinal - 1) % document_count) + 1
    }

    fn quoted_table_name(&self) -> Result<String, WorkloadError> {
        let table_name = self.config.document_table.as_str();
        let is_valid = !table_name.is_empty()
            && table_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');

        if !is_valid {
            return Err(WorkloadError::InvalidTableName(table_name.to_string()));
        }

        Ok(format!("\"{table_name}\""))
    }

    fn format_workload_error(error: &WorkloadError) -> String {
        let mut message = format!("{error}; debug: {error:?}");
        let mut source = error.source();

        while let Some(error_source) = source {
            message.push_str("; caused by: ");
            message.push_str(&error_source.to_string());
            source = error_source.source();
        }

        message
    }
}
