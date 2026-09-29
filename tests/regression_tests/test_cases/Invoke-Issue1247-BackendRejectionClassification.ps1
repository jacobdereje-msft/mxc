# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

# Purpose: Verify a Tier 1 backend policy rejection returns a typed envelope and exit code 1.

param(
    [string]$WxcExec,
    [string]$WorkDirectory = (Join-Path $env:TEMP "mxc-issue-1247"),
    [switch]$CheckPrerequisites
)

$ErrorActionPreference = "Stop"
. (Join-Path (Split-Path -Parent $PSScriptRoot) "inc\Resolve-RegressionExecutable.ps1")
. (Join-Path (Split-Path -Parent $PSScriptRoot) "inc\Complete-RegressionTest.ps1")
. (Join-Path (Split-Path -Parent $PSScriptRoot) "inc\Add-RegressionCommandLine.ps1")

$WxcExec = Resolve-RegressionExecutable $WxcExec "wxc-exec.exe"
$cmdExe = Join-Path $env:SystemRoot "System32\cmd.exe"

New-Item -ItemType Directory -Force -Path $WorkDirectory | Out-Null

$configJson = @"
{
    "version": "0.8.0-alpha",
    "containment": "processcontainer",
    "process": {
        "env": $((ConvertTo-Json -InputObject @("SystemRoot=$env:SystemRoot", "TEMP=$WorkDirectory", "TMP=$WorkDirectory") -Compress))
    }
}
"@

$commandLine = "`"$cmdExe`" /d /c echo COMMAND_EXECUTED"
$json = Add-RegressionCommandLine $configJson $commandLine
$base64 = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($json))

$probeOutput = (& $WxcExec --probe --config-base64 $base64 2>&1 | Out-String)
$probeExitCode = $LASTEXITCODE
if ($probeExitCode -ne 0) {
    Write-Host $probeOutput
    Write-Error "The Tier 1 request probe failed with exit code $probeExitCode."
    exit 1
}

$probe = $probeOutput | ConvertFrom-Json
if ($probe.tier -ne "base-container") {
    Write-Host "SKIPPED: The request selected '$($probe.tier)' instead of Tier 1 BaseContainer." -ForegroundColor Yellow
    exit 77
}

if ($CheckPrerequisites) {
    exit 0
}

$output = (& $WxcExec --config-base64 $base64 2>&1 | Out-String)
$exitCode = $LASTEXITCODE
$rejectionText = "missing the required variable(s): LOCALAPPDATA"

Write-Host "Issue #1247: Tier 1 backend policy rejection classification" -ForegroundColor Cyan
Write-Host "Exit code: $exitCode"
Write-Host $output

$messageCount = ([regex]::Matches($output, [regex]::Escape($rejectionText))).Count
$hasGenericCode = $output.Contains('"code":"backend_error"')
$hasPolicyCode = $output.Contains('"code":"policy_validation"')
$commandDidNotRun = -not $output.Contains("COMMAND_EXECUTED")
$passed = $exitCode -eq 1 `
    -and $messageCount -eq 1 `
    -and -not $hasGenericCode `
    -and $hasPolicyCode `
    -and $commandDidNotRun

Complete-RegressionTest -Passed $passed `
    -SuccessMessage "Tier 1 rejection exited 1 with one policy_validation envelope." `
    -FailureMessage "Expected a typed Tier 1 rejection; exit=$exitCode, messageCount=$messageCount, genericCode=$hasGenericCode, policyCode=$hasPolicyCode, commandDidNotRun=$commandDidNotRun."
