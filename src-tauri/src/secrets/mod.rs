//! Secret storage. Secrets (WordPress Application Passwords, API keys) never go
//! into SQLite — the DB only keeps a key name. On Windows the backend is the
//! Windows Credential Manager; elsewhere (dev/test) an in-memory store is used.

#[cfg(windows)]
use crate::error::AppError;
use crate::error::AppResult;
use std::collections::HashMap;
use std::sync::Mutex;

pub trait SecretStore: Send + Sync {
    fn set(&self, key: &str, secret: &str) -> AppResult<()>;
    fn get(&self, key: &str) -> AppResult<Option<String>>;
    fn delete(&self, key: &str) -> AppResult<()>;
}

/// In-memory store: used in tests and on non-Windows dev machines.
#[derive(Default)]
pub struct MemorySecretStore {
    inner: Mutex<HashMap<String, String>>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for MemorySecretStore {
    fn set(&self, key: &str, secret: &str) -> AppResult<()> {
        self.inner.lock().unwrap().insert(key.to_string(), secret.to_string());
        Ok(())
    }
    fn get(&self, key: &str) -> AppResult<Option<String>> {
        Ok(self.inner.lock().unwrap().get(key).cloned())
    }
    fn delete(&self, key: &str) -> AppResult<()> {
        self.inner.lock().unwrap().remove(key);
        Ok(())
    }
}

#[cfg(windows)]
pub struct KeyringSecretStore;

#[cfg(windows)]
const SERVICE: &str = "MyArticleManager";

#[cfg(windows)]
impl SecretStore for KeyringSecretStore {
    fn set(&self, key: &str, secret: &str) -> AppResult<()> {
        let entry = keyring::Entry::new(SERVICE, key)
            .map_err(|_| AppError::secret_store("دسترسی به Windows Credential Manager ممکن نیست."))?;
        entry
            .set_password(secret)
            .map_err(|_| AppError::secret_store("ذخیره‌ی امن اطلاعات محرمانه ناموفق بود."))
    }
    fn get(&self, key: &str) -> AppResult<Option<String>> {
        let entry = keyring::Entry::new(SERVICE, key)
            .map_err(|_| AppError::secret_store("دسترسی به Windows Credential Manager ممکن نیست."))?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(AppError::secret_store("خواندن اطلاعات محرمانه ناموفق بود.")),
        }
    }
    fn delete(&self, key: &str) -> AppResult<()> {
        let entry = keyring::Entry::new(SERVICE, key)
            .map_err(|_| AppError::secret_store("دسترسی به Windows Credential Manager ممکن نیست."))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(AppError::secret_store("حذف اطلاعات محرمانه ناموفق بود.")),
        }
    }
}

/// Default store for the running app.
pub fn default_store() -> std::sync::Arc<dyn SecretStore> {
    #[cfg(windows)]
    {
        std::sync::Arc::new(KeyringSecretStore)
    }
    #[cfg(not(windows))]
    {
        tracing::warn!("non-Windows build: using in-memory secret store (dev only, secrets are not persisted)");
        std::sync::Arc::new(MemorySecretStore::new())
    }
}
