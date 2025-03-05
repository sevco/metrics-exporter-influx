use chrono::{DateTime, Utc};
use itertools::Itertools;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
pub enum MetricData {
    Float(f64),
    Integer(i64),
    UInteger(u64),
    String(String),
    Boolean(bool),
    Timestamp(DateTime<Utc>),
}

impl From<f32> for MetricData {
    fn from(value: f32) -> Self {
        (value as f64).into()
    }
}

impl From<f64> for MetricData {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<i32> for MetricData {
    fn from(value: i32) -> Self {
        (value as i64).into()
    }
}

impl From<i64> for MetricData {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<usize> for MetricData {
    fn from(value: usize) -> Self {
        (value as u64).into()
    }
}

impl From<u8> for MetricData {
    fn from(value: u8) -> Self {
        (value as u64).into()
    }
}

impl From<u16> for MetricData {
    fn from(value: u16) -> Self {
        (value as u64).into()
    }
}

impl From<u32> for MetricData {
    fn from(value: u32) -> Self {
        (value as u64).into()
    }
}

impl From<u64> for MetricData {
    fn from(value: u64) -> Self {
        Self::UInteger(value)
    }
}

impl From<String> for MetricData {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for MetricData {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<bool> for MetricData {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<DateTime<Utc>> for MetricData {
    fn from(value: DateTime<Utc>) -> Self {
        Self::Timestamp(value)
    }
}

impl Display for MetricData {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Float(f) => f.to_string(),
            Self::Integer(i) => format!("{i}i"),
            // send unsigned as integer, even though the spec says unsigned are supported
            // Grafana cloud does not write these
            Self::UInteger(u) => format!("{u}i"),
            Self::String(s) => {
                format!("\"{}\"", s.replace('"', r#"\""#))
            }
            Self::Boolean(b) => b.to_string(),
            Self::Timestamp(t) => t.timestamp_nanos_opt().unwrap().to_string(),
        };
        f.write_str(&s)
    }
}

pub struct InfluxMetric {
    pub name: String,
    pub timestamp: DateTime<Utc>,
    pub fields: HashMap<String, MetricData>,
    pub tags: HashMap<String, String>,
}

impl Display for InfluxMetric {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let tags = if self.tags.is_empty() {
            None
        } else {
            Some(
                self.tags
                    .iter()
                    .sorted_by_key(|(k, _)| *k)
                    .map(|(k, v)| format!("{}={}", escape_string(k), escape_string(v)))
                    .join(","),
            )
        };
        let fields = if self.fields.is_empty() {
            None
        } else {
            Some(
                self.fields
                    .iter()
                    .sorted_by_key(|(k, _)| *k)
                    .map(|(k, v)| format!("{}={}", escape_string(k), v))
                    .join(","),
            )
        };

        let mut s = escape_string(&self.name);

        if let Some(tags) = tags {
            s.push_str(&format!(",{tags}"));
        }

        if let Some(fields) = fields {
            s.push_str(&format!(" {fields}"));
        }

        s.push_str(&format!(
            " {}",
            self.timestamp.timestamp_nanos_opt().unwrap()
        ));

        f.write_str(&s)
    }
}

fn escape_string(s: &str) -> String {
    s.replace(' ', r#"\ "#)
        .replace(',', r#"\,"#)
        .replace('=', r#"\="#)
}

#[cfg(test)]
mod tests {
    use crate::data::{InfluxMetric, MetricData};
    use chrono::{DateTime, TimeZone, Utc};

    #[test]
    fn format() {
        let now = Utc.with_ymd_and_hms(2020, 1, 1, 1, 1, 1).unwrap();
        let metric = InfluxMetric {
            name: "test =metric".to_string(),
            timestamp: DateTime::from_timestamp(0, 0).unwrap(),
            fields: vec![
                ("float".to_string(), MetricData::Float(1.11)),
                ("\"int\"".to_string(), MetricData::Integer(-100)),
                ("uint".to_string(), MetricData::UInteger(100)),
                (
                    "string".to_string(),
                    MetricData::String(r#""metric", 🚀"#.to_string()),
                ),
                ("bool".to_string(), MetricData::Boolean(false)),
                ("t".to_string(), MetricData::Timestamp(now)),
            ]
            .into_iter()
            .collect(),
            tags: vec![
                ("tag Key1".to_string(), "tag Value1".to_string()),
                ("key".to_string(), "value".to_string()),
            ]
            .into_iter()
            .collect(),
        };

        assert_eq!(
            metric.to_string(),
            r#"test\ \=metric,key=value,tag\ Key1=tag\ Value1 "int"=-100i,bool=false,float=1.11,string="\"metric\", 🚀",t=1577840461000000000,uint=100i 0"#
        );
    }
}
