# Windows.AI.IsolationSession SDK selection

MXC supports two explicit IsolationSession build modes:

| Cargo feature | Metadata and runtime |
|---|---|
| `isolation_session` | Uses the committed OS-generated Rust bindings and normal inbox WinRT activation. |
| `isolation_session_lifted` | Restores the pinned SDK package from NuGet.org, generates bindings from its Preview WinMD, and stages its lifted activation payload. This feature implies `isolation_session`. |

The exact package version pinned in `src/core/mxc_build_common/src/lib.rs` is
checked in beside this README. Lifted builds resolve it in this order, verifying
the pinned SHA-256 every time:

1. `ISOLATION_SESSION_SDK_PACKAGE`, if set, for validating a different local
   copy of the same package.
2. The checked-in `.nupkg` in this directory.
3. The standard global NuGet package cache, then the NuGet.org V3
   flat-container endpoint (cached after download).

## Runtime prerequisite

Lifted binaries bind the version-pinned runtime installed by the
IsolationSession MSI. Install the matching release with:

```powershell
winget install Microsoft.AI.IsolationSession
```

The build stages `IsoSessionApp.dll` and `IsoSession.manifest` beside
`wxc-exec.exe` and `mxc_ffi.dll`. MXC never falls back to the inbox runtime
when the payload or MSI is missing; it reports `BackendUnavailable` with this
remediation.

## Build commands

From `src`:

```powershell
# Existing inbox OS behavior
cargo build --release -p wxc --features isolation_session

# Lifted SDK NuGet and MSI behavior
cargo build --release -p wxc --features isolation_session_lifted
```

The same features exist on `mxc-sdk` and `mxc_ffi`. From the repository root,
`build.bat --with-isolation-session` builds inbox mode and
`build.bat --with-isolation-session-lifted` builds lifted mode and copies the
payload into the Node and .NET SDK runtimes. The .NET SDK uses
`-p:MxcWithIsolationSession=true` (inbox) or `-p:MxcIsolationSessionLifted=true`
(lifted).

Lifted mode fails the build if package download, integrity validation, metadata
generation, or activation-payload staging fails. It never silently produces an
inbox-mode binary.

## Updating the lifted SDK

1. Add the new `Microsoft.Windows.AI.IsolationSession.SDK` `.nupkg` to this
   directory (`git add -f`, since `*.nupkg` is ignored) and remove the old one.
2. Update `PACKAGE_VERSION` and `PACKAGE_SHA256` in
   `src/core/mxc_build_common/src/lib.rs`.
3. Update `GENERATION_INFO.toml` with the package and WinMD provenance.
4. Build and test both feature configurations.

`windows-bindgen` is pinned in the bindings crate and must remain compatible
with the workspace `windows` crate.
