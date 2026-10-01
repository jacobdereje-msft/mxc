// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Per-run Seatbelt report collection and decoding.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use chrono::{Local, NaiveDateTime, TimeZone, Utc};
use learning_mode_core::{
    unique_capture_output_paths, write_denials_output, AccessType, AnalysisResult, DedupKey,
    DeniedResource, ResourceType, VerboseLoggingOutcomeReason, VerboseLoggingProvider,
    VerboseLoggingSignature,
};
use wxc_common::models::{CaptureDenialsConfig, CaptureDenialsMode, CaptureDenialsOutput};

const MAX_CAPTURE_BYTES: usize = 16 * 1024 * 1024;
const MAX_UNIQUE_DENIALS: usize = 10_000;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(3);
const FLUSH_DELAY: Duration = Duration::from_millis(250);
const WINDOWS_EPOCH_OFFSET_TICKS: i128 = 116_444_736_000_000_000;

struct CaptureRead {
    lines: Vec<String>,
    overflowed: bool,
}

/// Active per-run Seatbelt report stream.
pub struct SeatbeltCapture {
    marker: String,
    mode: CaptureDenialsMode,
    output_path: std::path::PathBuf,
    trace_path: Option<std::path::PathBuf>,
    child: Child,
    stdout_thread: Option<JoinHandle<std::io::Result<CaptureRead>>>,
    stderr_thread: Option<JoinHandle<std::io::Result<String>>>,
    trace_written: bool,
    finished: bool,
}

impl SeatbeltCapture {
    /// Starts a marker-filtered unified-log stream and waits until it is ready.
    pub fn start(config: &CaptureDenialsConfig) -> Result<Self, String> {
        let marker = format!(
            "MXC-CAPTURE-{}",
            learning_mode_core::random_capture_suffix()?
        );
        let startup_marker = format!(
            "MXC-CAPTURE-READY-{}",
            learning_mode_core::random_capture_suffix()?
        );
        let paths = unique_capture_output_paths(
            config.output_path.as_deref(),
            config.retain_trace,
            "seatbelt.log",
        )?;
        let predicate = format!(
            "sender == \"Sandbox\" AND (eventMessage CONTAINS \"{marker}\" OR \
             eventMessage CONTAINS \"{startup_marker}\")"
        );
        let mut child = Command::new("/usr/bin/log")
            .args([
                "stream",
                "--style",
                "compact",
                "--level",
                "debug",
                "--predicate",
                &predicate,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Seatbelt capture could not start log stream: {error}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Seatbelt capture log stream did not expose stdout".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "Seatbelt capture log stream did not expose stderr".to_string())?;
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let reader_marker = marker.clone();
        let reader_startup_marker = startup_marker.clone();
        let stdout_thread = thread::spawn(move || {
            read_capture_stream(stdout, ready_tx, &reader_marker, &reader_startup_marker)
        });
        let stderr_thread = thread::spawn(move || read_bounded(stderr));
        if let Err(message) = run_startup_probe(&startup_marker) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(message);
        }
        match ready_rx.recv_timeout(STARTUP_TIMEOUT) {
            Ok(Ok(())) => Ok(Self {
                marker,
                mode: config.mode,
                output_path: paths.denials,
                trace_path: paths.trace,
                child,
                stdout_thread: Some(stdout_thread),
                stderr_thread: Some(stderr_thread),
                trace_written: false,
                finished: false,
            }),
            Ok(Err(message)) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(message)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = child.kill();
                let _ = child.wait();
                Err("Seatbelt capture log stream exited before becoming ready".to_string())
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = child.kill();
                let _ = child.wait();
                Err("Seatbelt capture log stream did not become ready within 3 seconds".to_string())
            }
        }
    }

    /// Unique message attached to every report produced by this run.
    pub fn marker(&self) -> &str {
        &self.marker
    }

    /// Stops collection, decodes reports, and writes the denial documents.
    pub fn finish(&mut self, exit_code: i32) -> std::io::Result<CaptureDenialsOutput> {
        thread::sleep(FLUSH_DELAY);
        let _ = self.child.kill();
        let _ = self.child.wait();
        let captured = join_capture(self.stdout_thread.take())?;
        let stderr = join_stderr(self.stderr_thread.take())?;
        self.finished = true;
        if let Some(trace_path) = self.trace_path.as_ref() {
            let write_result = write_trace(trace_path, &captured.lines);
            self.trace_written = trace_path.exists();
            write_result?;
        }
        if captured.overflowed {
            return Err(std::io::Error::other(
                "Seatbelt capture exceeded the 16 MiB raw-report limit",
            ));
        }
        if !stderr.trim().is_empty() {
            return Err(std::io::Error::other(format!(
                "Seatbelt capture log stream failed: {}",
                stderr.trim()
            )));
        }
        let analysis = parse_reports(&captured.lines, &self.marker, self.mode)?;
        let pointer = write_denials_output(analysis, exit_code, &self.output_path)?;
        Ok(CaptureDenialsOutput {
            kind: pointer.kind,
            output_path: pointer.output_path,
            exit_code: pointer.exit_code,
            total_denials: pointer.total_denials,
            denied_resources_truncated: pointer.denied_resources_truncated,
            etl_path: None,
            trace_path: self
                .trace_path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
        })
    }

    /// Retained raw trace path, when retention was requested.
    pub fn trace_path(&self) -> Option<&Path> {
        if self.trace_written {
            self.trace_path.as_deref()
        } else {
            None
        }
    }

    fn stop_without_output(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = join_capture(self.stdout_thread.take());
        let _ = join_stderr(self.stderr_thread.take());
    }
}

impl Drop for SeatbeltCapture {
    fn drop(&mut self) {
        if !self.finished {
            self.stop_without_output();
        }
    }
}

fn read_capture_stream(
    stdout: impl Read,
    ready_tx: mpsc::SyncSender<Result<(), String>>,
    marker: &str,
    startup_marker: &str,
) -> std::io::Result<CaptureRead> {
    let lines = BufReader::new(stdout).lines();
    let mut retained = Vec::new();
    let mut retained_bytes = 0usize;
    let mut overflowed = false;
    let mut ready = false;
    let mut pending: Option<String> = None;
    for line in lines {
        let line = line?;
        if line == startup_marker {
            pending = None;
            if !ready {
                ready = true;
                let _ = ready_tx.send(Ok(()));
            }
            continue;
        }
        if line == marker {
            if ready {
                if let Some(event) = pending.take() {
                    let line_bytes = event.len().saturating_add(marker.len()).saturating_add(2);
                    if !overflowed && retained_bytes.saturating_add(line_bytes) <= MAX_CAPTURE_BYTES
                    {
                        retained_bytes += line_bytes;
                        retained.push(event);
                        retained.push(marker.to_string());
                    } else {
                        overflowed = true;
                    }
                }
            }
            continue;
        }
        pending = Some(line);
    }
    if !ready {
        let _ = ready_tx.send(Err(
            "Seatbelt capture log stream exited before the startup probe was observed".to_string(),
        ));
    }
    Ok(CaptureRead {
        lines: retained,
        overflowed,
    })
}

fn run_startup_probe(marker: &str) -> Result<(), String> {
    let profile = format!("(version 1) (allow (with report) default (with message \"{marker}\"))");
    let output = Command::new("/usr/bin/sandbox-exec")
        .args(["-p", &profile, "/usr/bin/true"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("Seatbelt capture startup probe failed to launch: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Seatbelt capture startup probe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn read_bounded(stderr: impl Read) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    stderr
        .take(64 * 1024)
        .read_to_end(&mut bytes)
        .map(|_| String::from_utf8_lossy(&bytes).into_owned())
}

fn join_capture(
    thread: Option<JoinHandle<std::io::Result<CaptureRead>>>,
) -> std::io::Result<CaptureRead> {
    thread
        .ok_or_else(|| std::io::Error::other("Seatbelt capture reader was already consumed"))?
        .join()
        .map_err(|_| std::io::Error::other("Seatbelt capture reader panicked"))?
}

fn join_stderr(thread: Option<JoinHandle<std::io::Result<String>>>) -> std::io::Result<String> {
    thread
        .ok_or_else(|| std::io::Error::other("Seatbelt capture stderr reader was consumed"))?
        .join()
        .map_err(|_| std::io::Error::other("Seatbelt capture stderr reader panicked"))?
}

fn write_trace(path: &Path, lines: &[String]) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    for line in lines {
        writeln!(file, "{line}")?;
    }
    file.flush()
}

fn parse_reports(
    lines: &[String],
    marker: &str,
    mode: CaptureDenialsMode,
) -> std::io::Result<AnalysisResult> {
    let mut pending: Option<String> = None;
    let mut seen = HashSet::<DedupKey>::new();
    let mut denials = Vec::new();
    let mut truncated = false;
    let mut verbose_logging = learning_mode_core::VerboseLoggingSummary::default();
    for line in lines {
        if line == marker {
            if let Some(event_line) = pending.take() {
                let occurrences = report_occurrences(&event_line);
                let report = match parse_report_fields(&event_line) {
                    Ok(report) => report,
                    Err(_) => {
                        verbose_logging
                            .record_occurrences(malformed_signature(0, None), occurrences);
                        continue;
                    }
                };
                let denial = match report
                    .as_ref()
                    .and_then(|report| denial_from_report(&event_line, report, mode).transpose())
                    .transpose()
                {
                    Ok(denial) => denial,
                    Err(_) => {
                        verbose_logging.record_occurrences(
                            malformed_signature(0, report.as_ref()),
                            occurrences,
                        );
                        continue;
                    }
                };
                if let Some(report) = report {
                    verbose_logging.record_occurrences(
                        verbose_signature(&report, denial.as_ref(), mode),
                        occurrences,
                    );
                }
                if let Some(denial) = denial {
                    if seen.insert(denial.dedup_key()) {
                        if denials.len() < MAX_UNIQUE_DENIALS {
                            denials.push(denial);
                        } else {
                            truncated = true;
                        }
                    }
                }
            }
        } else {
            pending = Some(line.clone());
        }
    }
    Ok(AnalysisResult {
        denials,
        denied_resources_truncated: truncated,
        verbose_logging,
    })
}

fn malformed_signature(
    fallback_pid: u32,
    report: Option<&ReportFields<'_>>,
) -> VerboseLoggingSignature {
    let properties = report.map_or_else(Vec::new, |report| {
        vec![
            (
                "disposition".to_string(),
                report.disposition.chars().take(64).collect(),
            ),
            (
                "operation".to_string(),
                report.operation.chars().take(128).collect(),
            ),
        ]
    });
    VerboseLoggingSignature {
        provider: VerboseLoggingProvider::Seatbelt,
        provider_guid: "com.apple.sandbox".to_string(),
        event_id: 0,
        reason: VerboseLoggingOutcomeReason::EventPayloadMalformed,
        pid: report.map_or(fallback_pid, |report| report.pid),
        access_type: None,
        resource_type: None,
        properties,
    }
}

fn report_occurrences(line: &str) -> u64 {
    let prefix = line
        .split_once("Sandbox: ")
        .map_or(line, |(prefix, _)| prefix);
    prefix
        .rsplit_once(" duplicate report")
        .and_then(|(prefix, _)| prefix.split_whitespace().last())
        .and_then(|count| count.parse::<u64>().ok())
        .unwrap_or(1)
}

#[cfg(test)]
fn parse_report(line: &str, mode: CaptureDenialsMode) -> std::io::Result<Option<DeniedResource>> {
    let Some(report) = parse_report_fields(line)? else {
        return Ok(None);
    };
    denial_from_report(line, &report, mode)
}

struct ReportFields<'a> {
    pid: u32,
    disposition: &'a str,
    operation: &'a str,
    resource: &'a str,
}

fn parse_report_fields(line: &str) -> std::io::Result<Option<ReportFields<'_>>> {
    let Some(payload) = line.split_once("Sandbox: ").map(|(_, payload)| payload) else {
        return Ok(None);
    };
    let Some(open_pid) = payload.find('(') else {
        return Ok(None);
    };
    let Some(close_pid) = payload[open_pid + 1..].find(')') else {
        return Ok(None);
    };
    let close_pid = open_pid + 1 + close_pid;
    let pid = payload[open_pid + 1..close_pid]
        .parse::<u32>()
        .map_err(|error| std::io::Error::other(format!("invalid Seatbelt report PID: {error}")))?;
    let mut fields = payload[close_pid + 1..].trim().splitn(3, ' ');
    let disposition = fields.next().unwrap_or_default();
    let operation = fields.next().unwrap_or_default();
    let resource = fields.next().unwrap_or_default();
    Ok(Some(ReportFields {
        pid,
        disposition,
        operation,
        resource,
    }))
}

fn denial_from_report(
    line: &str,
    report: &ReportFields<'_>,
    mode: CaptureDenialsMode,
) -> std::io::Result<Option<DeniedResource>> {
    let expected = match mode {
        CaptureDenialsMode::Block => report.disposition.starts_with("deny"),
        CaptureDenialsMode::Allow => report.disposition == "allow",
    };
    if !expected || report.resource.is_empty() {
        return Ok(None);
    }
    let Some((resource, resource_type, access_type)) = classify(report.operation, report.resource)
    else {
        return Ok(None);
    };
    let filetime = parse_filetime(line)?;
    Ok(Some(DeniedResource {
        resource,
        resource_type,
        access_type,
        pid: report.pid,
        filetime,
    }))
}

fn verbose_signature(
    report: &ReportFields<'_>,
    denial: Option<&DeniedResource>,
    mode: CaptureDenialsMode,
) -> VerboseLoggingSignature {
    let expected_disposition = match mode {
        CaptureDenialsMode::Block => report.disposition.starts_with("deny"),
        CaptureDenialsMode::Allow => report.disposition == "allow",
    };
    let reason = if denial.is_some() {
        VerboseLoggingOutcomeReason::Actionable
    } else if !expected_disposition || report.resource.is_empty() {
        VerboseLoggingOutcomeReason::NotActionable
    } else {
        VerboseLoggingOutcomeReason::UnsupportedObjectType
    };
    VerboseLoggingSignature {
        provider: VerboseLoggingProvider::Seatbelt,
        provider_guid: "com.apple.sandbox".to_string(),
        event_id: 0,
        reason,
        pid: report.pid,
        access_type: denial.map(|value| value.access_type),
        resource_type: denial.map(|value| value.resource_type),
        properties: vec![
            (
                "disposition".to_string(),
                report.disposition.chars().take(64).collect(),
            ),
            (
                "operation".to_string(),
                report.operation.chars().take(128).collect(),
            ),
        ],
    }
}

fn classify(operation: &str, resource: &str) -> Option<(String, ResourceType, AccessType)> {
    if resource.starts_with('/') && operation.starts_with("file-read") {
        Some((resource.to_string(), ResourceType::File, AccessType::Read))
    } else if resource.starts_with('/') && operation.starts_with("file-write") {
        Some((resource.to_string(), ResourceType::File, AccessType::Write))
    } else if resource.starts_with('/')
        && (operation == "file-map-executable" || operation.starts_with("process-exec"))
    {
        Some((
            resource.to_string(),
            ResourceType::File,
            AccessType::Execute,
        ))
    } else if operation.starts_with("network-") {
        (!resource.is_empty()).then(|| {
            (
                resource.to_string(),
                ResourceType::Network,
                AccessType::Unknown,
            )
        })
    } else if is_ui_resource(operation, resource) {
        Some((resource.to_string(), ResourceType::Ui, AccessType::Unknown))
    } else if operation == "mach-lookup" && !resource.is_empty() {
        Some((
            resource.to_string(),
            ResourceType::Other,
            AccessType::Unknown,
        ))
    } else {
        None
    }
}

fn is_ui_resource(operation: &str, resource: &str) -> bool {
    (operation == "mach-lookup"
        && matches!(
            resource,
            "com.apple.windowserver.active"
                | "com.apple.windowserver.session"
                | "com.apple.coreservices.launchservicesd"
                | "com.apple.pasteboard.1"
        ))
        || (operation == "iokit-open" && resource.contains("IOHID"))
}

fn parse_filetime(line: &str) -> std::io::Result<u64> {
    let timestamp = line
        .get(..23)
        .ok_or_else(|| std::io::Error::other("Seatbelt report timestamp is missing"))?;
    let local = NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S%.3f")
        .map_err(|error| std::io::Error::other(format!("invalid Seatbelt timestamp: {error}")))?;
    let local = Local
        .from_local_datetime(&local)
        .earliest()
        .ok_or_else(|| std::io::Error::other("invalid Seatbelt report local timestamp"))?;
    let nanos = local
        .with_timezone(&Utc)
        .timestamp_nanos_opt()
        .ok_or_else(|| {
            std::io::Error::other("Seatbelt report timestamp is outside the supported range")
        })?;
    let ticks = i128::from(nanos)
        .checked_div(100)
        .and_then(|value| value.checked_add(WINDOWS_EPOCH_OFFSET_TICKS))
        .and_then(|value| u64::try_from(value).ok())
        .ok_or_else(|| std::io::Error::other("Seatbelt report timestamp overflow"))?;
    Ok(ticks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_marked_file_report() {
        let line = "2026-10-01 15:24:27.814 E  kernel[0:1] (Sandbox) \
                    Sandbox: cat(42) deny(1) file-read-data /private/secret";
        let denial = parse_report(line, CaptureDenialsMode::Block)
            .unwrap()
            .unwrap();
        assert_eq!(denial.resource, "/private/secret");
        assert_eq!(denial.resource_type, ResourceType::File);
        assert_eq!(denial.access_type, AccessType::Read);
        assert_eq!(denial.pid, 42);
    }

    #[test]
    fn maps_arbitrary_mach_services_to_other_resources() {
        let line = "2026-10-01 15:24:27.814 E  kernel[0:1] (Sandbox) \
                    Sandbox: cat(42) deny(1) mach-lookup com.example.service";
        let denial = parse_report(line, CaptureDenialsMode::Block)
            .unwrap()
            .unwrap();

        assert_eq!(denial.resource, "com.example.service");
        assert_eq!(denial.resource_type, ResourceType::Other);
        assert_eq!(denial.access_type, AccessType::Unknown);
    }

    #[test]
    fn parses_network_report_as_actionable() {
        let line = "2026-10-01 15:24:27.814 E  kernel[0:1] (Sandbox) \
                    Sandbox: nc(42) deny(1) network-outbound remote:*:443";
        let denial = parse_report(line, CaptureDenialsMode::Block)
            .unwrap()
            .unwrap();

        assert_eq!(denial.resource, "remote:*:443");
        assert_eq!(denial.resource_type, ResourceType::Network);
        assert_eq!(denial.access_type, AccessType::Unknown);
    }

    #[test]
    fn preserves_network_bind_direction() {
        let line = "2026-10-01 15:24:27.814 E  kernel[0:1] (Sandbox) \
                    Sandbox: nc(42) deny(1) network-bind local:*:8080";
        let denial = parse_report(line, CaptureDenialsMode::Block)
            .unwrap()
            .unwrap();

        assert_eq!(denial.resource, "local:*:8080");
        assert_eq!(denial.resource_type, ResourceType::Network);
        assert_eq!(denial.access_type, AccessType::Unknown);
    }

    #[test]
    fn parses_known_ui_service_as_actionable() {
        let line = "2026-10-01 15:24:27.814 E  kernel[0:1] (Sandbox) \
                    Sandbox: osascript(42) deny(1) mach-lookup \
                    com.apple.coreservices.launchservicesd";
        let denial = parse_report(line, CaptureDenialsMode::Block)
            .unwrap()
            .unwrap();

        assert_eq!(denial.resource, "com.apple.coreservices.launchservicesd");
        assert_eq!(denial.resource_type, ResourceType::Ui);
        assert_eq!(denial.access_type, AccessType::Unknown);
    }

    #[test]
    fn maps_non_ui_mach_services_to_other_resources() {
        let line = "2026-10-01 15:24:27.814 E  kernel[0:1] (Sandbox) \
                    Sandbox: osascript(42) deny(1) mach-lookup com.apple.diagnosticd";

        let denial = parse_report(line, CaptureDenialsMode::Block)
            .unwrap()
            .unwrap();

        assert_eq!(denial.resource, "com.apple.diagnosticd");
        assert_eq!(denial.resource_type, ResourceType::Other);
        assert_eq!(denial.access_type, AccessType::Unknown);
    }

    #[test]
    fn marked_reports_are_deduplicated_and_preserve_verbose_outcomes() {
        let marker = "MXC-MARK";
        let file = "2026-10-01 15:24:27.814 E  kernel[0:1] (Sandbox) \
                    Sandbox: cat(42) deny(1) file-read-data /private/secret";
        let mach = "2026-10-01 15:24:27.815 E  kernel[0:1] (Sandbox) \
                    Sandbox: cat(42) deny(1) mach-lookup com.example.service";
        let lines = vec![
            file.to_string(),
            marker.to_string(),
            file.to_string(),
            marker.to_string(),
            mach.to_string(),
            marker.to_string(),
        ];

        let analysis = parse_reports(&lines, marker, CaptureDenialsMode::Block).unwrap();

        assert_eq!(analysis.denials.len(), 2);
        assert_eq!(analysis.verbose_logging.total_occurrences, 3);
        assert_eq!(analysis.verbose_logging.signatures.len(), 2);
        assert!(analysis.verbose_logging.signatures.iter().all(|aggregate| {
            aggregate.signature.reason == VerboseLoggingOutcomeReason::Actionable
        }));
    }

    #[test]
    fn stream_reader_discards_startup_and_unmarked_events() {
        let input = b"unrelated\nstartup event\nREADY\ncaptured event\nMARK\n";
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);

        let captured = read_capture_stream(&input[..], ready_tx, "MARK", "READY").unwrap();

        ready_rx.recv().unwrap().unwrap();
        assert_eq!(
            captured.lines,
            vec!["captured event".to_string(), "MARK".to_string()]
        );
        assert!(!captured.overflowed);
    }

    #[test]
    fn allow_mode_ignores_deny_dispositions() {
        let line = "2026-10-01 15:24:27.814 E  kernel[0:1] (Sandbox) \
                    Sandbox: cat(42) deny(1) file-read-data /private/secret";

        assert!(parse_report(line, CaptureDenialsMode::Allow)
            .unwrap()
            .is_none());
    }

    #[test]
    fn duplicate_report_summaries_preserve_occurrence_count() {
        let marker = "MXC-MARK";
        let line = "2026-10-01 15:24:27.814 E  kernel[0:1] (Sandbox) \
                    12 duplicate reports for Sandbox: cat(42) deny(1) \
                    mach-lookup com.example.service";
        let lines = vec![line.to_string(), marker.to_string()];

        let analysis = parse_reports(&lines, marker, CaptureDenialsMode::Block).unwrap();

        assert_eq!(analysis.verbose_logging.total_occurrences, 12);
        assert_eq!(analysis.verbose_logging.signatures[0].count, 12);
    }

    #[test]
    fn resource_text_cannot_inflate_duplicate_count() {
        let line = "2026-10-01 15:24:27.814 E  kernel[0:1] (Sandbox) \
                    Sandbox: cat(42) deny(1) file-read-data \
                    /private/99 duplicate reports";

        assert_eq!(report_occurrences(line), 1);
    }

    #[test]
    fn malformed_report_does_not_discard_valid_denials() {
        let marker = "MXC-MARK";
        let malformed = "2026-10-01 15:24:27.814 E kernel (Sandbox) \
                         Sandbox: cat(not-a-pid) deny(1) file-read-data /bad";
        let valid = "2026-10-01 15:24:27.815 E kernel (Sandbox) \
                     Sandbox: cat(42) deny(1) file-read-data /private/secret";
        let lines = vec![
            malformed.to_string(),
            marker.to_string(),
            valid.to_string(),
            marker.to_string(),
        ];

        let analysis = parse_reports(&lines, marker, CaptureDenialsMode::Block).unwrap();

        assert_eq!(analysis.denials.len(), 1);
        assert!(analysis.verbose_logging.signatures.iter().any(|aggregate| {
            aggregate.signature.reason == VerboseLoggingOutcomeReason::EventPayloadMalformed
        }));
    }
}
