# SDK API reference

These references describe the supported V1 SDK authoring surfaces: operation
signatures, public types, fields, and containment choices. They complement the
SDK READMEs and backend guides; they do not replace native policy validation or
certify backend availability.

| SDK | Reference | Public entrypoint |
|---|---|---|
| Rust | [V1](rust/v1/README.md) | `mxc_sdk::v1` |
| .NET | [V1](dotnet/v1/README.md) | `Microsoft.Mxc.Sdk.V1` |
| Node | [V1](node/v1/README.md) | `@microsoft/mxc-sdk/v1` |

## Common contract

- Creation takes `ContainerRequest` followed by operation-specific options.
- Provision takes `ProvisionRequest` followed by `ProvisionOptions`.
- Provision results expose an optional `metadata`/`Metadata` value through
  `ProvisionMetadata`. Rust uses an enum, .NET a closed hierarchy, and Node a
  backend-inferred `ProvisionMetadata<C>` type. IsolationSession metadata is
  `IsolationSessionProvisionMetadata`; WSLC currently returns none.
- Provision results also expose policy and operational warnings. IsolationSession
  provision metadata requires agent user name, agent user SID, and ephemeral
  workspace path when the metadata object is present.
- Start, stop, and deprovision take the opaque `ContainerId` followed by their
  operation-specific options, and return `LifecycleResult` with warnings.
- Execution in an existing container takes `ContainerId`, `ExecutionRequest`,
  then operation-specific options.
- Validation returns `ValidationResult` with policy and operational warnings,
  without executing the requested operation.
- Captured results and live process handles expose optional `ExecutionMetadata`,
  including `CaptureDenialsResult` and `CaptureDenialsError` when produced.
- Runtime-only network values use `NetworkRuntimeConfig`; the native JSON field
  remains `runtimeConfig`.
- Invocation telemetry uses `TelemetryConfig` on operation options. Its optional
  `enabled`/`Enabled` value preserves omission and explicit `false`; emission
  remains subject to persisted MXC consent and administrative restrictions.
- All filesystem helpers accept an optional `environment` input. Omission reads
  the process environment; an explicitly empty collection does not. Tool
  discovery also accepts `ToolsPolicyOptions` for Windows ProcessContainer
  ALL APPLICATION PACKAGES filtering. Failed ACL inspections retain the
  directory and emit a diagnostic warning.
- Initial PTY dimensions belong to the PTY operation's options. Later resizing
  is an operation on the returned terminal process.
- Containment is a closed SDK-owned choice, defaulting to generic Process
  intent. Backend configuration payloads use consistent `*Config` names where
  configuration types are separate from containment choices.
- Preserve environment omission versus an explicitly empty environment,
  native policy validation, error propagation, and process ownership.

Language-appropriate casing, constructors, enums, and discriminated unions are
intentional. Rust is synchronous; .NET and Node expose asynchronous operations.
Node captured execution in an existing container has synchronous and asynchronous
forms for IsolationSession and WSLC. .NET cancellation tokens are trailing parameters.

## Keeping references current

Update the affected signature and type pages whenever a public SDK API changes.
Review the corresponding APIs in all three SDKs, including options, defaults,
nullability, ownership, platform gates, and examples.

Breaking changes to a published SDK API require a new versioned (V*) API
surface and matching signature/type references under each affected SDK's
`docs/reference/<sdk>/v*/` directory. Preserve the published version's
references.
