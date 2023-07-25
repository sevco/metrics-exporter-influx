use crate::data::{InfluxMetric, MetricData};
use crate::distribution::{Distribution, DistributionBuilder};
use crate::exporter::{InfluxExporter, InfluxFileExporter};
use crate::http::{APIVersion, InfluxHttpExporter};
use crate::registry::AtomicStorage;
use crate::BuildError;
use itertools::Itertools;
use metrics::{Counter, Gauge, Histogram, Key, KeyName, Label, Recorder, SharedString, Unit};
use metrics_util::registry::Registry;
use quanta::Instant;
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
    pub global_tags: HashMap<String, String>,
    pub global_fields: HashMap<String, MetricData>,
    // pub distributions: Arc<RwLock<HashMap<String, IndexMap<Vec<(String, String)>, Distribution>>>>,
    pub distribution_builder: DistributionBuilder,
}

pub struct InfluxRecorder {
    inner: Arc<Inner>,
    exporter_config: ExporterConfig,
}

impl InfluxRecorder {
    pub(crate) fn new(inner: Arc<Inner>, exporter_config: ExporterConfig) -> Self {
        Self {
            inner,
            exporter_config,
        }
    }

    pub fn handle(&self) -> InfluxHandle {
        InfluxHandle {
            inner: self.inner.to_owned(),
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

    fn register_histogram(&self, key: &Key) -> Histogram {
        self.inner
            .registry
            .get_or_create_histogram(key, |b| b.to_owned().into())
    }
}

pub struct InfluxHandle {
    inner: Arc<Inner>,
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

        let distributions = self
            .inner
            .registry
            .get_histogram_handles()
            .into_iter()
            .map(|(key, value)| {
                let mut distribution = self.inner.distribution_builder.get_distribution(key.name());
                value.clear_with(|samples| distribution.record_samples(samples));
                (key, distribution)
            })
            .collect_vec();

        let histogram_metrics = distributions.into_iter().flat_map(|(key, dist)| {
            let (tags, fields) = parse_labels(
                self.inner.global_tags.to_owned(),
                self.inner.global_fields.to_owned(),
                key.labels(),
            );
            match dist {
                Distribution::Histogram(histogram) => {
                    let fields = fields
                        .into_iter()
                        .chain([
                            ("sum".to_string(), histogram.sum().into()),
                            ("count".to_string(), histogram.count().into()),
                        ])
                        .chain(
                            histogram
                                .buckets()
                                .into_iter()
                                .map(|(le, count)| (format!("{:.2}", le), count.into())),
                        )
                        .collect();

                    Some(InfluxMetric {
                        name: key.name().to_string(),
                        fields,
                        tags,
                    })
                }
                Distribution::Summary(summary, quantiles, sum) => {
                    if !summary.is_empty() {
                        let snapshot = summary.snapshot(Instant::now());
                        let fields = fields
                            .into_iter()
                            .chain(
                                [
                                    ("sum".to_string(), sum.into()),
                                    ("count".to_string(), summary.count().into()),
                                ]
                                .into_iter(),
                            )
                            .chain(quantiles.iter().map(|quantile| {
                                (
                                    quantile.label().to_string(),
                                    snapshot
                                        .quantile(quantile.value())
                                        .unwrap_or_default()
                                        .into(),
                                )
                            }))
                            .collect();
                        Some(InfluxMetric {
                            name: key.name().to_string(),
                            fields,
                            tags,
                        })
                    } else {
                        None
                    }
                }
            }
        });

        let counter_gauge_metrics = gauges.chain(counters).map(|(key, value)| {
            let (tags, mut fields) = parse_labels(
                self.inner.global_tags.to_owned(),
                self.inner.global_fields.to_owned(),
                key.labels(),
            );
            fields.insert("value".to_string(), value);
            InfluxMetric {
                name: key.name().to_string(),
                fields,
                tags,
            }
        });

        let metrics = counter_gauge_metrics.chain(histogram_metrics).collect_vec();
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
