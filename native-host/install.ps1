# Install the Llflash RTMP native messaging host for Chrome/Chromium/Edge/Brave/Firefox on Windows.
#
# Three modes — picked automatically:
#   1. Bootstrap (piped from `irm | iex`): downloads the latest release zip
#      from GitHub and installs it under %LOCALAPPDATA%\LongLiveFlash\rtmp-host.
#        irm https://github.com/luthebao/luthebao/releases/download/llflash/rtmp-host-install.ps1 | iex
#   2. Local-zip: when the .exe sits next to this script (extracted release
#      zip), uses that binary directly — no network.
#   3. Dev workspace: when run from the source tree, uses
#      ..\target\{release,debug}\.
#
# Usage:
#   .\install.ps1 [browser]
#     browser in {chrome, chromium, edge, brave, firefox, all}  (default: chrome)
#   PowerShell -ExecutionPolicy Bypass -File install.ps1 [browser]
#
# Env overrides:
#   $env:LLFLASH_REPO         GitHub owner/repo to download from
#                             (default: luthebao/luthebao)
#   $env:LLFLASH_TAG          Release tag (default: llflash)
#   $env:LLFLASH_INSTALL_DIR  Where to extract the binary in bootstrap mode
#                             (default: $env:LOCALAPPDATA\LongLiveFlash\rtmp-host)
#
# The host name is hard-coded to "com.longliveflash.rtmp_host" because the
# wasm bridge inside the extension hard-codes that string in its
# runtime.connectNative call. Don't rename without updating both ends.

param(
    [string]$Browser = "chrome"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Repo = if ($env:LLFLASH_REPO) { $env:LLFLASH_REPO } else { 'luthebao/luthebao' }
$Tag  = if ($env:LLFLASH_TAG)  { $env:LLFLASH_TAG }  else { 'llflash' }
$InstallDir = if ($env:LLFLASH_INSTALL_DIR) {
    $env:LLFLASH_INSTALL_DIR
} else {
    Join-Path $env:LOCALAPPDATA 'LongLiveFlash\rtmp-host'
}

$HostName = "com.longliveflash.rtmp_host"

# ---- Resolve binary location ---------------------------------------------
# When piped from `irm | iex`, $MyInvocation.MyCommand.Path is empty —
# that's how we detect bootstrap mode vs a script invoked from disk.
$ScriptDir = ''
if ($MyInvocation.MyCommand.Path) {
    $ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
}

$Binary   = ''
$Template = ''

function Try-LocalLayouts {
    if (-not $script:ScriptDir) { return $false }
    $sibling  = Join-Path $script:ScriptDir 'llflash-rtmp-host.exe'
    $wsRoot   = Split-Path -Parent $script:ScriptDir
    $relBin   = Join-Path $wsRoot 'target\release\llflash-rtmp-host.exe'
    $dbgBin   = Join-Path $wsRoot 'target\debug\llflash-rtmp-host.exe'
    $template = Join-Path $script:ScriptDir "manifest\$script:HostName.template.json"

    if (Test-Path $sibling) {
        $script:Binary = $sibling
    } elseif (Test-Path $relBin) {
        $script:Binary = $relBin
    } elseif (Test-Path $dbgBin) {
        $script:Binary = $dbgBin
    } else {
        return $false
    }
    if (-not (Test-Path $template)) { return $false }
    $script:Template = $template
    return $true
}

function Bootstrap-Download {
    $apiUrl = "https://api.github.com/repos/$script:Repo/releases/tags/$script:Tag"
    Write-Host "Fetching release '$script:Tag' from $script:Repo..."

    try {
        $release = Invoke-RestMethod -Uri $apiUrl -Headers @{
            'Accept'     = 'application/vnd.github+json'
            'User-Agent' = 'llflash-rtmp-host-installer'
        }
    } catch {
        throw "Failed to fetch ${apiUrl}: $_"
    }

    # Assets at one tag belong to one release, so there's at most one match.
    $asset = $release.assets `
        | Where-Object { $_.name -like 'llflash-rtmp-host-*-Windows-x64.zip' } `
        | Select-Object -First 1

    if (-not $asset) {
        throw "No llflash-rtmp-host-*-Windows-x64.zip asset on ${script:Repo}@${script:Tag}"
    }

    Write-Host "Downloading $($asset.browser_download_url)"
    $zipPath    = Join-Path $env:TEMP "llflash-rtmp-host-$([System.Guid]::NewGuid()).zip"
    $extractTmp = Join-Path $env:TEMP "llflash-rtmp-host-extract-$([System.Guid]::NewGuid())"

    try {
        Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zipPath -UseBasicParsing

        if (Test-Path $script:InstallDir) {
            Remove-Item $script:InstallDir -Recurse -Force
        }
        New-Item -ItemType Directory -Force -Path $script:InstallDir | Out-Null

        # Expand-Archive preserves the top-level "llflash-rtmp-host\" folder
        # inside the zip; flatten it into $InstallDir so paths match the
        # macOS/Linux layout.
        Expand-Archive -Path $zipPath -DestinationPath $extractTmp -Force
        $inner = Get-ChildItem -Path $extractTmp -Directory | Select-Object -First 1
        if (-not $inner) {
            throw "Unexpected zip layout: no top-level directory in $zipPath"
        }
        Get-ChildItem -Path $inner.FullName -Force | Move-Item -Destination $script:InstallDir -Force
    } finally {
        if (Test-Path $zipPath)    { Remove-Item $zipPath -Force -ErrorAction SilentlyContinue }
        if (Test-Path $extractTmp) { Remove-Item $extractTmp -Recurse -Force -ErrorAction SilentlyContinue }
    }

    $script:Binary   = Join-Path $script:InstallDir 'llflash-rtmp-host.exe'
    $script:Template = Join-Path $script:InstallDir "manifest\$script:HostName.template.json"
}

if (-not (Try-LocalLayouts)) {
    Bootstrap-Download
}

if (-not (Test-Path $Binary))   { Write-Error "Binary not found at $Binary"; exit 1 }
if (-not (Test-Path $Template)) { Write-Error "Template not found at $Template"; exit 1 }

Write-Host "binary:    $Binary"
Write-Host "host name: $HostName"

# ---- Render manifest -----------------------------------------------------
# JSON-escape the binary path (backslashes must be doubled).
$EscapedBin   = $Binary -replace '\\', '\\'
$ManifestJson = (Get-Content $Template -Raw) -replace '__BINARY_PATH__', $EscapedBin

# Drop the rendered manifest in %APPDATA%\LongLiveFlash\ so it survives across
# re-runs without needing to be rewritten.
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
