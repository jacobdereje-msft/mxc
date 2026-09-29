// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::logger::Logger;
use crate::models::{ExecutionRequest, FailurePhase, ScriptResponse};
use crate::validator::{validate_common, validate_network_policy_support, NetworkPolicySupport};

/// Trait for executing scripts within a containment backend.
///
/// Each backend (AppContainer, Windows Sandbox, etc.) implements this trait
/// to provide a uniform interface for `wxc-exec`.
///
/// Implementors provide [`execute`](ScriptRunner::execute) and optionally
/// [`validate_runner`](ScriptRunner::validate_runner). The provided
/// [`run`](ScriptRunner::run) method handles validation, dry-run mode,
/// and delegates to [`execute`](ScriptRunner::execute).
pub trait ScriptRunner {
    /// Validate shared network support and runner-specific constraints.
    fn validate_runner(&self, request: &ExecutionRequest) -> Result<(), ScriptResponse> {
        validate_network_policy_support(request, NetworkPolicySupport::LEGACY)?;
        Ok(())
    }

    /// Execute the script inside this backend's containment and return the response.
    /// Implement this instead of `run` — validation and dry-run are handled by the trait.
    fn execute(&mut self, request: &ExecutionRequest, logger: &mut Logger) -> ScriptResponse;

    /// Entry point called by the binary. Runs shared validation, runner-specific
    /// validation, checks for dry-run mode, then delegates to
    /// [`execute`](ScriptRunner::execute).
    fn run(&mut self, request: &ExecutionRequest, logger: &mut Logger) -> ScriptResponse {
        if let Err(response) = validate_common(request) {
            return response;
        }

        if let Err(response) = self.validate_runner(request) {
            return response;
        }

        if request.dry_run {
            return ScriptResponse {
                exit_code: 0,
                ..Default::default()
            };
        }

        self.execute(request, logger)
    }
}

/// Convert a timeout value to milliseconds, treating 0 as infinite (INFINITE = `u32::MAX`).
pub fn get_timeout_milliseconds(timeout: u32) -> u32 {
    if timeout == 0 {
        u32::MAX
    } else {
        timeout
    }
}

/// Print a dry-run result message to the logger, flush, and exit the process.
pub fn handle_dry_run_exit(response: &ScriptResponse, logger: &mut Logger) -> ! {
    use std::fmt::Write;
    if response.exit_code == 0 {
        let _ = writeln!(logger, "Dry run completed. Result: validation passed");
    } else {
        let _ = writeln!(logger, "Dry run completed. Result: validation failed");
    }
    print!("{}", logger.get_buffer());
    std::process::exit(process_exit_code(response));
}

/// Process exit code for a completed run.
///
/// A rejected request exits 1, matching a parser-side rejection, so a caller
/// can tell a refused policy from a crash, a launch failure or a timeout — all
/// of which report -1 (see [`FailurePhase::Timeout`]). Every other phase keeps
/// the runner's own exit code, including a faithfully propagated guest code.
pub fn process_exit_code(response: &ScriptResponse) -> i32 {
    match response.failure_phase {
        FailurePhase::Rejected => 1,
        _ => response.exit_code,
    }
}

/// Whether [`emit_backend_error_envelope`] will emit for this response.
fn envelope_applies(response: &ScriptResponse) -> bool {
    response.exit_code != 0 && !response.error_message.is_empty()
}

/// Relay a completed run's captured stderr, terminating it with a newline so
/// that a diagnostic written afterwards starts on its own line.
///
/// Error responses that never ran a process copy `error_message` into
/// `standard_err`; [`emit_backend_error_envelope`] already carries that text,
/// so it is skipped here rather than printed twice.
pub fn emit_captured_stderr(response: &ScriptResponse) {
    if response.standard_err.is_empty()
        || (envelope_applies(response) && response.standard_err == response.error_message)
    {
        return;
    }
    if response.standard_err.ends_with('\n') {
        eprint!("{}", response.standard_err);
    } else {
        eprintln!("{}", response.standard_err);
    }
}

/// Emit a structured JSON error envelope on stderr when a completed run carries
/// an infrastructure error message.
///
/// Shared by `wxc-exec` and `lxc-exec` so that MXC never exits non-zero on an
/// infrastructure failure without first printing a machine-readable diagnostic
/// (see issue #564). This deliberately keys off a **non-empty**
/// `error_message`: a sandboxed process that merely exits non-zero on its own
/// (a faithfully propagated guest exit code, no MXC error) leaves
/// `error_message` empty and is intentionally not annotated here.
///
/// The code is derived from the response's [`FailurePhase`], so a backend
/// rejection is reported as `policy_validation` — the same typed code a
/// parser-side rejection produces.
///
/// In non-debug mode the diagnostic `Logger` is buffered and never flushed, so
/// this envelope is the only place the error surfaces to the caller.
pub fn emit_backend_error_envelope(response: &ScriptResponse) {
    if !envelope_applies(response) {
        return;
    }

    let mut envelope = serde_json::json!({
        "error": {
            "code": response.failure_phase.error_code().as_str(),
            "message": response.error_message,
        }
    });
    if !response.extended_error.is_empty() {
        envelope["error"]["extended_error"] =
            serde_json::Value::String(response.extended_error.clone());
    }
    if let Ok(json) = serde_json::to_string(&envelope) {
        eprintln!("{json}");
    }
}

#[cfg(test)]
mod tests {
    use super::get_timeout_milliseconds;

    #[test]
    fn timeout_zero_returns_u32_max() {
        let result = get_timeout_milliseconds(0);
        assert_eq!(result, u32::MAX);
    }

    #[test]
    fn timeout_non_zero_returns_same_value() {
        let value = 1500u32;
        let result = get_timeout_milliseconds(value);
        assert_eq!(result, value);
    }

    #[test]
    fn error_envelope_is_noop_without_error() {
        use crate::models::ScriptResponse;
        // exit 0 => no-op; non-zero but empty message (clean sandbox exit) => no-op.
        super::emit_backend_error_envelope(&ScriptResponse {
            exit_code: 0,
            error_message: "ignored on success".to_string(),
            ..Default::default()
        });
        super::emit_backend_error_envelope(&ScriptResponse {
            exit_code: 1,
            error_message: String::new(),
            ..Default::default()
        });
    }

    #[test]
    fn error_envelope_emits_on_infra_failure() {
        use crate::models::ScriptResponse;
        // Exercises the serialization branch (writes to stderr); must not panic.
        super::emit_backend_error_envelope(&ScriptResponse {
            exit_code: 1,
            error_message: "backend unavailable".to_string(),
            extended_error: "WIN32_ERROR(1920)".to_string(),
            ..Default::default()
        });
    }

    #[test]
    fn failure_phase_selects_the_envelope_code() {
        use crate::models::FailurePhase;
        use crate::mxc_error::MxcErrorCode;

        assert_eq!(
            FailurePhase::Rejected.error_code(),
            MxcErrorCode::PolicyValidation
        );
        assert_eq!(
            FailurePhase::BackendUnavailable.error_code(),
            MxcErrorCode::BackendUnavailable
        );
        for phase in [
            FailurePhase::None,
            FailurePhase::LaunchFailed,
            FailurePhase::PostLaunchFailed,
            FailurePhase::ProcessExited,
            FailurePhase::Timeout,
        ] {
            assert_eq!(phase.error_code(), MxcErrorCode::BackendError, "{phase:?}");
        }
    }

    #[test]
    fn a_rejection_exits_one_and_every_other_failure_keeps_its_code() {
        use crate::models::{FailurePhase, ScriptResponse};

        assert_eq!(
            super::process_exit_code(&ScriptResponse::rejected("unsupported policy")),
            1
        );
        // -1 stays -1 for a crash, a launch failure or a timeout, so a caller
        // that sees 1 knows the request itself was refused.
        for phase in [FailurePhase::LaunchFailed, FailurePhase::Timeout] {
            assert_eq!(
                super::process_exit_code(&ScriptResponse {
                    failure_phase: phase,
                    ..ScriptResponse::error("boom")
                }),
                -1
            );
        }
        assert_eq!(
            super::process_exit_code(&ScriptResponse {
                exit_code: 3,
                failure_phase: FailurePhase::ProcessExited,
                ..Default::default()
            }),
            3
        );
    }

    #[test]
    fn captured_stderr_skips_the_copy_the_envelope_carries() {
        use crate::models::ScriptResponse;
        // A rejection duplicates its message into `standard_err`; the envelope
        // is the machine-readable copy, so nothing is relayed bare here.
        super::emit_captured_stderr(&ScriptResponse::rejected("unsupported policy"));
        // Real workload stderr is relayed even when an MXC error is also set.
        super::emit_captured_stderr(&ScriptResponse {
            exit_code: -1,
            standard_err: "workload wrote this".to_string(),
            error_message: "script timed out after 2000ms".to_string(),
            ..Default::default()
        });
    }
}
