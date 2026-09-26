# Publishing the Rust SDK crates

`.azure-pipelines/1ES.Publish.Rust.yml` is the manual pipeline for publishing
the Rust `mxc-sdk` package to the private `Mxc-Azure-Feed` Cargo registry.

The pipeline defaults to package-only validation. Set its `publish` parameter
to `true` only for a release whose version in `src/Cargo.toml` has not already
been published.

The release set is derived from `mxc-sdk` rather than maintained as a second
list. `scripts/ci/Get-MxcSdkCrateOrder.ps1` follows non-development first-party
path dependencies and emits them leaf-first. Optional and target-specific
dependencies are included because they remain dependencies of the published
package. Workspace crates outside that closure are not packaged or published.

Every publishable first-party path dependency must include a version. Cargo
uses that version when replacing the local path with a registry dependency in
the packaged manifest. A crate in the closure with `publish = false` causes the
pipeline to fail before packaging.

The closure inherits `publish = ["Mxc-Azure-Feed"]` from the workspace, so
Cargo rejects attempts to publish these internal crates to another registry.

The pipeline authenticates only to `Mxc-Azure-Feed` through
`Cargo.Setup.Private.yml@self`; it does not configure the public dependency
feed.

To inspect the current order from the repository root:

```powershell
pwsh scripts/ci/Get-MxcSdkCrateOrder.ps1
```

To reproduce the package-only operation after configuring and authenticating
the private registry:

```powershell
pwsh scripts/ci/Invoke-MxcSdkCratePublish.ps1 -Mode Package
```
