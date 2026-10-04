# Rust V1 operation signatures

Public entrypoint: `mxc_sdk::v1`. [Types](types.md) | [Overview](README.md)

Signatures describe the supported consumer API and omit implementation bodies and serialization attributes. Raw Rust JSON bridges are listed separately from typed requests; native wire compatibility is unchanged.

## `mxc_sdk::v1::available_backends`

Probe the host and return only the backends it can currently run.

```rust
pub fn available_backends() -> Vec<AvailableBackend>;
```


## `mxc_sdk::v1::available_tools_policy`, `mxc_sdk::v1::policy::available_tools_policy`

Discover tool and SDK directories from environment (defaults to the process environment) as read-only policy paths.

```rust
pub fn available_tools_policy(environment: Option<&[(String, String)]>, options: ToolsPolicyOptions) -> FilesystemPolicyResult;
```


## `mxc_sdk::v1::container::deprovision_container`

Deprovision an existing container.

```rust
pub fn deprovision(
container_id: &ContainerId,
options: DeprovisionOptions,
) -> Result<LifecycleResult, Error>;
```


## `mxc_sdk::v1::container::provision_container`

Provision a container from typed Rust policy.

```rust
pub fn provision(
request: ProvisionRequest,
options: ProvisionOptions,
) -> Result<ProvisionResult, Error>;
```


## `mxc_sdk::v1::container::spawn_in_container_with_pty`

Spawn a workload in an existing container with a caller-controlled PTY.

```rust
pub fn spawn_in_container_with_pty(
container_id: &ContainerId,
request: ExecutionRequest,
options: SpawnInContainerWithPtyOptions,
) -> Result<MxcPtyProcess, Error>;
```


## `mxc_sdk::v1::container::start_container`

Start an existing container.

```rust
pub fn start(container_id: &ContainerId, options: StartOptions) -> Result<LifecycleResult, Error>;
```


## `mxc_sdk::v1::container::stop_container`

Stop an existing container.

```rust
pub fn stop(container_id: &ContainerId, options: StopOptions) -> Result<LifecycleResult, Error>;
```


## `mxc_sdk::v1::container::validate_deprovision`

Validate a deprovision request without changing the container.

```rust
pub fn validate_deprovision(
container_id: &ContainerId,
options: DeprovisionOptions,
) -> Result<ValidationResult, Error>;
```


## `mxc_sdk::v1::container::validate_process`

Validate an execution request without running a workload.

```rust
pub fn validate_process(
container_id: &ContainerId,
request: ExecutionRequest,
options: SpawnInContainerOptions,
) -> Result<ValidationResult, Error>;
```


## `mxc_sdk::v1::container::validate_provision`

Validate a provision request without creating a container.

```rust
pub fn validate_provision(
request: ProvisionRequest,
options: ProvisionOptions,
) -> Result<ValidationResult, Error>;
```


## `mxc_sdk::v1::container::validate_start`

Validate a start request without starting the container.

```rust
pub fn validate_start(
container_id: &ContainerId,
options: StartOptions,
) -> Result<ValidationResult, Error>;
```


## `mxc_sdk::v1::container::validate_stop`

Validate a stop request without stopping the container.

```rust
pub fn validate_stop(
container_id: &ContainerId,
options: StopOptions,
) -> Result<ValidationResult, Error>;
```


## `mxc_sdk::v1::execute_lifecycle`

Execute a lifecycle request (as a JSON string) as a **live streaming** process, returning a [MxcProcess] handle for output streaming, waiting, and termination — exactly like [v1::spawn].

```rust
pub fn execute_lifecycle(request_json: &str, experimental: bool) -> Result<MxcProcess, Error>;
```


## `mxc_sdk::v1::execute_lifecycle_json`

Execute a raw lifecycle JSON request as a live streaming process.

```rust
pub fn execute_lifecycle_json(
request_json: &str,
experimental: bool,
) -> Result<MxcProcess, Error>;
```


## `mxc_sdk::v1::platform_support`

Detect MXC support on the current host.

```rust
pub fn platform_support() -> PlatformSupport;
```


## `mxc_sdk::v1::probe`

Probe an optional ProcessContainer request without creating a container.

```rust
pub fn probe(request: Option<&ContainerRequest>) -> Result<crate::ProbeOutput, Error>;
```


## `mxc_sdk::v1::run`

Run a [ContainerRequest] to completion and capture its output.

```rust
pub fn run(request: ContainerRequest, options: RunOptions) -> Result<ExecutionResult, Error>;
```


## `mxc_sdk::v1::run_in_container`

Run a workload in an existing container to completion and capture output.

```rust
pub fn run_in_container(
container_id: &ContainerId,
request: ExecutionRequest,
options: RunInContainerOptions,
) -> Result<ExecutionResult, Error>;
```


## `mxc_sdk::v1::run_json`

Run a raw exact-version JSON container request to completion and capture its output.

```rust
pub fn run_json(request_json: &str, experimental: bool) -> Result<ExecutionResult, Error>;
```


## `mxc_sdk::v1::run_lifecycle_json`

Run a lifecycle request (as a JSON string) and return the response-envelope JSON string.

```rust
pub fn run_lifecycle_json(
request_json: &str,
dry_run: bool,
experimental: bool,
) -> Result<String, Error>;
```


## `mxc_sdk::v1::spawn`

Spawn a [ContainerRequest] and return its live process.

```rust
pub fn spawn(request: ContainerRequest, options: SpawnOptions) -> Result<MxcProcess, Error>;
```


## `mxc_sdk::v1::spawn_container_json`

Spawn a raw exact-version JSON container request as a live process.

```rust
pub fn spawn_container_json(
request_json: &str,
experimental: bool,
) -> Result<MxcProcess, Error>;
```


## `mxc_sdk::v1::spawn_in_container`

Spawn a workload in an existing container and return its live process.

```rust
pub fn spawn_in_container(
container_id: &ContainerId,
request: ExecutionRequest,
options: SpawnInContainerOptions,
) -> Result<MxcProcess, Error>;
```


## `mxc_sdk::v1::spawn_in_container_with_pty_json`

Spawn a lifecycle execution request with a caller-controlled PTY.

```rust
pub fn spawn_in_container_with_pty_json(
request_json: &str,
size: v1::MxcPtySize,
experimental: bool,
) -> Result<v1::MxcPtyProcess, Error>;
```


## `mxc_sdk::v1::spawn_with_pty`

Spawn a [ContainerRequest] attached to a caller-controlled PTY.

```rust
pub fn spawn_with_pty(
request: ContainerRequest,
options: SpawnWithPtyOptions,
) -> Result<MxcPtyProcess, Error>;
```


## `mxc_sdk::v1::spawn_with_pty_json`

Spawn an exact-version JSON request attached to a caller-controlled PTY.

```rust
pub fn spawn_with_pty_json(
request_json: &str,
experimental: bool,
size: v1::MxcPtySize,
) -> Result<v1::MxcPtyProcess, Error>;
```


## `mxc_sdk::v1::telemetry::get_consent`

Return the consent state currently effective for telemetry authorization.

```rust
pub fn get_consent() -> ConsentState;
```


## `mxc_sdk::v1::telemetry::get_consent_status`

Read stored and effective consent.

```rust
pub fn get_consent_status() -> ConsentStatus;
```


## `mxc_sdk::v1::telemetry::get_policy`

Read the administrative telemetry policy.

```rust
pub fn get_policy() -> PolicyState;
```


## `mxc_sdk::v1::telemetry::is_blocked_by_policy`

Whether an administrator has blocked telemetry on this machine.

```rust
pub fn is_blocked_by_policy() -> bool;
```


## `mxc_sdk::v1::telemetry::needs_consent_prompt`

Whether a host should show the first-run consent prompt.

```rust
pub fn needs_consent_prompt() -> bool;
```


## `mxc_sdk::v1::telemetry::request_consent`

Invoke a host presenter and persist its decision.

```rust
pub fn request_consent<F>(
locale: Option<&str>,
presenter: F,
) -> Result<ConsentActionOutcome, ConsentError>
where
F: FnOnce(&ConsentPrompt) -> Result<ConsentDecision, String>;
```


## `mxc_sdk::v1::telemetry::request_consent_async`

Asynchronous counterpart to [request_consent].

```rust
pub async fn request_consent_async<F, Fut>(
locale: Option<&str>,
presenter: F,
) -> Result<ConsentActionOutcome, ConsentError>
where
F: FnOnce(ConsentPrompt) -> Fut,
Fut: std::future::Future<Output = Result<ConsentDecision, String>>;
```


## `mxc_sdk::v1::telemetry::withdraw_consent`

Idempotently withdraw telemetry consent.

```rust
pub fn withdraw_consent() -> Result<ConsentActionOutcome, ConsentError>;
```


## `mxc_sdk::v1::temporary_files_policy`, `mxc_sdk::v1::policy::temporary_files_policy`

Read-write policy for the host temporary directory.

```rust
pub fn temporary_files_policy(environment: Option<&[(String, String)]>) -> FilesystemPolicyResult;
```


## `mxc_sdk::v1::user_profile_policy`, `mxc_sdk::v1::policy::user_profile_policy`

Read-only policy for standard user-profile application data locations.

```rust
pub fn user_profile_policy(environment: Option<&[(String, String)]>) -> FilesystemPolicyResult;
```
