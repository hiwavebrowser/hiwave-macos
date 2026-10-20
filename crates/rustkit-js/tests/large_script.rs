//! What a 10.9 MB script costs Boa: parse and run timed apart, and whether
//! the interrupt word can stop it while it parses. A measurement, not a
//! gate: run with
//! `cargo test -p rustkit-js --release --test large_script -- --ignored --nocapture`.
//! The bundle is synthetic, of the shape an app bundle has (many small
//! module closures run once at the top level); youtube's own bundle is
//! measured only through Handtest.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use boa_engine::{Context, Script, Source};

/// youtube's app bundle, 10.9 MB.
const BUNDLE_BYTES: usize = 10_900_000;

/// About 300 bytes of module closure, `i` making every name unique.
fn chunk(i: usize) -> String {
    format!(
        "var m{i}=(function(){{function f{i}(a,b){{var c=a+b*2;if(c>10){{return[c,\"s{i}\"].join(\"-\")}}\
         return{{k:c,s:\"x{i}\"}}}}var o={{a:1,b:[1,2,3],c:function(x){{return f{i}(x,2)}},\
         d:\"some string literal of moderate length {i}\"}};return{{f:f{i},o:o}}}})();\n"
    )
}

fn bundle() -> String {
    let mut out = String::with_capacity(BUNDLE_BYTES + 512);
    let mut i = 0;
    while out.len() < BUNDLE_BYTES {
        out.push_str(&chunk(i));
        i += 1;
    }
    out
}

#[test]
#[ignore = "measurement; run in --release with --nocapture"]
fn a_ten_megabyte_bundle_parse_and_run_times() {
    let source = bundle();
    for run in 0..3 {
        let mut context = Context::default();
        let started = Instant::now();
        let script = Script::parse(Source::from_bytes(&source), None, &mut context).expect("parses");
        let parsed = started.elapsed();
        let compile_at = Instant::now();
        script.codeblock(&mut context).expect("compiles");
        let compiled = compile_at.elapsed();
        let ran_at = Instant::now();
        script.evaluate(&mut context).expect("runs");
        let ran = ran_at.elapsed();
        println!(
            "large_script run={run} bytes={} parse_ms={} compile_ms={} run_ms={} total_ms={}",
            source.len(),
            parsed.as_millis(),
            compiled.as_millis(),
            ran.as_millis(),
            started.elapsed().as_millis()
        );
    }
}

/// The interrupt word set 100 ms into the parse: does the parse end then,
/// or only once it is done and the first instructions run?
#[test]
#[ignore = "measurement; run in --release with --nocapture"]
fn the_interrupt_word_during_the_parse() {
    let source = bundle();
    let word = Arc::new(AtomicU64::new(0));
    let mut context = Context::default();
    context.set_interrupt_word(Some(word.clone()));
    context.set_interrupt_turn(1);
    let setter = {
        let word = word.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            word.fetch_max(1, Ordering::Relaxed);
            Instant::now()
        })
    };
    let started = Instant::now();
    let parsed = Script::parse(Source::from_bytes(&source), None, &mut context);
    let parse_ended = Instant::now();
    let script = parsed.expect("parses");
    script.codeblock(&mut context).expect("compiles");
    let compile_ended = Instant::now();
    let result = script.evaluate(&mut context);
    let ended = Instant::now();
    let set_at = setter.join().unwrap();
    println!(
        "large_script interrupt: parse_ms={} compile_ms={} stopped={} ended_after_set_ms={}",
        parse_ended.duration_since(started).as_millis(),
        compile_ended.duration_since(parse_ended).as_millis(),
        result.is_err(),
        ended.saturating_duration_since(set_at).as_millis()
    );
}
