// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

using System.Text;
using Microsoft.Mxc.Sdk;
using Microsoft.Mxc.Sdk.V1;
using Xunit;

namespace Microsoft.Mxc.Sdk.Tests;

public class MxcPtyProcessE2ETests
{
    [Fact]
    public async Task ProcessContainerPty_RoundTripsInputAndOutput()
    {
        Assert.SkipUnless(
            OperatingSystem.IsWindows()
                && Environment.GetEnvironmentVariable("MXC_PROCESSCONTAINER_E2E") == "1",
            "set MXC_PROCESSCONTAINER_E2E=1 on a Windows ProcessContainer host");

        const string workingDirectory = @"C:\Windows";
        var request = new SandboxRequest(
            new SandboxPolicy
            {
                Network = new NetworkPolicy
                {
                    Egress = new NetworkEgressPolicy
                    {
                        Default = NetworkAction.Allow,
                    },
                },
                Ui = new UiPolicy
                {
                    AllowWindows = true,
                },
                TimeoutMs = 5_000,
            },
            "cmd.exe /d /q /c \"set /p value= & echo MXC_DOTNET_PROCESSCONTAINER_PTY_OK\"")
        {
            Containment = new ProcessContainerContainment(),
            WorkingDirectory = workingDirectory,
        };

        using var terminal = MxcSandbox.SpawnWithPty(
            request,
            new MxcPtySize(24, 80));
        terminal.Resize(new MxcPtySize(40, 120));

        using var reader = new StreamReader(terminal.Output, Encoding.UTF8);
        var outputTask = reader.ReadToEndAsync(TestContext.Current.CancellationToken);
        var input = terminal.Input;
        await input.WriteAsync(
            "hello\r\n"u8.ToArray(),
            TestContext.Current.CancellationToken);
        await input.FlushAsync(TestContext.Current.CancellationToken);

        var result = terminal.Wait();
        input.Dispose();
        var output = await outputTask;

        Assert.False(result.TimedOut);
        Assert.Equal(0, result.ExitCode);
        Assert.Contains(
            "MXC_DOTNET_PROCESSCONTAINER_PTY_OK",
            output,
            StringComparison.Ordinal);
    }
}
