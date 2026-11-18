// SPDX-License-Identifier: MPL-2.0

//! Power management: powering off a kernelet is the `stop` service, which
//! ends the sandbox with the exit code.

use crate::abi::svc;

/// An exit code that denotes the reason for restarting or powering off.
#[derive(Clone, Copy)]
pub enum ExitCode {
    /// The code that indicates a successful exit.
    Success,
    /// The code that indicates a failed exit.
    Failure,
}

/// Powers off the system.
///
/// On a Linux host this stops the kernelet instance: the endovisor logs the
/// exit code and kills every carrier, this one included. The calling coroutine
/// is abandoned, so this function does not return.
pub(crate) fn stop_instance(code: ExitCode) -> ! {
    let code = match code {
        ExitCode::Success => 0,
        ExitCode::Failure => 1,
    };
    // SAFETY: the service never returns.
    unsafe { (svc().stop)(code) }
}

static POWEROFF_HANDLER: crate::utils::Once<fn(ExitCode)> = crate::utils::Once::new();
static RESTART_HANDLER: crate::utils::Once<fn(ExitCode)> = crate::utils::Once::new();
pub fn inject_poweroff_handler(handler: fn(ExitCode)) {
    POWEROFF_HANDLER.call_once(|| handler);
}
pub fn inject_restart_handler(handler: fn(ExitCode)) {
    RESTART_HANDLER.call_once(|| handler);
}
pub fn poweroff(code: ExitCode) -> ! {
    if let Some(handler) = POWEROFF_HANDLER.get() {
        handler(code)
    }
    stop_instance(code)
}
pub fn restart(code: ExitCode) -> ! {
    if let Some(handler) = RESTART_HANDLER.get() {
        handler(code)
    }
    stop_instance(code)
}
