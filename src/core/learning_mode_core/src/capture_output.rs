// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Cross-platform capture output and lifecycle helpers.

use std::path::{Path, PathBuf};

use crate::{
    verbose_logging_sibling_path, write_document, write_paired_output_files,
    write_verbose_logging_document, AnalysisResult, DenialSummary, DenialsDocument,
    DenialsOutputPointer, ExistingOutputPolicy, VerboseLoggingDocument,
};

/// Paired actionable-output and optional native-trace paths for one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureOutputPaths {
    /// Path of the actionable denials JSON document.
    pub denials: PathBuf,
    /// Path of the retained provider-native trace, when requested.
    pub trace: Option<PathBuf>,
}

/// Writes actionable and verbose denial documents and returns their pointer.
pub fn write_denials_output(
    analysis: AnalysisResult,
    exit_code: i32,
    output_path: &Path,
) -> std::io::Result<DenialsOutputPointer> {
    let verbose_path = verbose_logging_sibling_path(output_path)
        .map_err(|error| std::io::Error::other(format!("captureDenials {error}")))?;
    let summary = DenialSummary::new(
        exit_code,
        analysis.denials.len(),
        analysis.denied_resources_truncated,
    );
    let document = DenialsDocument::new(analysis.denials, summary);
    let verbose_document = VerboseLoggingDocument::new(&analysis.verbose_logging);
    write_paired_output_files(
        "captureDenials",
        output_path,
        &verbose_path,
        ExistingOutputPolicy::CreateNew,
        |writer| write_document(writer, &document),
        |writer| write_verbose_logging_document(writer, &verbose_document),
    )?;
    Ok(DenialsOutputPointer::new(
        output_path.to_string_lossy(),
        &document.summary,
    ))
}

/// Inserts a run identifier into a configured output path's file stem.
#[must_use]
pub fn insert_run_id_into_stem(path: &Path, run_id: &str) -> PathBuf {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.to_path_buf();
    };
    let new_name = match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) => {
            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or(file_name);
            format!("{stem}.{run_id}.{extension}")
        }
        None => format!("{file_name}.{run_id}"),
    };
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(new_name),
        _ => PathBuf::from(new_name),
    }
}

/// Resolves unique denials and optional retained-trace paths for one run.
pub fn unique_capture_output_paths(
    configured_path: Option<&str>,
    retain_trace: bool,
    trace_extension: &str,
) -> Result<CaptureOutputPaths, String> {
    let suffix = random_capture_suffix()?;
    let run_id = format!("{}_{suffix}", std::process::id());
    let denials = match configured_path {
        Some(path) => insert_run_id_into_stem(Path::new(path), &run_id),
        None => std::env::temp_dir().join(format!("mxc_denials_{run_id}.json")),
    };
    let trace = retain_trace.then(|| {
        let directory = configured_path
            .and_then(|path| Path::new(path).parent())
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(std::env::temp_dir);
        directory.join(format!("mxc_denials_{run_id}.{trace_extension}"))
    });
    Ok(CaptureOutputPaths { denials, trace })
}

/// Generates a random hexadecimal suffix for per-run identifiers.
pub fn random_capture_suffix() -> Result<String, String> {
    let mut nonce = [0u8; 16];
    getrandom::getrandom(&mut nonce).map_err(|error| {
        format!("captureDenials could not generate a unique output path: {error}")
    })?;
    Ok(nonce.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Removes a runner-managed capture file, treating an absent file as success.
pub fn remove_internal_capture_file(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(std::io::Error::other(format!(
            "captureDenials failed to remove internal capture file {}: {error}",
            path.display()
        ))),
    }
}

/// Combines process wait and capture teardown results without hiding either error.
pub fn combine_process_and_teardown_results(
    process_result: std::io::Result<i32>,
    teardown_result: std::io::Result<()>,
) -> std::io::Result<i32> {
    match (process_result, teardown_result) {
        (Ok(exit_code), Ok(())) => Ok(exit_code),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(wait_error), Err(teardown_error)) => Err(std::io::Error::new(
            wait_error.kind(),
            format!("{wait_error}; captureDenials teardown also failed: {teardown_error}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_paths_use_requested_trace_extension() {
        let paths = unique_capture_output_paths(None, true, "seatbelt.log").unwrap();
        assert_eq!(
            paths.trace.as_ref().and_then(|path| path.extension()),
            Some(std::ffi::OsStr::new("log"))
        );
        assert_ne!(paths.denials, paths.trace.unwrap());
    }

    #[test]
    fn combined_errors_preserve_wait_kind_and_both_messages() {
        let error = combine_process_and_teardown_results(
            Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "wait")),
            Err(std::io::Error::other("capture")),
        )
        .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(error.to_string().contains("wait"));
        assert!(error.to_string().contains("capture"));
    }
}
