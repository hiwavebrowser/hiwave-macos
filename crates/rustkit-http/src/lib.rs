//! # RustKit HTTP
//!
//! Minimal HTTP/1.1 client for the RustKit browser engine.
//!
//! This crate provides a simple async HTTP client, eliminating the need for
//! reqwest and its transitive dependencies. TLS is rustls with a
//! browser-typical client profile (ALPN h2+http/1.1 advertised, negotiated
//! http/1.1 fallback) — the network-lane change measured in exchange #276.
//! The previous native-tls stack remains available for one release behind
//! the `native-tls` feature as a rollback path (Atlas #535).

use std::io::{self, Write};
use std::time::Duration;

use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Version};
use std::sync::Arc;
use tokio_rustls::rustls::pki_types::ServerName;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use tracing::{debug, trace, warn};
use url::Url;

mod addr;
mod h2_pool;
pub mod cookies;
pub use cookies::{CookieJar, CookieSource};
pub use addr::{is_local_name, is_public_ip, AddressPolicy, Resolve, ResolveFuture, SystemResolver};

/// HTTP client errors.
#[derive(Error, Debug)]
pub enum HttpError {
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// The host name did not resolve (or resolved to no address). The same
    /// text as [`ConnectionFailed`](Self::ConnectionFailed); its own variant
    /// so a caller tells the two apart without reading the text.
    #[error("Connection failed: {0}")]
    LookupFailed(String),

    #[error("TLS error: {0}")]
    TlsError(String),

    #[error("Request timeout")]
    Timeout,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Too many redirects")]
    TooManyRedirects,

    #[error("Unsupported scheme: {0}")]
    UnsupportedScheme(String),

    #[error("Address not permitted: {0}")]
    AddressDenied(String),

    #[error("Response body exceeds {0} bytes")]
    BodyTooLarge(usize),
}

/// HTTP response.
#[derive(Debug)]
pub struct Response {
    /// HTTP status code.
    pub status: StatusCode,
    /// HTTP version.
    pub version: Version,
    /// Response headers.
    pub headers: HeaderMap,
    /// Response body.
    pub body: Bytes,
    /// Final URL (after redirects).
    pub url: Url,
    /// Body bytes as they came over the wire, before any
    /// `Content-Encoding` was undone (`body.len()` after).
    pub encoded_body_size: u64,
    /// The redirects followed to get here, in order: each URL that answered
    /// with a redirect, and its status.
    pub redirect_chain: Vec<(Url, StatusCode)>,
}

impl Response {
    /// Get a header value as a string.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    /// Get content-length from headers.
    pub fn content_length(&self) -> Option<u64> {
        self.header("content-length").and_then(|s| s.parse().ok())
    }

    /// Get content-type from headers.
    pub fn content_type(&self) -> Option<&str> {
        self.header("content-type")
    }

    /// Check if response is success (2xx).
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    /// Get body as text.
    pub fn text(&self) -> Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.body.to_vec())
    }
}

/// The honest HiWave user agent: real product, real engine, real platform.
///
/// NEVER Chrome's UA — the network lane's constitution. The Mozilla/5.0
/// prefix is the universal compatibility token every shipping browser keeps;
/// everything after it says exactly what we are. amazon's WAF measurably
/// scores UA/TLS coherence (diagnosis, exchange #276), so this string ships
/// in the same change as the rustls profile, never separately.
pub fn default_user_agent() -> String {
    #[cfg(target_os = "macos")]
    let platform = "Macintosh; Intel Mac OS X 10_15_7";
    #[cfg(target_os = "windows")]
    let platform = "Windows NT 10.0; Win64; x64";
    #[cfg(all(unix, not(target_os = "macos")))]
    let platform = "X11; Linux x86_64";
    format!("Mozilla/5.0 ({platform}) HiWave/1.0 RustKit/1.0")
}


/// The platform root store, loaded ONCE per process.
///
/// `rustls_native_certs::load_native_certs` walks the macOS keychain's trust
/// settings and costs SECONDS there. Loaded per `Client` (as #346 shipped
/// it) it added ~5 s to every engine start — measured by the trench as the
/// real-site board falling 16/30 -> 7/30 when develop picked #346 up, every
/// lost site a 30 s-budget timeout, not a block. Linux reads a bundle file
/// in milliseconds, which is why the author's probes never saw it: the
/// platform-verification gap the network lane declared on day one, now with
/// its first scar. Approach and measurements from Atlas's
/// rs-tls-roots-once (307fc8e), rebuilt here against post-#355 develop —
/// #355 already removed the second (per-connection) load site.
#[cfg(not(feature = "native-tls"))]
fn platform_roots() -> Result<Arc<tokio_rustls::rustls::RootCertStore>, HttpError> {
    static ROOTS: RootsCache = std::sync::Mutex::new(None);
    roots_cached(&ROOTS, || {
        let mut roots = tokio_rustls::rustls::RootCertStore::empty();
        let loaded = rustls_native_certs::load_native_certs();
        let mut rejected = 0usize;
        for cert in loaded.certs {
            // A single unparseable platform cert must not kill the store.
            if roots.add(cert).is_err() {
                rejected += 1;
            }
        }
        let mut diagnostics = String::new();
        if !loaded.errors.is_empty() || rejected > 0 {
            diagnostics = format!(
                "{} load errors, {} rejected certs, accepted {}",
                loaded.errors.len(),
                rejected,
                roots.len()
            );
            if let Some(first) = loaded.errors.first() {
                diagnostics.push_str(&format!(", first error: {first}"));
            }
            warn!(target: "rustkit_http::roots", "platform root load: {diagnostics}");
        }
        (roots, diagnostics)
    })
}

type RootsCache = std::sync::Mutex<Option<Arc<tokio_rustls::rustls::RootCertStore>>>;

fn roots_cached(
    cache: &RootsCache,
    load: impl FnOnce() -> (tokio_rustls::rustls::RootCertStore, String),
) -> Result<Arc<tokio_rustls::rustls::RootCertStore>, HttpError> {
    let mut slot = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(roots) = slot.as_ref() {
        return Ok(roots.clone());
    }
    // Only a usable store is cached: a transient platform failure (keychain
    // busy under load) must not turn every later handshake into an error.
    let (roots, diagnostics) = load();
    let roots = Arc::new(roots);
    if roots.is_empty() {
        return Err(HttpError::TlsError(if diagnostics.is_empty() {
            "no usable platform root certificates".into()
        } else {
            format!("no usable platform root certificates ({diagnostics})")
        }));
    }
    *slot = Some(roots.clone());
    Ok(roots)
}

/// ALPN outcome of a TLS handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NegotiatedProtocol {
    H2,
    Http1,
}

/// HTTP client configuration.
#[derive(Clone)]
pub struct ClientConfig2 {
    /// User agent string.
    pub user_agent: String,
    /// Default request timeout.
    pub timeout: Duration,
    /// Maximum number of redirects to follow.
    pub max_redirects: usize,
    /// Whether to follow redirects.
    pub follow_redirects: bool,
}

impl Default for ClientConfig2 {
    fn default() -> Self {
        Self {
            user_agent: default_user_agent(),
            timeout: Duration::from_secs(30),
            max_redirects: 10,
            follow_redirects: true,
        }
    }
}

/// HTTP client.
#[derive(Clone)]
pub struct Client {
    config: ClientConfig2,
    address_policy: AddressPolicy,
    resolver: Arc<dyn Resolve>,
    max_body: Option<usize>,
    /// Where `Set-Cookie` goes and `Cookie` comes from, at every hop; `None`
    /// sends and keeps no cookies.
    cookies: Option<Arc<CookieJar>>,
    /// One HTTP/2 connection per host. Clones share it: a stylesheet and a
    /// script to the same host are streams, not two sockets.
    h2_pool: Arc<h2_pool::H2Pool>,
    #[cfg(not(feature = "native-tls"))]
    tls_connector: TlsConnector,
    #[cfg(feature = "native-tls")]
    tls_connector: tokio_native_tls::TlsConnector,
}

impl Client {
    /// Create a new HTTP client with default configuration.
    pub fn new() -> Result<Self, HttpError> {
        Self::with_config(ClientConfig2::default())
    }

    /// Create a new HTTP client with custom configuration.
    /// Rollback constructor (Atlas #535): the pre-network-lane native-tls
    /// handshake, byte-for-byte the old behaviour. One release only.
    #[cfg(feature = "native-tls")]
    pub fn with_config(config: ClientConfig2) -> Result<Self, HttpError> {
        let native_connector = native_tls::TlsConnector::new()
            .map_err(|e| HttpError::TlsError(e.to_string()))?;
        let tls_connector = tokio_native_tls::TlsConnector::from(native_connector);
        Ok(Self {
            config,
            address_policy: AddressPolicy::default(),
            resolver: Arc::new(SystemResolver),
            max_body: None,
            cookies: None,
            h2_pool: Arc::new(h2_pool::H2Pool::new()),
            tls_connector,
        })
    }

    #[cfg(feature = "native-tls")]
    async fn connect_tls(
        &self,
        host: &str,
        _addr: &str,
        stream: tokio::net::TcpStream,
    ) -> Result<(tokio_native_tls::TlsStream<tokio::net::TcpStream>, NegotiatedProtocol), HttpError>
    {
        // Rollback stack: no ALPN configured, identical to pre-lane behavior.
        let tls = self
            .tls_connector
            .connect(host, stream)
            .await
            .map_err(|e| HttpError::TlsError(e.to_string()))?;
        Ok((tls, NegotiatedProtocol::Http1))
    }

    #[cfg(not(feature = "native-tls"))]
    pub fn with_config(config: ClientConfig2) -> Result<Self, HttpError> {
        // rustls with a browser-typical client profile (network lane).
        //
        // MEASURED (2026-09-28 diagnosis, exchange #276): Cloudflare, Akamai
        // and DataDome default-deny known-library TLS ClientHellos at request
        // one; header shape and even real HTTP/2 do not flip the verdict, and
        // amazon actively punishes browser-claiming headers that ride a
        // library fingerprint. So the TLS layer is where coherence starts.
        // The old native-tls connector was built with `::new()` and sent NO
        // ALPN extension at all — an immediate tell.
        //
        // This is a WELL-FORMED MODERN CLIENT, not a Chrome imitation: rustls
        // already produces a contemporary extension set (X25519/P-256 key
        // shares, TLS 1.3 + 1.2, session tickets); we advertise ALPN
        // h2+http/1.1 like every current browser. If the peer selects h2 we
        // currently keep speaking HTTP/1.1 only when the peer permits it —
        // see `connect_tls`, which records the negotiated protocol so the
        // caller can refuse mismatches loudly instead of desyncing.
        let roots = platform_roots()?;

        // When multiple crypto providers (e.g. aws-lc-rs from rustkit-http and ring from
        // reqwest in the workspace) are active, rustls requires an explicit default provider
        // installed to avoid panicking on ClientConfig::builder().
        let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();

        let mut tls_config = tokio_rustls::rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        // Browser-typical ALPN advertisement. http/1.1 first would be a lie
        // about preference; browsers prefer h2. Until this client SPEAKS h2,
        // connect_tls() falls back to a second, http/1.1-only handshake when
        // the peer selects h2 — an honest downgrade the peer agrees to, not
        // a silent protocol desync.
        tls_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

        let tls_connector = TlsConnector::from(Arc::new(tls_config));

        Ok(Self {
            config,
            address_policy: AddressPolicy::default(),
            resolver: Arc::new(SystemResolver),
            max_body: None,
            cookies: None,
            h2_pool: Arc::new(h2_pool::H2Pool::new()),
            tls_connector,
        })
    }

    /// Open a TLS connection and report the ALPN-negotiated protocol.
    ///
    /// PR 1 of the network lane advertised h2 but re-handshook http/1.1-only
    /// when the peer selected it — disclosed as an odd, costly pattern.
    /// PR 2 removes it: the caller now SPEAKS whichever protocol was
    /// negotiated, h2 included, over this single handshake.
    #[cfg(not(feature = "native-tls"))]
    async fn connect_tls(
        &self,
        host: &str,
        _addr: &str,
        stream: tokio::net::TcpStream,
    ) -> Result<(tokio_rustls::client::TlsStream<tokio::net::TcpStream>, NegotiatedProtocol), HttpError>
    {
        let server_name = ServerName::try_from(host.to_string())
            .map_err(|e| HttpError::TlsError(format!("invalid server name: {e}")))?;

        let tls_stream = self
            .tls_connector
            .connect(server_name, stream)
            .await
            .map_err(|e| HttpError::TlsError(e.to_string()))?;

        let negotiated = match tls_stream.get_ref().1.alpn_protocol() {
            Some(b"h2") => NegotiatedProtocol::H2,
            _ => NegotiatedProtocol::Http1,
        };
        Ok((tls_stream, negotiated))
    }

    /// Create a client builder.
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// The same client restricted to the given resolved-address policy.
    pub fn with_address_policy(mut self, policy: AddressPolicy) -> Self {
        self.address_policy = policy;
        self
    }

    /// The same client with a different name resolver (tests, DoH later).
    pub fn with_resolver(mut self, resolver: Arc<dyn Resolve>) -> Self {
        self.resolver = resolver;
        self
    }

    /// The same client, following redirects or not. With `false` a 3xx comes
    /// back as the response (status, `Location` and all) for the caller to
    /// vet and follow itself, hop by hop.
    pub fn with_follow_redirects(mut self, follow: bool) -> Self {
        self.config.follow_redirects = follow;
        self
    }

    /// The same client with this cookie jar, or with none.
    pub fn with_cookies(mut self, jar: Option<Arc<CookieJar>>) -> Self {
        self.cookies = jar;
        self
    }

    /// The jar this client sends and keeps cookies with.
    pub fn cookie_jar(&self) -> Option<&Arc<CookieJar>> {
        self.cookies.as_ref()
    }

    /// The same client, refusing any response body (after decoding) larger
    /// than `max` bytes with [`HttpError::BodyTooLarge`].
    pub fn with_max_body(mut self, max: usize) -> Self {
        self.max_body = Some(max);
        self
    }

    /// Resolve `host`, vet every address, connect to a vetted one.
    async fn connect(&self, host: &str, port: u16) -> Result<TcpStream, HttpError> {
        addr::connect_vetted(&*self.resolver, &self.address_policy, host, port).await
    }

    /// Perform a GET request.
    pub async fn get(&self, url: &str) -> Result<Response, HttpError> {
        self.request(Method::GET, url, HeaderMap::new(), None).await
    }

    /// Perform a POST request.
    pub async fn post(&self, url: &str, body: Bytes) -> Result<Response, HttpError> {
        self.request(Method::POST, url, HeaderMap::new(), Some(body))
            .await
    }

    /// Perform an HTTP request.
    pub async fn request(
        &self,
        method: Method,
        url: &str,
        headers: HeaderMap,
        body: Option<Bytes>,
    ) -> Result<Response, HttpError> {
        let parsed_url = Url::parse(url).map_err(|e| HttpError::InvalidUrl(e.to_string()))?;
        self.request_url(method, parsed_url, headers, body, 0).await
    }

    /// Internal request implementation with redirect counting.
    async fn request_url(
        &self,
        method: Method,
        url: Url,
        headers: HeaderMap,
        body: Option<Bytes>,
        redirect_count: usize,
    ) -> Result<Response, HttpError> {
        if redirect_count > self.config.max_redirects {
            return Err(HttpError::TooManyRedirects);
        }

        let scheme = url.scheme();
        let host = url
            .host_str()
            .ok_or_else(|| HttpError::InvalidUrl("Missing host".to_string()))?;
        let port = url.port_or_known_default().unwrap_or(if scheme == "https" {
            443
        } else {
            80
        });

        debug!(method = %method, url = %url, "HTTP request");

        // The jar's cookies for this hop; a caller's own Cookie header stands.
        let mut headers = headers;
        if let Some(jar) = &self.cookies {
            if !headers.contains_key(http::header::COOKIE) {
                if let Some(value) = jar.header_for(&url, false).and_then(|v| HeaderValue::from_str(&v).ok()) {
                    headers.insert(http::header::COOKIE, value);
                }
            }
        }

        // Connect with timeout
        let response = timeout(self.config.timeout, async {
            match scheme {
                "https" => self.request_https(host, port, &method, &url, &headers, &body).await,
                "http" => self.request_http(host, port, &method, &url, &headers, &body).await,
                _ => Err(HttpError::UnsupportedScheme(scheme.to_string())),
            }
        })
        .await
        .map_err(|_| HttpError::Timeout)??;

        // Every hop's Set-Cookie is kept, a redirect's included.
        if let Some(jar) = &self.cookies {
            for value in response.headers.get_all(http::header::SET_COOKIE) {
                if let Ok(value) = value.to_str() {
                    jar.set(&url, value, CookieSource::Http);
                }
            }
        }

        // Handle redirects
        if self.config.follow_redirects && response.status.is_redirection() {
            if let Some(location) = response.header("location") {
                let redirect_url = url
                    .join(location)
                    .map_err(|e| HttpError::InvalidUrl(e.to_string()))?;
                debug!(from = %url, to = %redirect_url, "Following redirect");
                let mut followed =
                    Box::pin(self.request_url(Method::GET, redirect_url, HeaderMap::new(), None, redirect_count + 1))
                        .await?;
                followed.redirect_chain.insert(0, (url, response.status));
                return Ok(followed);
            }
        }

        Ok(Response {
            status: response.status,
            version: response.version,
            headers: response.headers,
            body: response.body,
            url,
            encoded_body_size: response.encoded_len,
            redirect_chain: Vec::new(),
        })
    }

    fn h2_key(&self, host: &str, port: u16) -> h2_pool::H2Key {
        let policy = match &self.address_policy {
            AddressPolicy::Any => h2_pool::POLICY_ANY,
            AddressPolicy::PublicOnly => h2_pool::POLICY_PUBLIC,
            AddressPolicy::Custom(_) => h2_pool::POLICY_CUSTOM,
        };
        h2_pool::H2Key {
            host: host.to_string(),
            port,
            policy,
        }
    }

    /// HTTPS request.
    async fn request_https(
        &self,
        host: &str,
        port: u16,
        method: &Method,
        url: &Url,
        headers: &HeaderMap,
        body: &Option<Bytes>,
    ) -> Result<RawResponse, HttpError> {
        let key = self.h2_key(host, port);
        // A custom address policy does not share a socket with another policy.
        if key.policy != h2_pool::POLICY_CUSTOM {
            let slot = self.h2_pool.slot(key).await;
            let cached = slot.lock().await.as_ref().cloned();
            // One retry in all, on a new connection, for an idempotent
            // request whose connection was lost or whose stream the server
            // never processed (REFUSED_STREAM, above a GOAWAY).
            let mut retried = false;
            if let Some(send) = cached {
                match self.dispatch_h2(send, method, url, headers, body).await {
                    Ok(response) => return Ok(response),
                    Err(e) if h2_connection_lost(&e) && is_idempotent(method) => {
                        *slot.lock().await = None;
                        retried = true;
                    }
                    Err(e) => return Err(e),
                }
            }
            loop {
                let mut guard = slot.lock().await;
                let result = if let Some(send) = guard.as_ref().cloned() {
                    drop(guard);
                    self.dispatch_h2(send, method, url, headers, body).await
                } else {
                    match self.dial_tls(host, port).await? {
                        Dialed::H2(send) => {
                            *guard = Some(send.clone());
                            drop(guard);
                            self.dispatch_h2(send, method, url, headers, body).await
                        }
                        Dialed::Http1(stream) => {
                            drop(guard);
                            return self.send_request(stream, host, method, url, headers, body).await;
                        }
                    }
                };
                match result {
                    Err(e) if !retried && h2_connection_lost(&e) && is_idempotent(method) => {
                        *slot.lock().await = None;
                        retried = true;
                    }
                    result => return result,
                }
            }
        } else {
            match self.dial_tls(host, port).await? {
                Dialed::H2(send) => self.dispatch_h2(send, method, url, headers, body).await,
                Dialed::Http1(stream) => {
                    self.send_request(stream, host, method, url, headers, body).await
                }
            }
        }
    }

    /// TCP + TLS. HTTP/2 stores the sender on the pool's lock (the caller
    /// holds it) after the connection driver is spawned.
    async fn dial_tls(
        &self,
        host: &str,
        port: u16,
    ) -> Result<
        Dialed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static>,
        HttpError,
    > {
        let addr = format!("{host}:{port}");
        let stream = self.connect(host, port).await?;
        let (tls_stream, negotiated) = self.connect_tls(host, &addr, stream).await?;
        match negotiated {
            NegotiatedProtocol::H2 => {
                let (send, connection) = h2_client().handshake(tls_stream)
                    .await
                    .map_err(|e| HttpError::ConnectionFailed(format!("h2 handshake: {e}")))?;
                // The driver is bound to whichever runtime is current. Page
                // loads and live turns share the process host runtime, so a
                // stream opened during the load is still driven after it
                // returns. One driver per host: every later request to this
                // host is another stream, and `ready` waits when the server's
                // stream limit is full instead of opening a socket the server
                // will not read.
                tokio::spawn(async move {
                    let _ = connection.await;
                });
                Ok(Dialed::H2(send))
            }
            NegotiatedProtocol::Http1 => Ok(Dialed::Http1(tls_stream)),
        }
    }

    /// One request on an HTTP/2 connection this process already drives.
    ///
    /// `ready` is the stream slot. Connection-specific headers are h2-illegal
    /// and are not sent.
    async fn dispatch_h2(
        &self,
        mut send_request: h2::client::SendRequest<Bytes>,
        method: &Method,
        url: &Url,
        headers: &HeaderMap,
        body: &Option<Bytes>,
    ) -> Result<RawResponse, HttpError> {
        send_request = send_request
            .ready()
            .await
            .map_err(|e| HttpError::ConnectionFailed(format!("h2 ready: {e}")))?;

        let mut request = http::Request::builder()
            .method(method.clone())
            .uri(url.as_str())
            .version(http::Version::HTTP_2);

        for (name, value) in h2_request_headers(&self.config.user_agent, headers) {
            request = request.header(name, value);
        }

        let request = request
            .body(())
            .map_err(|e| HttpError::InvalidResponse(format!("h2 request build: {e}")))?;

        let has_body = body.is_some();
        let (response_fut, mut send_stream) = send_request
            .send_request(request, !has_body)
            .map_err(|e| HttpError::ConnectionFailed(format!("h2 send: {e}")))?;
        if let Some(b) = body {
            send_stream
                .send_data(b.clone(), true)
                .map_err(|e| HttpError::ConnectionFailed(format!("h2 body: {e}")))?;
        }

        let response = response_fut.await.map_err(|e| {
            if h2_unprocessed(&e) {
                // Never processed (RFC 9113 8.7): safe to send again.
                HttpError::ConnectionFailed(format!("h2 unprocessed: {e}"))
            } else {
                HttpError::InvalidResponse(format!("h2 response: {e}"))
            }
        })?;
        let status = response.status();
        let mut response_headers = HeaderMap::new();
        for (name, value) in response.headers() {
            response_headers.insert(name.clone(), value.clone());
        }

        let mut recv = response.into_body();
        let mut collected: Vec<u8> = Vec::new();
        while let Some(chunk) = recv.data().await {
            let chunk =
                chunk.map_err(|e| HttpError::InvalidResponse(format!("h2 body read: {e}")))?;
            collected.extend_from_slice(&chunk);
            if self.max_body.is_some_and(|max| collected.len() > max) {
                return Err(HttpError::BodyTooLarge(self.max_body.unwrap_or(0)));
            }
            // Flow control: hand the window back or the peer stalls at 64KB.
            let _ = recv.flow_control().release_capacity(chunk.len());
        }

        let encoded_len = collected.len() as u64;
        let body =
            decode_content_encoding_capped(Bytes::from(collected), &mut response_headers, self.max_body)?;

        Ok(RawResponse {
            status,
            version: Version::HTTP_2,
            headers: response_headers,
            body,
            encoded_len,
        })
    }

    /// Cleartext HTTP/2, same pool and dispatch as the TLS path. Tests use it
    /// so a second request is observable as a stream on the first connection
    /// rather than a second TCP accept.
    #[cfg(test)]
    async fn request_h2_cleartext(&self, url: &str) -> Result<RawResponse, HttpError> {
        let url = Url::parse(url).map_err(|e| HttpError::InvalidUrl(e.to_string()))?;
        let host = url
            .host_str()
            .ok_or_else(|| HttpError::InvalidUrl("Missing host".to_string()))?;
        let port = url
            .port()
            .ok_or_else(|| HttpError::InvalidUrl("Missing port".to_string()))?;
        let slot = self.h2_pool.slot(self.h2_key(host, port)).await;
        // The same single retry as `request_https` (a GET is idempotent).
        let mut retried = false;
        loop {
            let mut guard = slot.lock().await;
            let send = if let Some(send) = guard.as_ref().cloned() {
                drop(guard);
                send
            } else {
                let stream = self.connect(host, port).await?;
                let (send, connection) = h2_client().handshake(stream)
                    .await
                    .map_err(|e| HttpError::ConnectionFailed(format!("h2 handshake: {e}")))?;
                tokio::spawn(async move {
                    let _ = connection.await;
                });
                *guard = Some(send.clone());
                drop(guard);
                send
            };
            match self
                .dispatch_h2(send, &Method::GET, &url, &HeaderMap::new(), &None)
                .await
            {
                Err(e) if !retried && h2_connection_lost(&e) => {
                    *slot.lock().await = None;
                    retried = true;
                }
                result => return result,
            }
        }
    }

    /// HTTP request.
    async fn request_http(
        &self,
        host: &str,
        port: u16,
        method: &Method,
        url: &Url,
        headers: &HeaderMap,
        body: &Option<Bytes>,
    ) -> Result<RawResponse, HttpError> {
        let stream = self.connect(host, port).await?;

        self.send_request(stream, host, method, url, headers, body)
            .await
    }

    /// Send HTTP request and read response.
    async fn send_request<S>(
        &self,
        stream: S,
        host: &str,
        method: &Method,
        url: &Url,
        headers: &HeaderMap,
        body: &Option<Bytes>,
    ) -> Result<RawResponse, HttpError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let (reader, mut writer) = tokio::io::split(stream);
        // 64KB, not the 8KB default: a module body is one Content-Length
        // read, and a small buffer only adds round trips. There is no
        // per-chunk delay here.
        let mut reader = BufReader::with_capacity(64 * 1024, reader);

        // Build request
        let path = if let Some(query) = url.query() {
            format!("{}?{}", url.path(), query)
        } else {
            url.path().to_string()
        };
        let path = if path.is_empty() { "/" } else { &path };

        // BROWSER-SHAPED EMISSION (network lane). The old block wrote five
        // Title-Case headers, `Accept: */*`, `Connection: close`, then every
        // caller header in lowercase after them — three tells in one block
        // (mixed casing, close-on-navigate, wildcard Accept). MEASURED
        // (#276): header shape alone does not unblock any WAF vendor, but
        // amazon scores header/TLS COHERENCE, so the shape ships together
        // with the rustls profile as one coherent client identity.
        //
        // Order and casing follow shipping browsers' HTTP/1.1 form. Caller
        // headers override any default; the ordered known set is emitted
        // first, remaining caller headers after, all in canonical casing.
        let mut request = Vec::new();
        writeln!(request, "{} {} HTTP/1.1\r", method, path)?;
        let host_header = headers.get("host").and_then(|v| v.to_str().ok()).unwrap_or(host);
        writeln!(request, "Host: {}\r", host_header)?;
        writeln!(request, "Connection: keep-alive\r")?;
        writeln!(request, "User-Agent: {}\r", self.config.user_agent)?;

        let canonical = |name: &str| -> String {
            // HTTP/1.1 browser casing: Title-Case per hyphenated segment,
            // with the Sec-* and *-CH-* families' internal caps preserved
            // by the segment rule itself.
            name.split('-')
                .map(|seg| {
                    let mut c = seg.chars();
                    match c.next() {
                        Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join("-")
        };

        // Known headers in browser order, caller value winning over default.
        const ORDERED: &[(&str, Option<&str>)] = &[
            (
                "accept",
                Some(
                    "text/html,application/xhtml+xml,application/xml;q=0.9,\
image/avif,image/webp,*/*;q=0.8",
                ),
            ),
            ("accept-language", None),
            ("accept-encoding", Some(ACCEPT_ENCODING)),
            ("upgrade-insecure-requests", Some("1")),
            ("sec-fetch-dest", Some("document")),
            ("sec-fetch-mode", Some("navigate")),
            ("sec-fetch-site", Some("none")),
            ("sec-fetch-user", Some("?1")),
            ("referer", None),
            ("cookie", None),
        ];
        let mut written: Vec<&str> = vec![];
        for (name, default) in ORDERED {
            let value = headers
                .get(*name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
                .or_else(|| default.map(str::to_string));
            if let Some(v) = value {
                writeln!(request, "{}: {}\r", canonical(name), v)?;
                written.push(name);
            }
        }

        // Remaining caller headers, canonical casing, after the known set.
        for (name, value) in headers.iter() {
            if name.as_str() == "host" || written.contains(&name.as_str()) {
                continue;
            }
            if let Ok(v) = value.to_str() {
                writeln!(request, "{}: {}\r", canonical(name.as_str()), v)?;
            }
        }

        // Content-Length for body
        if let Some(b) = body {
            writeln!(request, "Content-Length: {}\r", b.len())?;
        }

        writeln!(request, "\r")?;

        // Send headers
        writer.write_all(&request).await?;

        // Send body
        if let Some(b) = body {
            writer.write_all(b).await?;
        }

        writer.flush().await?;

        // Read response status line
        let mut status_line = String::new();
        reader.read_line(&mut status_line).await?;

        let (version, status) = parse_status_line(&status_line)?;

        // Read headers
        let mut response_headers = HeaderMap::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await?;
            let line = line.trim();
            if line.is_empty() {
                break;
            }

            if let Some((name, value)) = line.split_once(':') {
                if let (Ok(n), Ok(v)) = (
                    HeaderName::try_from(name.trim()),
                    HeaderValue::try_from(value.trim()),
                ) {
                    response_headers.insert(n, v);
                }
            }
        }

        // Read body
        let body = read_body(&mut reader, &response_headers, self.max_body).await?;
        let encoded_len = body.len() as u64;
        let body = decode_content_encoding_capped(body, &mut response_headers, self.max_body)?;

        trace!(status = %status, body_len = body.len(), "Response received");

        Ok(RawResponse {
            status,
            version,
            headers: response_headers,
            body,
            encoded_len,
        })
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new().expect("Failed to create default HTTP client")
    }
}

/// Raw response (before redirect handling).
struct RawResponse {
    status: StatusCode,
    version: Version,
    headers: HeaderMap,
    body: Bytes,
    /// Body length before `Content-Encoding` was undone.
    encoded_len: u64,
}

impl RawResponse {
    /// Get a header value as a string.
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }
}

/// Client builder for configuring HTTP client.
pub struct ClientBuilder {
    config: ClientConfig2,
    cookies: bool,
}

impl ClientBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            config: ClientConfig2::default(),
            cookies: false,
        }
    }

    /// Set user agent.
    pub fn user_agent(mut self, user_agent: &str) -> Self {
        self.config.user_agent = user_agent.to_string();
        self
    }

    /// Set timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Set redirect policy.
    pub fn redirect(mut self, follow: bool, max: usize) -> Self {
        self.config.follow_redirects = follow;
        self.config.max_redirects = max;
        self
    }

    /// Keep and send cookies, in a jar of the client's own.
    pub fn cookie_store(mut self, enabled: bool) -> Self {
        self.cookies = enabled;
        self
    }

    /// Build the client.
    pub fn build(self) -> Result<Client, HttpError> {
        let jar = self.cookies.then(|| Arc::new(CookieJar::new()));
        Ok(Client::with_config(self.config)?.with_cookies(jar))
    }
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse HTTP status line.
fn parse_status_line(line: &str) -> Result<(Version, StatusCode), HttpError> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(HttpError::InvalidResponse("Invalid status line".to_string()));
    }

    let version = match parts[0] {
        "HTTP/1.0" => Version::HTTP_10,
        "HTTP/1.1" => Version::HTTP_11,
        "HTTP/2" | "HTTP/2.0" => Version::HTTP_2,
        _ => Version::HTTP_11,
    };

    let status_code: u16 = parts[1]
        .parse()
        .map_err(|_| HttpError::InvalidResponse("Invalid status code".to_string()))?;

    let status = StatusCode::from_u16(status_code)
        .map_err(|_| HttpError::InvalidResponse("Invalid status code".to_string()))?;

    Ok((version, status))
}

/// The `Accept-Encoding` sent on requests. `decode_content_encoding` can undo
/// all of it, and `br` as well: brotli is decoded when a server sends it
/// unasked, but asking for it would change what every server sends us.
const ACCEPT_ENCODING: &str = "gzip, deflate";

/// Connection-specific header fields (RFC 9113 §8.1.2) plus `host`
/// (carried by `:authority`). Must not appear on an HTTP/2 request.
fn is_h2_illegal_request_header(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "keep-alive"
            | "proxy-connection"
            | "transfer-encoding"
            | "upgrade"
            | "host"
    )
}

/// Header pairs `send_request_h2` writes onto the request, in emission order.
/// Caller headers that are connection-specific, already emitted, or the
/// user-agent (set from config) are dropped.
enum Dialed<S> {
    H2(h2::client::SendRequest<Bytes>),
    Http1(S),
}

/// Chromium's HTTP/2 receive windows: 6 MB per stream, 15 MB per connection.
/// At h2's defaults (64 KB each) a stream whose body is not being read, such
/// as a parser image parked while a load yields, filled the connection window,
/// and every other stream to that host waited for the client timeout.
const H2_STREAM_WINDOW: u32 = 6 * 1024 * 1024;
const H2_CONNECTION_WINDOW: u32 = 15 * 1024 * 1024;

/// Streams the client opens before the server's SETTINGS are read. h2's
/// default is 100, and a server whose limit is lower answers the extra
/// streams with REFUSED_STREAM. With one, later requests queue on the
/// connection until SETTINGS give the server's own limit.
const H2_STREAMS_BEFORE_SETTINGS: usize = 1;

fn h2_client() -> h2::client::Builder {
    let mut builder = h2::client::Builder::new();
    builder
        .initial_window_size(H2_STREAM_WINDOW)
        .initial_connection_window_size(H2_CONNECTION_WINDOW)
        .initial_max_send_streams(H2_STREAMS_BEFORE_SETTINGS);
    builder
}

fn is_idempotent(method: &Method) -> bool {
    matches!(method, &Method::GET | &Method::HEAD | &Method::OPTIONS)
}

/// The connection is gone, so a retry on a new one can succeed. An HTTP error
/// status is not this: the request reached the server.
fn h2_connection_lost(err: &HttpError) -> bool {
    matches!(err, HttpError::ConnectionFailed(_))
}

/// The server never processed this stream (RFC 9113 8.7): it refused it, or
/// it is above the last stream id of the server's GOAWAY. h2 resets only
/// those streams with the GOAWAY's error; the ones at or below it go on.
fn h2_unprocessed(err: &h2::Error) -> bool {
    err.reason() == Some(h2::Reason::REFUSED_STREAM) || (err.is_go_away() && err.is_remote())
}

fn h2_request_headers(user_agent: &str, headers: &HeaderMap) -> Vec<(HeaderName, HeaderValue)> {
    const ORDERED_H2: &[(&str, Option<&str>)] = &[
        (
            "accept",
            Some(
                "text/html,application/xhtml+xml,application/xml;q=0.9,\
image/avif,image/webp,*/*;q=0.8",
            ),
        ),
        ("accept-language", None),
        ("accept-encoding", Some(ACCEPT_ENCODING)),
        ("upgrade-insecure-requests", Some("1")),
        ("sec-fetch-dest", Some("document")),
        ("sec-fetch-mode", Some("navigate")),
        ("sec-fetch-site", Some("none")),
        ("sec-fetch-user", Some("?1")),
        ("referer", None),
        ("cookie", None),
    ];

    let mut out = Vec::new();
    out.push((
        HeaderName::from_static("user-agent"),
        HeaderValue::from_str(user_agent).unwrap_or_else(|_| HeaderValue::from_static("")),
    ));
    let mut written: Vec<&str> = vec![];
    for (name, default) in ORDERED_H2 {
        // Same as before the extract: a non-UTF-8 caller value falls through
        // to the default (or is omitted when there is none).
        let value = headers
            .get(*name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
            .or_else(|| default.map(str::to_string));
        if let Some(v) = value {
            out.push((
                HeaderName::from_static(*name),
                HeaderValue::from_str(&v).unwrap_or_else(|_| HeaderValue::from_static("")),
            ));
            written.push(name);
        }
    }
    for (name, value) in headers.iter() {
        let n = name.as_str();
        if written.contains(&n) || is_h2_illegal_request_header(n) || n == "user-agent" {
            continue;
        }
        out.push((name.clone(), value.clone()));
    }
    out
}

/// Undo the response's `Content-Encoding`, so callers always see the
/// resource's bytes. A decoded body drops `Content-Encoding` and
/// `Content-Length` (which described the encoded bytes).
///
/// A body cut off mid-stream keeps what decoded, as a truncated chunked
/// body does. An encoding we cannot undo is passed through untouched.
#[cfg(test)]
fn decode_content_encoding(body: Bytes, headers: &mut HeaderMap) -> Result<Bytes, HttpError> {
    decode_content_encoding_capped(body, headers, None)
}

/// As [`decode_content_encoding`], refusing to inflate past `max` bytes (a
/// small compressed body must not become an unbounded allocation).
fn decode_content_encoding_capped(
    body: Bytes,
    headers: &mut HeaderMap,
    max: Option<usize>,
) -> Result<Bytes, HttpError> {
    use std::io::Read;
    let limit = max.map_or(u64::MAX, |m| m as u64 + 1);

    let Some(encoding) = headers
        .get("content-encoding")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_ascii_lowercase())
    else {
        return Ok(body);
    };
    if body.is_empty() || encoding.is_empty() || encoding == "identity" {
        return Ok(body);
    }

    let mut out = Vec::new();
    let result = match encoding.as_str() {
        "gzip" | "x-gzip" => flate2::read::MultiGzDecoder::new(&body[..]).take(limit).read_to_end(&mut out),
        // "deflate" is zlib-wrapped per RFC 9110, but some servers send raw
        // deflate; browsers accept both.
        "deflate" => match flate2::read::ZlibDecoder::new(&body[..]).take(limit).read_to_end(&mut out) {
            Ok(n) => Ok(n),
            Err(_) if out.is_empty() => {
                flate2::read::DeflateDecoder::new(&body[..]).take(limit).read_to_end(&mut out)
            }
            Err(e) => Err(e),
        },
        // Not in `ACCEPT_ENCODING`, and sent all the same by some servers.
        "br" => brotli_decompressor::Decompressor::new(&body[..], 4096).take(limit).read_to_end(&mut out),
        other => {
            warn!(encoding = other, "Unsupported Content-Encoding; body left encoded");
            return Ok(body);
        }
    };
    if let Some(max) = max {
        if out.len() > max {
            return Err(HttpError::BodyTooLarge(max));
        }
    }
    if let Err(e) = result {
        if out.is_empty() {
            return Err(HttpError::InvalidResponse(format!(
                "Content-Encoding {encoding}: {e}"
            )));
        }
        warn!(%encoding, error = %e, decoded = out.len(), "Encoded body cut off; keeping what decoded");
    }
    headers.remove("content-encoding");
    headers.remove("content-length");
    Ok(Bytes::from(out))
}

/// Read response body based on headers.
async fn read_body<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    headers: &HeaderMap,
    max: Option<usize>,
) -> Result<Bytes, HttpError> {
    // Check for Content-Length
    if let Some(len) = headers
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<usize>().ok())
    {
        if let Some(max) = max {
            if len > max {
                return Err(HttpError::BodyTooLarge(max));
            }
        }
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).await?;
        return Ok(Bytes::from(buf));
    }

    // Check for chunked transfer encoding
    if let Some(te) = headers.get("transfer-encoding").and_then(|v| v.to_str().ok()) {
        if te.to_lowercase().contains("chunked") {
            return read_chunked_body(reader, max).await;
        }
    }

    // Read until EOF
    let mut buf = Vec::new();
    match max {
        Some(max) => {
            (&mut *reader).take(max as u64 + 1).read_to_end(&mut buf).await?;
            if buf.len() > max {
                return Err(HttpError::BodyTooLarge(max));
            }
        }
        None => {
            reader.read_to_end(&mut buf).await?;
        }
    }
    Ok(Bytes::from(buf))
}

/// Read chunked transfer encoding body.
async fn read_chunked_body<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    max: Option<usize>,
) -> Result<Bytes, HttpError> {
    let mut body = Vec::new();

    loop {
        let mut size_line = String::new();
        if reader.read_line(&mut size_line).await? == 0 {
            // The peer closed before the terminating 0-size chunk. Keep what
            // arrived, as Chrome does for a document: netflix.com's edge
            // intermittently closes at exactly 512 KiB of a ~660 KiB page,
            // and failing the whole navigation left a blank tab.
            warn!(received = body.len(), "chunked body truncated by EOF; using partial body");
            break;
        }

        // RFC 9112 §7.1.1: chunk-size may be followed by chunk extensions.
        let size_field = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_field, 16)
            .map_err(|_| HttpError::InvalidResponse("Invalid chunk size".to_string()))?;

        if size == 0 {
            // Read trailing CRLF
            let mut _trailer = String::new();
            let _ = reader.read_line(&mut _trailer).await;
            break;
        }

        if max.is_some_and(|max| body.len().saturating_add(size) > max) {
            return Err(HttpError::BodyTooLarge(max.unwrap_or(0)));
        }
        let mut chunk = Vec::with_capacity(size);
        (&mut *reader).take(size as u64).read_to_end(&mut chunk).await?;
        let complete = chunk.len() == size;
        body.extend_from_slice(&chunk);
        if !complete {
            warn!(received = body.len(), "chunked body truncated by EOF mid-chunk; using partial body");
            break;
        }

        // Read trailing CRLF after chunk
        let mut _crlf = [0u8; 2];
        let _ = reader.read_exact(&mut _crlf).await;
    }

    Ok(Bytes::from(body))
}

/// Streaming response for downloads.
pub struct StreamingResponse {
    /// HTTP status code.
    pub status: StatusCode,
    /// Response headers.
    pub headers: HeaderMap,
    /// Content length if known.
    pub content_length: Option<u64>,
    /// The underlying stream reader.
    reader: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
}

impl StreamingResponse {
    /// Read a chunk of data.
    pub async fn chunk(&mut self, buf: &mut [u8]) -> Result<usize, HttpError> {
        use tokio::io::AsyncReadExt;
        let n = self.reader.read(buf).await?;
        Ok(n)
    }
}

/// Client extension for streaming downloads.
impl Client {
    /// Start a streaming GET request (for downloads).
    pub async fn get_streaming(&self, url: &str) -> Result<StreamingResponse, HttpError> {
        let parsed_url = Url::parse(url).map_err(|e| HttpError::InvalidUrl(e.to_string()))?;

        let scheme = parsed_url.scheme();
        let host = parsed_url
            .host_str()
            .ok_or_else(|| HttpError::InvalidUrl("Missing host".to_string()))?;
        let port = parsed_url.port_or_known_default().unwrap_or(if scheme == "https" {
            443
        } else {
            80
        });

        match scheme {
            "https" => self.streaming_https(host, port, &parsed_url).await,
            "http" => self.streaming_http(host, port, &parsed_url).await,
            _ => Err(HttpError::UnsupportedScheme(scheme.to_string())),
        }
    }

    async fn streaming_https(
        &self,
        host: &str,
        port: u16,
        url: &Url,
    ) -> Result<StreamingResponse, HttpError> {
        let addr = format!("{}:{}", host, port);
        let stream = self.connect(host, port).await?;

        // Streaming stays HTTP/1.1 in this PR: the streaming reader is a
        // BufRead line/chunk parser. When h2 is negotiated we buffer via the
        // h2 path and stream from memory — correct, just not incremental;
        // incremental h2 streaming is the follow-up.
        let (tls_stream, negotiated) = self.connect_tls(host, &addr, stream).await?;
        if negotiated == NegotiatedProtocol::H2 {
            let (send, connection) = h2_client().handshake(tls_stream)
                .await
                .map_err(|e| HttpError::ConnectionFailed(format!("h2 handshake: {e}")))?;
            tokio::spawn(async move {
                let _ = connection.await;
            });
            let raw = self
                .dispatch_h2(send, &Method::GET, url, &HeaderMap::new(), &None)
                .await?;
            let len = raw.body.len() as u64;
            return Ok(StreamingResponse {
                status: raw.status,
                headers: raw.headers,
                content_length: Some(len),
                reader: Box::new(std::io::Cursor::new(raw.body)),
            });
        }

        self.send_streaming_request(tls_stream, host, url).await
    }

    async fn streaming_http(
        &self,
        host: &str,
        port: u16,
        url: &Url,
    ) -> Result<StreamingResponse, HttpError> {
        let stream = self.connect(host, port).await?;

        self.send_streaming_request(stream, host, url).await
    }

    async fn send_streaming_request<S>(
        &self,
        mut stream: S,
        host: &str,
        url: &Url,
    ) -> Result<StreamingResponse, HttpError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        // Build and send request
        let path = if let Some(query) = url.query() {
            format!("{}?{}", url.path(), query)
        } else {
            url.path().to_string()
        };
        let path = if path.is_empty() { "/" } else { &path };

        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: {}\r\nAccept: */*\r\nConnection: close\r\n\r\n",
            path, host, self.config.user_agent
        );

        stream.write_all(request.as_bytes()).await?;
        stream.flush().await?;

        // Read status and headers
        let mut reader = BufReader::new(stream);

        let mut status_line = String::new();
        reader.read_line(&mut status_line).await?;
        let (_, status) = parse_status_line(&status_line)?;

        let mut headers = HeaderMap::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await?;
            let line = line.trim();
            if line.is_empty() {
                break;
            }

            if let Some((name, value)) = line.split_once(':') {
                if let (Ok(n), Ok(v)) = (
                    HeaderName::try_from(name.trim()),
                    HeaderValue::try_from(value.trim()),
                ) {
                    headers.insert(n, v);
                }
            }
        }

        let content_length = headers
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok());

        Ok(StreamingResponse {
            status,
            headers,
            content_length,
            reader: Box::new(reader),
        })
    }
}

/// Blocking client for synchronous code (e.g., filter list downloads).
pub mod blocking {
    use super::*;

    /// Blocking HTTP client.
    pub struct Client {
        runtime: tokio::runtime::Runtime,
        inner: super::Client,
    }

    impl Client {
        /// Create a new blocking client with default config.
        pub fn new() -> Result<Self, HttpError> {
            Self::builder().build()
        }

        /// Create a client builder.
        pub fn builder() -> ClientBuilder {
            ClientBuilder {
                config: ClientConfig2::default(),
            }
        }

        /// Perform a blocking GET request.
        pub fn get(&self, url: &str) -> Result<Response, HttpError> {
            self.runtime.block_on(self.inner.get(url))
        }
    }

    /// Blocking client builder.
    pub struct ClientBuilder {
        config: ClientConfig2,
    }

    impl ClientBuilder {
        /// Set timeout.
        pub fn timeout(mut self, timeout: Duration) -> Self {
            self.config.timeout = timeout;
            self
        }

        /// Build the blocking client.
        pub fn build(self) -> Result<Client, HttpError> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| HttpError::IoError(io::Error::other(e)))?;

            let inner = super::Client::with_config(self.config)?;

            Ok(Client { runtime, inner })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h2_request_headers_strip_connection_specific_fields() {
        // RFC 9113 §8.1.2: these are illegal on h2. Before the extract they
        // lived only inside `send_request_h2`, so a regression could only be
        // caught by a live h2 negotiation.
        let mut headers = HeaderMap::new();
        headers.insert("connection", HeaderValue::from_static("keep-alive"));
        headers.insert("keep-alive", HeaderValue::from_static("timeout=5"));
        headers.insert("proxy-connection", HeaderValue::from_static("close"));
        headers.insert("transfer-encoding", HeaderValue::from_static("chunked"));
        headers.insert("upgrade", HeaderValue::from_static("h2c"));
        headers.insert("host", HeaderValue::from_static("evil.example"));
        headers.insert("x-custom", HeaderValue::from_static("ok"));
        headers.insert("referer", HeaderValue::from_static("https://doc.example/p"));

        let pairs = h2_request_headers("HiWave/test", &headers);
        let names: Vec<&str> = pairs.iter().map(|(n, _)| n.as_str()).collect();

        for illegal in [
            "connection",
            "keep-alive",
            "proxy-connection",
            "transfer-encoding",
            "upgrade",
            "host",
        ] {
            assert!(
                !names.contains(&illegal),
                "{illegal} must not appear on an h2 request: {names:?}"
            );
            assert!(is_h2_illegal_request_header(illegal));
        }

        assert_eq!(names[0], "user-agent");
        assert_eq!(pairs[0].1.to_str().unwrap(), "HiWave/test");
        assert!(names.contains(&"referer"));
        assert!(names.contains(&"x-custom"));
        assert!(names.contains(&"accept-encoding"));
        assert!(!is_h2_illegal_request_header("referer"));
        assert!(!is_h2_illegal_request_header("x-custom"));
    }

    #[test]
    fn test_parse_status_line() {
        let (version, status) = parse_status_line("HTTP/1.1 200 OK\r\n").unwrap();
        assert_eq!(version, Version::HTTP_11);
        assert_eq!(status, StatusCode::OK);

        let (version, status) = parse_status_line("HTTP/1.0 404 Not Found").unwrap();
        assert_eq!(version, Version::HTTP_10);
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn an_empty_platform_root_load_is_not_cached_for_the_life_of_the_process() {
        use tokio_rustls::rustls::pki_types::{Der, TrustAnchor};
        use tokio_rustls::rustls::RootCertStore;
        let one_anchor = || RootCertStore {
            roots: vec![TrustAnchor {
                subject: Der::from_slice(b"subject"),
                subject_public_key_info: Der::from_slice(b"spki"),
                name_constraints: None,
            }],
        };
        let cache: RootsCache = std::sync::Mutex::new(None);
        let mut loads = 0;
        assert!(
            roots_cached(&cache, || {
                loads += 1;
                (RootCertStore::empty(), String::new())
            })
            .is_err(),
            "an empty load is an error"
        );
        let got = roots_cached(&cache, || {
            loads += 1;
            (one_anchor(), String::new())
        });
        assert!(got.is_ok(), "a later successful load must be used, not the earlier empty one");
        assert_eq!(loads, 2);
        assert!(
            roots_cached(&cache, || {
                loads += 1;
                (RootCertStore::empty(), String::new())
            })
            .is_ok()
        );
        assert_eq!(loads, 2, "a good store is cached and not reloaded");
    }

    #[test]
    fn an_empty_platform_root_error_says_why() {
        use tokio_rustls::rustls::RootCertStore;
        let cache: RootsCache = std::sync::Mutex::new(None);
        let err = roots_cached(&cache, || {
            (RootCertStore::empty(), "2 load errors, first: keychain busy".to_string())
        })
        .unwrap_err()
        .to_string();
        assert!(err.contains("keychain busy"), "the platform's reason must reach the caller: {err}");
        assert!(err.contains("no usable platform root certificates"), "{err}");
    }

    #[test]
    fn test_client_builder() {
        let client = Client::builder()
            .user_agent("TestAgent/1.0")
            .timeout(Duration::from_secs(10))
            .redirect(true, 5)
            .build()
            .unwrap();

        assert_eq!(client.config.user_agent, "TestAgent/1.0");
        assert_eq!(client.config.timeout, Duration::from_secs(10));
        assert_eq!(client.config.max_redirects, 5);
    }

    #[test]
    fn test_response_helpers() {
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("text/html"));
        headers.insert("content-length", HeaderValue::from_static("1234"));

        let response = Response {
            status: StatusCode::OK,
            version: Version::HTTP_11,
            headers,
            body: Bytes::from("Hello"),
            url: Url::parse("https://example.com").unwrap(),
            encoded_body_size: 5,
            redirect_chain: Vec::new(),
        };

        assert!(response.is_success());
        assert_eq!(response.content_type(), Some("text/html"));
        assert_eq!(response.content_length(), Some(1234));
        assert_eq!(response.text().unwrap(), "Hello");
    }

    fn decode_chunked(raw: &[u8]) -> Result<Bytes, HttpError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut reader = BufReader::new(raw);
        rt.block_on(read_chunked_body(&mut reader, None))
    }

    #[test]
    fn chunked_body_decodes_extensions_and_terminator() {
        let body = decode_chunked(b"5;name=val\r\nhello\r\n6\r\n world\r\n0\r\n\r\n").unwrap();
        assert_eq!(&body[..], b"hello world");
    }

    #[test]
    fn chunked_body_truncated_by_eof_keeps_what_arrived() {
        // EOF where the next size line should be (netflix.com's edge closes
        // at 512 KiB), and EOF inside a chunk: both used to fail the whole
        // navigation with "Invalid chunk size" / UnexpectedEof.
        let at_boundary = decode_chunked(b"5\r\nhello\r\n").unwrap();
        assert_eq!(&at_boundary[..], b"hello");
        let mid_chunk = decode_chunked(b"5\r\nhello\r\na\r\n wor").unwrap();
        assert_eq!(&mid_chunk[..], b"hello wor");
    }

    #[test]
    fn chunked_body_rejects_a_garbage_size_line() {
        assert!(matches!(
            decode_chunked(b"zz\r\nhello\r\n0\r\n\r\n"),
            Err(HttpError::InvalidResponse(_))
        ));
    }

    pub(super) fn gzip(data: &[u8]) -> Vec<u8> {
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut enc, data).unwrap();
        enc.finish().unwrap()
    }

    #[test]
    fn requests_gzip_and_decodes_it() {
        // Serves gzip only to a client that asks for it, as real sites do;
        // anything else gets the page a bot gets.
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&buf[..n]),
                }
            }
            let request = String::from_utf8_lossy(&request).to_ascii_lowercase();
            if request.contains("\r\naccept-encoding: gzip") {
                let body = gzip(b"<p>the real page</p>");
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            } else {
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\n\r\nautomated bot")
                    .unwrap();
            }
        });

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let client = Client::builder().build().unwrap();
        let response = rt
            .block_on(client.request(
                Method::GET,
                &format!("http://127.0.0.1:{port}/"),
                HeaderMap::new(),
                None,
            ))
            .unwrap();
        assert_eq!(response.text().unwrap(), "<p>the real page</p>");
        assert!(response.headers.get("content-encoding").is_none());
        assert!(response.headers.get("content-length").is_none());
    }

    #[test]
    fn requests_chunked_gzip_and_decodes_after_reassembling_chunks() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&buf[..n]),
                }
            }

            let body = gzip(b"<p>chunked and compressed</p>");
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\
                      Content-Encoding: gzip\r\n\r\n",
                )
                .unwrap();
            for chunk in body.chunks(7) {
                write!(stream, "{:x};part=test\r\n", chunk.len()).unwrap();
                stream.write_all(chunk).unwrap();
                stream.write_all(b"\r\n").unwrap();
            }
            stream.write_all(b"0\r\n\r\n").unwrap();
        });

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let client = Client::builder().build().unwrap();
        let response = rt
            .block_on(client.request(
                Method::GET,
                &format!("http://127.0.0.1:{port}/"),
                HeaderMap::new(),
                None,
            ))
            .unwrap();

        server.join().unwrap();
        assert_eq!(response.text().unwrap(), "<p>chunked and compressed</p>");
        assert!(response.headers.get("content-encoding").is_none());
    }

    #[test]
    fn content_encoding_decodes_deflate_both_ways_and_keeps_a_truncated_prefix() {
        let decode = |encoding: &str, body: Vec<u8>| {
            let mut headers = HeaderMap::new();
            headers.insert("content-encoding", HeaderValue::from_str(encoding).unwrap());
            decode_content_encoding(Bytes::from(body), &mut headers)
        };
        let text = b"hello hello hello hello world".repeat(50);

        let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut zlib, &text).unwrap();
        assert_eq!(&decode("deflate", zlib.finish().unwrap()).unwrap()[..], &text[..]);

        let mut raw = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut raw, &text).unwrap();
        assert_eq!(&decode("deflate", raw.finish().unwrap()).unwrap()[..], &text[..]);

        let big: Vec<u8> = (0..200_000u32).flat_map(|i| i.to_le_bytes()).collect();
        let mut cut = gzip(&big);
        cut.truncate(cut.len() / 2);
        let partial = decode("gzip", cut).unwrap();
        assert!(!partial.is_empty() && big.starts_with(&partial));

        // Cannot be undone here: left alone.
        assert_eq!(&decode("zstd", b"xyz".to_vec()).unwrap()[..], b"xyz");
        assert!(decode("gzip", b"not gzip".to_vec()).is_err());
    }

    /// `"hello hello hello hello world"` fifty times (1450 bytes), as Node's
    /// `zlib.brotliCompressSync` encodes it.
    const BROTLI_HELLO: [u8; 26] = [
        0x1b, 0xa9, 0x05, 0xe0, 0x8c, 0x94, 0x6e, 0xde, 0x44, 0x55, 0x86, 0x96, 0x6c, 0x20, 0x6f,
        0x39, 0xc8, 0x3c, 0x86, 0xd5, 0x81, 0x13, 0xe2, 0x88, 0x8f, 0x1a,
    ];

    /// Some servers send `Content-Encoding: br` to a client that did not ask
    /// for it (seen on web fonts in a hand test); the body must still reach
    /// the caller as the resource's bytes.
    #[test]
    fn content_encoding_decodes_brotli_sent_unasked() {
        let text = b"hello hello hello hello world".repeat(50);

        let mut headers = HeaderMap::new();
        headers.insert("content-encoding", HeaderValue::from_static("br"));
        headers.insert("content-length", HeaderValue::from_static("26"));
        let decoded = decode_content_encoding(Bytes::from(BROTLI_HELLO.to_vec()), &mut headers).unwrap();
        assert_eq!(&decoded[..], &text[..]);
        assert!(headers.get("content-encoding").is_none());
        assert!(headers.get("content-length").is_none());

        // The size cap holds for brotli as for gzip: 26 bytes must not become
        // more than the caller allows.
        let mut headers = HeaderMap::new();
        headers.insert("content-encoding", HeaderValue::from_static("br"));
        let capped = decode_content_encoding_capped(Bytes::from(BROTLI_HELLO.to_vec()), &mut headers, Some(1000));
        assert!(matches!(capped, Err(HttpError::BodyTooLarge(1000))));

        // A body that is not brotli is an error, as for gzip, and so is one
        // cut off before anything decoded.
        let decode = |body: Vec<u8>| {
            let mut headers = HeaderMap::new();
            headers.insert("content-encoding", HeaderValue::from_static("br"));
            decode_content_encoding(Bytes::from(body), &mut headers)
        };
        assert!(decode(vec![0xff; 16]).is_err());
        assert!(decode(BROTLI_HELLO[..20].to_vec()).is_err());

        // We still do not ask for it.
        assert!(!ACCEPT_ENCODING.contains("br"));
    }

    /// The same over a socket: the request asks for gzip and deflate only,
    /// the server answers in brotli, the caller gets the text.
    #[test]
    fn a_brotli_response_to_a_request_that_did_not_ask_is_decoded() {
        use std::io::{Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&buf[..n]),
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Encoding: br\r\nContent-Length: 26\r\n\r\n")
                .unwrap();
            stream.write_all(&BROTLI_HELLO).unwrap();
            String::from_utf8_lossy(&request).to_ascii_lowercase()
        });

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let client = Client::builder().build().unwrap();
        let response = rt
            .block_on(client.request(
                Method::GET,
                &format!("http://127.0.0.1:{port}/"),
                HeaderMap::new(),
                None,
            ))
            .unwrap();

        let request = server.join().unwrap();
        assert!(request.contains("\r\naccept-encoding: gzip, deflate\r\n"), "{request}");
        assert_eq!(response.text().unwrap(), "hello hello hello hello world".repeat(50));
        assert!(response.headers.get("content-encoding").is_none());
    }

    #[test]
    fn test_default_config() {
        let config = ClientConfig2::default();
        assert!(config.user_agent.starts_with("Mozilla/5.0 ("));
        assert!(config.user_agent.contains("HiWave/1.0"));
        assert!(config.user_agent.contains("RustKit/1.0"));
        assert!(!config.user_agent.contains("Chrome"), "never Chrome\'s UA");
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.max_redirects, 10);
        assert!(config.follow_redirects);
    }
}


#[cfg(test)]
mod governed_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Serve `reply` to each connection until the listener is dropped; count accepts.
    async fn serve(reply: Vec<u8>) -> (u16, Arc<std::sync::atomic::AtomicUsize>) {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let h = hits.clone();
        tokio::spawn(async move {
            while let Ok((mut s, _)) = l.accept().await {
                h.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let reply = reply.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = s.read(&mut buf).await;
                    let _ = s.write_all(&reply).await;
                    let _ = s.shutdown().await;
                });
            }
        });
        (port, hits)
    }

    fn client() -> Client {
        Client::new().unwrap()
    }

    #[tokio::test]
    async fn a_public_only_client_never_connects_to_loopback_on_any_site() {
        let (port, hits) = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi".to_vec()).await;
        let c = client().with_address_policy(AddressPolicy::PublicOnly);
        for url in [
            format!("http://127.0.0.1:{port}/"),
            format!("http://localhost:{port}/"),
            format!("http://[::1]:{port}/"),
        ] {
            let r = c.get(&url).await;
            assert!(matches!(r, Err(HttpError::AddressDenied(_))), "{url}: {r:?}");
            let r = c.get_streaming(&url).await;
            assert!(matches!(r, Err(HttpError::AddressDenied(_))), "streaming {url}");
        }
        // https goes through the same connect: refused before the handshake.
        let r = c.get(&format!("https://127.0.0.1:{port}/")).await;
        assert!(matches!(r, Err(HttpError::AddressDenied(_))), "https: {r:?}");
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 0, "a socket was opened");
    }

    #[tokio::test]
    async fn the_default_client_still_reaches_loopback() {
        let (port, hits) = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi".to_vec()).await;
        let r = client().get(&format!("http://127.0.0.1:{port}/")).await.unwrap();
        assert_eq!(r.text().unwrap(), "hi");
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn without_following_a_redirect_comes_back_as_the_response() {
        let (target_port, target_hits) = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi".to_vec()).await;
        let reply = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{target_port}/x\r\nContent-Length: 0\r\n\r\n"
        );
        let (port, _) = serve(reply.into_bytes()).await;
        let r = client()
            .with_follow_redirects(false)
            .get(&format!("http://127.0.0.1:{port}/"))
            .await
            .unwrap();
        assert_eq!(r.status.as_u16(), 302);
        assert_eq!(r.header("location"), Some(&format!("http://127.0.0.1:{target_port}/x")[..]));
        assert_eq!(target_hits.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_followed_redirect_chain_and_the_encoded_size_are_kept() {
        let body = super::tests::gzip(b"<p>the real page</p>");
        let mut reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .into_bytes();
        reply.extend_from_slice(&body);
        let (target_port, _) = serve(reply).await;
        let hop = format!(
            "HTTP/1.1 301 Moved Permanently\r\nLocation: http://127.0.0.1:{target_port}/end\r\nContent-Length: 0\r\n\r\n"
        );
        let (hop_port, _) = serve(hop.into_bytes()).await;
        let first = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{hop_port}/hop\r\nContent-Length: 0\r\n\r\n"
        );
        let (port, _) = serve(first.into_bytes()).await;
        let r = client().get(&format!("http://127.0.0.1:{port}/start")).await.unwrap();
        assert_eq!(r.text().unwrap(), "<p>the real page</p>");
        assert_eq!(r.encoded_body_size, body.len() as u64);
        let chain: Vec<(String, u16)> = r
            .redirect_chain
            .iter()
            .map(|(url, status)| (url.to_string(), status.as_u16()))
            .collect();
        assert_eq!(
            chain,
            vec![
                (format!("http://127.0.0.1:{port}/start"), 302),
                (format!("http://127.0.0.1:{hop_port}/hop"), 301),
            ]
        );
        assert_eq!(r.url.as_str(), format!("http://127.0.0.1:{target_port}/end"));
    }

    #[tokio::test]
    async fn a_body_cap_refuses_content_length_chunked_eof_and_gzip_bombs() {
        let small = client().with_max_body(10);
        let cl = format!("HTTP/1.1 200 OK\r\nContent-Length: 20\r\n\r\n{}", "x".repeat(20));
        let (p, _) = serve(cl.into_bytes()).await;
        let r = small.get(&format!("http://127.0.0.1:{p}/")).await;
        assert!(matches!(r, Err(HttpError::BodyTooLarge(10))), "content-length: {r:?}");

        let chunked = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            20,
            "x".repeat(20)
        );
        let (p, _) = serve(chunked.into_bytes()).await;
        let r = small.get(&format!("http://127.0.0.1:{p}/")).await;
        assert!(matches!(r, Err(HttpError::BodyTooLarge(10))), "chunked: {r:?}");

        let eof = format!("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{}", "x".repeat(20));
        let (p, _) = serve(eof.into_bytes()).await;
        let r = small.get(&format!("http://127.0.0.1:{p}/")).await;
        assert!(matches!(r, Err(HttpError::BodyTooLarge(10))), "eof: {r:?}");

        // 1 MiB of zeros gzips to about a kilobyte: small on the wire, large decoded.
        use flate2::write::GzEncoder;
        let mut enc = GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(&vec![0u8; 1 << 20]).unwrap();
        let gz = enc.finish().unwrap();
        let mut reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {}\r\n\r\n",
            gz.len()
        )
        .into_bytes();
        reply.extend_from_slice(&gz);
        let (p, _) = serve(reply).await;
        let r = small.get(&format!("http://127.0.0.1:{p}/")).await;
        assert!(matches!(r, Err(HttpError::BodyTooLarge(10))), "gzip bomb: {r:?}");

        // Under the cap is untouched.
        let ok = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
        let (p, _) = serve(ok.as_bytes().to_vec()).await;
        let r = small.get(&format!("http://127.0.0.1:{p}/")).await.unwrap();
        assert_eq!(r.text().unwrap(), "hello");
    }

    /// Two GETs to one host share the connection. Before the pool each request
    /// opened its own, and a server that only reads a few of them (the
    /// microsoft.com stylesheet stalls) never answered the rest.
    #[tokio::test]
    async fn h2_requests_to_one_host_share_a_connection() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let accepts = std::sync::Arc::new(AtomicUsize::new(0));
        let seen = accepts.clone();
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else {
                    break;
                };
                seen.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut connection = match h2::server::handshake(sock).await {
                        Ok(connection) => connection,
                        Err(_) => return,
                    };
                    while let Some(next) = connection.accept().await {
                        let (_request, mut respond) = match next {
                            Ok(parts) => parts,
                            Err(_) => break,
                        };
                        let response = http::Response::builder().status(200).body(()).unwrap();
                        let mut body = match respond.send_response(response, false) {
                            Ok(body) => body,
                            Err(_) => break,
                        };
                        let _ = body.send_data(Bytes::from_static(b"sheet"), true);
                    }
                });
            }
        });

        let client = client();
        let url_a = format!("http://{addr}/a.css");
        let url_b = format!("http://{addr}/b.css");
        let (a, b) = tokio::join!(
            client.request_h2_cleartext(&url_a),
            client.request_h2_cleartext(&url_b),
        );
        assert_eq!(a.unwrap().body.as_ref(), b"sheet");
        assert_eq!(b.unwrap().body.as_ref(), b"sheet");
        let third = client
            .request_h2_cleartext(&format!("http://{addr}/c.css"))
            .await
            .unwrap();
        assert_eq!(third.body.as_ref(), b"sheet");
        assert_eq!(
            accepts.load(Ordering::SeqCst),
            1,
            "a second stylesheet must be a stream on the host connection, not a new socket"
        );
    }

    /// A fetch that is not being polled (a parser image parked while a load
    /// yields) still receives its body into the connection's flow-control
    /// window. With h2's default 64 KB connection window, 200 KB of it left
    /// unread stalls every other stream on the host until the client timeout:
    /// the 12:15 head-of-line hang (Z lane, 2026-10-21).
    #[tokio::test]
    async fn an_unread_h2_body_does_not_stall_the_hosts_other_streams() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let Ok((sock, _)) = listener.accept().await else {
                return;
            };
            let mut connection = match h2::server::handshake(sock).await {
                Ok(connection) => connection,
                Err(_) => return,
            };
            while let Some(next) = connection.accept().await {
                let (request, mut respond) = match next {
                    Ok(parts) => parts,
                    Err(_) => break,
                };
                let response = http::Response::builder().status(200).body(()).unwrap();
                let Ok(mut body) = respond.send_response(response, false) else {
                    break;
                };
                if request.uri().path() == "/big.png" {
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                        let _ = body.send_data(Bytes::from(vec![7u8; 200 * 1024]), true);
                    });
                } else {
                    let _ = body.send_data(Bytes::from_static(b"ok"), true);
                }
            }
        });

        let client = client();
        let big_url = format!("http://{addr}/big.png");
        let big = client.request_h2_cleartext(&big_url);
        tokio::pin!(big);
        // Polled until it waits for its body, then parked: never polled again.
        let parked = tokio::time::timeout(std::time::Duration::from_millis(150), &mut big).await;
        assert!(parked.is_err(), "the big body is sent after 300 ms");
        // The big body arrives meanwhile, into the parked stream.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let small = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            client.request_h2_cleartext(&format!("http://{addr}/small.js")),
        )
        .await;
        let small = small.expect("a parked stream's unread body stalled the host's other streams");
        assert_eq!(small.unwrap().body.as_ref(), b"ok");
    }

    /// A burst to a fresh connection goes out before the server's SETTINGS
    /// are read. h2 lets the client open 100 streams until then, so a server
    /// whose limit is lower refuses the rest (walmart's ~113 scripts got
    /// REFUSED_STREAM at its limit). Streams over the limit must wait instead.
    #[tokio::test]
    async fn a_burst_waits_for_the_servers_stream_limit() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let Ok((sock, _)) = listener.accept().await else {
                return;
            };
            let mut connection = match h2::server::Builder::new()
                .max_concurrent_streams(2)
                .handshake::<_, Bytes>(sock)
                .await
            {
                Ok(connection) => connection,
                Err(_) => return,
            };
            while let Some(next) = connection.accept().await {
                let (_request, mut respond) = match next {
                    Ok(parts) => parts,
                    Err(_) => break,
                };
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    let response = http::Response::builder().status(200).body(()).unwrap();
                    if let Ok(mut body) = respond.send_response(response, false) {
                        let _ = body.send_data(Bytes::from_static(b"js"), true);
                    }
                });
            }
        });

        let client = client();
        let urls: Vec<String> = (0..6).map(|i| format!("http://{addr}/s{i}.js")).collect();
        let results = futures::future::join_all(urls.iter().map(|u| client.request_h2_cleartext(u))).await;
        for (url, result) in urls.iter().zip(results) {
            match result {
                Ok(response) => assert_eq!(response.body.as_ref(), b"js", "{url}"),
                Err(e) => panic!("{url} failed over the server's stream limit: {e:?}"),
            }
        }
    }

    /// How the h2 fixture below answers on its first `bad` connections.
    #[derive(Clone, Copy)]
    enum Unprocessed {
        /// RST_STREAM(REFUSED_STREAM) on every stream.
        Refuse,
        /// GOAWAY(NO_ERROR) before any stream is read, so every stream is
        /// above its last stream id.
        GoAway,
    }

    /// An h2 server whose first `bad` connections leave every stream
    /// unprocessed (RFC 9113 §8.7); later connections answer `ok`. Returns
    /// the address and the accept count.
    async fn serve_h2_unprocessed(
        how: Unprocessed,
        bad: usize,
    ) -> (std::net::SocketAddr, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let accepts = std::sync::Arc::new(AtomicUsize::new(0));
        let seen = accepts.clone();
        tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else {
                    break;
                };
                let nth = seen.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let Ok(mut connection) = h2::server::handshake(sock).await else {
                        return;
                    };
                    if nth < bad && matches!(how, Unprocessed::GoAway) {
                        connection.abrupt_shutdown(h2::Reason::NO_ERROR);
                        while let Some(next) = connection.accept().await {
                            if next.is_err() {
                                break;
                            }
                        }
                        return;
                    }
                    while let Some(next) = connection.accept().await {
                        let Ok((_request, mut respond)) = next else {
                            break;
                        };
                        if nth < bad {
                            respond.send_reset(h2::Reason::REFUSED_STREAM);
                            continue;
                        }
                        let response = http::Response::builder().status(200).body(()).unwrap();
                        if let Ok(mut body) = respond.send_response(response, false) {
                            let _ = body.send_data(Bytes::from_static(b"ok"), true);
                        }
                    }
                });
            }
        });
        (addr, accepts)
    }

    /// A refused stream was never processed (RFC 9113 §8.7): an idempotent
    /// request is sent again once, on a new connection.
    #[tokio::test]
    async fn a_refused_h2_stream_is_retried_once_on_a_new_connection() {
        let (addr, accepts) = serve_h2_unprocessed(Unprocessed::Refuse, 1).await;
        let response = client()
            .request_h2_cleartext(&format!("http://{addr}/a.js"))
            .await
            .expect("a refused GET is retried on a new connection");
        assert_eq!(response.body.as_ref(), b"ok");
        assert_eq!(accepts.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    /// A stream above a GOAWAY's last stream id was never processed either.
    #[tokio::test]
    async fn an_h2_stream_above_a_goaway_is_retried_once_on_a_new_connection() {
        let (addr, accepts) = serve_h2_unprocessed(Unprocessed::GoAway, 1).await;
        let response = client()
            .request_h2_cleartext(&format!("http://{addr}/a.js"))
            .await
            .expect("a GET above the GOAWAY is retried on a new connection");
        assert_eq!(response.body.as_ref(), b"ok");
        assert_eq!(accepts.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    /// Once only: a host that refuses on the new connection too is an error,
    /// not a loop.
    #[tokio::test]
    async fn an_h2_retry_happens_at_most_once() {
        let (addr, accepts) = serve_h2_unprocessed(Unprocessed::Refuse, usize::MAX).await;
        let result = client()
            .request_h2_cleartext(&format!("http://{addr}/a.js"))
            .await;
        assert!(result.is_err(), "a GET refused on every connection succeeded");
        assert_eq!(accepts.load(std::sync::atomic::Ordering::SeqCst), 2);
    }
}
