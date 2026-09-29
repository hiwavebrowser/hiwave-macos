p = '/Users/petecopeland/Repos/.worktrees/rs-grid-fixed-tracks/crates/rustkit-http/src/lib.rs'
t = open(p).read()
o = '''    let roots = ROOTS.get_or_init(|| {
        let mut roots = tokio_rustls::rustls::RootCertStore::empty();'''
n = '''    let roots = ROOTS.get_or_init(|| {
        ROOT_LOADS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut roots = tokio_rustls::rustls::RootCertStore::empty();'''
assert t.count(o) == 1
t = t.replace(o, n)
o = '''#[cfg(not(feature = "native-tls"))]
fn platform_roots()'''
n = '''#[cfg(not(feature = "native-tls"))]
static ROOT_LOADS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(not(feature = "native-tls"))]
fn platform_roots()'''
assert t.count(o) == 1
t = t.replace(o, n)
o = '''    #[test]
    fn test_default_config() {'''
n = '''    #[cfg(not(feature = "native-tls"))]
    #[test]
    fn the_platform_root_store_is_loaded_once_per_process() {
        // Every Client, and every h2 -> http/1.1 downgrade, shares one load.
        let clients: Vec<Client> = (0..3).map(|_| Client::new().expect("client")).collect();
        assert_eq!(clients.len(), 3);
        assert!(Arc::ptr_eq(&platform_roots().unwrap(), &platform_roots().unwrap()));
        assert_eq!(ROOT_LOADS.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn test_default_config() {'''
assert t.count(o) == 1
t = t.replace(o, n)
open(p, 'w').write(t)
print('ok')
