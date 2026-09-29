// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! WSLC state-aware lifecycle reachability from the Rust SDK.
//!
//! Host-dependent tests skip when the backend is unavailable, so this file is
//! safe to run on any host. They assume the image is already cached; pulling is
//! covered by the WSLC executor E2E suite.

#![cfg(all(target_os = "windows", feature = "wslc"))]

use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

use mxc_sdk::{
    sandbox, ErrorCode, ExecRequest, LifecycleRequest, OperationOptions, Output, ProvisionRequest,
    SandboxId, WaitOutcome,
};

const VERSION: &str = "0.9.0-alpha";

const EXEC_TIMEOUT_MS: u32 = 60_000;

const RUNTIME_UNITS: [&str; 2] = ["wslcsdk.dll", "wxc-wslc-daemon.exe"];

/// Cargo stages the WSLC runtime units one directory above `deps/`, where this
/// test binary runs.
fn stage_runtime_units() -> Result<(), String> {
    let exe =
        std::env::current_exe().map_err(|e| format!("cannot locate this test binary: {e}"))?;
    let test_dir = exe
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", exe.display()))?;
    let profile_dir = test_dir
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", test_dir.display()))?;

    for unit in RUNTIME_UNITS {
        let source = profile_dir.join(unit);
        let staged = test_dir.join(unit);
        if !source.exists() {
            if staged.exists() {
                continue;
            }
            return Err(format!(
                "{unit} is beside neither this test binary nor {}; produce both units with \
                 `cargo build -p wxc_wslc_daemon -p mxc-sdk --features mxc-sdk/wslc`",
                profile_dir.display()
            ));
        }
        if is_already_staged(&source, &staged) {
            continue;
        }
        std::fs::copy(&source, &staged).map_err(|e| {
            format!(
                "cannot stage {} over {}: {e}. A daemon left running by an earlier run holds \
                 its own image open; stop it or wait out its idle timeout",
                source.display(),
                staged.display()
            )
        })?;
    }
    Ok(())
}

/// Compares content rather than timestamps, because every rebuild re-copies an
/// identical `wslcsdk.dll` into the profile directory.
fn is_already_staged(source: &Path, staged: &Path) -> bool {
    let Ok(source) = std::fs::read(source) else {
        return false;
    };
    let Ok(staged) = std::fs::read(staged) else {
        return false;
    };
    source == staged
}

fn staging() -> &'static Result<(), String> {
    static STAGED: OnceLock<Result<(), String>> = OnceLock::new();
    STAGED.get_or_init(stage_runtime_units)
}

fn host_supports_wslc() -> bool {
    mxc_sdk::available_backends()
        .iter()
        .any(|b| b.backend == "wslc")
}

/// A skipped test reports as a pass, so a fully-skipped suite looks like one
/// that ran. `MXC_WSLC_TESTS_REQUIRED=1` turns every skip into a failure.
fn skips_are_failures() -> bool {
    matches!(
        std::env::var("MXC_WSLC_TESTS_REQUIRED").as_deref(),
        Ok("1") | Ok("true")
    )
}

macro_rules! skip_unless_supported {
    () => {
        if let Err(reason) = staging() {
            assert!(
                !skips_are_failures(),
                "MXC_WSLC_TESTS_REQUIRED is set, but the WSLC runtime units are not beside \
                 this test binary: {reason}"
            );
            eprintln!("skipping: {reason} (set MXC_WSLC_TESTS_REQUIRED=1 to make skips fail)");
            return;
        }
        if !host_supports_wslc() {
            assert!(
                !skips_are_failures(),
                "MXC_WSLC_TESTS_REQUIRED is set, but available_backends() does not report the \
                 WSLC backend. That needs a build with the wslc feature and a host running \
                 WSL2 with the WSLC runtime."
            );
            eprintln!(
                "skipping: WSLC is not available on this host \
                 (set MXC_WSLC_TESTS_REQUIRED=1 to make skips fail)"
            );
            return;
        }
    };
}

/// Concurrent WSLC sessions sharing an image store fail against each other.
fn host_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Provision mints a real container; a failure before deprovision leaves it on
/// the host.
struct Teardown(Option<SandboxId>);

impl Teardown {
    /// Gives up ownership after the test has deprovisioned itself, which is not
    /// idempotent — a second call fails the id as unprovisioned.
    fn defuse(mut self) {
        self.0 = None;
    }
}

impl Drop for Teardown {
    fn drop(&mut self) {
        let Some(id) = self.0.take() else {
            return;
        };
        let _ = sandbox::stop(
            &id,
            LifecycleRequest::new(VERSION),
            OperationOptions::default(),
        );
        if let Err(e) = sandbox::deprovision(
            &id,
            LifecycleRequest::new(VERSION),
            OperationOptions::default(),
        ) {
            eprintln!("WARNING: deprovision of {id} failed, the container may leak: {e:?}");
        }
    }
}

fn test_image() -> String {
    std::env::var("MXC_WSLC_TEST_IMAGE").unwrap_or_else(|_| "alpine:latest".to_string())
}

fn provision() -> (SandboxId, Teardown) {
    let request = ProvisionRequest::wslc(VERSION, Some(test_image()), None);
    let provisioned = sandbox::provision(request, OperationOptions::default())
        .expect("provision must succeed with the test image cached");
    let sandbox_id = provisioned.sandbox_id;
    let teardown = Teardown(Some(sandbox_id.clone()));
    (sandbox_id, teardown)
}

fn provision_and_start() -> (SandboxId, Teardown) {
    let (sandbox_id, teardown) = provision();
    sandbox::start(
        &sandbox_id,
        LifecycleRequest::new(VERSION),
        OperationOptions::default(),
    )
    .expect("start must succeed");
    (sandbox_id, teardown)
}

fn exec_capture(sandbox_id: &SandboxId, command: &str) -> Output {
    let mut request = ExecRequest::new(VERSION, command);
    request.set_timeout(EXEC_TIMEOUT_MS);
    sandbox::exec(sandbox_id, request, OperationOptions::default())
        .expect("exec must return a handle")
        .wait_with_output()
        .expect("waiting on the exec must succeed")
}

/// Staging is what makes the backend reachable from a test binary.
#[test]
fn available_backends_reports_wslc_to_a_test_binary() {
    skip_unless_supported!();

    let reported: Vec<String> = mxc_sdk::available_backends()
        .iter()
        .map(|b| b.backend.clone())
        .collect();
    assert!(
        reported.iter().any(|backend| backend == "wslc"),
        "available_backends() reported {reported:?}"
    );
}

#[test]
fn provision_returns_a_usable_sandbox_id() {
    skip_unless_supported!();
    let _serialized = host_lock();

    let (sandbox_id, _teardown) = provision();

    // Post-provision phases resolve the backend from this prefix.
    assert!(
        sandbox_id.as_str().starts_with("wslc:"),
        "provision returned {sandbox_id}"
    );
}

#[test]
fn start_and_exec_return_the_workload_output() {
    skip_unless_supported!();
    let _serialized = host_lock();

    let (sandbox_id, _teardown) = provision_and_start();
    let output = exec_capture(&sandbox_id, "echo rust-wslc-marker");

    assert_eq!(output.outcome, WaitOutcome::Exited(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("rust-wslc-marker"),
        "exec stdout did not carry the marker, got: {stdout:?}"
    );
}

#[test]
fn exec_propagates_a_non_zero_exit_code() {
    skip_unless_supported!();
    let _serialized = host_lock();

    let (sandbox_id, _teardown) = provision_and_start();
    let output = exec_capture(&sandbox_id, "exit 7");

    assert_eq!(output.outcome, WaitOutcome::Exited(7));
}

#[test]
fn deprovision_retires_the_sandbox_id() {
    skip_unless_supported!();
    let _serialized = host_lock();

    let (sandbox_id, teardown) = provision_and_start();
    sandbox::stop(
        &sandbox_id,
        LifecycleRequest::new(VERSION),
        OperationOptions::default(),
    )
    .expect("stop must succeed");
    sandbox::deprovision(
        &sandbox_id,
        LifecycleRequest::new(VERSION),
        OperationOptions::default(),
    )
    .expect("deprovision must succeed");
    teardown.defuse();

    let error = sandbox::start(
        &sandbox_id,
        LifecycleRequest::new(VERSION),
        OperationOptions::default(),
    )
    .expect_err("a deprovisioned id must not start");
    assert_eq!(error.code, ErrorCode::NotProvisioned);
}
