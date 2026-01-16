//! # RustKit Bench
//!
//! Performance benchmarking library for the RustKit browser engine.
//!
//! ## Features
//!
//! - HTML parsing benchmarks
//! - CSS parsing benchmarks
//! - Layout calculation benchmarks
//! - JavaScript execution benchmarks
//! - Memory usage tracking
//! - Regression detection
//!
//! ## Usage
//!
//! ```rust,ignore
//! use rustkit_bench::{Benchmark, BenchmarkResult};
//!
//! let bench = Benchmark::new();
//! let results = bench.run_all()?;
//! results.print_summary();
//! ```

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::debug;

/// Benchmark errors.
#[derive(Error, Debug)]
pub enum BenchError {
    #[error("Benchmark failed: {0}")]
    Failed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(String),
}

/// A single benchmark result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    /// Name of the benchmark.
    pub name: String,
    /// Number of iterations.
    pub iterations: u64,
    /// Total time in nanoseconds.
    pub total_ns: u64,
    /// Mean time per iteration in nanoseconds.
    pub mean_ns: u64,
    /// Standard deviation in nanoseconds.
    pub std_dev_ns: u64,
    /// Minimum time in nanoseconds.
    pub min_ns: u64,
    /// Maximum time in nanoseconds.
    pub max_ns: u64,
    /// Throughput in operations per second.
    pub ops_per_sec: f64,
}

impl BenchmarkResult {
    /// Create a new result from sample times.
    pub fn from_samples(name: impl Into<String>, samples: &[Duration]) -> Self {
        let name = name.into();
        let iterations = samples.len() as u64;

        let times_ns: Vec<u64> = samples.iter().map(|d| d.as_nanos() as u64).collect();
        let total_ns: u64 = times_ns.iter().sum();
        let mean_ns = total_ns / iterations;
        let min_ns = *times_ns.iter().min().unwrap_or(&0);
        let max_ns = *times_ns.iter().max().unwrap_or(&0);

        // Calculate standard deviation
        let variance: f64 = times_ns
            .iter()
            .map(|&t| {
                let diff = t as f64 - mean_ns as f64;
                diff * diff
            })
            .sum::<f64>()
            / iterations as f64;
        let std_dev_ns = variance.sqrt() as u64;

        let ops_per_sec = if mean_ns > 0 {
            1_000_000_000.0 / mean_ns as f64
        } else {
            0.0
        };

        Self {
            name,
            iterations,
            total_ns,
            mean_ns,
            std_dev_ns,
            min_ns,
            max_ns,
            ops_per_sec,
        }
    }

    /// Format the mean time as a human-readable string.
    pub fn format_mean(&self) -> String {
        format_duration(self.mean_ns)
    }

    /// Print a summary line.
    pub fn print_line(&self) {
        println!(
            "{:40} {:>12} {:>12} {:>12}/s",
            self.name,
            self.format_mean(),
            format!("±{}", format_duration(self.std_dev_ns)),
            format_ops(self.ops_per_sec),
        );
    }
}

/// Format nanoseconds as human-readable duration.
fn format_duration(ns: u64) -> String {
    if ns >= 1_000_000_000 {
        format!("{:.2} s", ns as f64 / 1_000_000_000.0)
    } else if ns >= 1_000_000 {
        format!("{:.2} ms", ns as f64 / 1_000_000.0)
    } else if ns >= 1_000 {
        format!("{:.2} µs", ns as f64 / 1_000.0)
    } else {
        format!("{} ns", ns)
    }
}

/// Format operations per second.
fn format_ops(ops: f64) -> String {
    if ops >= 1_000_000.0 {
        format!("{:.2}M", ops / 1_000_000.0)
    } else if ops >= 1_000.0 {
        format!("{:.2}K", ops / 1_000.0)
    } else {
        format!("{:.2}", ops)
    }
}

/// Collection of benchmark results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSuite {
    /// Suite name.
    pub name: String,
    /// Individual results.
    pub results: Vec<BenchmarkResult>,
    /// Total time.
    pub total_time: Duration,
}

impl BenchmarkSuite {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            results: Vec::new(),
            total_time: Duration::ZERO,
        }
    }

    pub fn add(&mut self, result: BenchmarkResult) {
        self.results.push(result);
    }

    /// Print summary of all results.
    pub fn print_summary(&self) {
        println!("\n{}", "=".repeat(80));
        println!("Benchmark Suite: {}", self.name);
        println!("{}", "=".repeat(80));
        println!(
            "{:40} {:>12} {:>12} {:>12}",
            "Name", "Mean", "StdDev", "Throughput"
        );
        println!("{}", "-".repeat(80));

        for result in &self.results {
            result.print_line();
        }

        println!("{}", "-".repeat(80));
        println!("Total time: {:?}", self.total_time);
        println!();
    }

    /// Save results to JSON file.
    pub fn save_json(&self, path: &str) -> Result<(), BenchError> {
        let json =
            serde_json::to_string_pretty(self).map_err(|e| BenchError::Failed(e.to_string()))?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

/// Benchmark runner.
pub struct Benchmark {
    /// Number of warmup iterations.
    pub warmup: u64,
    /// Number of measured iterations.
    pub iterations: u64,
}

impl Benchmark {
    pub fn new() -> Self {
        Self {
            warmup: 10,
            iterations: 100,
        }
    }

    pub fn with_iterations(mut self, iterations: u64) -> Self {
        self.iterations = iterations;
        self
    }

    pub fn with_warmup(mut self, warmup: u64) -> Self {
        self.warmup = warmup;
        self
    }

    /// Run a benchmark function.
    pub fn run<F>(&self, name: &str, mut f: F) -> BenchmarkResult
    where
        F: FnMut(),
    {
        debug!(
            name,
            warmup = self.warmup,
            iterations = self.iterations,
            "Running benchmark"
        );

        // Warmup
        for _ in 0..self.warmup {
            f();
        }

        // Measure
        let mut samples = Vec::with_capacity(self.iterations as usize);
        for _ in 0..self.iterations {
            let start = Instant::now();
            f();
            samples.push(start.elapsed());
        }

        BenchmarkResult::from_samples(name, &samples)
    }

    /// Run all standard benchmarks.
    pub fn run_all(&self) -> BenchmarkSuite {
        let start = Instant::now();
        let mut suite = BenchmarkSuite::new("RustKit Engine");

        // HTML parsing benchmarks
        suite.add(self.bench_html_parse_small());
        suite.add(self.bench_html_parse_medium());
        suite.add(self.bench_html_parse_large());

        // CSS parsing benchmarks
        suite.add(self.bench_css_parse_small());
        suite.add(self.bench_css_parse_medium());

        // Layout benchmarks
        suite.add(self.bench_layout_simple());
        suite.add(self.bench_layout_nested());

        suite.total_time = start.elapsed();
        suite
    }

    fn bench_html_parse_small(&self) -> BenchmarkResult {
        let html = r#"<!DOCTYPE html><html><head><title>Test</title></head><body><p>Hello</p></body></html>"#;
        self.run("html/parse/small (84 bytes)", || {
            let _ = rustkit_dom::Document::parse_html(html);
        })
    }

    fn bench_html_parse_medium(&self) -> BenchmarkResult {
        let html = generate_html_doc(100);
        self.run(&format!("html/parse/medium ({} bytes)", html.len()), || {
            let _ = rustkit_dom::Document::parse_html(&html);
        })
    }

    fn bench_html_parse_large(&self) -> BenchmarkResult {
        let html = generate_html_doc(1000);
        self.run(&format!("html/parse/large ({} bytes)", html.len()), || {
            let _ = rustkit_dom::Document::parse_html(&html);
        })
    }

    fn bench_css_parse_small(&self) -> BenchmarkResult {
        let css = "body { margin: 0; padding: 0; } h1 { font-size: 24px; }";
        self.run("css/parse/small (55 bytes)", || {
            let _ = rustkit_css::Stylesheet::parse(css);
        })
    }

    fn bench_css_parse_medium(&self) -> BenchmarkResult {
        let css = generate_css(50);
        self.run(&format!("css/parse/medium ({} bytes)", css.len()), || {
            let _ = rustkit_css::Stylesheet::parse(&css);
        })
    }

    fn bench_layout_simple(&self) -> BenchmarkResult {
        use rustkit_css::ComputedStyle;
        use rustkit_layout::{BoxType, Dimensions, LayoutBox, Rect};

        self.run("layout/simple (1 box)", || {
            let style = ComputedStyle::new();
            let mut root = LayoutBox::new(BoxType::Block, style);
            let containing = Dimensions {
                content: Rect::new(0.0, 0.0, 800.0, 600.0),
                ..Default::default()
            };
            root.layout(&containing);
        })
    }

    fn bench_layout_nested(&self) -> BenchmarkResult {
        use rustkit_css::ComputedStyle;
        use rustkit_layout::{BoxType, Dimensions, LayoutBox, Rect};

        self.run("layout/nested (10 boxes)", || {
            let style = ComputedStyle::new();
            let mut root = LayoutBox::new(BoxType::Block, style.clone());

            // Add nested children
            for _ in 0..10 {
                root.children
                    .push(LayoutBox::new(BoxType::Block, style.clone()));
            }

            let containing = Dimensions {
                content: Rect::new(0.0, 0.0, 800.0, 600.0),
                ..Default::default()
            };
            root.layout(&containing);
        })
    }
}

impl Default for Benchmark {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate an HTML document with n paragraphs.
fn generate_html_doc(n: usize) -> String {
    let mut html = String::from("<!DOCTYPE html><html><head><title>Test</title></head><body>");
    for i in 0..n {
        html.push_str(&format!("<p>Paragraph {}</p>", i));
    }
    html.push_str("</body></html>");
    html
}

/// Generate CSS with n rules.
fn generate_css(n: usize) -> String {
    let mut css = String::new();
    for i in 0..n {
        css.push_str(&format!(
            ".class{} {{ margin: {}px; padding: {}px; }}\n",
            i,
            i,
            i * 2
        ));
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_benchmark_result() {
        let samples = vec![
            Duration::from_micros(100),
            Duration::from_micros(120),
            Duration::from_micros(90),
        ];
        let result = BenchmarkResult::from_samples("test", &samples);
        assert_eq!(result.iterations, 3);
        assert!(result.mean_ns > 0);
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(500), "500 ns");
        assert_eq!(format_duration(1_500), "1.50 µs");
        assert_eq!(format_duration(1_500_000), "1.50 ms");
        assert_eq!(format_duration(1_500_000_000), "1.50 s");
    }

    #[test]
    fn test_generate_html() {
        let html = generate_html_doc(5);
        assert!(html.contains("Paragraph 0"));
        assert!(html.contains("Paragraph 4"));
    }
}
