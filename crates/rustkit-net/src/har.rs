//! QB-ARCHIVE-INPUT: a recording of what the loader fetched, as HAR 1.2, so a real-site load can
//! be replayed offline (the loopback HAR replay server, `--replay-proxy`) and a lost board point
//! bisected without loading the live site again.
//!
//! Off unless [`start_recording`] was called (parity-capture `--dump-har`); the loader then hands
//! every response it answers to [`record`]. Bodies are kept as the page received them (decoded:
//! `Content-Encoding`, `Content-Length` and `Transfer-Encoding` are left out of the recorded
//! headers). `Cookie`, `Set-Cookie` and `Authorization` values are never recorded. A memory-cache
//! hit and a request already sent to a replay proxy are not recorded. Each redirect hop is an
//! entry of its own with its status and a `Location` naming the next URL, so a replay of the first
//! URL follows the same chain.

use std::sync::Mutex;
use std::time::{Instant, SystemTime};

use base64::Engine as _;
use http::HeaderMap;
use serde_json::{json, Value};

use crate::{NetError, Request, Response};

struct Recording {
    entries: Vec<Value>,
}

static RECORDING: Mutex<Option<Recording>> = Mutex::new(None);

/// Header names never written to a recording.
const SECRET_HEADERS: &[&str] = &["cookie", "set-cookie", "authorization", "proxy-authorization"];
/// Headers that describe the transfer, not the decoded body the recording keeps.
const TRANSFER_HEADERS: &[&str] = &["content-encoding", "content-length", "transfer-encoding"];

/// Start recording this process's fetches (a recording already running is restarted).
pub fn start_recording() {
    *RECORDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(Recording { entries: Vec::new() });
}

/// Whether a recording is running.
pub fn recording() -> bool {
    RECORDING.lock().map(|r| r.is_some()).unwrap_or(false)
}

/// The recording so far as a HAR 1.2 document, or `None` when none was started. Recording goes on.
pub fn snapshot() -> Option<Value> {
    let guard = RECORDING.lock().unwrap_or_else(|e| e.into_inner());
    let recording = guard.as_ref()?;
    Some(json!({
        "log": {
            "version": "1.2",
            "creator": {"name": "rustkit-net", "version": env!("CARGO_PKG_VERSION")},
            "entries": recording.entries.clone(),
        }
    }))
}

fn headers_json(headers: &HeaderMap, drop_transfer: bool) -> Vec<Value> {
    headers
        .iter()
        .filter(|(name, _)| {
            let name = name.as_str();
            !SECRET_HEADERS.contains(&name) && !(drop_transfer && TRANSFER_HEADERS.contains(&name))
        })
        .map(|(name, value)| {
            json!({"name": name.as_str(), "value": String::from_utf8_lossy(value.as_bytes())})
        })
        .collect()
}

/// Now as an ISO 8601 UTC date with milliseconds (HAR `startedDateTime`).
fn started_iso() -> String {
    let ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let (h, m, s, milli) = (rem / 3_600_000, rem / 60_000 % 60, rem / 1000 % 60, rem % 1000);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}.{milli:03}Z")
}

fn entry(method: &str, url: &str, request_headers: &[Value], status: u16, headers: Vec<Value>, content: Value, ms: f64) -> Value {
    json!({
        "startedDateTime": started_iso(),
        "time": ms,
        "request": {
            "method": method,
            "url": url,
            "httpVersion": "HTTP/1.1",
            "headers": request_headers,
            "queryString": [],
            "cookies": [],
            "headersSize": -1,
            "bodySize": -1,
        },
        "response": {
            "status": status,
            "statusText": "",
            "httpVersion": "HTTP/1.1",
            "headers": headers,
            "cookies": [],
            "content": content,
            "redirectURL": "",
            "headersSize": -1,
            "bodySize": -1,
        },
        "cache": {},
        "timings": {"send": 0, "wait": ms, "receive": 0},
    })
}

/// Whether bytes of this MIME type are kept as text in the recording.
fn is_text(mime: &str, body: &[u8]) -> bool {
    let mime = mime.to_ascii_lowercase();
    let textual = mime.starts_with("text/")
        || mime.contains("json")
        || mime.contains("javascript")
        || mime.contains("xml")
        || mime.contains("css")
        || mime.contains("svg");
    textual && std::str::from_utf8(body).is_ok()
}

/// Record one answered fetch (called by the loader; does nothing while no recording runs).
pub(crate) fn record(request: &Request, result: &Result<Response, NetError>, began: Instant) {
    let Ok(response) = result else { return };
    if response.from_cache || request.is_replay_proxied {
        return;
    }
    let mut guard = RECORDING.lock().unwrap_or_else(|e| e.into_inner());
    let Some(recording) = guard.as_mut() else { return };
    let ms = began.elapsed().as_secs_f64() * 1000.0;
    let method = request.method.as_str().to_string();
    let request_headers = headers_json(&request.headers, false);
    // Each redirect hop: its URL, its status, and where it sent the request.
    let chain = &response.redirect_chain;
    for (i, (from, status)) in chain.iter().enumerate() {
        let next = chain.get(i + 1).map_or_else(|| response.url.to_string(), |(u, _)| u.to_string());
        let headers = vec![json!({"name": "location", "value": next})];
        let content = json!({"size": 0, "mimeType": ""});
        recording
            .entries
            .push(entry(&method, from.as_str(), &request_headers, status.as_u16(), headers, content, 0.0));
    }
    let mime = response
        .content_type
        .as_ref()
        .map(|m| m.to_string())
        .unwrap_or_default();
    let body = response.full_body().map(|b| b.to_vec()).unwrap_or_default();
    let mut content = json!({"size": body.len(), "mimeType": mime});
    if !body.is_empty() {
        if is_text(&mime, &body) {
            content["text"] = Value::String(String::from_utf8_lossy(&body).into_owned());
        } else {
            content["text"] = Value::String(base64::engine::general_purpose::STANDARD.encode(&body));
            content["encoding"] = Value::String("base64".into());
        }
    }
    let url = if chain.is_empty() { request.url.as_str() } else { response.url.as_str() };
    recording.entries.push(entry(
        &method,
        url,
        &request_headers,
        response.status.as_u16(),
        headers_json(&response.headers, true),
        content,
        ms,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{HeaderValue, StatusCode};

    fn response(url: &str, status: u16, mime: &str, body: &[u8], chain: Vec<(&str, u16)>) -> Response {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_str(mime).unwrap());
        headers.insert("set-cookie", HeaderValue::from_static("sid=secret"));
        headers.insert("content-encoding", HeaderValue::from_static("gzip"));
        headers.insert("x-kept", HeaderValue::from_static("yes"));
        Response::for_test(
            url::Url::parse(url).unwrap(),
            StatusCode::from_u16(status).unwrap(),
            headers,
            mime.parse().ok(),
            bytes::Bytes::copy_from_slice(body),
            chain
                .into_iter()
                .map(|(u, s)| (url::Url::parse(u).unwrap(), StatusCode::from_u16(s).unwrap()))
                .collect(),
        )
    }

    /// One test: the recording is process-wide.
    #[test]
    fn a_recording_keeps_bodies_and_redirect_hops_and_no_secrets() {
        let mut request = Request::get(url::Url::parse("http://a.test/start").unwrap());
        request.headers.insert("cookie", HeaderValue::from_static("sid=secret"));
        request.headers.insert("accept", HeaderValue::from_static("text/html"));
        // Not recording: nothing kept.
        record(&request, &Ok(response("http://a.test/start", 200, "text/html", b"x", vec![])), Instant::now());
        assert!(snapshot().is_none());

        start_recording();
        let page = response(
            "http://a.test/final",
            200,
            "text/html; charset=utf-8",
            b"<p>hi</p>",
            vec![("http://a.test/start", 301), ("http://a.test/mid", 302)],
        );
        record(&request, &Ok(page), Instant::now());
        let png = Request::get(url::Url::parse("http://a.test/i.png").unwrap());
        record(&png, &Ok(response("http://a.test/i.png", 200, "image/png", &[0, 159, 146, 150], vec![])), Instant::now());
        record(&png, &Err(NetError::Cancelled), Instant::now());

        let har = snapshot().unwrap();
        let entries = har["log"]["entries"].as_array().unwrap();
        let got: Vec<(String, u64)> = entries
            .iter()
            .map(|e| (e["request"]["url"].as_str().unwrap().to_string(), e["response"]["status"].as_u64().unwrap()))
            .collect();
        assert_eq!(
            got,
            [
                ("http://a.test/start".to_string(), 301),
                ("http://a.test/mid".to_string(), 302),
                ("http://a.test/final".to_string(), 200),
                ("http://a.test/i.png".to_string(), 200),
            ]
        );
        assert_eq!(entries[0]["response"]["headers"][0]["value"], "http://a.test/mid");
        assert_eq!(entries[1]["response"]["headers"][0]["value"], "http://a.test/final");
        assert_eq!(entries[2]["response"]["content"]["text"], "<p>hi</p>");
        assert_eq!(entries[3]["response"]["content"]["encoding"], "base64");
        assert_eq!(entries[3]["response"]["content"]["text"], "AJ+Slg==");
        let text = har.to_string();
        assert!(!text.contains("secret"), "no cookie value is recorded");
        assert!(!text.to_ascii_lowercase().contains("content-encoding"));
        assert!(text.contains("x-kept"));
        let date = entries[0]["startedDateTime"].as_str().unwrap();
        assert!(date.len() == 24 && date.ends_with('Z') && date.as_bytes()[10] == b'T', "{date}");
        *RECORDING.lock().unwrap() = None;
    }
}
