# metrics-exporter-influx


![build-badge][] [![downloads-badge][] ![release-badge][]][crate] [![docs-badge][]][docs] [![license-badge][]](#license)

[build-badge]: https://img.shields.io/github/actions/workflow/status/sevco/metrics-exporter-influx/ci.yml?branch=main
[downloads-badge]: https://img.shields.io/crates/d/metrics-exporter-influx.svg
[release-badge]: https://img.shields.io/crates/v/metrics-exporter-influx.svg
[license-badge]: https://img.shields.io/crates/l/metrics-exporter-influx.svg
[docs-badge]: https://docs.rs/metrics-exporter-influx/badge.svg
[crate]: https://crates.io/crates/metrics-exporter-influx
[docs]: https://docs.rs/metrics-exporter-influx


### Metrics reporter for https://github.com/metrics-rs/metrics that writes to InfluxDB.

## Usage

### Configuration

### Writing to a stderr

```rust
use std::time::Duration;

#[tokio::main]
async fn main() {
    InfluxBuilder::new().with_duration(Duration::from_secs(60)).install()?;
}
```

### Writing to a file
```rust
use std::fs::File;

#[tokio::main]
async fn main() {
    InfluxBuilder::new()
        .with_writer(File::create("/tmp/out.metrics")?)
        .install()?;
}

```

### Writing to http

#### Influx

```rust
#[tokio::main]
async fn main() {
    InfluxBuilder::new()
        .with_influx_api(
            "http://localhost:8086",
            "db/rp",
            None,
            None,
            None,
            Some("ns".to_string())
        )
        .install()?;
}
```

#### Grafana Cloud

[Grafana Cloud](https://grafana.com/docs/grafana-cloud/data-configuration/metrics/metrics-influxdb/push-from-telegraf/) 
supports the Influx Line Protocol exported by this exporter.

```rust
#[tokio::main]
async fn main() {
    InfluxBuilder::new()
        .with_grafna_cloud_api(
            "https://https://influx-prod-03-prod-us-central-0.grafana.net/api/v1/push/influx/write",
            Some("username".to_string()),
            Some("key")
        )
        .install()?;
}
```