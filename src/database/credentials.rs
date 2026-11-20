//! Database passwords in the macOS Keychain.
//!
//! All passwords live in ONE Keychain item: a JSON map of target id → password.
//! Why: an app signed without an Apple Team ID gets a Keychain "partition" tied to the
//! exact build, so after every update macOS asks once per item. One item means at most
//! one question, however many databases there are. The map is also cached for the rest
//! of the session, so the Keychain is read at most once per launch.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use security_framework::base::Error as KeychainError;
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

use crate::constants::{KEYCHAIN_ITEM_NOT_FOUND, KEYCHAIN_SERVICE, KEYCHAIN_VAULT_ACCOUNT};
use crate::database::owner_auth::confirm_device_owner;
use crate::database::target::DatabaseTarget;
use crate::error::AppError;

type PasswordVault = BTreeMap<String, String>;

/// The vault after its first read in this session. A process-wide cache because every
/// part of the app shares the same Keychain item.
static VAULT_CACHE: Mutex<Option<PasswordVault>> = Mutex::new(None);

pub fn save_password(target_id: &str, password: &str) -> Result<(), AppError> {
    let mut vault = load_vault()?;
    vault.insert(target_id.to_string(), password.to_string());
    store_vault(vault)
}

/// `Ok(None)` when nothing is saved for this target.
pub fn load_password(target_id: &str) -> Result<Option<String>, AppError> {
    if let Some(password) = load_vault()?.get(target_id) {
        return Ok(Some(password.clone()));
    }
    let Some(per_target_password) = read_per_target_password(target_id)? else {
        return Ok(None);
    };
    save_password(target_id, &per_target_password)?;
    Ok(Some(per_target_password))
}

/// Deleting a password that was never saved is not an error.
pub fn delete_password(target_id: &str) -> Result<(), AppError> {
    let mut vault = load_vault()?;
    if vault.remove(target_id).is_some() {
        store_vault(vault)?;
    }
    // Also remove the old per-target item, or `load_password` would bring the password
    // back. Deleting a database is rare, so a possible permission prompt is acceptable.
    remove_item(KEYCHAIN_SERVICE, target_id)
}

/// The password to connect with: what the user just typed, else the saved one,
/// else `None` (the PostgreSQL tools then try `~/.pgpass`).
pub fn resolve_password(
    target: &DatabaseTarget,
    typed_password: &str,
) -> Result<Option<String>, AppError> {
    if !typed_password.is_empty() {
        return Ok(Some(typed_password.to_string()));
    }
    if !target.is_password_saved {
        return Ok(None);
    }
    match load_password(&target.id)? {
        Some(password) => Ok(Some(password)),
        None => Err(AppError::Keychain(format!(
            "The saved password for \"{}\" is missing. Edit the database and save the password again.",
            target.name
        ))),
    }
}

/// The cached vault, reading the Keychain item only the first time.
fn load_vault() -> Result<PasswordVault, AppError> {
    let mut cache = VAULT_CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(vault) = cache.as_ref() {
        return Ok(vault.clone());
    }
    let vault = match read_item(KEYCHAIN_SERVICE, KEYCHAIN_VAULT_ACCOUNT)? {
        Some(bytes) => parse_vault(&bytes)?,
        None => PasswordVault::new(),
    };
    *cache = Some(vault.clone());
    Ok(vault)
}

fn store_vault(vault: PasswordVault) -> Result<(), AppError> {
    set_generic_password(
        KEYCHAIN_SERVICE,
        KEYCHAIN_VAULT_ACCOUNT,
        &serialize_vault(&vault),
    )
    .map_err(to_keychain_error)?;
    *VAULT_CACHE.lock().unwrap_or_else(PoisonError::into_inner) = Some(vault);
    Ok(())
}

fn read_per_target_password(target_id: &str) -> Result<Option<String>, AppError> {
    let Some(bytes) = read_item(KEYCHAIN_SERVICE, target_id)? else {
        return Ok(None);
    };
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| AppError::Keychain("The saved password is not valid text.".to_string()))
}

fn parse_vault(bytes: &[u8]) -> Result<PasswordVault, AppError> {
    serde_json::from_slice(bytes).map_err(|_| {
        AppError::Keychain(
            "The saved passwords could not be read. Save the database passwords again.".to_string(),
        )
    })
}

fn serialize_vault(vault: &PasswordVault) -> Vec<u8> {
    // Serializing a map of strings cannot fail.
    serde_json::to_vec(vault).unwrap_or_default()
}

fn read_item(service: &str, account: &str) -> Result<Option<Vec<u8>>, AppError> {
    match get_generic_password(service, account) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.code() == KEYCHAIN_ITEM_NOT_FOUND => Ok(None),
        Err(error) => Err(to_keychain_error(error)),
    }
}

fn remove_item(service: &str, account: &str) -> Result<(), AppError> {
    match delete_generic_password(service, account) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == KEYCHAIN_ITEM_NOT_FOUND => Ok(()),
        Err(error) => Err(to_keychain_error(error)),
    }
}

fn to_keychain_error(error: KeychainError) -> AppError {
    let detail = error
        .message()
        .unwrap_or_else(|| format!("error code {}", error.code()));
    AppError::Keychain(detail)
}

/// Like `resolve_password`, but asks for Touch ID first when the saved password is about to
/// be used and the target requires it. A typed password never needs Touch ID.
pub async fn unlock_password(
    target: &DatabaseTarget,
    typed_password: &str,
) -> Result<Option<String>, AppError> {
    let uses_saved_password = typed_password.is_empty() && target.is_password_saved;
    if uses_saved_password && target.should_require_touch_id {
        let reason = format!("use the saved password for {}", target.database);
        confirm_device_owner(&reason).await?;
    }
    resolve_password(target, typed_password)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target_with_saved_password() -> DatabaseTarget {
        DatabaseTarget {
            id: "test-target".to_string(),
            name: "Test".to_string(),
            database: "test_db".to_string(),
            is_password_saved: true,
            should_require_touch_id: true,
            ..DatabaseTarget::default()
        }
    }

    // Neither case may show the Touch ID prompt (it would block an unattended test run).
    #[tokio::test]
    async fn typed_password_is_used_without_touch_id() {
        let password = unlock_password(&target_with_saved_password(), "typed-secret").await;
        assert_eq!(password, Ok(Some("typed-secret".to_string())));
    }

    #[tokio::test]
    async fn no_saved_password_means_pgpass_without_touch_id() {
        let target = DatabaseTarget {
            is_password_saved: false,
            ..target_with_saved_password()
        };
        assert_eq!(unlock_password(&target, "").await, Ok(None));
    }

    #[test]
    fn vault_round_trips_through_keychain_bytes() {
        let vault = PasswordVault::from([
            ("target-a".to_string(), "secret a".to_string()),
            ("target-b".to_string(), "p@ss \"quoted\"".to_string()),
        ]);
        assert_eq!(parse_vault(&serialize_vault(&vault)), Ok(vault));
    }

    #[test]
    fn unreadable_vault_is_a_clear_error() {
        assert!(matches!(
            parse_vault(b"not json"),
            Err(AppError::Keychain(_))
        ));
    }

    #[test]
    fn touch_id_is_on_by_default_including_older_saved_targets() {
        assert!(DatabaseTarget::default().should_require_touch_id);
        let older: DatabaseTarget =
            serde_json::from_str(r#"{ "id": "x", "is_password_saved": true }"#).unwrap();
        assert!(older.should_require_touch_id);
    }
}
