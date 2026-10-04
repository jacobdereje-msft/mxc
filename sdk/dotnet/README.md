# Microsoft.Mxc.Sdk

`Microsoft.Mxc.Sdk` provides .NET APIs for authoring and executing MXC
container requests through the in-process native `mxc_ffi` library. The
versioned public API is in `Microsoft.Mxc.Sdk.V1`. All request and policy types
in this API use the owned V1 contract rather than exposing a wire-version
selector.

## One-shot execution

```csharp
using Microsoft.Mxc.Sdk.V1;

var request = new ContainerRequest("cmd /c echo hello")
{
    Filesystem = new FilesystemPolicy
    {
        ReadwritePaths = { @"C:\work" },
    },
    TimeoutMs = 30_000,
};

ExecutionResult output = await MxcContainer.RunAsync(request);
Console.WriteLine($"exit={output.ExitCode} stdout={output.Stdout}");
```

Use `MxcContainer.Run` / `RunAsync` for captured output or `Spawn` / `SpawnAsync` for a live
`MxcProcess` with separate stdin, stdout, and stderr streams. `MxcProcess`
provides wait, termination, and disposal operations. Shared filesystem,
network, and UI restrictions are authored directly on `ContainerRequest`,
alongside the selected backend configuration. The SDK owns the exact wire
contract; requests do not accept a caller-selected schema version.

`UiPolicy.Disable` defaults to `true`; clipboard and input-injection
permissions are authored separately.

`ContainerRequest.Containment` is a closed `Containment` choice. Select an
SDK-owned nested choice such as `Containment.Process` (the default),
`Containment.ProcessContainer`, or `Containment.Wslc`. Backend semantics are
validated by the native engine.

Creation methods accept their own `RunOptions`, `SpawnOptions`, or
`SpawnWithPtyOptions` after the request. Set `SpawnWithPtyOptions.Size` to
choose initial dimensions; it defaults to 24 rows by 80 columns. Async cancellation
tokens come last. `Experimental` authorizes native experimental features
without changing the SDK-owned wire contract.

`MxcContainer.SpawnWithPty(request, options?)` starts a one-shot request with a
caller-controlled terminal and returns an `MxcPtyProcess`. PTY support is
available for IsolationSession and supported Windows ProcessContainer requests.

## Existing containers

`ProvisionResult.Metadata` is an optional `ProvisionMetadata` value. For
IsolationSession, pattern-match `IsolationSessionProvisionMetadata` to read the
agent account and workspace details. WSLC returns no provision metadata. Native
metadata is mapped to this closed typed surface; callers do not parse raw JSON.

`MxcLifecycle` provides typed provision, start, exec, stop, and deprovision
operations. The provision result contains an opaque `ContainerId`; pass it to
subsequent operations rather than parsing it.

```csharp
using Microsoft.Mxc.Sdk.V1;

var provisioned = MxcLifecycle.ProvisionContainer(
    new WslcProvisionRequest { Image = "alpine:latest" });
ContainerId id = provisioned.ContainerId;

MxcLifecycle.StartContainer(id);
ExecutionResult output = await MxcLifecycle.RunInContainerAsync(
    id,
    new ExecutionRequest("echo hello"));
Console.WriteLine(output.Stdout);
MxcLifecycle.StopContainer(id);
MxcLifecycle.DeprovisionContainer(id);
```

Use `SpawnInContainer` or `SpawnInContainerAsync` for live piped execution. The
asynchronous methods are convenience wrappers over native operations and
support cancellation. Backend and phase-specific policy requirements are
described in the
[IsolationSession](../../docs/isolation-session/state-aware-rust.md) and
[WSLC](../../docs/wsl/wslc-state-aware.md) guides.
`MxcLifecycle.SpawnInContainerWithPty(id, request, options?)` starts an
IsolationSession exec with a caller-controlled terminal and returns an
`MxcPtyProcess`. Set `SpawnInContainerWithPtyOptions.Size` to choose initial
dimensions; it defaults to 24 rows by 80 columns.

`ExecutionRequest` supplies the command, working directory, environment, timeout,
telemetry, and runtime-only network settings to both `RunInContainer` and
`SpawnInContainer`. Existing-container network settings cannot change provision
policy:

```csharp
var request = new ExecutionRequest("echo hello")
{
    Network = new ProcessNetworkPolicy
    {
        RuntimeConfig = new NetworkRuntimeConfig
        {
            NetworkProxy = "http://proxy.example:8080",
        },
    },
};
```

The SDK maps `Network.RuntimeConfig` to the unchanged top-level wire
`runtimeConfig` field.
Provisioning uses the same `FilesystemPolicy` and `NetworkPolicy` authoring
types as container creation; the selected backend determines what it enforces.

Provisioning takes an SDK-owned `ProvisionRequest` followed by optional
`ProvisionOptions`. Start, stop, and deprovision use `StartOptions`,
`StopOptions`, and `DeprovisionOptions`. Existing-container execution takes
the identity, `ExecutionRequest`, and its own `SpawnInContainerOptions`,
`RunInContainerOptions`, or `SpawnInContainerWithPtyOptions`; PTY initial
dimensions are on that options object. Invocation
telemetry overrides request telemetry when supplied.

`ValidateProvision`, `ValidateStart`, `ValidateStop`, `ValidateDeprovision`,
and `ValidateProcess` perform native dry-run validation and return
`ValidationResult` with policy and operational `Warnings`, not an execution
result. They accept the corresponding operation options.
Backend/platform discovery, errors, telemetry, and helpers are also in
`Microsoft.Mxc.Sdk.V1`; no public SDK types remain outside it.

## Public V1 types

| Purpose | .NET type |
| --- | --- |
| One-shot workload and cross-backend restrictions | `ContainerRequest` |
| Persistent container identity | `ContainerId` |
| Persistent container provision input | `ProvisionRequest` |
| Existing-container workload | `ExecutionRequest` |
| Live process with standard pipes | `MxcProcess` |
| Live process with a terminal | `MxcPtyProcess` |
| Terminal dimensions | `MxcPtySize` |
| Captured execution | `ExecutionResult` |
| Terminal process outcome | `WaitResult` |

All types above are in `Microsoft.Mxc.Sdk.V1`. See the
[networking guide](../../docs/sandbox-policy/0.8.0/networking/networking.md)
and [schema reference](../../docs/schema.md) for policy behavior.

## Errors, warnings, and discovery

Native failures are surfaced as `MxcException`; inspect `Code`, `Operation`,
`NativeCode`, and `Remediation` when available. Security and operational
warnings are available on `ExecutionResult.Warnings` and `MxcProcess.Warnings`.
Provision returns identity, optional typed metadata, and `ProvisionResult.Warnings`.
Start, stop, and deprovision return `LifecycleResult.Warnings`. Omitted native
warnings become an empty array; malformed warnings fail explicitly. When
IsolationSession provision metadata is present, `AgentUserName`, `AgentUserSid`,
and `EphemeralWorkspacePath` are required non-null strings.

`MxcPlatform.GetPlatformSupport()` reports whether the SDK can launch a
sandbox on the current host. `MxcPlatform.GetAvailableBackends()` reports
host backend capabilities; availability is advisory and launch-time
validation still applies.

Creation telemetry is supplied through `Telemetry` on `RunOptions`,
`SpawnOptions`, or `SpawnWithPtyOptions`, not on `ContainerRequest`. Omission
leaves telemetry disabled; `new TelemetryConfig { Enabled = false }`
explicitly disables it. Opt-in is still gated by MXC's persisted user consent
and administrative policy.

Filesystem discovery helpers take an optional `environment` dictionary;
`null` snapshots the process environment and an empty dictionary remains
empty. `GetAvailableToolsPolicy` also takes `ToolsPolicyOptions`; set
`ContainerType = ToolsPolicyContainerType.ProcessContainer` to exclude
directories with ALL APPLICATION PACKAGES access on Windows. ACL inspection
is bounded to five seconds per directory; failures retain the directory and
emit a diagnostic warning. `GetUserProfilePolicy` uses the supplied environment,
and `GetTemporaryFilesPolicy` returns existing temporary storage without creating
directories.

## Package and native library

The package includes the native runtime assets for supported platforms.
Applications do not need to launch an MXC executor process. The governed build
pipeline produces the publishable NuGet package; `build.bat` creates local
architecture-specific packages under `output\packages`.

For examples, AOT requirements, and development commands, see the
[SDK project](Microsoft.Mxc.Sdk/README.md) and the repository's backend guides.
