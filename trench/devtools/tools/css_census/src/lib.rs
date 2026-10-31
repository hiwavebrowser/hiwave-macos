//! CSS property census (dev-tools T13): stubs, so the tests can be seen red.

use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Deserialize)]
pub struct PropSpec {
    pub name: String,
    pub context: String,
    #[serde(default)]
    pub base: String,
    pub values: Vec<String>,
    #[serde(default)]
    pub nonvisual: bool,
    #[serde(default)]
    pub pseudo: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct PageRead {
    pub computed: String,
    pub rects: Vec<(String, [f64; 4])>,
}

#[derive(Debug, Clone)]
pub struct Capture {
    pub read: Option<PageRead>,
    pub frame: Option<Frame>,
}

#[derive(Debug, Clone)]
pub struct Measure {
    pub chrome_value: Capture,
    pub chrome_control: Capture,
    pub rust_value: Capture,
    pub rust_control: Capture,
    pub arm: bool,
    pub nonvisual: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Match,
    Mismatch,
    Unexercised,
    NotVisual,
    NoCapture,
}

#[derive(Debug, Clone)]
pub struct Verdict {
    pub parse: bool,
    pub computed: bool,
    pub effect: Effect,
    pub computed_read_vacuous: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Unsupported,
    Parse,
    Computed,
    Effect_,
}

#[derive(Debug, Clone)]
pub struct ValueRow {
    pub value: String,
    pub parse: bool,
    pub computed: bool,
    pub effect: Effect,
    pub chrome_computed: String,
    pub engine_computed: String,
    pub probes_off: usize,
}

#[derive(Debug, Clone)]
pub struct Row {
    pub name: String,
    pub context: String,
    pub sites: Option<usize>,
    pub corpus: usize,
    pub class: Class,
    pub red: bool,
    pub values: Vec<ValueRow>,
}

#[derive(Debug, Clone)]
pub struct Meta {
    pub date: String,
    pub develop_sha: String,
    pub oracle: String,
    pub sites_loaded: usize,
    pub contexts_off: Vec<(String, usize)>,
}

pub struct TopSites {
    pub loaded: usize,
}

impl TopSites {
    pub fn from_census(_census: &serde_json::Value) -> TopSites {
        TopSites { loaded: 0 }
    }
    pub fn sites(&self, _property: &str) -> Option<usize> {
        None
    }
}

pub fn load_spec(_json: &str) -> Result<Vec<PropSpec>, String> {
    Err("stub".into())
}
pub fn context_exists(_name: &str) -> bool {
    false
}
pub fn fixture_html(_p: &PropSpec, _value: Option<&str>) -> String {
    String::new()
}
pub fn parse_read(_line: &str) -> Option<PageRead> {
    None
}
pub fn read_from_event_log(_log: &str) -> Option<PageRead> {
    None
}
pub fn read_ppm(_bytes: &[u8]) -> Option<Frame> {
    None
}
pub fn probe_diff(_a: &Frame, _b: &Frame) -> usize {
    usize::MAX
}
pub fn judge(_m: &Measure) -> Verdict {
    Verdict { parse: false, computed: false, effect: Effect::NoCapture, computed_read_vacuous: false }
}
pub fn computed_matches(_chrome: &str, _engine: &str) -> bool {
    false
}
pub fn class_of(_v: &[Verdict]) -> Class {
    Class::Unsupported
}
pub fn is_red(_class: Class, _v: &[Verdict]) -> bool {
    false
}
pub fn extract_arms(_src: &str) -> BTreeSet<String> {
    BTreeSet::new()
}
pub fn check_arms(_arms: BTreeSet<String>) -> Result<BTreeSet<String>, String> {
    Err("stub".into())
}
pub fn corpus_count(_html: &str, _property: &str) -> usize {
    0
}
pub fn sort_rows(_rows: &mut [Row]) {}
pub fn render_markdown(_meta: &Meta, _rows: &[Row]) -> String {
    String::new()
}
pub fn render_json(_meta: &Meta, _rows: &[Row]) -> serde_json::Value {
    serde_json::Value::Null
}
pub fn sentinel_sees(_m: &Measure) -> bool {
    false
}
