// https://github.com/metrics-rs/metrics/blob/0193688dac4ca646dbe44620040c20b9abf9bf5e/metrics-exporter-prometheus/src/distribution.rsa
// Copyright (c) 2021 Metrics Contributors
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

/// Matches a metric name in a specific way.
///
/// Used for specifying overrides for buckets, allowing a default set of histogram buckets to be
/// specified while adjusting the buckets that get used for specific metrics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Matcher {
    /// Matches the entire metric name.
    Full(String),
    /// Matches the beginning of the metric name.
    Prefix(String),
    /// Matches the end of the metric name.
    Suffix(String),
}

impl Matcher {
    /// Checks if the given key matches this matcher.
    pub fn matches(&self, key: &str) -> bool {
        match self {
            Matcher::Prefix(prefix) => key.starts_with(prefix),
            Matcher::Suffix(suffix) => key.ends_with(suffix),
            Matcher::Full(full) => key == full,
        }
    }
}
