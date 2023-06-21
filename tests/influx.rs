use httpmock::{Method, MockServer};
use metrics::{counter, gauge};
use metrics_exporter_influx::{InfluxBuilder, MetricData};
use tracing_subscriber::EnvFilter;

#[tokio::test(flavor = "multi_thread")]
async fn write_influx() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let server = MockServer::start();

    let mock = server.mock(|when, then| {
        when.header("authorization", "Token user:password")
            .method(Method::POST)
            .query_param("bucket", "db/rp")
            .query_param("org", "org_id")
            .query_param("precision", "s")
            .body(
                vec![
                    "counter,tag0=value0,tag1=value1,tag2=value2,tag3=value3 field0=false,field1=\"0\",value=2i",
                    "gauge,tag0=value0 field0=false,value=-1000"
                ].join("\n")
            );
        then.status(200);
    });

    let handle = InfluxBuilder::new()
        .with_influx_api(
            format!("http://{}", server.address()).as_str(),
            "db/rp".to_string(),
            Some("user".to_string()),
            Some("password".to_string()),
            Some("org_id".to_string()),
            Some("s".to_string()),
        )?
        .with_gzip(false)
        .add_global_tag("tag0", "value0")
        .add_global_field("field0", MetricData::Boolean(false))
        .install()?;
    counter!(
        "counter",
        2,
        "tag1" => "value1",
        "tag2" => "value2",
        "tag:tag3" => "value3",
        "field:field1" => "0",
    );
    gauge!("gauge", -1000.0);
    handle.close();
    unsafe { metrics::clear_recorder() }

    mock.assert();
    Ok(())
}
