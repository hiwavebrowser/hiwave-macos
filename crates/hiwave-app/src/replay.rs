//! `hiwave --replay-proxy <url>`: every request the RustKit views make goes
//! to a local HAR replay server (`tools/parity_oracle/har_server.py`)
//! instead of the network, the same flag parity-capture has (T18).
//!
//! The engine does the routing (`EngineConfig::replay_proxy`): each request
//! is rewritten to the proxy with its real URL in `X-Original-URL`, and the
//! HTTP client may connect only to a loopback address on the proxy's port,
//! so a request that escaped the rewrite is refused rather than sent live.
//!
//! The flag is refused, and the app does not start, when the value is
//! missing, unparseable, not `http:`, or not a loopback host. A typo must
//! never fall back to a live launch: the point of the flag is that the real
//! window can replay a site (eBay) this seat must never load live.
//!
//! Known limit: the shield's interceptor sees the rewritten (loopback) URL,
//! so it blocks nothing while replaying. The HAR decides what a page gets.

use std::sync::OnceLock;
use url::Url;

/// The flag's name on the command line.
pub const FLAG: &str = "--replay-proxy";

static REPLAY_PROXY: OnceLock<Option<Url>> = OnceLock::new();

/// The replay proxy from `args` (the command line without the program
/// name): `--replay-proxy URL` or `--replay-proxy=URL`. `Ok(None)` without
/// the flag. Arguments that are not this flag are left alone.
pub fn parse_replay_proxy<I>(args: I) -> Result<Option<Url>, String>
where
    I: IntoIterator<Item = String>,
{
    let mut found: Option<Url> = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let raw = if arg == FLAG {
            match args.next() {
                Some(value) if !value.starts_with("--") => value,
                _ => return Err(format!("{FLAG} needs a URL, e.g. {FLAG} http://127.0.0.1:8989")),
            }
        } else if let Some(value) = arg.strip_prefix(&format!("{FLAG}=")) {
            value.to_string()
        } else {
            continue;
        };
        if found.is_some() {
            return Err(format!("{FLAG} given twice"));
        }
        found = Some(checked(&raw)?);
    }
    Ok(found)
}

fn checked(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|e| format!("{FLAG}: not a URL ({e})"))?;
    if url.scheme() != "http" {
        return Err(format!("{FLAG}: the replay server is plain http, got {}:", url.scheme()));
    }
    let loopback = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        None => false,
    };
    if !loopback {
        return Err(format!(
            "{FLAG}: the replay server must be on this machine (127.0.0.1, ::1 or localhost)"
        ));
    }
    Ok(url)
}

/// Records the launch's replay proxy. The first call wins; later calls are
/// ignored (the views are built after it).
pub fn set_replay_proxy(proxy: Option<Url>) {
    let _ = REPLAY_PROXY.set(proxy);
}

/// The launch's replay proxy, `None` for a normal launch.
pub fn replay_proxy() -> Option<Url> {
    REPLAY_PROXY.get().cloned().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Option<Url>, String> {
        parse_replay_proxy(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_flag_is_a_normal_launch() {
        assert_eq!(parse(&[]), Ok(None));
        assert_eq!(parse(&["--something-else", "x"]), Ok(None));
    }

    #[test]
    fn the_flag_takes_a_loopback_http_url_in_either_form() {
        let want = Url::parse("http://127.0.0.1:8989/").unwrap();
        assert_eq!(parse(&["--replay-proxy", "http://127.0.0.1:8989"]), Ok(Some(want.clone())));
        assert_eq!(parse(&["--replay-proxy=http://127.0.0.1:8989"]), Ok(Some(want)));
        assert!(parse(&["--replay-proxy", "http://localhost:9000"]).unwrap().is_some());
        assert!(parse(&["--replay-proxy", "http://[::1]:9000"]).unwrap().is_some());
        assert!(parse(&["--replay-proxy", "http://127.0.0.2:9000"]).unwrap().is_some());
    }

    #[test]
    fn a_bad_value_refuses_the_launch_instead_of_going_live() {
        for bad in [
            &["--replay-proxy"][..],
            &["--replay-proxy", "--other"],
            &["--replay-proxy", "not a url"],
            &["--replay-proxy="],
            &["--replay-proxy", "https://127.0.0.1:8989"],
            &["--replay-proxy", "http://192.168.1.5:8989"],
            &["--replay-proxy", "http://www.ebay.com/"],
            &["--replay-proxy", "http://localhost.example:80"],
            &["--replay-proxy", "http://127.0.0.1:1", "--replay-proxy", "http://127.0.0.1:2"],
        ] {
            assert!(parse(bad).is_err(), "{bad:?} was accepted");
        }
    }
}
