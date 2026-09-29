// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

using Microsoft.Mxc.Sdk;
using Xunit;

namespace Microsoft.Mxc.Sdk.Tests;

/// <summary>
/// Drives the state-aware lifecycle against a live WSLC host, which needs
/// Windows, a host running WSL2 with the WSLC runtime, and a native library
/// built with the wslc feature.
/// </summary>
/// <remarks>
/// These establish that the lifecycle works end to end through this binding
/// rather than only through the engine: the id provision mints, and the output
/// and exit code an exec returns. The image is expected to be cached already;
/// pulling is covered by the WSLC executor E2E suite.
/// </remarks>
[Collection("MxcLiveHost")]
public class MxcWslcLifecycleE2ETests
{
    /// <summary>
    /// Deprovisions a sandbox the test did not deprovision itself. Provision
    /// mints a real container, so an assertion that throws part-way would
    /// otherwise leave it on the host.
    /// </summary>
    private sealed class Teardown : IDisposable
    {
        private SandboxId? _id;

        public Teardown(SandboxId id) => _id = id;

        /// <summary>
        /// Gives up ownership once the test has deprovisioned itself.
        /// Deprovision is not idempotent, so without this the disposal below
        /// would report a failure that did not happen.
        /// </summary>
        public void Defuse() => _id = null;

        public void Dispose()
        {
            if (_id is not { } id)
            {
                return;
            }
            _id = null;
            try
            {
                MxcLifecycle.StopSandbox(id);
            }
            catch (MxcException)
            {
                // A sandbox that never started, or already stopped, still has to
                // be deprovisioned — that is the step that frees the container.
            }
            try
            {
                MxcLifecycle.DeprovisionSandbox(id);
            }
            catch (MxcException e)
            {
                Console.Error.WriteLine(
                    $"WARNING: deprovision failed, the container may leak: {e.Message}");
            }
        }
    }

    private sealed record Started(SandboxId Id, Teardown Teardown);

    private static Started Provision()
    {
        var provisioned = MxcLifecycle.ProvisionSandbox(
            StateAwareContainment.Wslc,
            new WslcProvisionOptions { Image = WslcHost.Image });
        return new Started(provisioned.SandboxId, new Teardown(provisioned.SandboxId));
    }

    private static Started ProvisionAndStart()
    {
        var started = Provision();
        try
        {
            MxcLifecycle.StartSandbox(started.Id);
            return started;
        }
        catch
        {
            started.Teardown.Dispose();
            throw;
        }
    }

    /// <summary>Provision reaches the backend through this binding and mints an
    /// id the later phases accept.</summary>
    [Fact]
    public void Provision_ReturnsAUsableSandboxId()
    {
        WslcHost.Require();

        var started = Provision();
        using (started.Teardown)
        {
            // Post-provision phases resolve the backend from this prefix.
            Assert.StartsWith("wslc:", started.Id.Value, StringComparison.Ordinal);

            MxcLifecycle.StartSandbox(started.Id);
        }
    }

    /// <summary>The lifecycle runs a command and its output reaches the caller.</summary>
    [Fact]
    public async Task Lifecycle_RunsEndToEnd()
    {
        WslcHost.Require();

        var started = ProvisionAndStart();
        using (started.Teardown)
        {
            var run = await MxcLifecycle.ExecInSandboxAsync(
                started.Id,
                "echo dotnet-wslc-marker",
                TestContext.Current.CancellationToken);

            Assert.False(run.TimedOut);
            Assert.Equal(0, run.ExitCode);
            Assert.Contains("dotnet-wslc-marker", run.Stdout);
        }
    }

    /// <summary>The sandboxed process's exit code must reach the caller
    /// unchanged, through the streaming handle's wait.</summary>
    [Fact]
    public void Exec_PropagatesANonZeroExitCode()
    {
        WslcHost.Require();

        var started = ProvisionAndStart();
        using (started.Teardown)
        {
            using var proc = MxcLifecycle.ExecInSandbox(started.Id, "exit 7");
            var result = proc.Wait();

            Assert.False(result.TimedOut);
            Assert.Equal(7, result.ExitCode);
        }
    }

    /// <summary>
    /// Stop and deprovision must be reachable through this binding, and a
    /// deprovisioned id must not still be usable.
    /// </summary>
    [Fact]
    public void Deprovision_RetiresTheSandboxId()
    {
        WslcHost.Require();

        var started = ProvisionAndStart();
        using (started.Teardown)
        {
            MxcLifecycle.StopSandbox(started.Id);
            MxcLifecycle.DeprovisionSandbox(started.Id);
            started.Teardown.Defuse();

            var ex = Assert.Throws<MxcException>(
                () => MxcLifecycle.StartSandbox(started.Id));
            Assert.Equal(ErrorCode.NotProvisioned, ex.Code);
        }
    }
}
