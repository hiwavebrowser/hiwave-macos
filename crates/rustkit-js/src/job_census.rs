//! One microtask checkpoint's census.
//!
//! `HostJobExecutor::run_jobs` is a checkpoint, not the event loop. This
//! records how many jobs that drain ran, how long it took, and which
//! callbacks Boa actually invoked. A promise job is a Rust closure, so the
//! name comes from `HostCallJobCallback`: the function's compiled name and
//! the script path Boa stored on its code block. Boa does not expose that
//! function's start line on the public code block, so a label is
//! `name @path` when both exist.
//!
//! The counters live in a thread-local because the callback hook does not
//! hold the executor. One JavaScript thread drains one checkpoint at a time.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

/// Distinct callback labels kept for one drain. Past this, further new
/// labels share `other`.
const MAX_LABELS: usize = 64;
/// Labels kept on the finished census, slowest first.
const TOP_SOURCES: usize = 8;
/// Host-binding labels kept on one drain and in the cumulative map.
const MAX_HOST_LABELS: usize = 128;
/// Host bindings reported, slowest first.
const TOP_HOST: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobSource {
    pub label: String,
    pub count: u64,
    pub elapsed_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobCensus {
    pub jobs: u64,
    pub elapsed_ms: u64,
    pub promise: u64,
    pub async_continuations: u64,
    pub generic: u64,
    pub callbacks: u64,
    pub sources: Vec<JobSource>,
    /// Host bindings this drain called, slowest first (at most [`TOP_HOST`]).
    pub host: Vec<JobSource>,
}

impl JobCensus {
    /// One log line: counts, then the slowest callbacks.
    pub fn summary(&self) -> String {
        let top = self
            .sources
            .iter()
            .map(|source| format!("{}×{}/{}ms", source.label, source.count, source.elapsed_ms))
            .collect::<Vec<_>>()
            .join(",");
        let host = self
            .host
            .iter()
            .map(|source| format!("{}×{}/{}ms", source.label, source.count, source.elapsed_ms))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "jobs={} ms={} promise={} async={} generic={} callbacks={} top={}{}",
            self.jobs,
            self.elapsed_ms,
            self.promise,
            self.async_continuations,
            self.generic,
            self.callbacks,
            if top.is_empty() { "-".to_string() } else { top },
            if host.is_empty() {
                String::new()
            } else {
                format!(" host={host}")
            }
        )
    }

    /// How many recorded callbacks have `fragment` in their label.
    pub fn count_labeled(&self, fragment: &str) -> u64 {
        self.sources
            .iter()
            .filter(|source| source.label.contains(fragment))
            .map(|source| source.count)
            .sum()
    }
}

struct Acc {
    callbacks: u64,
    sources: HashMap<String, (u64, u128)>,
    host: HashMap<String, (u64, u128)>,
}

fn note_map(map: &mut HashMap<String, (u64, u128)>, label: String, elapsed: Duration, cap: usize) {
    let key = if map.len() >= cap && !map.contains_key(&label) {
        "other".to_string()
    } else {
        label
    };
    let entry = map.entry(key).or_insert((0, 0));
    entry.0 = entry.0.saturating_add(1);
    entry.1 = entry.1.saturating_add(elapsed.as_nanos());
}

fn ranked(map: &HashMap<String, (u64, u128)>, limit: usize) -> Vec<JobSource> {
    let mut sources: Vec<JobSource> = map
        .iter()
        .map(|(label, (count, nanos))| JobSource {
            label: label.clone(),
            count: *count,
            elapsed_ms: (nanos / 1_000_000) as u64,
        })
        .collect();
    sources.sort_by(|a, b| {
        b.elapsed_ms
            .cmp(&a.elapsed_ms)
            .then(b.count.cmp(&a.count))
            .then(a.label.cmp(&b.label))
    });
    sources.truncate(limit);
    sources
}

thread_local! {
    static OPEN: RefCell<Option<Acc>> = const { RefCell::new(None) };
    static LAST: RefCell<JobCensus> = RefCell::new(JobCensus::default());
    /// Every host call on this thread, including those outside a drain.
    /// A sync script's DOM writes happen before `run_jobs`.
    static HOST: RefCell<HashMap<String, (u64, u128)>> = RefCell::new(HashMap::new());
}

pub(crate) fn begin() {
    OPEN.with(|slot| {
        *slot.borrow_mut() = Some(Acc {
            callbacks: 0,
            sources: HashMap::new(),
            host: HashMap::new(),
        });
    });
}

pub(crate) fn callback_count() -> u64 {
    OPEN.with(|slot| slot.borrow().as_ref().map(|acc| acc.callbacks).unwrap_or(0))
}

/// A promise reaction (or finalizer) invoked a JavaScript function.
pub(crate) fn note_callback(label: String, elapsed: Duration) {
    OPEN.with(|slot| {
        let mut open = slot.borrow_mut();
        let Some(acc) = open.as_mut() else {
            return;
        };
        acc.callbacks = acc.callbacks.saturating_add(1);
        note_map(&mut acc.sources, label, elapsed, MAX_LABELS);
    });
}

/// Time spent in a job that did not enter a JavaScript callback.
pub(crate) fn note_source(label: &str, elapsed: Duration) {
    OPEN.with(|slot| {
        let mut open = slot.borrow_mut();
        let Some(acc) = open.as_mut() else {
            return;
        };
        note_map(&mut acc.sources, label.to_string(), elapsed, MAX_LABELS);
    });
}

/// One call through a host binding. Recorded on the open drain when a
/// checkpoint is running, and always on the thread's cumulative map.
pub fn note_host_call(label: &str, elapsed: Duration) {
    let label = label.to_string();
    HOST.with(|slot| {
        note_map(
            &mut slot.borrow_mut(),
            label.clone(),
            elapsed,
            MAX_HOST_LABELS,
        );
    });
    OPEN.with(|slot| {
        let mut open = slot.borrow_mut();
        let Some(acc) = open.as_mut() else {
            return;
        };
        note_map(&mut acc.host, label, elapsed, MAX_HOST_LABELS);
    });
}

/// Drop the cumulative host-call map. A harness calls this before a run
/// so an earlier script on the same thread is not in the top list.
pub fn reset_host_calls() {
    HOST.with(|slot| slot.borrow_mut().clear());
}

/// The slowest host bindings on this thread since [`reset_host_calls`].
pub fn top_host_calls(limit: usize) -> Vec<JobSource> {
    HOST.with(|slot| ranked(&slot.borrow(), limit))
}

pub(crate) fn finish(
    jobs: u64,
    promise: u64,
    async_continuations: u64,
    generic: u64,
    elapsed: Duration,
) -> JobCensus {
    let acc = OPEN.with(|slot| slot.borrow_mut().take());
    let (callbacks, sources, host) = match acc {
        Some(acc) => (
            acc.callbacks,
            ranked(&acc.sources, TOP_SOURCES),
            ranked(&acc.host, TOP_HOST),
        ),
        None => (0, Vec::new(), Vec::new()),
    };
    let census = JobCensus {
        jobs,
        elapsed_ms: elapsed.as_millis() as u64,
        promise,
        async_continuations,
        generic,
        callbacks,
        sources,
        host,
    };
    LAST.with(|slot| *slot.borrow_mut() = census.clone());
    census
}

/// The checkpoint that most recently finished on this thread.
pub fn last_census() -> JobCensus {
    LAST.with(|slot| slot.borrow().clone())
}

/// [`JobCensus::summary`] of [`last_census`].
pub fn summary() -> String {
    last_census().summary()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_promise_reaction_is_named_in_the_checkpoint() {
        let mut runtime = crate::JsRuntime::new().unwrap();
        runtime
            .evaluate_script("Promise.resolve().then(function reaction() {});")
            .unwrap();
        let census = runtime.last_job_census();
        assert!(census.promise >= 1, "{census:?}");
        assert!(
            census.count_labeled("reaction") >= 1,
            "the reaction's compiled name is on the census: {census:?}"
        );
        assert_eq!(census.count_labeled("timer"), 0, "{census:?}");
    }

    #[test]
    fn an_await_resume_is_named_for_the_function() {
        let mut runtime = crate::JsRuntime::new().unwrap();
        runtime
            .evaluate_script("async function paint() { await 1; } paint();")
            .unwrap();
        let census = runtime.last_job_census();
        assert!(
            census.count_labeled("await paint") >= 1,
            "the resume builtin carries the function name: {census:?}"
        );
        assert_eq!(
            census.count_labeled("native"),
            0,
            "an empty builtin name is no longer the label: {census:?}"
        );
    }

    #[test]
    fn a_host_binding_is_timed_by_its_name() {
        crate::reset_host_calls();
        let mut runtime = crate::JsRuntime::new().unwrap();
        runtime
            .register_host_function(
                "probe",
                1,
                Box::new(|args| match args.first() {
                    Some(crate::JsValue::String(op)) => crate::JsValue::String(op.clone()),
                    _ => crate::JsValue::Undefined,
                }),
            )
            .unwrap();
        runtime
            .evaluate_script("probe('setAttr'); probe('setAttr');")
            .unwrap();
        let top = crate::top_host_calls(5);
        assert!(
            top.iter()
                .any(|source| source.label == "probe" && source.count >= 2),
            "the binding name is the host-call key: {top:?}"
        );
    }
}
