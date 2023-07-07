use crate::data::MetricData;
#[cfg(feature = "http")]
use crate::http::APIVersion;
use crate::recorder::{ExporterConfig, HttpConfig, InfluxRecorder, Inner};
use metrics::SetRecorderError;
use metrics_util::registry::{AtomicStorage, Registry};
use metrics_util::RecoverableRecorder;
#[cfg(feature = "http")]
use reqwest::Url;
use std::collections::HashMap;
use std::fmt::Display;
use std::future::Future;
use std::io::Write;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use std::{io, thread};
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::{runtime, time};

type ExporterFuture = Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send + 'static>>;

pub struct InfluxRecorderHandle {
    inner: Option<RecoverableRecorder<InfluxRecorder>>,
}

impl InfluxRecorderHandle {
    pub fn close(self) {
        drop(self)
    }
}

impl Drop for InfluxRecorderHandle {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            inner.into_inner();
        }
    }
}

#[derive(Debug, Error)]
pub enum BuildError {
    /// An invalid URL was supplied
    #[cfg(feature = "http")]
    #[error("invalid endpoint `{0}`")]
    InvalidEndpoint(String),
    /// There was an error in http communications
    #[cfg(feature = "http")]
    #[error("http error `{0}`")]
    HttpError(#[from] reqwest::Error),
    /// There was an issue when creating the necessary Tokio runtime to launch the exporter.
    #[error("failed to create Tokio runtime for exporter: {0}")]
    FailedToCreateRuntime(String),
    /// Installing the recorder did not succeed.
    #[error("failed to install exporter as global recorder: {0}")]
    FailedToSetGlobalRecorder(#[from] SetRecorderError),
}

pub struct InfluxBuilder {
    pub(crate) exporter_config: ExporterConfig,
    pub(crate) duration: Option<Duration>,
    pub(crate) global_tags: Option<HashMap<String, String>>,
    pub(crate) global_fields: Option<HashMap<String, MetricData>>,
}

impl InfluxBuilder {
    pub fn new() -> Self {
        Self {
            exporter_config: ExporterConfig::File(Arc::new(Mutex::new(io::stderr()))),
            global_tags: None,
            duration: None,
            global_fields: None,
        }
    }

    pub fn add_global_tag<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        if let Some(tags) = &mut self.global_tags {
            tags.insert(key.into(), value.into());
        } else {
            self.global_tags = Some(vec![(key.into(), value.into())].into_iter().collect());
        }
        self
    }

    pub fn add_global_field<K: Into<String>>(mut self, key: K, value: MetricData) -> Self {
        if let Some(fields) = &mut self.global_fields {
            fields.insert(key.into(), value);
        } else {
            self.global_fields = Some(vec![(key.into(), value)].into_iter().collect());
        }
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    #[cfg(feature = "http")]
    pub fn with_influx_api<E>(
        mut self,
        endpoint: E,
        bucket: String,
        username: Option<String>,
        password: Option<String>,
        org: Option<String>,
        precision: Option<String>,
    ) -> Result<Self, BuildError>
    where
        Url: TryFrom<E>,
        <Url as TryFrom<E>>::Error: Display,
    {
        self.exporter_config = ExporterConfig::Http(Arc::new(HttpConfig {
            api_version: APIVersion::Influx {
                bucket,
                precision,
                org,
            },
            gzip: true,
            endpoint: Url::try_from(endpoint)
                .map_err(|e| BuildError::InvalidEndpoint(e.to_string()))?,
            username,
            password,
        }));
        Ok(self)
    }

    #[cfg(feature = "http")]
    pub fn with_gzip(mut self, gzip: bool) -> Self {
        self.exporter_config = match self.exporter_config {
            ExporterConfig::Http(http) => ExporterConfig::Http(Arc::new(HttpConfig {
                gzip,
                ..(*http).to_owned()
            })),
            config => config,
        };
        self
    }

    #[cfg(feature = "http")]
    pub fn with_grafana_cloud_api<E>(
        mut self,
        endpoint: E,
        username: Option<String>,
        password: Option<String>,
    ) -> Result<Self, BuildError>
    where
        Url: TryFrom<E>,
        <Url as TryFrom<E>>::Error: Display,
    {
        self.exporter_config = ExporterConfig::Http(Arc::new(HttpConfig {
            api_version: APIVersion::GrafanaCloud,
            gzip: true,
            endpoint: Url::try_from(endpoint)
                .map_err(|e| BuildError::InvalidEndpoint(e.to_string()))?,
            username,
            password,
        }));
        Ok(self)
    }

    pub fn with_writer<W: Write + Send + Sync + 'static>(mut self, writer: W) -> Self {
        self.exporter_config = ExporterConfig::File(Arc::new(Mutex::new(writer)));
        self
    }

    pub fn build_recorder(self) -> InfluxRecorder {
        InfluxRecorder::new(
            Arc::new(Inner {
                registry: Registry::new(AtomicStorage),
            }),
            self.exporter_config,
            self.global_tags.unwrap_or_default(),
            self.global_fields.unwrap_or_default(),
        )
    }

    pub fn build(self) -> Result<(InfluxRecorder, ExporterFuture), BuildError> {
        let interval = time::interval(self.duration.unwrap_or(Duration::from_secs(10)));
        let recorder = self.build_recorder();
        let mut exporter = recorder.exporter()?;
        let exporter_future = Box::pin(async move { exporter.run(interval).await });
        Ok((recorder, exporter_future))
    }

    pub fn install(self) -> Result<InfluxRecorderHandle, BuildError> {
        let recorder = if let Ok(handle) = runtime::Handle::try_current() {
            let (recorder, exporter) = {
                let _g = handle.enter();
                self.build()?
            };
            handle.spawn(exporter);
            recorder
        } else {
            let thread_name = format!(
                "metrics-exporter-influx-{}",
                self.exporter_config.as_type_str()
            );

            let runtime = runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| BuildError::FailedToCreateRuntime(e.to_string()))?;

            let (recorder, exporter) = {
                let _g = runtime.enter();
                self.build()?
            };

            thread::Builder::new()
                .name(thread_name)
                .spawn(move || runtime.block_on(exporter))
                .map_err(|e| BuildError::FailedToCreateRuntime(e.to_string()))?;

            recorder
        };

        Ok(InfluxRecorderHandle {
            inner: Some(RecoverableRecorder::from_recorder(recorder)?),
        })
    }
}

impl Default for InfluxBuilder {
    fn default() -> Self {
        InfluxBuilder::new()
    }
}
