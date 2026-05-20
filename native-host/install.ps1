# Install the Llflash RTMP native messaging host for Chrome/Chromium/Edge/Brave/Firefox on Windows.
#
# Usage:
#   .\install.ps1 [browser]
#     browser in {chrome, chromium, edge, brave, firefox, all}  (default: chrome)
#
# Run with:
#   PowerShell -ExecutionPolicy Bypass -File install.ps1 [browser]
#
# The extension ID is fixed (pinned via the `key` field in manifest.json5) —
# no extension ID argument needed.
# The host name is hard-coded to "com.longliveflash.rtmp_host" because the
# wasm bridge inside the extension hard-codes that string in its
# runtime.connectNative call. Don't rename without updating both ends.

param(
    [string]$Browser = "chrome"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$HostName    = "com.longliveflash.rtmp_host"
$ScriptDir   = Split-Path -Parent $MyInvocation.MyCommand.Path
$WorkspaceRoot = Split-Path -Parent $ScriptDir

# Two supported layouts:
#   1. Packaged release zip: llflash-rtmp-host.exe is a sibling of this script.
#   2. Dev workspace: binary lives in <repo>\target\{release,debug}\.
$BundledBin = Join-Path $ScriptDir "llflash-rtmp-host.exe"
$ReleaseBin = Join-Path $WorkspaceRoot "target\release\llflash-rtmp-host.exe"
$DebugBin   = Join-Path $WorkspaceRoot "target\debug\llflash-rtmp-host.exe"

if (Test-Path $BundledBin) {
    $Binary = $BundledBin
} elseif (Test-Path $ReleaseBin) {
    $Binary = $ReleaseBin
} elseif (Test-Path $DebugBin) {
    $Binary = $DebugBin
} else {
    Write-Error "llflash-rtmp-host.exe not found. Looked for:`n  $BundledBin`n  $ReleaseBin`n  $DebugBin`nBuild it first:`n  cargo build --release -p llflash_rtmp_host --target x86_64-pc-windows-msvc"
    exit 1
}

Write-Host "binary:    $Binary"
Write-Host "host name: $HostName"

$Template = Join-Path $ScriptDir "manifest\$HostName.template.json"
if (-not (Test-Path $Template)) {
    Write-Error "template not found at $Template"
    exit 1
}

# JSON-escape the binary path (backslashes must be doubled).
$EscapedBin  = $Binary -replace '\\', '\\'
$ManifestJson = (Get-Content $Template -Raw) -replace '__BINARY_PATH__', $EscapedBin

# Drop the rendered manifest in %APPDATA%\LongLiveFlash\ so it survives across
# builds without needing to re-run the installer.
$ManifestDir  = Join-Path $env:APPDATA "LongLiveFlash"
New-Item -ItemType Directory -Force -Path $ManifestDir | Out-Null
$ManifestPath = Join-Path $ManifestDir "$HostName.json"
Set-Content -Path $ManifestPath -Value $ManifestJson -Encoding UTF8
Write-Host "manifest:  $ManifestPath"

function Install-One {
    param([string]$Label, [string]$RegPath)
    $key = "HKCU:\$RegPath\$HostName"
    New-Item -Path $key -Force | Out-Null
    Set-ItemProperty -Path $key -Name "(Default)" -Value $ManifestPath
    Write-Host "[$Label] -> $key"
}

$reg = @{
    chrome   = "Software\Google\Chrome\NativeMessagingHosts"
    chromium = "Software\Chromium\NativeMessagingHosts"
    edge     = "Software\Microsoft\Edge\NativeMessagingHosts"
    brave    = "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts"
    firefox  = "Software\Mozilla\NativeMessagingHosts"
}

switch ($Browser.ToLower()) {
    "all" {
        foreach ($entry in $reg.GetEnumerator()) {
            Install-One $entry.Key $entry.Value
        }
    }
    { $reg.ContainsKey($_) } {
        Install-One $_ $reg[$_]
    }
    default {
        Write-Error "Unknown browser '$Browser'. Use chrome|chromium|edge|brave|firefox|all."
        exit 1
    }
}

Write-Host ""
Write-Host "Done. Restart the browser if it was already running."
