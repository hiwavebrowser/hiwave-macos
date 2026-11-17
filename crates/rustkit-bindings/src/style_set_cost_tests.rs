//! NATIVE-SETPROPERTY: what one inline style write costs, counted in VM opcodes (not time). MotionMark's
//! Multiply writes `el.style.transform` and two more properties per tile per frame; #1261 measured one
//! `el.style.transform =` at 223 us, almost all of it CSSStyleDeclaration's setProperty interpreted in
//! JavaScript (~11,800 opcodes: the attribute parsed, the value checked, the list written back as text and
//! put through `setAttribute`). The write should be one native call: a JS call costs ~139 ns, so a write
//! of a few dozen opcodes around it is a few microseconds (the row's target: under 5 us).
use super::*;

fn grab_total(rep: &str) -> u64 {
    for l in rep.lines() {
        if let Some(r) = l.strip_prefix("vm stats: ") {
            return r.split_whitespace().next().unwrap().parse().unwrap();
        }
    }
    panic!("no vm stats line in {rep}");
}

/// Opcodes one execution of `op` costs, net of the loop with `base` in it (what builds the value).
fn per_op(op: &str, base: &str) -> f64 {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(
        Document::parse_html(
            "<html><body><div id=t style='left: 1px; top: 2px; background-color: red'></div></body></html>",
        )
        .unwrap(),
    ))
    .unwrap();
    b.evaluate("var el = document.getElementById('t'), s = el.style; 'ok'").unwrap();
    let iters = 1000u64;
    let run = |body: &str| -> u64 {
        let before = grab_total(&rustkit_js::vm_stats_report());
        b.evaluate(&format!("(function(){{ for (var i = 0; i < {iters}; i++) {{ {body} }} return 'ok' }})()")).unwrap();
        grab_total(&rustkit_js::vm_stats_report()) - before
    };
    let empty = run(base);
    let cost = run(op);
    (cost as f64 - empty as f64) / iters as f64
}

/// (name, script, the loop body without the write, budget in opcodes per write). Every write changes
/// the value (`i` differs each time), so none is the unchanged-declaration no-op.
const OPS: &[(&str, &str, &str, f64)] = &[
    ("s.transform = v", "s.transform = 'rotate(' + i + 'deg)'", "var v = 'rotate(' + i + 'deg)'", 150.0),
    ("s.backgroundColor = v", "s.backgroundColor = 'hsl(' + i + ',50%,50%)'", "var v = 'hsl(' + i + ',50%,50%)'", 150.0),
    ("s.setProperty(n, v)", "s.setProperty('transform', 'rotate(' + i + 'deg)')", "var v = 'rotate(' + i + 'deg)'", 150.0),
    ("s.setProperty(n, v, 'important')", "s.setProperty('opacity', '0.' + i, 'important')", "var v = '0.' + i", 150.0),
    ("s.removeProperty then set", "s.removeProperty('left'); s.left = i + 'px'", "var v = i + 'px'", 300.0),
];

/// The measuring half, run in a child process (the VM's counter switch is read once per process).
#[test]
#[ignore]
fn style_set_cost_child() {
    for (name, op, base, _) in OPS {
        println!("COST|{name}|{:.1}", per_op(op, base));
    }
}

#[test]
fn an_inline_style_write_is_one_native_call() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "style_set_cost_tests::style_set_cost_child", "--ignored", "--nocapture", "--test-threads=1"])
        .env("RUSTKIT_VM_STATS", "1")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let mut over = Vec::new();
    let mut seen = 0;
    for l in text.lines().filter_map(|l| l.find("COST|").map(|i| &l[i + 5..])) {
        let mut p = l.splitn(2, '|');
        let name = p.next().unwrap();
        let n: f64 = p.next().unwrap().parse().unwrap();
        let budget = OPS.iter().find(|o| o.0 == name).unwrap().3;
        println!("COST {name:<36} {n:>9.1} (budget {budget})");
        seen += 1;
        if n > budget {
            over.push(format!("{name}: {n:.1} > {budget}"));
        }
    }
    assert_eq!(seen, OPS.len(), "the child measured every write:\n{text}");
    assert!(over.is_empty(), "inline style writes over budget (opcodes per write): {over:?}");
}
