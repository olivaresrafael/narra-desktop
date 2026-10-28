//! Secret storage for provider tokens (M5 slice 2, D-022).
//!
//! Tokens are stored in OS secret storage — macOS Keychain via the `keyring`
//! crate — and resolved in a fixed order: Keychain first, then the
//! `GITHUB_TOKEN` environment variable (headless/CI fallback). They never
//! appear in project files, argv, logs, error messages or `Debug` output.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Keychain service name for all Nara-stored secrets. Overridable via
/// environment for tests (isolated items that cannot collide with, or
/// expose, a real user's token).
pub const NARRA_SERVICE: &str = "narra";

/// Environment variable overriding the service name (tests, CI).
pub const NARRA_SERVICE_ENV: &str = "NARRA_KEYRING_SERVICE";

/// Keychain account (username) used for the GitHub token.
pub const GITHUB_TOKEN_ACCOUNT: &str = "github";
/// Keychain accounts for model-provider keys. The JSON settings store never
/// contains a key; these accounts are the only persistent home for one.
pub const OPENAI_COMPAT_API_KEY_ACCOUNT: &str = "openai-compat";
pub const OPENCODE_API_KEY_ACCOUNT: &str = "opencode";
pub const OPENCODE_GO_API_KEY_ACCOUNT: &str = "opencode-go";

/// Secret-storage failures. Messages describe the failure, never the secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretError {
    /// The store refused access to its storage (locked/unavailable).
    AccessDenied { detail: String },
    /// Anything else the platform reported.
    Other { detail: String },
}

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretError::AccessDenied { detail } => {
                write!(f, "no se pudo acceder al almacén de secretos: {detail}")
            }
            SecretError::Other { detail } => write!(f, "error del almacén de secretos: {detail}"),
        }
    }
}

impl std::error::Error for SecretError {}

/// Minimal secret-store surface the product needs. Implementations must
/// keep values out of `Debug`/error output by construction.
pub trait SecretStore: Send + Sync {
    /// The secret stored under `account`, or `None` when absent.
    fn get(&self, account: &str) -> Result<Option<String>, SecretError>;
    /// Create or replace the secret under `account`.
    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError>;
    /// Remove the secret under `account`. Idempotent: removing an absent
    /// entry succeeds (logout must never fail twice differently).
    fn delete(&self, account: &str) -> Result<(), SecretError>;
}

/// The OS keychain-backed store (macOS Keychain in the MVP).
pub struct OsKeychain {
    service: String,
}

impl Default for OsKeychain {
    fn default() -> Self {
        Self::new()
    }
}

impl OsKeychain {
    /// Default service (`narra`), or `NARRA_KEYRING_SERVICE` when set —
    /// tests use a throwaway service so they never touch a real token.
    pub fn new() -> Self {
        let service = std::env::var(NARRA_SERVICE_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| NARRA_SERVICE.to_string());
        Self { service }
    }
}

impl fmt::Debug for OsKeychain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OsKeychain")
            .field("service", &self.service)
            .finish()
    }
}

impl SecretStore for OsKeychain {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        match keyring::Entry::new(&self.service, account)
            .map_err(map_keyring_build)?
            .get_password()
        {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(map_keyring(error)),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError> {
        keyring::Entry::new(&self.service, account)
            .map_err(map_keyring_build)?
            .set_password(secret)
            .map_err(map_keyring)
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        match keyring::Entry::new(&self.service, account)
            .map_err(map_keyring_build)?
            .delete_credential()
        {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(map_keyring(error)),
        }
    }
}

fn map_keyring_build(error: keyring::Error) -> SecretError {
    map_keyring(error)
}

fn map_keyring(error: keyring::Error) -> SecretError {
    match error {
        keyring::Error::NoStorageAccess(detail) => SecretError::AccessDenied {
            detail: detail.to_string(),
        },
        // Never interpolate the secret: platform errors describe the
        // operation, not the value — and `Display` here is already free of
        // it by construction.
        other => SecretError::Other {
            detail: other.to_string(),
        },
    }
}

/// An in-memory store for tests and demonstrations: same semantics, no
/// platform dependency, nothing persisted.
#[derive(Default)]
pub struct MemorySecretStore {
    entries: Mutex<BTreeMap<String, String>>,
}

impl fmt::Debug for MemorySecretStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.entries.lock().map(|map| map.len()).unwrap_or(0);
        f.debug_struct("MemorySecretStore")
            .field("entries", &format_args!("<{count} redacted>"))
            .finish()
    }
}

impl SecretStore for MemorySecretStore {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        let map = self.entries.lock().map_err(|_| SecretError::Other {
            detail: "store bloqueado".to_string(),
        })?;
        Ok(map.get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError> {
        let mut map = self.entries.lock().map_err(|_| SecretError::Other {
            detail: "store bloqueado".to_string(),
        })?;
        map.insert(account.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        let mut map = self.entries.lock().map_err(|_| SecretError::Other {
            detail: "store bloqueado".to_string(),
        })?;
        map.remove(account);
        Ok(())
    }
}

/// One cached answer per account.
#[derive(Clone)]
enum CachedEntry {
    Present(String),
    Absent,
    Failed(SecretError),
}

/// Session-scoped read-through cache in front of another [`SecretStore`].
///
/// macOS asks for keychain access again whenever the requesting binary is not
/// the one the item trusts. Unsigned and ad-hoc development builds change
/// identity on every compile, and the app used to read the same accounts
/// several times per flow. This cache reaches the wrapped store at most once
/// per account for the life of the process, caches misses and failures too,
/// updates the cache on writes, and lets callers [`Self::invalidate`] an
/// account for an explicit retry.
///
/// The cache holds the secret in memory for the session. It never writes it to
/// disk, argv, logs or errors, and `Debug` reports counts only.
pub struct CachedSecretStore {
    inner: Box<dyn SecretStore>,
    entries: Mutex<BTreeMap<String, CachedEntry>>,
    reads: AtomicUsize,
}

impl CachedSecretStore {
    pub fn new(inner: Box<dyn SecretStore>) -> Self {
        Self {
            inner,
            entries: Mutex::new(BTreeMap::new()),
            reads: AtomicUsize::new(0),
        }
    }

    /// Number of reads that reached the wrapped store. The app exposes it in
    /// its dump to prove that a flow does not re-read secrets.
    pub fn read_count(&self) -> usize {
        self.reads.load(Ordering::Relaxed)
    }

    /// The cached answer for `account` without reading the store. `None` means
    /// the account has not been read yet in this session.
    pub fn peek(&self, account: &str) -> Option<Result<Option<String>, SecretError>> {
        let entries = self.entries.lock().ok()?;
        entries.get(account).map(|entry| match entry {
            CachedEntry::Present(secret) => Ok(Some(secret.clone())),
            CachedEntry::Absent => Ok(None),
            CachedEntry::Failed(error) => Err(error.clone()),
        })
    }

    /// Forgets the cached answer so the next read reaches the store again.
    pub fn invalidate(&self, account: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.remove(account);
        }
    }
}

impl fmt::Debug for CachedSecretStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cached = self
            .entries
            .lock()
            .map(|entries| entries.len())
            .unwrap_or(0);
        f.debug_struct("CachedSecretStore")
            .field("cached_accounts", &cached)
            .field("reads", &self.read_count())
            .finish()
    }
}

impl SecretStore for CachedSecretStore {
    fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        // The lock is held across the read on purpose: two threads asking for
        // the same account must not each trigger an OS prompt. Accounts are
        // few and reads are rare, so serializing them is the safer trade.
        let mut entries = self.entries.lock().map_err(|_| SecretError::Other {
            detail: "secret cache poisoned".to_string(),
        })?;
        if let Some(entry) = entries.get(account) {
            return match entry {
                CachedEntry::Present(secret) => Ok(Some(secret.clone())),
                CachedEntry::Absent => Ok(None),
                CachedEntry::Failed(error) => Err(error.clone()),
            };
        }
        self.reads.fetch_add(1, Ordering::Relaxed);
        let result = self.inner.get(account);
        let entry = match &result {
            Ok(Some(secret)) => CachedEntry::Present(secret.clone()),
            Ok(None) => CachedEntry::Absent,
            Err(error) => CachedEntry::Failed(error.clone()),
        };
        entries.insert(account.to_string(), entry);
        result
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), SecretError> {
        self.inner.set(account, secret)?;
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(
                account.to_string(),
                CachedEntry::Present(secret.to_string()),
            );
        }
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        self.inner.delete(account)?;
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(account.to_string(), CachedEntry::Absent);
        }
        Ok(())
    }
}

/// Resolve the GitHub token: Keychain first, `GITHUB_TOKEN` env fallback
/// (the caller passes the env value so resolution stays testable without
/// mutating the process environment). Empty values are treated as absent.
pub fn resolve_secret(
    store: &dyn SecretStore,
    account: &str,
    env_value: Option<&str>,
) -> Result<Option<String>, SecretError> {
    match store.get(account) {
        Ok(Some(secret)) if !secret.trim().is_empty() => Ok(Some(secret)),
        Ok(_) => Ok(env_value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)),
        // Headless/CI: the keychain may be unavailable while an explicit
        // environment key is present. Use the env fallback; otherwise
        // surface the keychain failure.
        Err(_) if env_value.is_some_and(|value| !value.trim().is_empty()) => Ok(env_value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)),
        Err(error) => Err(error),
    }
}

pub fn github_token(
    store: &dyn SecretStore,
    env_token: Option<&str>,
) -> Result<Option<String>, SecretError> {
    resolve_secret(store, GITHUB_TOKEN_ACCOUNT, env_token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn memory_store_roundtrip_and_idempotent_delete() {
        let store = MemorySecretStore::default();
        assert_eq!(store.get("github").expect("get"), None);
        store.set("github", "ghp_value").expect("set");
        assert_eq!(
            store.get("github").expect("get"),
            Some("ghp_value".to_string())
        );
        store.delete("github").expect("delete");
        assert_eq!(store.get("github").expect("get"), None);
        // Second delete is a no-op, not an error (logout idempotence).
        store.delete("github").expect("delete again");
    }

    #[test]
    fn memory_store_debug_never_shows_values() {
        let store = MemorySecretStore::default();
        store.set("github", "ghp_debugleak").expect("set");
        let printed = format!("{store:?}");
        assert!(!printed.contains("ghp_debugleak"), "{printed}");
    }

    #[test]
    fn stored_token_wins_over_env() {
        let store = MemorySecretStore::default();
        store.set("github", "ghp_stored").expect("set");
        let token = github_token(&store, Some("ghp_fromenv")).expect("resolve");
        assert_eq!(token.as_deref(), Some("ghp_stored"));
    }

    #[test]
    fn env_token_is_the_fallback_when_keychain_is_empty() {
        let store = MemorySecretStore::default();
        let token = github_token(&store, Some("ghp_fromenv")).expect("resolve");
        assert_eq!(token.as_deref(), Some("ghp_fromenv"));
    }

    #[test]
    fn generic_secret_resolution_uses_the_requested_provider_account() {
        let store = MemorySecretStore::default();
        store.set("opencode", "sk-opencode").expect("set");
        assert_eq!(
            resolve_secret(&store, "opencode", Some("sk-env")).expect("resolve"),
            Some("sk-opencode".to_string())
        );
        assert_eq!(
            resolve_secret(&store, "opencode-go", Some("sk-go-env")).expect("resolve"),
            Some("sk-go-env".to_string())
        );
        assert_eq!(
            resolve_secret(&store, "opencode-go", None).expect("resolve"),
            None
        );
    }

    #[test]
    fn empty_values_are_absent() {
        let store = MemorySecretStore::default();
        store.set("github", "   ").expect("set");
        let token = github_token(&store, Some("  ")).expect("resolve");
        assert_eq!(token, None);
    }

    #[test]
    fn no_token_anywhere_is_none_not_an_error() {
        let store = MemorySecretStore::default();
        let token = github_token(&store, None).expect("resolve");
        assert_eq!(token, None);
    }

    /// Store double that counts reads and can simulate a denied prompt.
    #[derive(Default)]
    struct CountingStore {
        inner: MemorySecretStore,
        reads: Arc<AtomicUsize>,
        fail: Arc<Mutex<bool>>,
    }

    impl SecretStore for CountingStore {
        fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
            self.reads.fetch_add(1, Ordering::Relaxed);
            if *self.fail.lock().expect("fail flag") {
                return Err(SecretError::AccessDenied {
                    detail: "prompt denied".to_string(),
                });
            }
            self.inner.get(account)
        }

        fn set(&self, account: &str, secret: &str) -> Result<(), SecretError> {
            self.inner.set(account, secret)
        }

        fn delete(&self, account: &str) -> Result<(), SecretError> {
            self.inner.delete(account)
        }
    }

    #[test]
    fn repeated_reads_reach_the_store_once() {
        let store = CountingStore::default();
        store.inner.set("opencode", "sk-test").expect("seed");
        let reads = store.reads.clone();
        let cache = CachedSecretStore::new(Box::new(store));

        assert_eq!(
            cache.get("opencode").expect("first"),
            Some("sk-test".to_string())
        );
        assert_eq!(
            cache.get("opencode").expect("second"),
            Some("sk-test".to_string())
        );
        assert_eq!(reads.load(Ordering::Relaxed), 1);
        assert_eq!(cache.read_count(), 1);
    }

    #[test]
    fn misses_and_failures_are_cached_and_invalidate_retries() {
        let store = CountingStore::default();
        let reads = store.reads.clone();
        let fail = store.fail.clone();
        let cache = CachedSecretStore::new(Box::new(store));

        assert_eq!(cache.get("github").expect("miss"), None);
        assert_eq!(cache.get("github").expect("cached miss"), None);
        assert_eq!(reads.load(Ordering::Relaxed), 1);

        *fail.lock().expect("fail flag") = true;
        cache.invalidate("github");
        let error = cache.get("github").expect_err("failure");
        assert!(matches!(error, SecretError::AccessDenied { .. }), "{error}");
        cache.get("github").expect_err("cached failure");
        assert_eq!(reads.load(Ordering::Relaxed), 2);

        *fail.lock().expect("fail flag") = false;
        cache.invalidate("github");
        assert_eq!(cache.get("github").expect("retry"), None);
        assert_eq!(reads.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn writes_update_the_cache_without_reading() {
        let store = CountingStore::default();
        let reads = store.reads.clone();
        let cache = CachedSecretStore::new(Box::new(store));

        cache.set("opencode", "sk-new").expect("set");
        assert_eq!(
            cache.get("opencode").expect("cached write"),
            Some("sk-new".to_string())
        );
        assert_eq!(reads.load(Ordering::Relaxed), 0);

        cache.delete("opencode").expect("delete");
        assert_eq!(cache.get("opencode").expect("cached delete"), None);
        assert_eq!(reads.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn peek_never_reads_and_reports_unknown_accounts() {
        let store = CountingStore::default();
        store.inner.set("github", "ghp_seed").expect("seed");
        let reads = store.reads.clone();
        let cache = CachedSecretStore::new(Box::new(store));

        assert_eq!(cache.peek("github"), None);
        assert_eq!(reads.load(Ordering::Relaxed), 0);

        assert_eq!(
            cache.get("github").expect("read"),
            Some("ghp_seed".to_string())
        );
        assert_eq!(cache.peek("github"), Some(Ok(Some("ghp_seed".to_string()))));
        assert_eq!(reads.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn debug_reports_counts_not_values() {
        let cache = CachedSecretStore::new(Box::new(MemorySecretStore::default()));
        cache.set("github", "ghp_debugleak").expect("set");
        let printed = format!("{cache:?}");
        assert!(!printed.contains("ghp_debugleak"), "{printed}");
        assert!(printed.contains("cached_accounts"), "{printed}");
        assert!(printed.contains("reads"), "{printed}");
    }
}
