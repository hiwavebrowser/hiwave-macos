//! Password manager and secure storage

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use rand::RngCore;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use url::Url;
use hiwave_core::HiWaveResult;

const LEGACY_SALT: &[u8] = b"pureflow-salt";
const SALT_LEN: usize = 16;
const VERIFY_LABEL: &[u8] = b"zen-vault-verifier";
const NONCE_LEN: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    pub id: i64,
    pub url: String,
    pub username: String,
    #[serde(skip)]
    pub password: String,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct Vault {
    conn: Connection,
    master_key: Option<Vec<u8>>,
}

impl Vault {
    pub fn new<P: AsRef<Path>>(db_path: P) -> HiWaveResult<Self> {
        log::info!("Opening vault at {:?}", db_path.as_ref());

        let conn = Connection::open(db_path)
            .map_err(|e| hiwave_core::HiWaveError::Vault(format!("Failed to open database: {}", e)))?;

        // Create tables
        conn.execute(
            "CREATE TABLE IF NOT EXISTS credentials (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                username TEXT NOT NULL,
                password_encrypted BLOB NOT NULL,
                nonce BLOB NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| hiwave_core::HiWaveError::Vault(format!("Failed to create table: {}", e)))?;

        conn.execute("CREATE INDEX IF NOT EXISTS idx_url ON credentials(url)", [])
            .map_err(|e| hiwave_core::HiWaveError::Vault(format!("Failed to create index: {}", e)))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS vault_meta (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                salt BLOB NOT NULL,
                verifier BLOB NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| {
            hiwave_core::HiWaveError::Vault(format!("Failed to create vault metadata: {}", e))
        })?;

        Ok(Self {
            conn,
            master_key: None,
        })
    }

    pub fn unlock(&mut self, master_password: &str) -> HiWaveResult<()> {
        log::info!("Unlocking vault");

        if let Some((salt, stored_verifier)) = self.load_meta()? {
            let key = derive_key(master_password, &salt);
            let verifier = derive_verifier(&key);

            if !constant_time_eq(&verifier, &stored_verifier) {
                return Err(hiwave_core::HiWaveError::Vault(
                    "Invalid master password".to_string(),
                ));
            }

            self.master_key = Some(key.to_vec());
            return Ok(());
        }

        let sample = self.sample_credential()?;
        let (salt, key) = if let Some((encrypted, nonce)) = sample {
            let salt = LEGACY_SALT.to_vec();
            let key = derive_key(master_password, &salt);
            self.decrypt_password(&encrypted, &nonce, &key)
                .map_err(|_| hiwave_core::HiWaveError::Vault("Invalid master password".to_string()))?;
            (salt, key)
        } else {
            let mut salt = vec![0u8; SALT_LEN];
            OsRng.fill_bytes(&mut salt);
            let key = derive_key(master_password, &salt);
            (salt, key)
        };

        let verifier = derive_verifier(&key);
        self.store_meta(&salt, &verifier)?;
        self.master_key = Some(key.to_vec());

        Ok(())
    }

    pub fn lock(&mut self) {
        log::info!("Locking vault");
        self.master_key = None;
    }

    pub fn is_unlocked(&self) -> bool {
        self.master_key.is_some()
    }

    pub fn save_credential(&mut self, url: &Url, username: &str, password: &str) -> HiWaveResult<i64> {
        if !self.is_unlocked() {
            return Err(hiwave_core::HiWaveError::Vault("Vault is locked".to_string()));
        }

        let domain = Self::normalize_domain(url)?;
        log::info!("Saving credential for {}", domain);

        let master_key = self.master_key.as_ref().unwrap();

        // Encrypt password
        let (encrypted, nonce_bytes) = self.encrypt_password(password, master_key)?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.conn
            .execute(
                "INSERT INTO credentials (url, username, password_encrypted, nonce, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![domain, username, encrypted, nonce_bytes, now, now],
            )
            .map_err(|e| hiwave_core::HiWaveError::Vault(format!("Failed to save credential: {}", e)))?;

        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_credentials(&self, url: &Url) -> HiWaveResult<Vec<Credential>> {
        if !self.is_unlocked() {
            return Err(hiwave_core::HiWaveError::Vault("Vault is locked".to_string()));
        }

        let domain = Self::normalize_domain(url)?;
        let master_key = self.master_key.as_ref().unwrap();

        let mut stmt = self
            .conn
            .prepare("SELECT id, url, username, password_encrypted, nonce, created_at, updated_at FROM credentials WHERE url = ?1")
            .map_err(|e| hiwave_core::HiWaveError::Vault(format!("Failed to prepare query: {}", e)))?;

        let mut rows = stmt.query(params![domain]).map_err(|e| {
            hiwave_core::HiWaveError::Vault(format!("Failed to query credentials: {}", e))
        })?;

        let mut credentials = Vec::new();
        while let Some(row) = rows.next().map_err(|e| {
            hiwave_core::HiWaveError::Vault(format!("Failed to read credential row: {}", e))
        })? {
            let encrypted: Vec<u8> = row.get(3).map_err(|e| {
                hiwave_core::HiWaveError::Vault(format!("Failed to read encrypted password: {}", e))
            })?;
            let nonce_bytes: Vec<u8> = row
                .get(4)
                .map_err(|e| hiwave_core::HiWaveError::Vault(format!("Failed to read nonce: {}", e)))?;

            let password = self.decrypt_password(&encrypted, &nonce_bytes, master_key)?;

            credentials.push(Credential {
                id: row
                    .get(0)
                    .map_err(|e| hiwave_core::HiWaveError::Vault(format!("Failed to read id: {}", e)))?,
                url: row
                    .get(1)
                    .map_err(|e| hiwave_core::HiWaveError::Vault(format!("Failed to read url: {}", e)))?,
                username: row.get(2).map_err(|e| {
                    hiwave_core::HiWaveError::Vault(format!("Failed to read username: {}", e))
                })?,
                password,
                created_at: row.get(5).map_err(|e| {
                    hiwave_core::HiWaveError::Vault(format!("Failed to read created_at: {}", e))
                })?,
                updated_at: row.get(6).map_err(|e| {
                    hiwave_core::HiWaveError::Vault(format!("Failed to read updated_at: {}", e))
                })?,
            });
        }

        Ok(credentials)
    }

    pub fn get_all_credentials(&self) -> HiWaveResult<Vec<Credential>> {
        if !self.is_unlocked() {
            return Err(hiwave_core::HiWaveError::Vault("Vault is locked".to_string()));
        }

        let master_key = self.master_key.as_ref().unwrap();

        let mut stmt = self
            .conn
            .prepare("SELECT id, url, username, password_encrypted, nonce, created_at, updated_at FROM credentials ORDER BY url, username")
            .map_err(|e| hiwave_core::HiWaveError::Vault(format!("Failed to prepare query: {}", e)))?;

        let mut rows = stmt.query([]).map_err(|e| {
            hiwave_core::HiWaveError::Vault(format!("Failed to query credentials: {}", e))
        })?;

        let mut credentials = Vec::new();
        while let Some(row) = rows.next().map_err(|e| {
            hiwave_core::HiWaveError::Vault(format!("Failed to read credential row: {}", e))
        })? {
            let encrypted: Vec<u8> = row.get(3).map_err(|e| {
                hiwave_core::HiWaveError::Vault(format!("Failed to read encrypted password: {}", e))
            })?;
            let nonce_bytes: Vec<u8> = row
                .get(4)
                .map_err(|e| hiwave_core::HiWaveError::Vault(format!("Failed to read nonce: {}", e)))?;

            let password = self.decrypt_password(&encrypted, &nonce_bytes, master_key)?;

            credentials.push(Credential {
                id: row
                    .get(0)
                    .map_err(|e| hiwave_core::HiWaveError::Vault(format!("Failed to read id: {}", e)))?,
                url: row
                    .get(1)
                    .map_err(|e| hiwave_core::HiWaveError::Vault(format!("Failed to read url: {}", e)))?,
                username: row.get(2).map_err(|e| {
                    hiwave_core::HiWaveError::Vault(format!("Failed to read username: {}", e))
                })?,
                password,
                created_at: row.get(5).map_err(|e| {
                    hiwave_core::HiWaveError::Vault(format!("Failed to read created_at: {}", e))
                })?,
                updated_at: row.get(6).map_err(|e| {
                    hiwave_core::HiWaveError::Vault(format!("Failed to read updated_at: {}", e))
                })?,
            });
        }

        Ok(credentials)
    }

    pub fn delete_credential(&mut self, id: i64) -> HiWaveResult<()> {
        if !self.is_unlocked() {
            return Err(hiwave_core::HiWaveError::Vault("Vault is locked".to_string()));
        }

        log::info!("Deleting credential {}", id);

        self.conn
            .execute("DELETE FROM credentials WHERE id = ?1", params![id])
            .map_err(|e| {
                hiwave_core::HiWaveError::Vault(format!("Failed to delete credential: {}", e))
            })?;

        Ok(())
    }

    fn encrypt_password(&self, password: &str, key: &[u8]) -> HiWaveResult<(Vec<u8>, Vec<u8>)> {
        // Use first 32 bytes of key for AES-256
        let key_bytes: [u8; 32] = key
            .get(..32)
            .ok_or_else(|| hiwave_core::HiWaveError::Vault("Key too short".to_string()))?
            .try_into()
            .map_err(|_| hiwave_core::HiWaveError::Vault("Invalid key length".to_string()))?;

        let cipher = Aes256Gcm::new(&key_bytes.into());

        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let encrypted = cipher
            .encrypt(nonce, password.as_bytes())
            .map_err(|e| hiwave_core::HiWaveError::Vault(format!("Encryption failed: {}", e)))?;

        Ok((encrypted, nonce_bytes.to_vec()))
    }

    fn decrypt_password(
        &self,
        encrypted: &[u8],
        nonce_bytes: &[u8],
        key: &[u8],
    ) -> HiWaveResult<String> {
        let key_bytes: [u8; 32] = key
            .get(..32)
            .ok_or_else(|| hiwave_core::HiWaveError::Vault("Key too short".to_string()))?
            .try_into()
            .map_err(|_| hiwave_core::HiWaveError::Vault("Invalid key length".to_string()))?;

        if nonce_bytes.len() != NONCE_LEN {
            return Err(hiwave_core::HiWaveError::Vault(
                "Invalid nonce length".to_string(),
            ));
        }

        let cipher = Aes256Gcm::new(&key_bytes.into());
        let nonce = Nonce::from_slice(nonce_bytes);

        let decrypted = cipher
            .decrypt(nonce, encrypted)
            .map_err(|e| hiwave_core::HiWaveError::Vault(format!("Decryption failed: {}", e)))?;

        String::from_utf8(decrypted)
            .map_err(|e| hiwave_core::HiWaveError::Vault(format!("Invalid UTF-8: {}", e)))
    }

    fn load_meta(&self) -> HiWaveResult<Option<(Vec<u8>, Vec<u8>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT salt, verifier FROM vault_meta WHERE id = 1")
            .map_err(|e| {
                hiwave_core::HiWaveError::Vault(format!("Failed to prepare vault metadata query: {}", e))
            })?;

        let result = stmt.query_row([], |row| {
            let salt: Vec<u8> = row.get(0)?;
            let verifier: Vec<u8> = row.get(1)?;
            Ok((salt, verifier))
        });

        match result {
            Ok(meta) => Ok(Some(meta)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(hiwave_core::HiWaveError::Vault(format!(
                "Failed to load vault metadata: {}",
                e
            ))),
        }
    }

    fn store_meta(&self, salt: &[u8], verifier: &[u8]) -> HiWaveResult<()> {
        let now = current_timestamp();
        self.conn
            .execute(
                "INSERT INTO vault_meta (id, salt, verifier, created_at, updated_at)
                 VALUES (1, ?1, ?2, ?3, ?4)",
                params![salt, verifier, now, now],
            )
            .map_err(|e| {
                hiwave_core::HiWaveError::Vault(format!("Failed to store vault metadata: {}", e))
            })?;
        Ok(())
    }

    fn sample_credential(&self) -> HiWaveResult<Option<(Vec<u8>, Vec<u8>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT password_encrypted, nonce FROM credentials LIMIT 1")
            .map_err(|e| {
                hiwave_core::HiWaveError::Vault(format!(
                    "Failed to prepare credential sample query: {}",
                    e
                ))
            })?;

        let result = stmt.query_row([], |row| {
            let encrypted: Vec<u8> = row.get(0)?;
            let nonce: Vec<u8> = row.get(1)?;
            Ok((encrypted, nonce))
        });

        match result {
            Ok(sample) => Ok(Some(sample)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(hiwave_core::HiWaveError::Vault(format!(
                "Failed to query credential sample: {}",
                e
            ))),
        }
    }

    fn normalize_domain(url: &Url) -> HiWaveResult<String> {
        if let Some(domain) = url.domain() {
            return Ok(domain.to_string());
        }

        if let Some(host) = url.host_str() {
            return Ok(host.to_string());
        }

        Err(hiwave_core::HiWaveError::Vault(format!(
            "URL has no host: {}",
            url
        )))
    }
}

fn derive_key(master_password: &str, salt: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(master_password.as_bytes());
    hasher.update(salt);
    let result = hasher.finalize();

    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

fn derive_verifier(key: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(key);
    hasher.update(VERIFY_LABEL);
    let result = hasher.finalize();

    let mut verifier = [0u8; 32];
    verifier.copy_from_slice(&result);
    verifier
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut diff = 0u8;
    for (left, right) in a.iter().zip(b.iter()) {
        diff |= left ^ right;
    }
    diff == 0
}

fn current_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_lifecycle() {
        let vault_path = "test_vault.db";
        let mut vault = Vault::new(vault_path).unwrap();

        assert!(!vault.is_unlocked());

        vault.unlock("test_password").unwrap();
        assert!(vault.is_unlocked());

        vault.lock();
        assert!(!vault.is_unlocked());

        // Cleanup
        std::fs::remove_file(vault_path).ok();
    }
}
