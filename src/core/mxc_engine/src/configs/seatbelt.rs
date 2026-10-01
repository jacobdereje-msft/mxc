// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Seatbelt-specific configuration types.

use super::CaptureDenialsMode;

/// Seatbelt denial-capture settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
#[non_exhaustive]
pub struct SeatbeltCaptureDenials {
    /// How each ungranted access check is handled while it is recorded.
    pub mode: CaptureDenialsMode,
    /// Absolute path for the JSON denials document.
    pub output_path: Option<String>,
    /// Preserve the raw Seatbelt unified-log trace.
    pub retain_trace: bool,
}

/// macOS Seatbelt settings.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Seatbelt {
    /// Replace the generated sandbox profile entirely.
    pub profile_override: Option<String>,
    /// Allow GUI applications to reach WindowServer and related services.
    pub gui_access: bool,
    /// Allow the contained process to allocate nested pseudo-terminals.
    pub nested_pty: bool,
    /// Allow access to the macOS Keychain.
    pub keychain_access: bool,
    /// Additional Mach service global names the process may resolve.
    pub extra_mach_lookups: Vec<String>,
    /// Optional denial-capture settings.
    pub capture_denials: Option<SeatbeltCaptureDenials>,
}

impl Default for Seatbelt {
    fn default() -> Self {
        Self {
            profile_override: None,
            gui_access: false,
            nested_pty: true,
            keychain_access: false,
            extra_mach_lookups: Vec::new(),
            capture_denials: None,
        }
    }
}
