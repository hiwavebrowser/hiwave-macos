//! Z row S1 (Pete's hand test 11-03, gmail): after a redirect, the document
//! is the redirect target's. Chromium: `www.gmail.com` 301 -> `mail.google.com`
//! 302 -> `accounts.google.com/...`, and `location.href`, `document.baseURI`,
//! the address bar and every relative URL in the page belong to the last hop.
//! The engine fetched the same hops but committed the requested URL, so every
//! relative stylesheet, script and image was asked of the first host (Pete's
//! twelve `www.gmail.com/assets/*.css` 404s).
//!
//! Two loopback servers on different ports are two origins: `/start` on the
//! first redirects to `/app/page` on the second, whose page links
//! `style.css` relatively.

use super::*;
use crate::error_page_script_tests::{engine, serve, text};
use std::sync::atomic::{AtomicU16, Ordering};

static TARGET_PORT: AtomicU16 = AtomicU16::new(0);

const PAGE: &str = r#"<!doctype html><html><head><title>Target</title>
<link rel="stylesheet" href="style.css"></head>
<body><div id="box">box</div><img src="pic.png" width="1" height="1"></body></html>"#;

fn first_host(path: &str, _cookie: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
    match path {
        "/start" => {
            let to: &'static str = Box::leak(
                format!("http://127.0.0.1:{}/app/page", TARGET_PORT.load(Ordering::SeqCst)).into_boxed_str(),
            );
            (302, "text/html", vec![("Location", to)], String::new())
        }
        _ => (404, "text/plain", vec![], "not found on the first host".into()),
    }
}

fn second_host(path: &str, _cookie: &str) -> (u16, &'static str, Vec<(&'static str, &'static str)>, String) {
    match path {
        "/app/page" => (200, "text/html", vec![], PAGE.into()),
        "/app/style.css" => (200, "text/css", vec![], "#box{width:123px;height:45px;display:block}".into()),
        "/app/pic.png" => (404, "text/plain", vec![], String::new()),
        _ => (404, "text/plain", vec![], "not found on the second host".into()),
    }
}

#[test]
fn a_redirected_document_belongs_to_the_redirect_target() {
    let target = serve(second_host);
    TARGET_PORT.store(target.url("/").port().unwrap(), Ordering::SeqCst);
    let first = serve(first_host);
    let (mut engine, view, rt) = engine();
    rt.block_on(engine.load_url(view, first.url("/start"))).expect("load");

    let want = target.url("/app/page").to_string();
    assert_eq!(engine.views[&view].title.as_deref(), Some("Target"), "the target's page is shown");
    assert_eq!(
        engine.views[&view].url.as_ref().map(|u| u.to_string()),
        Some(want.clone()),
        "the view's URL (the address bar) is the last hop's"
    );
    assert_eq!(text(&mut engine, view, "location.href"), format!(r#"String("{want}")"#));
    assert_eq!(text(&mut engine, view, "document.baseURI"), format!(r#"String("{want}")"#));
    assert_eq!(
        text(&mut engine, view, "document.querySelector('link').href"),
        format!(r#"String("{}")"#, target.url("/app/style.css"))
    );

    let first_paths: Vec<String> = first.requests().into_iter().map(|(p, _)| p).collect();
    assert_eq!(first_paths, vec!["/start"], "nothing relative is asked of the first host");
    let second_paths: Vec<String> = target.requests().into_iter().map(|(p, _)| p).collect();
    assert!(
        second_paths.iter().any(|p| p == "/app/style.css"),
        "the stylesheet is fetched from the target: {second_paths:?}"
    );
    assert_eq!(
        text(&mut engine, view, "String(document.getElementById('box').getBoundingClientRect().width)"),
        r#"String("123")"#,
        "and applied"
    );
}
