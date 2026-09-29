// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

using Microsoft.Mxc.Sdk;
using Xunit;

namespace Microsoft.Mxc.Sdk.Tests;

/// <summary>
/// The live-host gate the WSLC suite shares. A live run needs Windows, a host
/// running WSL2 with the WSLC runtime, and a native library built with the
/// wslc feature.
/// </summary>
internal static class WslcHost
{
    // Evaluated once: this answer decides failure versus skip, so it has to be
    // the same for every test that consults it.
    private static readonly Lazy<bool> Available = new(() =>
        MxcSandbox.GetAvailableBackends()
            .Any(b => b.Backend == ContainmentBackend.Wslc));

    // Without this, a run in which everything skipped is indistinguishable from
    // one that passed. The same variable the Rust WSLC suite honours.
    private static bool SkipsAreFailures =>
        Environment.GetEnvironmentVariable("MXC_WSLC_TESTS_REQUIRED") is "1" or "true";

    internal static string Image =>
        Environment.GetEnvironmentVariable("MXC_WSLC_TEST_IMAGE") ?? "alpine:latest";

    /// <summary>Skips the calling test when the backend is unavailable, or fails
    /// it when skips have been declared failures.</summary>
    internal static void Require()
    {
        Assert.False(
            SkipsAreFailures && !Available.Value,
            "MXC_WSLC_TESTS_REQUIRED is set, but GetAvailableBackends() does not "
                + "report the WSLC backend. That needs both a build with "
                + "MxcWithWslc and a host running WSL2 with the WSLC runtime.");
        Assert.SkipUnless(
            Available.Value,
            "GetAvailableBackends() does not report the WSLC backend");
    }
}
