//! The engine reads only own properties of its own objects: a
//! page-defined accessor on a built-in prototype is never invoked by
//! engine code, whatever the engine does next (load, timers, input,
//! focus, the network bridge, canvas, Intl), and the page holds no object
//! it did not create or was not given.

use super::*;

const PAGE: &str = "<!DOCTYPE html><html><head><title>t</title></head><body>\
    <p id=\"p\">text</p><form id=\"f\"><input id=\"i\" value=\"v\"><textarea id=\"t\"></textarea>\
    <button id=\"b\" type=\"button\">b</button></form><a id=\"a\" href=\"#frag\">a</a><img id=\"img\">\
    <canvas id=\"c\"></canvas><div id=\"frag\" draggable=\"true\">d</div></body></html>";

/// Member names of the engine's registries that its scripts fill in later.
const REGISTRY_NAMES: &str = "hasListener hostOf mutation retarget scripts shadowParent windowNamed \
    request take deliver shelve unshelve fail_all waiting queued begin prepare settle";

/// The names this crate's scripts use on the engine's own object, read
/// from the sources, and the registries' member names.
fn engine_names() -> String {
    let mut names = std::collections::BTreeSet::new();
    let mut stack = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if path.is_dir() {
                stack.push(path);
            } else if !name.ends_with("_tests.rs") && (name.ends_with(".rs") || name.ends_with(".js")) {
                let source = std::fs::read_to_string(&path).unwrap();
                for part in source.split("__rk.").skip(1) {
                    let member: String =
                        part.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$').collect();
                    if !member.is_empty() {
                        names.insert(member);
                    }
                }
            }
        }
    }
    names.extend(REGISTRY_NAMES.split_whitespace().map(str::to_string));
    names.into_iter().collect::<Vec<_>>().join(" ")
}

/// The page: an accessor on `Object.prototype` for every name, counting
/// its calls. `mine` holds what the page made or was given; a receiver
/// that is not in it, not a node, an event or a function, is counted as
/// not the page's. A set goes on to define the own property, as a plain
/// assignment would have.
const TRAP: &str = r#"
var calls = 0, notMine = 0, which = {}, inTrap = false;
var mine = new WeakSet([window, document, Object.prototype]);
var defineProperty = Object.defineProperty;
function note(name, kind, o) {
    if (inTrap) return;
    inTrap = true;
    calls++;
    var page = o === null || (typeof o !== 'object') || mine.has(o) || o instanceof Node || o instanceof Event;
    if (!page) notMine++;
    which[name + ':' + kind + (page ? '' : ':not-the-page\'s')] = true;
    inTrap = false;
}
function trap(name) {
    defineProperty(Object.prototype, name, {
        configurable: true,
        get: function () { note(name, 'get', this); return undefined; },
        set: function (v) {
            note(name, 'set', this);
            defineProperty(this, name, { value: v, writable: true, enumerable: true, configurable: true });
        }
    });
}
NAMES.split(/\s+/).forEach(trap);
"#;

/// What the page does itself, through the public surface.
const PAGE_USES: &str = r#"
(function () {
    var $ = function (id) { return document.getElementById(id); };
    function attempt(f) { try { f(); } catch (e) {} }
    attempt(function () { document.hasFocus(); });
    attempt(function () { $('i').focus(); $('i').blur(); $('b').click(); $('a').click(); });
    attempt(function () { $('img').src = 'x.png'; $('img').decode && $('img').decode()['catch'](function () {}); });
    attempt(function () { document.adoptNode(document.createElement('p')); document.importNode($('p'), true); });
    attempt(function () { setTimeout(function () {}, 0); requestAnimationFrame(function () {}); });
    attempt(function () { new MutationObserver(function () {}).observe(document.body, { childList: true }); document.body.appendChild(document.createElement('i')); });
    attempt(function () { var s = document.createElement('script'); s.text = 'void 0'; document.body.appendChild(s); });
    attempt(function () { $('p').attachShadow({ mode: 'open' }).innerHTML = '<b>x</b>'; });
    attempt(function () { window.postMessage('m', '*'); var c = new MessageChannel(); c.port1.onmessage = function () {}; c.port2.postMessage(1); });
    attempt(function () { history.pushState({}, '', '#x'); location.hash = '#frag'; });
    attempt(function () { window.scrollTo(0, 10); $('frag').scrollIntoView(); });
    attempt(function () { window.matchMedia('(min-width: 1px)').addListener(function () {}); });
    attempt(function () { var x = $('c').getContext('2d'); x.fillRect(0, 0, 1, 1); x.measureText('a'); });
    attempt(function () { new Intl.DateTimeFormat('en').format(new Date(0)); new Intl.NumberFormat('en').format(1); });
    attempt(function () { console.log('x', {}); });
    attempt(function () { var x = new XMLHttpRequest(); x.open('GET', 'https://example.test/a'); x.send(); });
    attempt(function () { fetch('https://example.test/b')['catch'](function () {}); });
    attempt(function () { btoa('a'); atob('YQ=='); new TextEncoder().encode('a'); new TextDecoder().decode(new Uint8Array(1)); });
    attempt(function () { URL.revokeObjectURL(URL.createObjectURL(new Blob(['a']))); });
    attempt(function () { new CSSStyleSheet().replaceSync('p{}'); document.adoptedStyleSheets = [new CSSStyleSheet()]; });
    attempt(function () { $('i').dispatchEvent(new Event('change')); $('f').requestSubmit && $('f').requestSubmit(); });
    attempt(function () { window.getSelection().selectAllChildren($('p')); document.execCommand('selectAll'); });
    attempt(function () { $('p').animate([{ opacity: 0 }, { opacity: 1 }], 10); });
    attempt(function () { new IntersectionObserver(function () {}).observe($('p')); new ResizeObserver(function () {}).observe($('p')); });
    attempt(function () { customElements.define('x-a', class extends HTMLElement {}); document.createElement('x-a'); });
})();
"#;

/// How many accessor calls there were, how many with a receiver that is
/// not the page's, and which.
const VERDICT: &str = "calls + ' ' + notMine + ' ' + Object.keys(which).sort().join(',')";

fn text(v: JsValue) -> String {
    match v {
        JsValue::String(s) => s,
        other => format!("{other:?}"),
    }
}

/// Every engine path this crate has, with the traps in place.
fn exercise(b: &DomBindings, doc: &Document, net: bool) {
    let id = |name: &str| doc.get_element_by_id(name).unwrap().id.raw();
    let (input, button, link, para, drag) = (id("i"), id("b"), id("a"), id("p"), id("frag"));
    let mouse = MouseEventBindingData { client_x: 5.0, client_y: 5.0, ..Default::default() };
    let key = KeyboardEventBindingData { key: "a".into(), code: "KeyA".into(), ..Default::default() };
    let _ = b.set_ready_state("interactive");
    let _ = b.fire_lifecycle_event(LifecycleTarget::Document, "DOMContentLoaded");
    let _ = b.set_ready_state("complete");
    let _ = b.fire_lifecycle_event(LifecycleTarget::Window, "load");
    let _ = b.evaluate(PAGE_USES);
    let _ = b.run_jobs();
    let _ = b.run_timers(100, 100);
    let _ = b.advance_timers(50, 100);
    let _ = b.run_timers_for_frame(200, 100, 16);
    let _ = b.advance_timers_for_frame(50, 100, 16);
    let _ = b.clock_now_ms();
    let _ = b.next_timer_delay();
    for kind in ["mousemove", "mousedown", "mouseup", "click"] {
        let _ = b.fire_mouse_event(button, kind, &mouse);
    }
    let _ = b.fire_mouse_event(link, "click", &mouse);
    for kind in ["dragstart", "dragover", "drop", "dragend"] {
        let _ = b.fire_drag_event(drag, kind, &mouse, "move");
    }
    let _ = b.set_view_focused(false);
    let _ = b.set_view_focused(true);
    let _ = b.set_focus(Some(input));
    let _ = b.take_focus_move();
    for kind in ["keydown", "keypress", "keyup"] {
        let _ = b.fire_key_event(Some(input), kind, &key);
    }
    let _ = b.fire_input_event(input, Some("a"));
    let _ = b.user_edit(input, &rustkit_dom::forms::EditCommand::SelectAll);
    let _ = b.edit_key(input, "insert", "x");
    let _ = b.implicit_submit(input);
    let _ = b.set_selection(Some((para, 0, para, 1)));
    let _ = b.take_selection_change();
    let _ = b.set_focus(None);
    let _ = b.wheel_blocks(para);
    let _ = b.fire_wheel_event(para, &mouse, 0.0, 10.0, true);
    b.notify_scrolled();
    b.notify_element_scrolled(para);
    let _ = b.tick_observers();
    let _ = b.fire_media_query_changes();
    let _ = b.fire_script_event(id("img"), "load");
    let _ = b.fire_image_load(id("img"), 1, 1, "https://example.test/x.png");
    let _ = b.fire_image_error(id("img"));
    let _ = b.fire_image_gave_up(id("img"));
    let _ = b.fire_transition_event(para, None, "transitionend", "opacity", 0.1);
    let _ = b.fire_animation_event(para, None, "animationend", "n", 0.1);
    let _ = b.set_current_script(Some(para));
    let _ = b.set_current_script(None);
    let _ = b.set_location(&Url::parse("https://example.test/page#frag").unwrap());
    let _ = b.net_bridge_enabled();
    let requests = b.take_net_requests();
    if net {
        for request in &requests {
            let _ = b.shelve_net_request(request.id);
        }
        let _ = b.unshelve_net_requests();
        if let Some(first) = requests.first() {
            let _ = b.deliver_net_response(first.id, NetDelivery::Error("blocked".into()));
        }
    }
    let _ = b.pending_net_requests();
    let _ = b.fail_pending_net_requests("done");
    let _ = b.run_jobs();
    let _ = b.run_timers(1000, 100);
}

fn accessor_calls(net: bool) -> String {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    let doc = Rc::new(Document::parse_html(PAGE).unwrap());
    b.set_document(doc.clone()).unwrap();
    b.set_location(&Url::parse("https://example.test/page").unwrap()).unwrap();
    if net {
        b.enable_net_bridge().unwrap();
    }
    b.evaluate(&format!("var NAMES = {:?};", engine_names())).unwrap();
    b.evaluate(TRAP).unwrap();
    exercise(&b, &doc, net);
    text(b.evaluate(VERDICT).unwrap())
}

#[test]
fn engine_code_invokes_no_page_defined_accessor() {
    assert_eq!(accessor_calls(true), "0 0 ", "accessor calls, receivers not the page's, which");
}

#[test]
fn engine_code_invokes_no_page_defined_accessor_without_the_net_bridge() {
    assert_eq!(accessor_calls(false), "0 0 ", "accessor calls, receivers not the page's, which");
}

/// The engine's bookkeeping reads (the clock, the network queue) invoke
/// no page-defined accessor either.
#[test]
fn bookkeeping_reads_invoke_no_page_defined_accessor() {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(Document::parse_html(PAGE).unwrap())).unwrap();
    b.evaluate(&format!("var NAMES = {:?};", engine_names())).unwrap();
    b.evaluate(TRAP).unwrap();
    let _ = b.take_net_requests();
    let _ = b.unshelve_net_requests();
    let _ = b.pending_net_requests();
    let _ = b.shelve_net_request(1);
    let _ = b.clock_now_ms();
    let _ = b.next_timer_delay();
    assert_eq!(text(b.evaluate(VERDICT).unwrap()), "0 0 ", "accessor calls, receivers not the page's, which");
}

/// Built-ins replaced by the page, each counting its calls.
const REPLACED: &str = r#"
var x = new XMLHttpRequest(); x.open('GET', 'https://example.test/a'); x.send();
var ran = 0;
function count() { ran++; }
JSON.stringify = function () { ran++; return '[]'; };
Object.keys = function () { ran++; return []; };
Array.prototype.splice = function () { ran++; return []; };
Array.prototype.push = function () { ran++; return 0; };
Array.prototype.toJSON = function () { ran++; return []; };
Object.prototype.toJSON = function () { ran++; return {}; };
Object.defineProperty(Object.prototype, 'stray', { get: count, set: count, enumerable: true, configurable: true });
ran = 0;
"#;

/// The network bridge's bookkeeping (taking the queue, shelving, counting)
/// uses the built-ins of install time, and reads the same answers whatever
/// a page has put in their place.
#[test]
fn net_bookkeeping_calls_nothing_a_page_replaced() {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(Document::parse_html(PAGE).unwrap())).unwrap();
    b.set_location(&Url::parse("https://example.test/page").unwrap()).unwrap();
    b.enable_net_bridge().unwrap();
    b.evaluate(REPLACED).unwrap();
    let requests = b.take_net_requests();
    let waiting = b.pending_net_requests();
    let shelved = requests.first().map(|r| b.shelve_net_request(r.id));
    let back = b.unshelve_net_requests();
    let _ = b.clock_now_ms();
    let _ = b.next_timer_delay();
    assert_eq!(text(b.evaluate("String(ran)").unwrap()), "0", "calls of replaced built-ins");
    assert_eq!(
        (requests.len(), requests.first().map(|r| r.url.clone()), waiting, shelved, back),
        (1, Some("https://example.test/a".to_string()), 1, Some(true), 1),
        "what the bookkeeping read"
    );
}

/// The natives the runtime puts on the host object itself (rustkit-js).
const HOST_NATIVES: &[&str] = &[
    "reportError", "unsupported", "base64Decode", "base64Encode", "binaryDecode", "binaryEncode", "blobGet",
    "blobNumber", "blobPut", "blobRevoke", "takeBytes", "utf8Decode", "utf8Encode",
];

/// Every name this crate's sources read or write on the host object is a
/// reserved slot or a native: a new one is added to `HOST_SLOTS` with it.
#[test]
fn every_host_name_is_reserved() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut missing = std::collections::BTreeSet::new();
    let mut stack = vec![dir];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if name.ends_with("_tests.rs") || !(name.ends_with(".rs") || name.ends_with(".js")) {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            for part in source.split("__rk.").skip(1) {
                let member: String = part.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$').collect();
                if !member.is_empty() && !HOST_SLOTS.contains(&member.as_str()) && !HOST_NATIVES.contains(&member.as_str()) {
                    missing.insert(format!("{member} ({name})"));
                }
            }
        }
    }
    assert!(missing.is_empty(), "host names that are not reserved slots: {missing:?}");
}

/// The host object and what hangs off it inherit from nothing, and every
/// reserved slot is an own property from the start.
#[test]
fn the_host_object_inherits_nothing() {
    let b = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    b.set_document(Rc::new(Document::parse_html(PAGE).unwrap())).unwrap();
    b.enable_net_bridge().unwrap();
    let slots = HOST_SLOTS.iter().chain(HOST_NATIVES).map(|s| format!("{s:?}")).collect::<Vec<_>>().join(",");
    let got = b
        .host_turn(
            &format!(
                "var proto = Object.getPrototypeOf, has = Object.prototype.hasOwnProperty, out = [];
                 if (proto(__rk) !== null) out.push('host');
                 [{slots}].forEach(function (n) {{ if (!has.call(__rk, n)) out.push(n); }});
                 ['domHooks', 'net', 'dragStore'].forEach(function (n) {{ if (proto(__rk[n]) !== null) out.push(n); }});
                 ['shadowParent', 'retarget', 'hostOf', 'mutation', 'scripts', 'windowNamed', 'hasListener'].forEach(function (n) {{
                     if (!has.call(__rk.domHooks, n)) out.push('domHooks.' + n);
                 }});
                 return out.join(' ');"
            ),
            &[],
        )
        .unwrap();
    assert_eq!(text(got), "");
}
