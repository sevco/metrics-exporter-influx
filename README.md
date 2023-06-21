# metrics-exporter-influx

![GitHub Workflow Status](https://img.shields.io/github/workflow/status/sevco/metrics-datadog-exporter-rs/CI)

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