use itertools::Itertools;
use metrics::{counter, gauge, histogram};
use metrics_exporter_influx::InfluxBuilder;
use std::io::{Read, Seek};
use tempfile::tempfile;

#[tokio::test]
async fn write_file() -> anyhow::Result<()> {
    let mut temp = tempfile()?;
    let handle = InfluxBuilder::new()
        .with_writer(temp.try_clone()?)
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

    for i in 0..100 {
        histogram!("histogram", i as f64);
    }

    handle.close();
    unsafe { metrics::clear_recorder() }

    // read results into string
    let mut results = String::new();
    temp.rewind()?;
    temp.read_to_string(&mut results)?;

    assert_eq!(
        results.lines().sorted().collect_vec(),
        vec![
            "counter,tag1=value1,tag2=value2,tag3=value3 field1=\"0\",value=2i",
            "gauge value=-1000",
            "histogram count=100i,max=99,min=0,p50=49.00390593892515,p90=89.00566416071958,p95=94.00049142147152,p99=97.99338832106014,p999=97.99338832106014,sum=4950"
        ]
    );
    Ok(())
}
