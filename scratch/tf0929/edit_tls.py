p = '/Users/petecopeland/Repos/.worktrees/rs-grid-fixed-tracks/crates/rustkit-http/src/lib.rs'
t = open(p).read()

edits = [
# 1. Client carries the http/1.1-only connector too.
('''    #[cfg(not(feature = "native-tls"))]
    tls_connector: TlsConnector,
    #[cfg(feature = "native-tls")]
    tls_connector: tokio_native_tls::TlsConnector,
}
''',
'''    #[cfg(not(feature = "native-tls"))]
    tls_connector: TlsConnector,
    /// http/1.1-only offer for the negotiated downgrade in `connect_tls`.
    /// Built once here, not per connection.
    #[cfg(not(feature = "native-tls"))]
    h1_connector: TlsConnector,
    #[cfg(feature = "native-tls")]
    tls_connector: tokio_native_tls::TlsConnector,
}

/// The platform root store, loaded once per process. `load_native_certs`
/// walks the macOS keychain's trust settings and costs seconds: loaded per
/// `Client` it added ~5 s to every engine start (two clients), and loaded
/// per h2-selecting connection it added ~2.5 s to every request to reddit,
/// google, x and most large origins, pushing them past the 30 s load budget.
#[cfg(not(feature = "native-tls"))]
fn platform_roots() -> Result<Arc<tokio_rustls::rustls::RootCertStore>, HttpError> {
    static ROOTS: std::sync::OnceLock<Arc<tokio_rustls::rustls::RootCertStore>> =
        std::sync::OnceLock::new();
    let roots = ROOTS.get_or_init(|| {
        let mut roots = tokio_rustls::rustls::RootCertStore::empty();
        for cert in rustls_native_certs::load_native_certs().certs {
            // A single unparseable platform cert must not kill the store.
            let _ = roots.add(cert);
        }
        Arc::new(roots)
    });
    if roots.is_empty() {
        return Err(HttpError::TlsError(
            "no usable platform root certificates".into(),
        ));
    }
    Ok(roots.clone())
}
'''),
# 2. with_config uses the shared store and builds both connectors.
('''        let mut roots = tokio_rustls::rustls::RootCertStore::empty();
        let native = rustls_native_certs::load_native_certs();
        for cert in native.certs {
            // A single unparseable platform cert must not kill the store.
            let _ = roots.add(cert);
        }
        if roots.is_empty() {
            return Err(HttpError::TlsError(
                "no usable platform root certificates".into(),
            ));
        }

        let mut tls_config = tokio_rustls::rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();''',
'''        let roots = platform_roots()?;

        let mut tls_config = tokio_rustls::rustls::ClientConfig::builder()
            .with_root_certificates(roots.clone())
            .with_no_client_auth();'''),
('''        let tls_connector = TlsConnector::from(Arc::new(tls_config));

        Ok(Self {
            config,
            tls_connector,
        })
    }

    /// Open a TLS connection speaking HTTP/1.1, honestly.''',
'''        let tls_connector = TlsConnector::from(Arc::new(tls_config));

        let mut h1_only = tokio_rustls::rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        h1_only.alpn_protocols = vec![b"http/1.1".to_vec()];
        let h1_connector = TlsConnector::from(Arc::new(h1_only));

        Ok(Self {
            config,
            tls_connector,
            h1_connector,
        })
    }

    /// Open a TLS connection speaking HTTP/1.1, honestly.'''),
# 3. connect_tls reuses the prebuilt connector.
('''            let mut roots = tokio_rustls::rustls::RootCertStore::empty();
            for cert in rustls_native_certs::load_native_certs().certs {
                let _ = roots.add(cert);
            }
            let mut h1_only = tokio_rustls::rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            h1_only.alpn_protocols = vec![b"http/1.1".to_vec()];
            let connector = TlsConnector::from(Arc::new(h1_only));
            let fresh = tokio::net::TcpStream::connect(addr)
                .await
                .map_err(|e| HttpError::ConnectionFailed(e.to_string()))?;
            return connector
                .connect(server_name, fresh)''',
'''            let fresh = tokio::net::TcpStream::connect(addr)
                .await
                .map_err(|e| HttpError::ConnectionFailed(e.to_string()))?;
            return self
                .h1_connector
                .connect(server_name, fresh)'''),
]
for o, n in edits:
    assert t.count(o) == 1, o[:80]
    t = t.replace(o, n)
open(p, 'w').write(t)
print('ok')
