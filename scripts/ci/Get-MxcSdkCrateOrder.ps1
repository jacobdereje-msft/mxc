# Copyright (c) Microsoft Corporation. All rights reserved.
# Licensed under the MIT License.
#
# Prints the publishable first-party dependency closure of mxc-sdk, leaf-first.

[CmdletBinding()]
param
(
    [string] $ManifestPath = 'src/Cargo.toml',
    [string] $RootCrate = 'mxc-sdk',
    [string] $Registry,
    [switch] $Yaml
)

$ErrorActionPreference = 'Stop'

$metadata = cargo metadata --format-version 1 --no-deps --manifest-path $ManifestPath | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed with exit $LASTEXITCODE" }

$members = @{}
foreach ($package in $metadata.packages)
{
    $members[$package.name] = $package
}

if (-not $members.ContainsKey($RootCrate))
{
    throw "root crate '$RootCrate' is not a member of $ManifestPath"
}

function Get-FirstPartyDependencies([string] $Name)
{
    $dependencies = @()
    foreach ($dependency in $members[$Name].dependencies)
    {
        if ($dependency.kind -eq 'dev' -or -not $dependency.path -or -not $members.ContainsKey($dependency.name))
        {
            continue
        }

        if (-not $dependency.req -or $dependency.req -eq '*')
        {
            throw "$Name has publishable path dependency '$($dependency.name)' without a version"
        }

        $dependencies += $dependency.name
    }

    return $dependencies | Sort-Object -Unique
}

$closure = [System.Collections.Generic.HashSet[string]]::new()
$pending = [System.Collections.Generic.Stack[string]]::new()
$pending.Push($RootCrate)
while ($pending.Count -gt 0)
{
    $name = $pending.Pop()
    if (-not $closure.Add($name))
    {
        continue
    }

    $package = $members[$name]
    if ($package.publish -is [array] -and $package.publish.Count -eq 0)
    {
        throw "$name is in the $RootCrate publish closure but has publish = false"
    }
    if ($Registry -and $package.publish -is [array] -and $Registry -notin $package.publish)
    {
        throw "$name cannot be published to registry '$Registry'"
    }

    foreach ($dependency in Get-FirstPartyDependencies $name)
    {
        $pending.Push($dependency)
    }
}

$edges = @{}
foreach ($name in $closure)
{
    $edges[$name] = @(Get-FirstPartyDependencies $name | Where-Object { $closure.Contains($_) })
}

$order = @()
$remaining = [System.Collections.Generic.HashSet[string]]::new($closure)
while ($remaining.Count -gt 0)
{
    $ready = @(
        $remaining |
            Where-Object { @($edges[$_] | Where-Object { $remaining.Contains($_) }).Count -eq 0 }
    )
    if ($ready.Count -eq 0)
    {
        throw "dependency cycle among: $($remaining -join ', ')"
    }

    $ready = [string[]] $ready
    [Array]::Sort($ready, [System.StringComparer]::Ordinal)
    foreach ($name in $ready)
    {
        $order += $name
        $remaining.Remove($name) | Out-Null
    }
}

if ($Yaml)
{
    foreach ($name in $order)
    {
        "    - $name"
    }
}
else
{
    $order
}
