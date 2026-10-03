// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Windows ProcessContainer streaming integration test, in its own
//! Windows-gated file. The sibling `streaming.rs` is `#![cfg(macos)]`, which
//! would otherwise make a `#[cfg(windows)]` test there impossible to compile.
//! Requires a Windows host with a usable ProcessContainer tier, so it is
//! `#[ignore]`d. PTY execution uses the AppContainer fallback; policies that
//! require host DACL mutation may also require the preparation described in
//! docs/host-prep.md.

#![cfg(target_os = "windows")]

use mxc_sdk::v1::{build_request, spawn_sandbox, spawn_with_pty, SandboxPolicy};
use mxc_sdk::{MxcPtySize, WaitOutcome};

fn processcontainer_request(command: &str) -> mxc_sdk::v1::SandboxRequest {
    let mut policy = SandboxPolicy::default();
    let mut egress = mxc_sdk::v1::NetworkEgressSection::default();
    egress.default = Some(mxc_sdk::v1::NetworkAction::Allow);
    let mut network = mxc_sdk::v1::policy::NetworkSection::default();
    network.egress = Some(egress);
    policy.network = Some(network);
    policy.ui = Some(mxc_sdk::v1::policy::UiSection {
        allow_windows: true,
        ..Default::default()
    });
    policy.timeout_ms = Some(5_000);
    let mut request = build_request(&policy, command, None).expect("build_request");
    request.set_working_directory("C:\\Windows");
    request
}

#[test]
#[ignore = "requires a Windows host with a usable ProcessContainer tier"]
fn streaming_processcontainer_bidirectional_stdio() {
    use std::io::{Read, Write};

    // `cmd /c more` echoes stdin to stdout until EOF, then exits.
    let request = processcontainer_request("cmd /c more");
    let mut proc = spawn_sandbox(request).expect("spawn");

    let mut stdin = proc.take_stdin().expect("stdin available");
    let mut stdout = proc.take_stdout().expect("stdout available");

    stdin.write_all(b"ping-pong\r\n").expect("write stdin");
    drop(stdin);

    let mut out = String::new();
    stdout.read_to_string(&mut out).expect("read stdout");
    assert!(out.contains("ping-pong"), "got: {:?}", out);

    assert_eq!(proc.wait().expect("wait"), WaitOutcome::Exited(0));
}

#[test]
#[ignore = "requires a Windows host with a usable ProcessContainer tier"]
fn processcontainer_pty_supports_io_resize_and_wait() {
    use std::io::{Read, Write};

    let request = processcontainer_request(
        "cmd.exe /d /q /c \"set /p value= & echo MXC_PROCESSCONTAINER_PTY_OK\"",
    );
    let terminal = spawn_with_pty(request, MxcPtySize::default()).expect("spawn_with_pty");
    let resized = MxcPtySize {
        rows: 40,
        cols: 120,
        ..MxcPtySize::default()
    };
    terminal.resize(resized).expect("resize");
    assert_eq!(terminal.size().expect("size"), resized);

    let mut reader = terminal.try_clone_reader().expect("reader");
    let reader_thread = std::thread::spawn(move || {
        let mut output = String::new();
        reader.read_to_string(&mut output).expect("read output");
        output
    });
    let mut writer = terminal.take_writer().expect("writer");
    writer.write_all(b"hello\r\n").expect("write input");
    let outcome = terminal.wait().expect("wait");
    drop(writer);
    let output = reader_thread.join().expect("reader thread");
    assert_eq!(
        outcome,
        WaitOutcome::Exited(0),
        "terminal output: {output:?}"
    );
    assert!(
        output.contains("MXC_PROCESSCONTAINER_PTY_OK"),
        "got: {output:?}"
    );
}
