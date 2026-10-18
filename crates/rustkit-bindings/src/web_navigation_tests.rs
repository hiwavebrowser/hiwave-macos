//! Navigation API, WebIDL accessor placement, and the history.state polyfill cycle.
//!
//! `history.state` is an accessor on `History.prototype`. Calling the saved
//! getter does not re-enter `history.state`. The Navigation polyfill's own
//! wrapper still loops, because the saved getter returns `null` and `??`
//! reads `history.state` again. `window.navigation` keeps that polyfill from
//! applying. The loop in the test stops itself after a few turns.

use super::*;
use std::sync::{Mutex, OnceLock};

const PAGE: &str = r#"<!DOCTYPE html><html><body>
<a id="rel" href="../page?x=1#frag">rel</a>
</body></html>"#;

fn bound() -> DomBindings {
    let bindings = DomBindings::new(JsRuntime::new().unwrap()).unwrap();
    bindings
        .set_document(Rc::new(Document::parse_html(PAGE).unwrap()))
        .unwrap();
    bindings
        .set_location(&Url::parse("https://site.test/dir/index.html?a=1#top").unwrap())
        .unwrap();
    bindings
}

fn ev(bindings: &DomBindings, script: &str) -> String {
    match bindings.evaluate(script) {
        Ok(JsValue::String(s)) => s,
        Ok(JsValue::Boolean(b)) => b.to_string(),
        Ok(JsValue::Number(n)) => n.to_string(),
        Ok(JsValue::Null) => "null".to_string(),
        Ok(JsValue::Undefined) => "undefined".to_string(),
        Ok(other) => panic!("{script} evaluated to {other:?}"),
        Err(err) => panic!("{script} failed: {err}"),
    }
}

fn host_lines() -> &'static Mutex<Vec<String>> {
    static LINES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
    LINES.get_or_init(|| Mutex::new(Vec::new()))
}

fn host_hook(depth: usize, histogram: &str, frames: &str) {
    host_lines().lock().unwrap().push(format!(
        "depth={depth} histogram={histogram} frames={frames}"
    ));
}

struct Disarm;
impl Drop for Disarm {
    fn drop(&mut self) {
        rustkit_js::set_host_stack_armed(false);
        rustkit_js::set_host_stack_diag(None);
        rustkit_js::set_host_stack_first_mark(None);
    }
}

#[test]
fn history_state_is_a_prototype_accessor_and_the_saved_getter_does_not_recurse() {
    let b = bound();
    assert_eq!(
        ev(
            &b,
            "var d = Object.getOwnPropertyDescriptor(History.prototype, 'state'); \
             var r = [typeof d.get, d.configurable, d.enumerable, d.value === undefined, \
                      Object.getOwnPropertyDescriptor(history, 'state') === undefined]; \
             r.push(d.get.call(history) === null); \
             r.push(Reflect.get(History.prototype, 'state', history) === null); \
             try { d.get.call({}); r.push('no throw'); } catch (e) { r.push(e instanceof TypeError); } \
             try { Reflect.get(History.prototype, 'state', {}); r.push('no throw'); } \
             catch (e) { r.push(e instanceof TypeError); } \
             var n = 0, orig = d.get; \
             Object.defineProperty(history, 'state', { configurable: true, get: function () { n++; return orig.call(history); } }); \
             history.state; history.state; \
             history.pushState({ a: 1 }, ''); \
             r.push(n, history.state.a, history.state === history.state); \
             r.join(',')"
        ),
        "function,true,true,true,true,true,true,true,true,2,1,true"
    );
}

#[test]
fn location_and_window_location_are_unforgeable_own_accessors() {
    let b = bound();
    assert_eq!(
        ev(
            &b,
            "var href = Object.getOwnPropertyDescriptor(location, 'href'); \
             var win = Object.getOwnPropertyDescriptor(window, 'location'); \
             var proto = Object.getOwnPropertyDescriptor(Location.prototype, 'href'); \
             var r = [href.configurable, href.enumerable, typeof href.get, typeof href.set, \
                      win.configurable, typeof win.get, proto === undefined]; \
             r.push(href.get.call(location) === location.href); \
             try { href.get.call({}); r.push('no throw'); } catch (e) { r.push(e instanceof TypeError); } \
             try { Object.defineProperty(location, 'href', { get: function () { return 'x'; } }); r.push('redefined'); } \
             catch (e) { r.push(e instanceof TypeError); } \
             try { Object.defineProperty(window, 'location', { get: function () { return null; } }); r.push('redefined'); } \
             catch (e) { r.push(e instanceof TypeError); } \
             r.push(Object.getOwnPropertyDescriptor(location, 'origin').set === undefined); \
             r.push(Object.getOwnPropertyDescriptor(location, 'assign').configurable); \
             r.join(',')"
        ),
        "false,true,function,function,false,function,true,true,true,true,true,true,false"
    );

    ev(&b, "location.pathname = location.pathname; 0");
    assert_eq!(b.take_navigation_requests(), Vec::<String>::new());
    ev(&b, "location.pathname = '/elsewhere'; 0");
    assert_eq!(
        b.take_navigation_requests(),
        ["https://site.test/elsewhere?a=1#top"]
    );
    assert_eq!(
        ev(&b, "location.href"),
        "https://site.test/dir/index.html?a=1#top"
    );
}

#[test]
fn the_polyfill_state_wrapper_alternates_get_and_get_state_until_it_is_capped() {
    let _disarm = Disarm;
    let b = bound();
    host_lines().lock().unwrap().clear();
    rustkit_js::set_host_stack_first_mark(Some(4));
    rustkit_js::set_host_stack_diag(Some(host_hook));
    rustkit_js::set_host_stack_armed(true);
    assert_eq!(
        ev(
            &b,
            "var orig = Object.getOwnPropertyDescriptor(History.prototype, 'state').get; \
             var steps = 0; \
             var poly = { get state() { \
               steps++; \
               if (steps > 6) return 'capped'; \
               var saved = history.originalState; \
               return saved == null ? history.state : saved; \
             } }; \
             Object.defineProperty(history, 'originalState', { \
               configurable: true, get: function () { return orig.call(history); } \
             }); \
             Object.defineProperty(history, 'state', { \
               configurable: true, get: function get() { return poly.state; } \
             }); \
             var result = history.state; \
             result + ':' + steps"
        ),
        "capped:7"
    );
    let logged = host_lines().lock().unwrap().clone();
    let line = logged
        .iter()
        .find(|line| line.contains("get state [state]"))
        .unwrap_or_else(|| panic!("no get-state frame in {logged:?}"));
    assert!(line.contains("histogram="), "{line}");
    assert!(line.contains("get="), "{line}");
    assert!(line.contains("get state="), "{line}");
    assert!(line.contains("get [state]"), "{line}");
    // The saved getter is invoked with `.call`, so it has no property key and
    // does not re-enter `history.state`. The wrapper and `get state` do.
    assert!(line.contains("get History.state @"), "{line}");
    assert!(line.contains(":1:"), "{line}");
}

#[test]
fn a_truthy_window_navigation_keeps_the_virtualstate_polyfill_from_patching_history() {
    let b = bound();
    let source = include_str!("../fixtures/virtualstate-navigation/polyfill-rollup.js");
    let script = format!(
        "var exports = {{}};\n{source}\n\
         var own = Object.getOwnPropertyDescriptor(history, 'state'); \
         [own === undefined, history.state === null, typeof navigation.navigate, \
          navigation instanceof globalThis.Navigation, navigation instanceof globalThis.EventTarget, \
          Object.hasOwn(window, 'navigation'), window.navigation === navigation, \
          typeof exports.setSerializer].join(',')"
    );
    assert_eq!(
        ev(&b, &script),
        "true,true,function,true,true,false,true,function"
    );
}

#[test]
fn navigate_intercept_commits_same_document_and_get_state_is_a_fresh_clone() {
    let b = bound();
    assert_eq!(
        ev(
            &b,
            "var log = []; \
             navigation.addEventListener('navigate', function (e) { \
               log.push('nav:' + e.navigationType + ':' + e.canIntercept + ':' + e.destination.sameDocument + ':' + e.hashChange + ':' + e.isTrusted); \
               if (String(e.destination.url).indexOf('/app') >= 0) { \
                 e.intercept({ handler: function () { log.push('handler:' + location.pathname); e.scroll(); } }); \
               } \
             }); \
             navigation.addEventListener('currententrychange', function (e) { \
               log.push('change:' + e.navigationType + ':' + location.pathname + ':' + (e.from === navigation.currentEntry)); \
             }); \
             navigation.addEventListener('navigatesuccess', function () { log.push('success'); }); \
             var st = { a: 1 }; \
             navigation.navigate('/app', { state: st }); \
             var gs = navigation.currentEntry.getState(); \
             log.push('sync:' + location.pathname + ':' + location.href + ':' + gs.a + ':' + (gs !== st) + ':' + (gs !== navigation.currentEntry.getState())); \
             log.push('hist:' + String(history.state) + ':' + history.length + ':' + navigation.canGoBack + ':' + navigation.canGoForward); \
             log.push('same:' + (navigation.entries() !== navigation.entries()) + ':' + (navigation.entries()[0] === navigation.entries()[0]) + ':' + (navigation.entries()[1] === navigation.currentEntry)); \
             log.join('|')"
        ),
        "nav:push:true:false:false:true|change:push:/app:false|handler:/app|success|\
         sync:/app:https://site.test/app:1:true:true|hist:null:2:true:false|same:true:true:true"
    );
    assert_eq!(b.take_navigation_requests(), Vec::<String>::new());
}

#[test]
fn push_state_updates_classic_state_and_not_navigation_state() {
    let b = bound();
    assert_eq!(
        ev(
            &b,
            "var navs = 0, changes = [], key = navigation.currentEntry.key, id = navigation.currentEntry.id; \
             navigation.addEventListener('navigate', function () { navs++; }); \
             navigation.addEventListener('currententrychange', function (e) { changes.push(String(e.navigationType)); }); \
             navigation.updateCurrentEntry({ state: { a: 1 } }); \
             var g1 = navigation.currentEntry.getState(); \
             history.pushState({ p: 1 }, '', '/one'); \
             history.replaceState({ c: 9 }, ''); \
             [navs, changes.join(','), history.state.c, String(navigation.currentEntry.getState()), \
              g1.a, navigation.currentEntry.getState() === g1, \
              navigation.currentEntry.key === key, navigation.currentEntry.id === id, \
              history.length, location.pathname].join('|')"
        ),
        "0|null,push,replace|9|undefined|1|false|false|false|2|/one"
    );
}

#[test]
fn update_current_entry_null_type_and_fragment_copies_navigation_state() {
    let b = bound();
    assert_eq!(
        ev(
            &b,
            "navigation.updateCurrentEntry({ state: 5 }); \
             var before = navigation.currentEntry.getState(); \
             location.href = '#zz'; \
             var after = navigation.currentEntry.getState(); \
             [before === 5, before === navigation.currentEntry.getState(), after === 5, \
              location.hash, history.length, String(history.state)].join(',')"
        ),
        "true,true,true,#zz,2,null"
    );
    assert_eq!(b.take_navigation_requests(), Vec::<String>::new());
}

#[test]
fn early_errors_reject_and_prevent_default_aborts_without_navigating() {
    let b = bound();
    ev(
        &b,
        "window.names = []; \
         navigation.navigate('javascript:alert(1)').committed.catch(function (e) { names.push(e.name); }); \
         navigation.navigate('http://[').finished.catch(function (e) { names.push(e.name); }); \
         navigation.navigate(location.href, { history: 'push' }).committed.catch(function (e) { names.push(e.name); }); \
         navigation.navigate('/x', { state: function () {} }).committed.catch(function (e) { names.push(e.name); }); \
         navigation.addEventListener('navigate', function (e) { e.preventDefault(); }); \
         navigation.navigate('/nope').finished.catch(function (e) { names.push(e.name); }); \
         0",
    );
    assert_eq!(
        ev(&b, "names.join(',')"),
        "NotSupportedError,SyntaxError,NotSupportedError,DataCloneError,AbortError"
    );
    assert_eq!(
        ev(&b, "location.href"),
        "https://site.test/dir/index.html?a=1#top"
    );
    assert_eq!(b.take_navigation_requests(), Vec::<String>::new());
}

#[test]
fn cross_document_navigate_is_a_request_and_intercept_outside_dispatch_throws() {
    let b = bound();
    ev(
        &b,
        "window.err = ''; window.held = null; \
         navigation.addEventListener('navigate', function (e) { \
           held = e; \
           try { e.intercept(); err = 'no'; } catch (ex) { err = ex.name; } \
         }); \
         navigation.navigate('https://evil.test/x'); 0",
    );
    assert_eq!(
        ev(&b, "err + '|' + location.href"),
        "SecurityError|https://site.test/dir/index.html?a=1#top"
    );
    assert_eq!(b.take_navigation_requests(), ["https://evil.test/x"]);
    assert_eq!(
        ev(&b, "try { held.intercept(); 'no'; } catch (e) { e.name; }"),
        "InvalidStateError"
    );
}

#[test]
fn back_forward_and_traverse_to_move_the_same_document_entry() {
    let b = bound();
    ev(
        &b,
        "navigation.addEventListener('navigate', function (e) { \
           if (e.navigationType !== 'traverse') e.intercept({ handler: function () {} }); \
         }); \
         navigation.navigate('/a', { state: { n: 1 } }); \
         navigation.navigate('/b', { state: 'bee' }); \
         window.keyA = navigation.entries()[1].key; \
         window.urlB = location.pathname; 0",
    );
    assert_eq!(
        ev(
            &b,
            "urlB + '|' + navigation.canGoBack + '|' + navigation.currentEntry.getState()"
        ),
        "/b|true|bee"
    );
    ev(&b, "navigation.back(); 0");
    assert_eq!(
        ev(&b, "location.pathname + '|' + navigation.currentEntry.getState().n + '|' + navigation.canGoForward"),
        "/a|1|true"
    );
    ev(&b, "navigation.traverseTo(navigation.entries()[0].key); 0");
    assert_eq!(
        ev(
            &b,
            "location.pathname + '|' + String(navigation.currentEntry.getState())"
        ),
        "/dir/index.html|undefined"
    );
    ev(&b, "navigation.traverseTo(keyA); 0");
    assert_eq!(ev(&b, "location.pathname"), "/a");
    assert_eq!(
        ev(&b, "navigation.traverseTo('missing').committed.catch(function (e) { window.bad = e.name; }); window.bad || ''"),
        ""
    );
    assert_eq!(ev(&b, "bad"), "InvalidStateError");
    // A dropped forward entry reports index -1.
    ev(
        &b,
        "window.dropped = navigation.entries()[2]; navigation.navigate('/c'); 0",
    );
    assert_eq!(
        ev(&b, "dropped.index + '|' + dropped.sameDocument"),
        "-1|false"
    );
}

#[test]
fn reload_without_intercept_requests_the_document_and_intercept_stays() {
    let b = bound();
    ev(&b, "navigation.reload(); 0");
    assert_eq!(
        b.take_navigation_requests_with_replace(),
        [("https://site.test/dir/index.html?a=1#top".to_string(), true)]
    );
    ev(
        &b,
        "navigation.addEventListener('navigate', function (e) { \
           if (e.navigationType === 'reload') e.intercept({ handler: function () { window.reloaded = location.href; } }); \
         }); \
         navigation.reload(); 0",
    );
    assert_eq!(b.take_navigation_requests(), Vec::<String>::new());
    assert_eq!(
        ev(&b, "reloaded"),
        "https://site.test/dir/index.html?a=1#top"
    );
}
