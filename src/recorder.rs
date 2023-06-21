use crate::data::{InfluxMetric, MetricData};
use crate::exporter::{InfluxExporter, InfluxFileExporter};
use crate::http::{APIVersion, InfluxHttpExporter};
use crate::BuildError;
use itertools::Itertools;
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Label, Recorder, SharedString, Unit};
use metrics_util::registry::{AtomicStorage, Registry};
use reqwest::Url;
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;
use tokio::runtime;
use tokio::sync::Mutex;
use tracing::error;

#[derive(Clone)]
pub(crate) enum ExporterConfig {
    #[cfg(feature = "http")]
    Http(Arc<HttpConfig>),
    File(Arc<Mutex<dyn Write + Send + Sync>>),
}

#[cfg(feature = "http")]
#[derive(Clone)]
pub(crate) struct HttpConfig {
    pub(crate) api_version: APIVersion,
    pub(crate) gzip: bool,
    pub(crate) endpoint: Url,
    pub(crate) username: Option<String>,
    pub(crate) password: Option<String>,
}

impl ExporterConfig {
    pub fn as_type_str(&self) -> &str {
        match self {
            Self::Http { .. } => "http",
            Self::File(_) => "file",
        }
    }
}

pub(crate) struct Inner {
    pub registry: Registry<Key, AtomicStorage>,
}

pub struct InfluxRecorder {
    inner: Arc<Inner>,
    global_tags: HashMap<String, String>,
    global_fields: HashMap<String, MetricData>,
    exporter_config: ExporterConfig,
}

impl InfluxRecorder {
    pub(crate) fn new(
        inner: Arc<Inner>,
        exporter_config: ExporterConfig,
        global_tags: HashMap<String, String>,
        global_fields: HashMap<String, MetricData>,
    ) -> Self {
        Self {
            inner,
            global_tags,
            global_fields,
            exporter_config,
        }
    }

    pub fn handle(&self) -> InfluxHandle {
        InfluxHandle {
            inner: self.inner.to_owned(),
            global_tags: self.global_tags.to_owned(),
            global_fields: self.global_fields.to_owned(),
        }
    }

    pub fn exporter(&self) -> Result<Box<dyn InfluxExporter>, BuildError> {
        match &self.exporter_config {
            ExporterConfig::File(f) => Ok(Box::new(InfluxFileExporter::new(
                self.handle(),
                f.to_owned(),
            ))),
            #[cfg(feature = "http")]
            ExporterConfig::Http(http_config) => Ok(Box::new(InfluxHttpExporter::new(
                self.handle(),
                http_config.api_version.to_owned(),
                http_config.gzip,
                http_config.endpoint.to_owned(),
                http_config.username.as_ref(),
                http_config.password.as_ref(),
            )?)),
        }
    }
}

impl Drop for InfluxRecorder {
    fn drop(&mut self) {
        if let Ok(handle) = runtime::Handle::try_current() {
            match self.exporter() {
                Ok(mut exporter) => {
                    let thread_handle = thread::spawn(move || {
                        handle.block_on(async move {
                            if let Err(e) = exporter.write().await {
                                error!("failed to flush metrics on drop `{e}`");
                            }
                        })
                    });
                    if thread_handle.join().is_err() {
                        error!("failed to flush metrics on drop");
                    }
                }
                Err(e) => {
                    error!("failed to flush metrics on drop `{e}`");
                }
            }
        }
    }
}

impl Recorder for InfluxRecorder {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {
        unimplemented!()
    }

    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {
        unimplemented!()
    }

    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {
        unimplemented!()
    }

    fn register_counter(&self, key: &Key) -> Counter {
        self.inner
            .registry
            .get_or_create_counter(key, |c| c.to_owned().into())
    }

    fn register_gauge(&self, key: &Key) -> Gauge {
        self.inner
            .registry
            .get_or_create_gauge(key, |c| c.to_owned().into())
    }

    fn register_histogram(&self, _key: &Key) -> Histogram {
        unimplemented!()
    }
}

pub struct InfluxHandle {
    inner: Arc<Inner>,
    global_tags: HashMap<String, String>,
    global_fields: HashMap<String, MetricData>,
}

impl InfluxHandle {
    pub fn render(&self) -> (usize, String) {
        let gauges = self
            .inner
            .registry
            .get_gauge_handles()
            .into_iter()
            .map(|(key, value)| {
                // value here is really an f64, just stored as u64
                let value = f64::from_bits(value.load(Ordering::Acquire));
                (key, MetricData::from(value))
            });
        let counters = self
            .inner
            .registry
            .get_counter_handles()
            .into_iter()
            .map(|(key, value)| (key, MetricData::from(value.load(Ordering::Acquire))));
        let metrics = gauges
            .chain(counters)
            .map(|(key, value)| {
                let (tags, mut fields) = parse_labels(
                    self.global_tags.to_owned(),
                    self.global_fields.to_owned(),
                    key.labels(),
                );
                fields.insert("value".to_string(), value);
                InfluxMetric {
                    name: key.name().to_string(),
                    fields,
                    tags,
                }
            })
            .collect_vec();
        let count = metrics.len();
        let metrics = metrics
            .into_iter()
            .map(|m| m.to_string())
            .sorted()
            .join("\n");
        (count, metrics)
    }

    pub fn clear(&self) {
        self.inner.registry.clear();
    }
}

fn parse_labels(
    global_tags: HashMap<String, String>,
    global_fields: HashMap<String, MetricData>,
    labels: std::slice::Iter<Label>,
) -> (HashMap<String, String>, HashMap<String, MetricData>) {
    labels.fold(
        (global_tags, global_fields),
        |(mut tags, mut fields), label| {
            let (k, v) = label.to_owned().into_parts();
            if let Some(stripped) = k.strip_prefix("field:") {
                fields.insert(stripped.to_string(), v.to_string().into());
            } else if let Some(stripped) = k.strip_prefix("tag:") {
                tags.insert(stripped.to_string(), v.to_string());
            } else {
                tags.insert(k.to_string(), v.to_string());
            }
            (tags, fields)
        },
    )
}