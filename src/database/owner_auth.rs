//! Touch ID before a saved database password is used. Falls back to the Mac's login
//! password when Touch ID is unavailable (no sensor, lid closed, too many attempts).

use std::sync::Mutex;

use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{NSError, NSString};
use objc2_local_authentication::{LAContext, LAError, LAPolicy};
use tokio::sync::oneshot;

use crate::error::AppError;

/// LocalAuthentication codes meaning "the user (or system) dismissed the prompt".
const CANCELLATION_CODES: [LAError; 3] = [
    LAError::UserCancel,
    LAError::SystemCancel,
    LAError::AppCancel,
];

/// Shows the system Touch ID prompt: "DB Refresh is trying to <reason>."
pub async fn confirm_device_owner(reason: &str) -> Result<(), AppError> {
    let (result_sender, result_receiver) = oneshot::channel();
    // The reply block is `Fn`, but the sender can be used once; the Mutex<Option> lets
    // the first (and only) reply take it.
    let result_sender = Mutex::new(Some(result_sender));
    let reply = RcBlock::new(move |is_success: Bool, error: *mut NSError| {
        let outcome = if is_success.as_bool() {
            Ok(())
        } else {
            Err(authentication_error(error))
        };
        if let Some(sender) = result_sender.lock().ok().and_then(|mut slot| slot.take()) {
            let _ = sender.send(outcome);
        }
    });

    // SAFETY: plain Objective-C calls. `context` lives until the reply arrives (it is
    // held across the `.await` below), and `reason`/`reply` are valid for the call.
    let context = unsafe { LAContext::new() };
    let reason = NSString::from_str(reason);
    unsafe {
        context.evaluatePolicy_localizedReason_reply(
            LAPolicy::DeviceOwnerAuthentication,
            &reason,
            &reply,
        );
    }

    result_receiver.await.unwrap_or_else(|_| {
        Err(AppError::Authentication(
            "Touch ID did not answer.".to_string(),
        ))
    })
}

fn authentication_error(error: *mut NSError) -> AppError {
    // SAFETY: LocalAuthentication passes either null or an NSError that is valid for
    // the duration of the reply call, which is where this runs.
    let Some(error) = (unsafe { error.as_ref() }) else {
        return AppError::Authentication("Authentication failed.".to_string());
    };
    if CANCELLATION_CODES.contains(&LAError(error.code())) {
        return AppError::AuthenticationCancelled;
    }
    AppError::Authentication(error.localizedDescription().to_string())
}
