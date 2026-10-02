// Needs a headless view: `cargo test -p rustkit-engine --features headless`.
#[cfg(all(test, feature = "headless"))]
mod background_image_fetch_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};

    /// A 2x2 red PNG.
    const DOT_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x08, 0x02, 0x00, 0x00, 0x00, 0xfd,
        0xd4, 0x9a, 0x73, 0x00, 0x00, 0x00, 0x10, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x3c,
        0xc1, 0x00, 0x02, 0x4c, 0x60, 0x92, 0x01, 0x00, 0x0a, 0x52, 0x00, 0xcc, 0x82, 0xce, 0x0a,
        0x79, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    const PAGE: &str = r#"<html><head><style>
div { width: 50px; height: 20px; }
.shown { background: url(/shown.png) no-repeat; }
.again { background-image: url(/shown.png); }
.cross { background-image: url(http://localhost:PORT/cross.png); }
.blocked { background-image: url(/tracker.png); }
.hidden { display: none; background-image: url(/hidden.png); }
</style></head><body><div class="shown"></div><div class="again"></div><div class="cross"></div>
<div class="blocked"></div><div class="hidden"></div></body></html>"#;

    type Seen = Arc<Mutex<Vec<(String, Option<String>)>>>;

    /// Serve `/page` as [`PAGE`], anything else as a PNG, and record each
    /// image request's path and Referer.
    fn recording_server() -> (u16, Seen) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen: Seen = Arc::default();
        let log = seen.clone();
        let html = PAGE.replace("PORT", &port.to_string());
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
                let referer = head
                    .lines()
                    .filter_map(|l| l.split_once(':'))
                    .find(|(k, _)| k.eq_ignore_ascii_case("referer"))
                    .map(|(_, v)| v.trim().to_string());
                let (ctype, body): (&str, &[u8]) = if path.starts_with("/page") {
                    ("text/html", html.as_bytes())
                } else {
                    log.lock().unwrap().push((path, referer));
                    ("image/png", DOT_PNG)
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(body);
            }
        });
        (port, seen)
    }

    /// Only `<img>` was fetched: a `background: url(...)` over http never
    /// left the display list, so no page's CSS image painted. It goes out as
    /// an `<img>` does: through the loader, past the shield, with the
    /// policy's Referer.
    #[test]
    fn a_css_background_image_is_fetched() {
        let (port, seen) = recording_server();
        let mut shield = rustkit_net::RequestInterceptor::new();
        shield.block(rustkit_net::intercept::UrlPattern::contains("tracker"));
        let mut engine =
            Engine::with_interceptor(EngineConfig::default(), Some(shield)).expect("engine");
        let view = engine
            .create_headless_view(Bounds { x: 0, y: 0, width: 200, height: 100 })
            .expect("view");
        let url = Url::parse(&format!("http://127.0.0.1:{port}/page?q=1#frag")).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            engine.load_url(view, url).await.expect("load_url");
            engine.load_subresources(view).await.expect("subresources");
        });

        let mut seen = seen.lock().unwrap().clone();
        seen.sort();
        assert_eq!(
            seen,
            vec![
                ("/cross.png".to_string(), Some(format!("http://127.0.0.1:{port}/"))),
                ("/shown.png".to_string(), Some(format!("http://127.0.0.1:{port}/page?q=1"))),
            ],
            "one request per background the shield allows, each with the policy's Referer; \
             none for the blocked one, none for a box that is not rendered"
        );

        let at = |path: &str| Url::parse(&format!("http://127.0.0.1:{port}{path}")).unwrap();
        assert!(
            engine.image_manager.is_cached(&at("/shown.png")),
            "the background image is in the cache paint reads"
        );
        assert!(!engine.is_image_cached(&at("/tracker.png")));
    }
}
