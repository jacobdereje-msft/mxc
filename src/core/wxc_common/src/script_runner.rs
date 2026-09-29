// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::logger::Logger;
use crate::models::{ExecutionRequest, ScriptResponse};
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
    eprint!("{}", cli_standard_error(response));
    std::process::exit(cli_exit_code(response));
}

/// Render stderr for an executor CLI response.
///
/// MXC failures use one typed JSON envelope so executor consumers can
/// distinguish policy rejection, backend unavailability, and other backend
/// failures. The envelope replaces the backend's duplicate bare diagnostic.
/// Workload stderr passes through unchanged.
pub fn cli_standard_error(response: &ScriptResponse) -> String {
    if !is_mxc_failure(response) {
        return response.standard_err.clone();
    }

    let message = if response.error_message.is_empty() {
        response.standard_err.trim().to_string()
    } else {
        response.error_message.clone()
    };
    let mut envelope = serde_json::json!({
        "error": {
            "code": cli_error_code(response),
            "message": message,
        }
    });
    if !response.extended_error.is_empty() {
        envelope["error"]["extended_error"] =
            serde_json::Value::String(response.extended_error.clone());
    }
    format!(
        "{}\n",
        serde_json::to_string(&envelope).unwrap_or_else(|_| {
            r#"{"error":{"code":"backend_error","message":"failed to serialize error envelope"}}"#
                .to_string()
        })
    )
}

fn is_mxc_failure(response: &ScriptResponse) -> bool {
    use crate::models::FailurePhase;

    match response.failure_phase {
        FailurePhase::None => !response.error_message.is_empty(),
        FailurePhase::ProcessExited => false,
        _ => true,
    }
}

fn cli_error_code(response: &ScriptResponse) -> &'static str {
    use crate::models::FailurePhase;

    match response.failure_phase {
        FailurePhase::Rejected => "policy_validation",
        FailurePhase::BackendUnavailable => "backend_unavailable",
        _ => "backend_error",
    }
}

/// Return the executor process exit code for a completed response.
///
/// Workload exit codes pass through unchanged. MXC failures use `1` rather
/// than leaking the internal `-1` sentinel through the process boundary.
pub fn cli_exit_code(response: &ScriptResponse) -> i32 {
    use crate::models::FailurePhase;

    match response.failure_phase {
        FailurePhase::ProcessExited => response.exit_code,
        FailurePhase::None if response.error_message.is_empty() => response.exit_code,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{cli_exit_code, cli_standard_error, get_timeout_milliseconds};
    use crate::models::{FailurePhase, ScriptResponse};

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
    fn cli_exit_code_normalizes_mxc_failures() {
        assert_eq!(
            cli_exit_code(&ScriptResponse {
                exit_code: -1,
                error_message: "policy rejected".to_string(),
                failure_phase: FailurePhase::Rejected,
                ..Default::default()
            }),
            1
        );
    }

    #[test]
    fn cli_exit_code_normalizes_legacy_unclassified_mxc_failures() {
        assert_eq!(cli_exit_code(&ScriptResponse::error("backend failed")), 1);
    }

    #[test]
    fn cli_exit_code_preserves_workload_exit() {
        assert_eq!(
            cli_exit_code(&ScriptResponse {
                exit_code: 42,
                error_message: "actionable child diagnostic".to_string(),
                failure_phase: FailurePhase::ProcessExited,
                ..Default::default()
            }),
            42
        );
    }

    #[test]
    fn cli_exit_code_preserves_clean_nonzero_workload_exit() {
        assert_eq!(
            cli_exit_code(&ScriptResponse {
                exit_code: 7,
                failure_phase: FailurePhase::ProcessExited,
                ..Default::default()
            }),
            7
        );
    }

    #[test]
    fn cli_standard_error_maps_policy_rejection() {
        assert_eq!(
            cli_standard_error(&ScriptResponse {
                error_message: "policy rejected".to_string(),
                standard_err: "policy rejected".to_string(),
                failure_phase: FailurePhase::Rejected,
                ..Default::default()
            }),
            "{\"error\":{\"code\":\"policy_validation\",\"message\":\"policy rejected\"}}\n"
        );
    }

    #[test]
    fn cli_standard_error_maps_legacy_unclassified_failure() {
        assert_eq!(
            cli_standard_error(&ScriptResponse {
                error_message: "backend failed".to_string(),
                ..Default::default()
            }),
            "{\"error\":{\"code\":\"backend_error\",\"message\":\"backend failed\"}}\n"
        );
    }

    #[test]
    fn cli_standard_error_includes_extended_detail() {
        assert_eq!(
            cli_standard_error(&ScriptResponse {
                standard_err: "backend failed".to_string(),
                error_message: "backend failed".to_string(),
                extended_error: "HRESULT(0x80004005)".to_string(),
                ..Default::default()
            }),
            "{\"error\":{\"code\":\"backend_error\",\"extended_error\":\"HRESULT(0x80004005)\",\"message\":\"backend failed\"}}\n"
        );
    }

    #[test]
    fn cli_standard_error_preserves_workload_stderr() {
        assert_eq!(
            cli_standard_error(&ScriptResponse {
                standard_err: "child failed\n".to_string(),
                error_message: "actionable child diagnostic".to_string(),
                failure_phase: FailurePhase::ProcessExited,
                ..Default::default()
            }),
            "child failed\n"
        );
    }
}
