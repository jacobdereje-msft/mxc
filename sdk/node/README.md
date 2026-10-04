# `@microsoft/mxc-sdk`

Node.js / TypeScript SDK for MXC (Microsoft eXecution Containers). The
versioned public request, execution, and lifecycle APIs are exported from
`@microsoft/mxc-sdk/v1`, including platform discovery, errors, and telemetry consent.
The package root exports no public APIs. These APIs use the SDK-owned V1 contract; callers
do not select the wire version.

```bash
npm install @microsoft/mxc-sdk
```

Node.js 24 or later is required. On Windows, native stdio transfer requires
Node.js 24.21.0 or later within the Node.js 24 release line, or Node.js 26.8.0
or later.

## One-shot execution

```typescript
import { getPlatformSupport } from '@microsoft/mxc-sdk/v1';
import { runAsync, spawn } from '@microsoft/mxc-sdk/v1';
import type { ContainerRequest } from '@microsoft/mxc-sdk/v1';

if (!getPlatformSupport().isSupported) {
  throw new Error('MXC is not available on this host');
}

const request: ContainerRequest = {
  filesystem: { readonlyPaths: [process.cwd()] },
  network: { egress: { default: 'deny' } },
  timeoutMs: 30_000,
  command: 'node -e "console.log(\\'hello from sandbox\\')"',
};

const output = await runAsync(request);
console.log(output.stdout, output.exitCode);

const processHandle = spawn(request);
processHandle.standardOutput?.on('data', (chunk) => process.stdout.write(chunk));
const outcome = await processHandle.waitAsync();
processHandle.dispose();
```

`run` / `runAsync` capture stdout and stderr in an `ExecutionResult`. `spawn` /
`spawnAsync` return an `MxcProcess` with standard pipes, wait, termination, and
disposal operations. Access output streams before awaiting completion; any
untaken streams are drained internally to avoid pipe-buffer deadlocks.
Each operation accepts its own optional options type: `RunOptions`,
`SpawnOptions`, or `SpawnWithPtyOptions`. `experimental` authorizes native
experimental features; it does not change the SDK-owned wire contract.
Execution options do not support `dryRun`.

`ContainerRequest` holds the command, cross-backend filesystem, network, and UI
settings, and the selected backend's typed configuration. The SDK selects its
exact V1 contract; callers do not provide a schema version or raw executor
configuration. One-shot proxy settings are authored at
`network.runtimeConfig.networkProxy`; the SDK maps them to the existing
top-level wire `runtimeConfig`.

When UI settings are supplied, `ui.disable` explicitly controls whether UI is
disabled; clipboard and input-injection permissions remain separate.

`spawnWithPty(request, options?)` starts a one-shot request with a caller-driven
terminal and returns a `Promise<MxcPtyProcess>`. Set `options.size` for initial
dimensions; it defaults to 24 rows by 80 columns. PTY support is currently
available for IsolationSession and supported Windows ProcessContainer requests.

## Existing containers

`ProvisionResult<C>.metadata` uses `ProvisionMetadata<C>` to select the
backend's metadata type. IsolationSession returns
`IsolationSessionProvisionMetadata`; WSLC returns no provision metadata.

The V1 lifecycle API provisions and controls supported persistent backends.
`provisionContainer` returns a branded `ContainerId`; use it for later phases
without inspecting its runtime string.

```typescript
import {
  deprovisionContainer,
  runInContainerAsync,
  provisionContainer,
  startContainer,
  stopContainer,
} from '@microsoft/mxc-sdk/v1';

const { containerId } = await provisionContainer({
  containment: 'wslc',
  image: 'alpine:latest',
});
await startContainer(containerId);
const result = await runInContainerAsync(containerId, {
  command: 'echo hello',
});
console.log(result.stdout, result.exitCode);
await stopContainer(containerId);
await deprovisionContainer(containerId);
```

`spawnInContainer` / `spawnInContainerAsync` return a live pipe-backed
`MxcProcess`; `runInContainer` / `runInContainerAsync` return captured
`ExecutionResult` values. The synchronous form blocks Node's event loop while
the native SDK drains both output streams and waits for completion.
`spawnInContainerWithPty(containerId, request, options?)` starts an
IsolationSession exec with a caller-driven terminal and returns a
`Promise<MxcPtyProcess>`. Set `options.size` for initial dimensions; it defaults
to 24 rows by 80 columns.
IsolationSession provision requires an explicit unrestricted directional
network posture; WSLC network posture is fixed at provision. See the
[IsolationSession](../../docs/isolation-session/state-aware-typescript.md) and
[WSLC](../../docs/wsl/wslc-state-aware.md) guides for backend and phase
requirements.

Provisioning takes a discriminated `ProvisionRequest` and optional
`ProvisionOptions`. Start, stop, and deprovision take the identity followed by
their own `StartOptions`, `StopOptions`, or `DeprovisionOptions`.
Existing-container execution takes the identity, a flat `ExecutionRequest`, and
`SpawnInContainerOptions` or `RunInContainerOptions`; PTY execution carries
initial dimensions on `SpawnInContainerWithPtyOptions`. Process settings use
`command`, `workingDirectory`, `environment`, `inheritDefaultEnvironment`, and
`timeoutMs`, just as creation does.

Lifecycle and existing-container options can override request telemetry.
`validateProvision`, `validateStart`, `validateStop`, `validateDeprovision`,
and `validateProcess` perform native dry-run validation without creating a
container or returning an execution result. They return `ValidationResult`
with a `warnings` array and use the corresponding
operation options. Captured existing-container execution is asynchronous;
there is no synchronous `runInContainer` API.
Existing-container execution accepts runtime-only network settings at
`network.runtimeConfig`; it cannot change the container's provision-time
network policy.
The runtime values are typed as `NetworkRuntimeConfig`.

The creation containment types are compile-time-only choices under the
`Containment` namespace, such as `Containment.Process` and
`Containment.ProcessContainer`. `Containment` is also their closed union;
the SDK does not create runtime containment objects or factories.

`ContainerRequest` uses the named `FilesystemPolicy`, `NetworkPolicy`, and
`UiPolicy` types for cross-backend restrictions. Backend-specific settings
remain on the selected containment configuration.

## Public V1 types

| Purpose | TypeScript type |
| --- | --- |
| One-shot request and cross-backend restrictions | `ContainerRequest` |
| Persistent container identity | `ContainerId` |
| Persistent container provision input | `ProvisionRequest` |
| Existing-container workload | `ExecutionRequest` |
| Live process with standard pipes | `MxcProcess` |
| Live process with a terminal | `MxcPtyProcess` |
| Terminal dimensions | `MxcPtySize` |
| Captured execution | `ExecutionResult` |
| Terminal process outcome | `WaitResult` |
| Validation warnings | `ValidationResult` |
| Provisioned identity, optional metadata, and warnings | `ProvisionResult<C>` |
| Start, stop, and deprovision warnings | `LifecycleResult` |
| Structured execution outputs | `ExecutionMetadata` |
| Denial-capture output and failure | `CaptureDenialsResult`, `CaptureDenialsError` |
| Runtime network values | `NetworkRuntimeConfig` |

Network policy details are in the
[networking guide](../../docs/sandbox-policy/0.8.0/networking/networking.md);
host-specific behavior and supported capabilities are documented in the
backend guides under [`docs/`](../../docs/).

## Errors, warnings, and telemetry

Native errors are surfaced as `MxcError` with a typed error code and optional
operation, native status, and remediation. Security and operational warnings
are returned in `ExecutionResult.warnings` and `MxcProcess.warnings`.
Validation warnings are returned in `ValidationResult.warnings`.
Provision warnings are returned in `ProvisionResult.warnings`; start, stop,
and deprovision return `LifecycleResult.warnings`. Omitted native warnings
become an empty array; malformed warnings fail explicitly. When provision
metadata is present for IsolationSession, all three fields are required:
`agentUserName`, `agentUserSid`, and `ephemeralWorkspacePath`.
`ExecutionResult.outputMetadata`, `MxcProcess.outputMetadata`, and the inherited
PTY property expose optional `ExecutionMetadata`. Live-process metadata is
available after terminal settling; denial-capture fields are populated only
when the backend produces them.

Creation telemetry is supplied through `telemetry: { enabled: true }` on
`RunOptions`, `SpawnOptions`, or `SpawnWithPtyOptions`, not on `ContainerRequest`.
Omission leaves telemetry disabled; `enabled: false` explicitly disables it.
Opt-in remains subject to MXC's persisted user consent and administrative policy. Telemetry consent
APIs and `getPlatformSupport` are exported from `@microsoft/mxc-sdk/v1`.

Filesystem discovery helpers take an optional `environment` map; omission uses
`process.env`, and `{}` stays empty. `getAvailableToolsPolicy` also accepts
`ToolsPolicyOptions`; set `containerType: 'processcontainer'` to exclude
directories with ALL APPLICATION PACKAGES access on Windows. ACL inspection
is bounded to five seconds per directory; failures retain the directory and
emit a diagnostic warning. `getUserProfilePolicy` uses the supplied environment,
and `getTemporaryFilesPolicy` returns existing temporary storage without creating
directories.
