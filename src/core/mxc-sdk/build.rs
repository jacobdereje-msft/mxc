// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

fn main() {
    // Lifted mode activates through IsoSessionApp.dll + IsoSession.manifest
    // beside the executable; stage them from the pinned SDK package.
    #[cfg(all(windows, feature = "isolation_session_lifted"))]
    mxc_build_common::isolation_session_sdk::stage_runtime()
        .unwrap_or_else(|e| panic!("IsolationSession SDK staging failed: {e}"));
}
