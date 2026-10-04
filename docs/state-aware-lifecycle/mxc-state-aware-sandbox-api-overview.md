# MXC State-Aware Sandbox API - Overview

Companion to [mxc-state-aware-sandbox-api.md](./mxc-state-aware-sandbox-api.md).

MXC separates container creation from persistent lifecycle operations.
Creation runs a `ContainerRequest`; persistent execution provisions a
container, starts it, runs `ExecutionRequest` workloads, then stops and
deprovisions it. The public SDK surface is versioned independently of the
native wire contract.

## Public SDK surface

All public types, operations, probes, backend/platform discovery, telemetry,
and helpers live under `mxc_sdk::v1`, `Microsoft.Mxc.Sdk.V1`, or
`@microsoft/mxc-sdk/v1`. The Node package root exports no APIs.

| Capability | Rust | .NET | Node |
|---|---|---|---|
| Creation input | `ContainerRequest` | `ContainerRequest` | `ContainerRequest` |
| Captured creation | `run` | `MxcContainer.Run` / `RunAsync` | `run` / `runAsync` |
| Live standard pipes | `spawn` | `MxcContainer.Spawn` / `SpawnAsync` | `spawn` / `spawnAsync` |
| Provision | `container::provision_container` | `MxcLifecycle.ProvisionContainer` | `provisionContainer` |
| Start | `container::start_container` | `MxcLifecycle.StartContainer` | `startContainer` |
| Existing-container capture | `run_in_container` | `RunInContainer` / `RunInContainerAsync` | `runInContainerAsync` |
| Existing-container streaming | `spawn_in_container` | `SpawnInContainer` / `SpawnInContainerAsync` | `spawnInContainer` / `spawnInContainerAsync` |
| Stop | `container::stop_container` | `MxcLifecycle.StopContainer` | `stopContainer` |
| Deprovision | `container::deprovision_container` | `MxcLifecycle.DeprovisionContainer` | `deprovisionContainer` |

- Creation takes a request followed by its operation-specific options.
- Provision takes `ProvisionRequest` followed by `ProvisionOptions`.
- Start, stop, and deprovision take `ContainerId` followed by their own options.
- Existing-container execution takes `ContainerId`, `ExecutionRequest`, and
  its execution-specific options. Initial PTY size is a field of those options.
- .NET asynchronous cancellation tokens are last. Rust remains synchronous.

Common process settings are command, working directory, environment,
inherit-default-environment, and timeout. SDKs use properties or setters
according to their language conventions. Containment choices are closed:
Rust enum variants, SDK-owned .NET subclasses, and Node discriminated unions.
Creation defaults to generic `Process` intent.

Explicit PTY APIs return SDK-owned terminal process handles with interactive
input, resize, wait, termination, and disposal. Native attached execution is
preserved but is not publicly exposed by the Rust/.NET SDKs and is not
provided by Node.

Explicit validation APIs perform native dry-run validation and return no
execution result. Backend policy and feature support remain native-engine
responsibilities. Invocation telemetry cannot grant persisted consent or
override restrictive administrative policy.

## Lifecycle

| Phase | Valid from state | Resulting state | Output |
|---|---|---|---|
| `provision` | Not provisioned | Provisioned | Opaque identity and optional metadata |
| `start` | Provisioned | Running | Optional metadata |
| `exec` | Running | Running | Live streams or captured execution output |
| `stop` | Running | Provisioned | Optional metadata |
| `deprovision` | Provisioned | Not provisioned | Optional metadata |

SDK identities are opaque `ContainerId` values. The native persistent wire
envelope retains `sandboxId`; it differs from creation's caller-selected
`containerId` label. Provision carries containment, while later wire phases
route using the persistent identity. Public SDK names do not change wire
fields or native ABI names.

## Native architecture and contracts

`mxc_engine` owns backend routing. Exact registered wire roots are selected
by version, phase, and provision containment, then adapted through private
`CommonRequestIR` normalization and checked backend binding. High-level SDKs
own their exact V1 contract; raw exact JSON is a separately named native
compatibility lane.

Persistent backends implement `StatefulSandboxBackend`. Common typed
operations and results retain lifecycle semantics; backend-specific policy,
idempotence, concurrency, cleanup, and error mapping are documented in the
backend guides. SDKs surface structured errors and warnings rather than
converting failures into successful-looking output.

## References

- [Full lifecycle and wire contract](./mxc-state-aware-sandbox-api.md)
- [Rust SDK](../../src/core/mxc-sdk/README.md)
- [.NET SDK](../../sdk/dotnet/README.md)
- [Node SDK](../../sdk/node/README.md)
- [IsolationSession TypeScript guide](../isolation-session/state-aware-typescript.md)
- [WSLC lifecycle guide](../wsl/wslc-state-aware.md)
