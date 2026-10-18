//! One HTTP/2 connection per host.
//!
//! Each request used to open its own connection and `tokio::spawn` a driver
//! for it. A page's script fetches start in the same turn as its render-blocking
//! stylesheets, so a host that will only run a few connections (microsoft.com's
//! stylesheets, every bin since the script lists landed) never read the sheet
//! connections. They sat until the client's 30s timeout. Streams on one
//! connection wait on the server's `MAX_CONCURRENT_STREAMS` instead.

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use h2::client::SendRequest;
use tokio::sync::Mutex;

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct H2Key {
    pub host: String,
    pub port: u16,
    /// [`AddressPolicy`](crate::AddressPolicy) tag. A custom policy does not
    /// share a connection with another policy.
    pub policy: u8,
}

pub(crate) const POLICY_ANY: u8 = 0;
pub(crate) const POLICY_PUBLIC: u8 = 1;
pub(crate) const POLICY_CUSTOM: u8 = 2;

pub(crate) struct H2Pool {
    slots: Mutex<HashMap<H2Key, Arc<Mutex<Option<SendRequest<Bytes>>>>>>,
}

impl H2Pool {
    pub(crate) fn new() -> Self {
        Self {
            slots: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) async fn slot(&self, key: H2Key) -> Arc<Mutex<Option<SendRequest<Bytes>>>> {
        let mut map = self.slots.lock().await;
        map.entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone()
    }
}

impl Default for H2Pool {
    fn default() -> Self {
        Self::new()
    }
}
