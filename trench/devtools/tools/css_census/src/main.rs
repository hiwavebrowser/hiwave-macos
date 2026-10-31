//! `css-census`: one command from a clean clone to the report.
//!
//!     cargo run --profile parity -p css-census
//!
//! Builds `parity-capture`, installs the oracle's node modules if they are
//! missing, renders every fixture in both arms, and writes
//! `docs/census/CSS_PROPERTY_CENSUS_<date>.md` and `.json`.
//!
//!     --parity-capture <path>   use this binary instead of building
//!     --out <dir>               where the report goes (default docs/census)
//!     --work <dir>              fixtures and frames (default target/css_census)
//!     --only <a,b>              these properties only; no report is written

use css_census::*;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

const ORACLE: &str = "143.0.7499.4";
const WORKERS: usize = 4;
/// Plainly painting, each in its own context: if one of these does not read as
/// effect, or reads as effect with the declaration taken away, the instrument
/// is at fault and the run stops rather than print a table.
const SENTINELS: [&str; 3] = ["background-color", "width", "color"];

fn sh(cmd: &mut Command) -> Result<String, String> {
    let out = cmd.output().map_err(|e| format!("{cmd:?}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{cmd:?}: {}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// A fixture's file stem: `<property index>-<value index>`, `c` for the control.
fn stem(prop: usize, value: Option<usize>) -> String {
    match value {
        Some(v) => format!("{prop}-{v}"),
        None => format!("{prop}-c"),
    }
}

fn capture_engine(binary: &Path, work: &Path, id: &str) -> bool {
    let html = work.join("fixtures").join(format!("{id}.html"));
    let ppm = work.join("rustkit").join(format!("{id}.ppm"));
    let log = work.join("rustkit").join(format!("{id}.jsonl"));
    let (w, h) = (VIEWPORT.0.to_string(), VIEWPORT.1.to_string());
    let attempt = || {
        Command::new(binary)
            .arg("--html-file")
            .arg(&html)
            .args([
                "--width",
                &w,
                "--height",
                &h,
                "--timeout-ms",
                "20000",
                "--run-inline-scripts",
            ])
            .arg("--dump-frame")
            .arg(&ppm)
            .arg("--event-log")
            .arg(&log)
            .arg("--event-log-console-text")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    // Once more on a failure: a GPU guard can time out under load.
    attempt() || attempt()
}

fn engine_capture(work: &Path, id: &str) -> Capture {
    let dir = work.join("rustkit");
    Capture {
        read: fs::read_to_string(dir.join(format!("{id}.jsonl")))
            .ok()
            .and_then(|l| read_from_event_log(&l)),
        frame: fs::read(dir.join(format!("{id}.ppm")))
            .ok()
            .and_then(|b| read_ppm(&b)),
    }
}

fn chrome_capture(work: &Path, id: &str) -> Capture {
    let dir = work.join("chrome");
    Capture {
        read: fs::read_to_string(dir.join(format!("{id}.txt")))
            .ok()
            .and_then(|l| parse_read(l.trim())),
        frame: fs::read(dir.join(format!("{id}.png")))
            .ok()
            .and_then(|b| read_png(&b)),
    }
}

fn corpus_text(root: &Path) -> String {
    let mut text = String::new();
    for dir in ["websuite/cases", "crates/hiwave-app/src/ui"] {
        let mut stack = vec![root.join(dir)];
        while let Some(d) = stack.pop() {
            let Ok(entries) = fs::read_dir(&d) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "html") {
                    text.push_str(&fs::read_to_string(&p).unwrap_or_default());
                }
            }
        }
    }
    text
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let target = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("target"));
    let work = arg(&args, "--work")
        .map(PathBuf::from)
        .unwrap_or_else(|| target.join("css_census"));
    let out = arg(&args, "--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("docs/census"));
    let only: Option<BTreeSet<String>> =
        arg(&args, "--only").map(|s| s.split(',').map(str::to_string).collect());

    let binary = match arg(&args, "--parity-capture") {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("building parity-capture");
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
            sh(Command::new(cargo).current_dir(&root).args([
                "build",
                "--profile",
                "parity",
                "-p",
                "parity-capture",
            ]))?;
            target.join("parity/parity-capture")
        }
    };
    let oracle_dir = root.join("tools/parity_oracle");
    if !oracle_dir.join("node_modules/playwright").exists() {
        eprintln!("installing the oracle's node modules");
        sh(Command::new("npm")
            .current_dir(&oracle_dir)
            .args(["ci", "--ignore-scripts"]))?;
    }

    let spec_all = load_spec(include_str!("../properties.json"))?;
    let spec: Vec<&PropSpec> = spec_all
        .iter()
        .filter(|p| only.as_ref().is_none_or(|o| o.contains(&p.name)))
        .collect();
    if spec.is_empty() {
        return Err("no properties selected".into());
    }
    let arms = check_arms(extract_arms(
        &fs::read_to_string(root.join("crates/rustkit-engine/src/lib.rs"))
            .map_err(|e| e.to_string())?,
    ))?;

    for dir in ["fixtures", "rustkit", "chrome"] {
        let d = work.join(dir);
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    }
    let mut ids = Vec::new();
    for (i, p) in spec.iter().enumerate() {
        let control = stem(i, None);
        fs::write(
            work.join("fixtures").join(format!("{control}.html")),
            fixture_html(p, None),
        )
        .map_err(|e| e.to_string())?;
        ids.push(control);
        for (v, value) in p.values.iter().enumerate() {
            let id = stem(i, Some(v));
            fs::write(
                work.join("fixtures").join(format!("{id}.html")),
                fixture_html(p, Some(value)),
            )
            .map_err(|e| e.to_string())?;
            ids.push(id);
        }
    }
    eprintln!("{} properties, {} fixtures", spec.len(), ids.len());

    // The oracle: one browser, every fixture, in order.
    let manifest = work.join("manifest.json");
    fs::write(&manifest, serde_json::to_string(&ids).unwrap()).map_err(|e| e.to_string())?;
    let (w, h) = (VIEWPORT.0.to_string(), VIEWPORT.1.to_string());
    let version = sh(Command::new("node")
        .current_dir(&oracle_dir)
        .arg("css_census_chrome.mjs")
        .arg(&work)
        .args([&w, &h]))?;
    if !version.ends_with(ORACLE) {
        return Err(format!("oracle is {version}, the campaign's is {ORACLE}"));
    }

    // The engine: a process per fixture.
    let next = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..WORKERS {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(id) = ids.get(i) else { break };
                if !capture_engine(&binary, &work, id) {
                    failed.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    });
    let failed = failed.into_inner();
    eprintln!("engine captures done, {failed} failed");

    let census: Value = serde_json::from_str(
        &fs::read_to_string(root.join("docs/diagnostics/census-top100.json"))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let top = TopSites::from_census(&census);
    let corpus = corpus_text(&root);

    let mut rows = Vec::new();
    let mut off_by_context: BTreeMap<String, usize> = BTreeMap::new();
    let mut chrome_blanks = Vec::new();
    for (i, p) in spec.iter().enumerate() {
        let chrome_control = chrome_capture(&work, &stem(i, None));
        let rust_control = engine_capture(&work, &stem(i, None));
        // A plain control only: a `base` declaration can miss on its own, and
        // that is that row's business, not the context's.
        if let (Some(a), Some(b), true) = (
            &chrome_control.frame,
            &rust_control.frame,
            p.base.is_empty(),
        ) {
            let off = off_by_context.entry(p.context.clone()).or_default();
            *off = (*off).max(probe_diff(a, b));
        }
        let mut verdicts = Vec::new();
        let mut values = Vec::new();
        for (v, value) in p.values.iter().enumerate() {
            let id = stem(i, Some(v));
            let m = Measure {
                chrome_value: chrome_capture(&work, &id),
                chrome_control: chrome_control.clone(),
                rust_value: engine_capture(&work, &id),
                rust_control: rust_control.clone(),
                arm: arms.contains(&p.name),
                nonvisual: p.nonvisual,
            };
            if m.chrome_value.read.is_none() || m.chrome_value.frame.is_none() {
                chrome_blanks.push(format!("{}: {value}", p.name));
            }
            if v == 0 && SENTINELS.contains(&p.name.as_str()) && !sentinel_sees(&m) {
                return Err(format!("sentinel {}: the census cannot tell it from its control; instrument is blind or the engine lost it, read target frames", p.name));
            }
            let verdict = judge(&m);
            let read = |c: &Capture| {
                c.read
                    .as_ref()
                    .map(|r| r.computed.clone())
                    .unwrap_or_default()
            };
            let off = match (&m.chrome_value.frame, &m.rust_value.frame) {
                (Some(a), Some(b)) => probe_diff(a, b),
                _ => 0,
            };
            values.push(ValueRow {
                value: value.clone(),
                parse: verdict.parse,
                computed: verdict.computed,
                effect: verdict.effect,
                chrome_computed: read(&m.chrome_value),
                engine_computed: read(&m.rust_value),
                probes_off: off,
                geometry_ok: geometry_matches(&m),
            });
            verdicts.push(verdict);
        }
        let class = class_of(&verdicts);
        rows.push(Row {
            name: p.name.clone(),
            context: p.context.clone(),
            sites: top.sites(&p.name),
            corpus: corpus_count(&corpus, &p.name),
            class,
            red: is_red(class, &verdicts),
            values,
        });
    }
    if !chrome_blanks.is_empty() {
        return Err(format!(
            "the oracle gave no answer on: {}",
            chrome_blanks.join("; ")
        ));
    }
    sort_rows(&mut rows);

    let date = sh(Command::new("date").arg("+%F"))?;
    let meta = Meta {
        date: date.clone(),
        // The engine under test: where this branch left develop.
        develop_sha: sh(Command::new("git").current_dir(&root).args([
            "merge-base",
            "HEAD",
            "origin/develop",
        ]))
        .or_else(|_| {
            sh(Command::new("git")
                .current_dir(&root)
                .args(["rev-parse", "HEAD"]))
        })?
        .chars()
        .take(8)
        .collect(),
        oracle: ORACLE.into(),
        sites_loaded: top.loaded,
        contexts_off: off_by_context.into_iter().filter(|(_, n)| *n > 0).collect(),
    };
    let tally = |c: Class| rows.iter().filter(|r| r.class == c).count();
    eprintln!(
        "effect {} computed {} parse {} unsupported {}; red {}",
        tally(Class::Effect_),
        tally(Class::Computed),
        tally(Class::Parse),
        tally(Class::Unsupported),
        rows.iter().filter(|r| r.red).count()
    );
    if only.is_some() {
        println!(
            "{}",
            serde_json::to_string_pretty(&render_json(&meta, &rows)).unwrap()
        );
        return Ok(());
    }
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let base = out.join(format!("CSS_PROPERTY_CENSUS_{date}"));
    fs::write(base.with_extension("md"), render_markdown(&meta, &rows))
        .map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&render_json(&meta, &rows)).unwrap() + "\n";
    fs::write(base.with_extension("json"), text).map_err(|e| e.to_string())?;
    println!("{}", base.with_extension("md").display());
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("css-census: {e}");
        std::process::exit(1);
    }
}
