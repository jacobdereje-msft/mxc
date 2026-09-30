# NVX State-Aware Daemon Operations

This document describes the minimum daemon operations needed for an NVX
backend to implement the MXC state-aware lifecycle.

The lifecycle follows the five phases defined by the MXC state-aware contract:
provision, start, exec, stop, and deprovision. NVX does not yet have a
registered state-aware exact contract, containment value, or sandbox ID prefix.
Values enclosed in angle brackets below are therefore design placeholders, not
currently valid MXC wire values.

## Minimum operations

| Operation | Minimum input | Minimum output |
| --- | --- | --- |
| `Provision` | Exact contract `version`, `phase: "provision"`, a registered `containment`, and every policy field required by that containment's exact provision root | Opaque `sandboxId` and optional backend-defined metadata |
| `Start` | Exact contract `version`, `phase: "start"`, and `sandboxId` | Successful transition from provisioned to running and optional backend-defined metadata |
| `Exec` | Exact contract `version`, `phase: "exec"`, `sandboxId`, and `process.commandLine` | Live stdin, stdout, and stderr plus a terminal exit, timeout, cancellation, or failure outcome |
| `Stop` | Exact contract `version`, `phase: "stop"`, and `sandboxId` | Successful transition from running to provisioned and optional backend-defined metadata |
| `Deprovision` | Exact contract `version`, `phase: "deprovision"`, and `sandboxId` | Confirmation that the provisioned resource was released and optional backend-defined metadata |

Successful lifecycle calls change backend state; they do not need to return a
second state field. `WaitExec`, `CancelExec`, `GetStatus`, `GetCapabilities`,
and `ShutdownDaemon` are not additional MXC lifecycle phases. Cancellation is
performed through the live execution handle. Any daemon protocol negotiation or
service administration belongs outside this lifecycle table.

## Example requests and results

The request examples below use the SDK/FFI JSON wire envelope. Direct
`wxc-exec` lifecycle calls instead pass the operation and sandbox ID through
`--operation` and `--sandbox-id`; those routing fields are rejected if they are
also supplied in the CLI JSON body.

### Provision

The following example records the intended complete provision shape: host
filesystem mappings, an isolated network posture, and the guest image layers
needed to create the VM. It is a proposed future exact-contract request, not
JSON accepted by the currently published state-aware registry. The final NVX
contract must register the version, `microvm` state-aware containment, ID
prefix, permanent `microvm.provision` section, and policy honor matrix together.

Example proposed request:

```json
{
  "$schema": "<nvx-schema-url>",
  "version": "<nvx-contract-version>",
  "phase": "provision",
  "containment": "microvm",
  "filesystem": {
    "readonlyPaths": [
      "C:\\workspace\\source"
    ],
    "readwritePaths": [
      "C:\\workspace\\output"
    ],
    "deniedPaths": [
      "C:\\workspace\\source\\secrets"
    ]
  },
  "network": {
    "egress": {
      "default": "deny"
    },
    "ingress": {
      "default": "deny",
      "hostLoopback": "deny"
    }
  },
  "microvm": {
    "provision": {
      "layers": [
        {
          "role": "distro",
          "path": "C:\\nvx\\images\\distro.erofs",
          "uuid": "11111111-1111-1111-1111-111111111111"
        },
        {
          "role": "runtime",
          "path": "C:\\nvx\\images\\runtime.erofs",
          "uuid": "22222222-2222-2222-2222-222222222222"
        }
      ],
      "scratchPath": "C:\\nvx\\images\\scratch.ext4"
    }
  }
}
```

The `distro` layer contains the read-only operating system and userspace. The
optional `runtime` layer contains workload runtime files. `scratchPath` names
the writable ext4 image that preserves changes for the lifetime of the
provisioned sandbox. All host paths and network policy must be validated before
VM resources are created; unsupported rules must be rejected rather than
ignored.

Example successful response:

```json
{
  "result": {
    "sandboxId": "<nvx-prefix>:abc123"
  }
}
```

The result may also contain backend-defined `metadata`. Callers must not depend
on a metadata field until it is part of the registered NVX response contract.

### Start

Example request:

```json
{
  "version": "<nvx-contract-version>",
  "phase": "start",
  "sandboxId": "<nvx-prefix>:abc123"
}
```

Example successful response:

```json
{
  "result": {}
}
```

An implementation may return backend-defined metadata, but it does not need to
echo `"state": "running"`.

### Exec

Only `process.commandLine` is required inside `process`. This example also
shows the optional working directory, environment, default-environment
inheritance, and timeout fields.

Example request:

```json
{
  "version": "<nvx-contract-version>",
  "phase": "exec",
  "sandboxId": "<nvx-prefix>:abc123",
  "process": {
    "commandLine": "python app.py",
    "cwd": "/workspace",
    "env": [
      "MODE=test"
    ],
    "inheritDefaultEnv": true,
    "timeout": 30000
  }
}
```

Exec does not return a buffered JSON object containing all stdout and stderr.
The caller receives the streams live and then receives the terminal execution
outcome. For example:

```text
stdout: "hello\n"
stderr: ""
outcome: Exited(0)
```

Timeout, cancellation, and backend failure are distinct terminal outcomes.

### Stop

Example request:

```json
{
  "version": "<nvx-contract-version>",
  "phase": "stop",
  "sandboxId": "<nvx-prefix>:abc123"
}
```

Example successful response:

```json
{
  "result": {}
}
```

The successful operation leaves the sandbox provisioned. It does not need to
return `"state": "provisioned"`.

### Deprovision

Example request:

```json
{
  "version": "<nvx-contract-version>",
  "phase": "deprovision",
  "sandboxId": "<nvx-prefix>:abc123"
}
```

Example successful response:

```json
{
  "result": {}
}
```

After success, the sandbox ID is no longer valid. There is no persistent
`deprovisioned` state represented by that ID.

### Error

Non-exec phases and exec validation failures return a typed error rather than a
success-shaped fallback:

```json
{
  "error": {
    "code": "unsupported_containment",
    "message": "The requested containment backend is not supported"
  }
}
```

The exact error code depends on the owning validation layer. Structural
contract errors must be reported before backend capability or execution errors.

## Schema configuration coverage

The table lists minimum required inputs. The exact 0.9 request roots also
define these optional fields:

| Scope | Optional fields |
| --- | --- |
| Every phase | `$schema`, `_comment`, and `telemetry.enabled` |
| `Provision` | Backend-specific. Published 0.9 IsolationSession accepts `isolationSession.provision.appId` and requires an explicit all-allow network posture. Published 0.9 WSLC accepts `filesystem`, `network`, `wslc.provision.image`, and `wslc.provision.imageTarPath`. A future NVX root must independently define the fields it accepts and enforces. |
| `Exec` | `process.cwd`, `process.env`, `process.inheritDefaultEnv`, `process.timeout`, `network`, `runtimeConfig.networkProxy`, and `telemetry` at the raw exact-contract layer. The NVX SDK configuration and backend policy honor matrix must narrow these to fields NVX can enforce. |
| `Start`, `Stop`, and `Deprovision` | `telemetry`; these published 0.9 roots do not accept process or policy configuration |

Exact request roots reject unknown fields. Schema acceptance establishes only
that a field is structurally valid; the backend must separately reject policy
that it cannot represent or enforce before creating or modifying a sandbox.
