use crate::exporter::InfluxExporter;
use crate::recorder::InfluxHandle;
use crate::BuildError;
use anyhow::anyhow;
use async_trait::async_trait;
use itertools::Itertools;
use reqwest::{Body, Client, RequestBuilder, Url};
use tracing::{debug, error};

#[derive(Clone)]
pub enum APIVersion {
    Influx {
        bucket: String,
        precision: Option<String>,
        org: Option<String>,
    },
    GrafanaCloud,
}

pub struct InfluxHttpExporter {
    handle: InfluxHandle,
    base: RequestBuilder,
}

impl InfluxHttpExporter {
    pub fn new(
        handle: InfluxHandle,
        api_version: APIVersion,
        gzip: bool,
        endpoint: Url,
        username: Option<&String>,
        password: Option<&String>,
    ) -> Result<Self, BuildError> {
        let client = Client::builder().gzip(gzip).build()?;

        let mut base = client.post(endpoint);
        base = match api_version {
            APIVersion::GrafanaCloud => match (username, password) {
                (Some(u), Some(p)) => base.bearer_auth(format!("{u}:{p}")),
                _ => base,
            },
            APIVersion::Influx {
                bucket,
                precision,
                org,
            } => {
                let query = vec![
                    Some(("bucket", bucket)),
                    precision.map(|p| ("precision", p)),
                    org.map(|o| ("org", o)),
                ]
                .into_iter()
                .flatten()
                .collect_vec();
                match (username, password) {
                    (Some(u), Some(p)) => base
                        .query(&query)
                        .header("authorization", format!("Token {u}:{p}")),
                    _ => base.query(&query),
                }
            }
        };
        Ok(Self { handle, base })
    }
}

#[async_trait]
impl InfluxExporter for InfluxHttpExporter {
    async fn write(&mut self) -> anyhow::Result<()> {
        let (count, body) = self.handle.render();
        if count > 0 {
            debug!("writing {count} metrics over http");
            let resp = self
                .base
                .try_clone()
                .ok_or_else(|| anyhow!("failed to clone base request"))?
                .body(Body::from(body))
                .send()
                .await?;

            match resp.error_for_status_ref() {
                Ok(_) => {
                    let status = resp.status().to_string();
                    let resp = resp.text().await?;
                    debug!(
                        status = status,
                        response = resp,
                        "received response from server"
                    );
                }
                Err(e) => {
                    let status = resp.status().to_string();
                    let resp = resp.text().await?;
                    error!(
                        error = ?e,
                        status = status,
                        response = resp,
                        "failed to write to server"
                    );
                }
            }
            self.handle.clear();
        } else {
            debug!("no metrics to write");
        }
        Ok(())
    }
}
