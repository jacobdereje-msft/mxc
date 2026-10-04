# .NET V1 operation signatures

Public entrypoint: `Microsoft.Mxc.Sdk.V1`. [Types](types.md) | [Overview](README.md)

Signatures describe the supported consumer API and omit implementation bodies and serialization attributes. Raw Rust JSON bridges are listed separately from typed requests; native wire compatibility is unchanged.

## `Microsoft.Mxc.Sdk.V1.ContainerId` — operator_declaration

Equality operator.

```csharp
public static bool operator ==(ContainerId left, ContainerId right);
```


## `Microsoft.Mxc.Sdk.V1.ContainerId` — operator_declaration

Inequality operator.

```csharp
public static bool operator !=(ContainerId left, ContainerId right);
```


## `Microsoft.Mxc.Sdk.V1.FilesystemPolicies` — GetAvailableToolsPolicy

Discover existing tool and SDK directories from PATH and well-known environment variables.

```csharp
public static FilesystemPolicyResult GetAvailableToolsPolicy(
    IReadOnlyDictionary<string, string?>? environment = null,
    ToolsPolicyOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.FilesystemPolicies` — GetUserProfilePolicy

Discover standard per-user application directories that should be granted read-only access.

```csharp
public static FilesystemPolicyResult GetUserProfilePolicy(
    IReadOnlyDictionary<string, string?>? environment = null);
```


## `Microsoft.Mxc.Sdk.V1.FilesystemPolicies` — GetTemporaryFilesPolicy

Discover the host temporary directory as a read-write policy fragment.

```csharp
public static FilesystemPolicyResult GetTemporaryFilesPolicy(
    IReadOnlyDictionary<string, string?>? environment = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcContainer` — Probe

Probe which Windows ProcessContainer tier can serve a request.

```csharp
public static ProbeOutput Probe(ContainerRequest? request = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcContainer` — Run

Run a container request to completion.

```csharp
public static ExecutionResult Run(ContainerRequest request, RunOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcContainer` — RunAsync

Run a complete request asynchronously and capture its output.

```csharp
public static Task<ExecutionResult> RunAsync(
    ContainerRequest request,
    RunOptions? options = null,
    CancellationToken cancellationToken = default);
```


## `Microsoft.Mxc.Sdk.V1.MxcContainer` — Spawn

Spawn a container request and return its live process handle.

```csharp
public static MxcProcess Spawn(ContainerRequest request, SpawnOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcContainer` — SpawnAsync

Spawn a complete request asynchronously with live standard streams.

```csharp
public static Task<MxcProcess> SpawnAsync(
    ContainerRequest request,
    SpawnOptions? options = null,
    CancellationToken cancellationToken = default);
```


## `Microsoft.Mxc.Sdk.V1.MxcContainer` — SpawnWithPty

Spawn a complete request attached to an MXC-owned PTY.

```csharp
public static MxcPtyProcess SpawnWithPty(
    ContainerRequest request,
    SpawnWithPtyOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcLifecycle` — ProvisionContainer

Provision a new container.

```csharp
public static ProvisionResult ProvisionContainer(
    ProvisionRequest request,
    ProvisionOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcLifecycle` — ValidateProvision

Parse and validate a provision request without allocating a container.

```csharp
public static ValidationResult ValidateProvision(
    ProvisionRequest request,
    ProvisionOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcLifecycle` — StartContainer

Start a provisioned container.

```csharp
public static LifecycleResult StartContainer(ContainerId id, StartOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcLifecycle` — ValidateStart

Validate a start request without starting the container.

```csharp
public static ValidationResult ValidateStart(ContainerId id, StartOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcLifecycle` — SpawnInContainer

Run a command in a started container and return live stdio streams.

```csharp
public static MxcProcess SpawnInContainer(
    ContainerId id,
    ExecutionRequest request,
    SpawnInContainerOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcLifecycle` — SpawnInContainerAsync

Spawn a command asynchronously and return live stdio streams.

```csharp
public static Task<MxcProcess> SpawnInContainerAsync(
    ContainerId id,
    ExecutionRequest request,
    SpawnInContainerOptions? options = null,
    CancellationToken cancellationToken = default);
```


## `Microsoft.Mxc.Sdk.V1.MxcLifecycle` — SpawnInContainerWithPty

Run a command in a started container and attach it to a caller-resized PTY.

```csharp
public static MxcPtyProcess SpawnInContainerWithPty(
    ContainerId id,
    ExecutionRequest request,
    SpawnInContainerWithPtyOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcLifecycle` — ValidateProcess

Validate an execution request without starting a process.

```csharp
public static ValidationResult ValidateProcess(
    ContainerId id,
    ExecutionRequest request,
    SpawnInContainerOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcLifecycle` — RunInContainerAsync

Run an execution request to completion and capture its output.

```csharp
public static Task<ExecutionResult> RunInContainerAsync(
    ContainerId id,
    ExecutionRequest request,
    RunInContainerOptions? options = null,
    CancellationToken cancellationToken = default);
```


## `Microsoft.Mxc.Sdk.V1.MxcLifecycle` — RunInContainer

Run an ExecutionRequest synchronously and capture its output.

```csharp
public static ExecutionResult RunInContainer(
    ContainerId id,
    ExecutionRequest request,
    RunInContainerOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcLifecycle` — StopContainer

Stop a running container.

```csharp
public static LifecycleResult StopContainer(ContainerId id, StopOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcLifecycle` — ValidateStop

Validate a stop request without stopping the container.

```csharp
public static ValidationResult ValidateStop(ContainerId id, StopOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcLifecycle` — DeprovisionContainer

Destroy a container and release its resources.

```csharp
public static LifecycleResult DeprovisionContainer(
    ContainerId id,
    DeprovisionOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcLifecycle` — ValidateDeprovision

Validate a deprovision request without destroying the container.

```csharp
public static ValidationResult ValidateDeprovision(
    ContainerId id,
    DeprovisionOptions? options = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcPlatform` — GetAvailableBackends

Probe every containment backend the current host can run.

```csharp
public static IReadOnlyList<AvailableBackend> GetAvailableBackends();
```


## `Microsoft.Mxc.Sdk.V1.MxcPlatform` — GetPlatformSupport

Probe whether the public SDK can launch containers on this host and which backends it can launch.

```csharp
public static PlatformSupport GetPlatformSupport();
```


## `Microsoft.Mxc.Sdk.V1.MxcPtySize` — Default

The default 24-row by 80-column terminal.

```csharp
public static MxcPtySize Default { get; } = new(24, 80);
```


## `Microsoft.Mxc.Sdk.V1.MxcTelemetry` — GetConsent

Read effective telemetry consent.

```csharp
public static TelemetryConsentState GetConsent();
```


## `Microsoft.Mxc.Sdk.V1.MxcTelemetry` — NeedsConsentPrompt

Whether the host should show the first-run consent prompt.

```csharp
public static bool NeedsConsentPrompt();
```


## `Microsoft.Mxc.Sdk.V1.MxcTelemetry` — GetPolicy

Read the administrative telemetry policy.

```csharp
public static TelemetryPolicyState GetPolicy();
```


## `Microsoft.Mxc.Sdk.V1.MxcTelemetry` — RequestConsent

Request consent through a synchronous host presenter.

```csharp
public static TelemetryConsentOutcome RequestConsent(
    Func<TelemetryConsentPrompt, TelemetryConsentDecision> presenter,
    string? locale = null);
```


## `Microsoft.Mxc.Sdk.V1.MxcTelemetry` — RequestConsentAsync

Request consent through an asynchronous host presenter.

```csharp
public static Task<TelemetryConsentOutcome> RequestConsentAsync(
    Func<TelemetryConsentPrompt, ValueTask<TelemetryConsentDecision>> presenter,
    string? locale = null,
    CancellationToken cancellationToken = default);
```


## `Microsoft.Mxc.Sdk.V1.MxcTelemetry` — GetConsentStatus

Read stored/effective consent and the administrative ceiling.

```csharp
public static TelemetryConsentStatus GetConsentStatus();
```


## `Microsoft.Mxc.Sdk.V1.MxcTelemetry` — WithdrawConsent

Idempotently withdraw telemetry consent.

```csharp
public static TelemetryConsentOutcome WithdrawConsent();
```
