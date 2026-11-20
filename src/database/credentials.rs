//! Database passwords in the macOS Keychain (one item per database target).

use security_framework::base::Error as KeychainError;
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

use crate::database::owner_auth::confirm_device_owner;
use crate::database::target::DatabaseTarget;
use crate::error::AppError;

const KEYCHAIN_SERVICE: &str = "dev.dbrefresh.app";
// The app was first called "Optimus DB Refresh". Passwords saved back then are copied
// to the current service the first time they are read.
const LEGACY_KEYCHAIN_SERVICE: &str = "dev.optimus.dbrefresh";
// `errSecItemNotFound` from Apple's Security framework.
const KEYCHAIN_ITEM_NOT_FOUND: i32 = -25300;

pub fn save_password(target_id: &str, password: &str) -> Result<(), AppError> {
    set_generic_password(KEYCHAIN_SERVICE, target_id, password.as_bytes()).map_err(keychain_error)
}

/// `Ok(None)` when nothing is saved for this target.
pub fn load_password(target_id: &str) -> Result<Option<String>, AppError> {
    if let Some(password) = read_password(KEYCHAIN_SERVICE, target_id)? {
        return Ok(Some(password));
    }
    let Some(legacy_password) = read_password(LEGACY_KEYCHAIN_SERVICE, target_id)? else {
        return Ok(None);
    };
    save_password(target_id, &legacy_password)?;
    // The old item is left in place on purpose: deleting an item created by an older
    // build makes macOS ask for permission a second time. `delete_password` removes it
    // when the database itself is deleted.
    Ok(Some(legacy_password))
}

/// Deleting a password that was never saved is not an error.
pub fn delete_password(target_id: &str) -> Result<(), AppError> {
    remove_password(KEYCHAIN_SERVICE, target_id)?;
    remove_password(LEGACY_KEYCHAIN_SERVICE, target_id)
}

/// The password to connect with: what the user just typed, else the saved one,
/// else `None` (the PostgreSQL tools then try `~/.pgpass`).
pub fn password_for(
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

fn read_password(service: &str, target_id: &str) -> Result<Option<String>, AppError> {
    match get_generic_password(service, target_id) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| AppError::Keychain("The saved password is not valid text.".to_string())),
        Err(error) if error.code() == KEYCHAIN_ITEM_NOT_FOUND => Ok(None),
        Err(error) => Err(keychain_error(error)),
    }
}

fn remove_password(service: &str, target_id: &str) -> Result<(), AppError> {
    match delete_generic_password(service, target_id) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == KEYCHAIN_ITEM_NOT_FOUND => Ok(()),
        Err(error) => Err(keychain_error(error)),
    }
}

fn keychain_error(error: KeychainError) -> AppError {
    let detail = error
        .message()
        .unwrap_or_else(|| format!("error code {}", error.code()));
    AppError::Keychain(detail)
}

/// Like `password_for`, but asks for Touch ID first when the saved password is about to
/// be used and the target requires it. A typed password never needs Touch ID.
pub async fn unlock_password(
    target: &DatabaseTarget,
    typed_password: &str,
) -> Result<Option<String>, AppError> {
    let uses_saved_password = typed_password.is_empty() && target.is_password_saved;
    if uses_saved_password && target.require_touch_id {
        let reason = format!("use the saved password for {}", target.database);
        confirm_device_owner(&reason).await?;
    }
    password_for(target, typed_password)
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
            require_touch_id: true,
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
    fn touch_id_is_on_by_default_including_older_saved_targets() {
        assert!(DatabaseTarget::default().require_touch_id);
        let older: DatabaseTarget =
            serde_json::from_str(r#"{ "id": "x", "is_password_saved": true }"#).unwrap();
        assert!(older.require_touch_id);
    }
}
