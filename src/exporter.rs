use crate::recorder::InfluxHandle;
use async_trait::async_trait;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Interval;
use tracing::error;

#[async_trait]
pub trait InfluxExporter: Send + Sync {
    async fn write(&mut self) -> anyhow::Result<()>;
    async fn run(&mut self, mut interval: Interval) -> anyhow::Result<()> {
        // first tick completes immediately, skip it
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(e) = self.write().await {
                error!("failed to write metrics `{e:?}`");
            }
        }
    }
}

pub struct InfluxFileExporter {
    handle: InfluxHandle,
    file: Arc<Mutex<dyn Write + Send + Sync>>,
}

impl InfluxFileExporter {
    pub fn new(handle: InfluxHandle, file: Arc<Mutex<dyn Write + Send + Sync>>) -> Self {
        Self { handle, file }
    }
}

#[async_trait]
impl InfluxExporter for InfluxFileExporter {
    async fn write(&mut self) -> anyhow::Result<()> {
        let (count, metrics) = self.handle.render();
        if count > 0 {
            let mut file = self.file.lock().await;
            file.write_all(metrics.as_bytes())?;
        }
        Ok(())
    }
}
