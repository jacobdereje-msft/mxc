# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License.
#
# Packages or publishes mxc-sdk and its first-party dependency closure.

[CmdletBinding()]
param
(
    [ValidateSet('Package', 'Publish')]
    [string] $Mode = 'Package',
    [string] $Registry = 'Mxc-Azure-Feed',
    [string] $ManifestPath = 'src/Cargo.toml'
)

$ErrorActionPreference = 'Stop'

$crates = @(
    & (Join-Path $PSScriptRoot 'Get-MxcSdkCrateOrder.ps1') `
        -ManifestPath $ManifestPath `
        -Registry $Registry
)
if ($crates.Count -eq 0)
{
    throw 'The mxc-sdk publish closure is empty'
}

Write-Host "$Mode $($crates.Count) crates through registry '$Registry':"
$crates | ForEach-Object { Write-Host "  $_" }

if ($Mode -eq 'Package')
{
    $packageArgs = @('package', '--manifest-path', $ManifestPath, '--registry', $Registry)
    foreach ($crate in $crates)
    {
        $packageArgs += @('-p', $crate)
    }

    cargo @packageArgs
    if ($LASTEXITCODE -ne 0)
    {
        throw "cargo package failed with exit $LASTEXITCODE"
    }
    return
}

foreach ($crate in $crates)
{
    Write-Host "Publishing $crate"
    cargo publish --manifest-path $ManifestPath --registry $Registry -p $crate
    if ($LASTEXITCODE -ne 0)
    {
        throw "cargo publish failed for $crate with exit $LASTEXITCODE"
    }
}
