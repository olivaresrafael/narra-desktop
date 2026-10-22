//! Secret storage for provider tokens (M5 slice 2, D-022).
//!
//! Tokens are stored in OS secret storage — macOS Keychain via the `keyring`
//! crate — and resolved in a fixed order: Keychain first, then the
//! `GITHUB_TOKEN` environment variable (headless/CI fallback). They never
//! appear in project files, argv, logs, error messages or `Debug` output.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Mutex;

/// Keychain service name for all Nara-stored secrets. Overridable via
/// environment for tests (isolated items that cannot collide with, or
/// expose, a real user's token).
pub const NARRA_SERVICE: &str = "narra";

/// Environment variable overriding the service name (tests, CI).
pub const NARRA_SERVICE_ENV: &str = "NARRA_KEYRING_SERVICE";

/// Keychain account (username) used for the GitHub token.
pub const GITHUB_TOKEN_ACCOUNT: &str = "github";

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

/// Resolve the GitHub token: Keychain first, `GITHUB_TOKEN` env fallback
/// (the caller passes the env value so resolution stays testable without
/// mutating the process environment). Empty values are treated as absent.
pub fn github_token(
    store: &dyn SecretStore,
    env_token: Option<&str>,
) -> Result<Option<String>, SecretError> {
    if let Some(stored) = store.get(GITHUB_TOKEN_ACCOUNT)?
        && !stored.trim().is_empty()
    {
        return Ok(Some(stored));
    }
    Ok(env_token
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
