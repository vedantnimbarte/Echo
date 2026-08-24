<#
.SYNOPSIS
    Installs Echo on Windows.

.DESCRIPTION
    Downloads the latest signed-nothing (see -Notes) NSIS installer from GitHub
    Releases, verifies it against the release checksums when available, and runs
    it.

    irm https://raw.githubusercontent.com/vedantnimbarte/Echo/main/scripts/install.ps1 | iex

.PARAMETER Version
    Install a specific tag (e.g. v0.1.0) instead of the latest release.

.PARAMETER Silent
    Run the installer without its UI.

.NOTES
    Echo is not code-signed yet. Windows SmartScreen will warn that the
    publisher is unknown; you have to choose "More info" then "Run anyway".
    See https://github.com/vedantnimbarte/Echo#installing
#>
[CmdletBinding()]
param(
    [string]$Version,
    [switch]$Silent
)

$ErrorActionPreference = 'Stop'
$Repo = 'vedantnimbarte/Echo'

function Write-Step($msg) { Write-Host $msg }
function Write-Warn($msg) { Write-Host "! $msg" -ForegroundColor Yellow }
function Fail($msg) { Write-Host "error: $msg" -ForegroundColor Red; exit 1 }

if ([Environment]::Is64BitOperatingSystem -ne $true) {
    Fail 'Echo ships 64-bit builds only.'
}

# TLS 1.2 is not the default on older Windows PowerShell hosts.
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$api = if ($Version) {
    "https://api.github.com/repos/$Repo/releases/tags/$Version"
} else {
    "https://api.github.com/repos/$Repo/releases/latest"
}

try {
    $release = Invoke-RestMethod -Uri $api -Headers @{ 'User-Agent' = 'echo-installer' }
} catch {
    Fail "Couldn't reach the GitHub releases API. $($_.Exception.Message)"
}

if (-not $release.tag_name) {
    Fail "No published release found for $Repo. If you're expecting one, it may still be a draft."
}

# Prefer the NSIS installer; fall back to the MSI if only that was built.
$asset = $release.assets | Where-Object { $_.name -like '*-setup.exe' } | Select-Object -First 1
if (-not $asset) {
    $asset = $release.assets | Where-Object { $_.name -like '*.msi' } | Select-Object -First 1
}
if (-not $asset) {
    Fail "Release $($release.tag_name) has no Windows installer attached."
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("echo-install-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
$installer = Join-Path $tmp $asset.name

try {
    Write-Step "Installing Echo $($release.tag_name)"
    Write-Step "  downloading $($asset.name)"
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $installer -UseBasicParsing

    # Verify against the release checksums when they're published.
    $sums = $release.assets | Where-Object { $_.name -eq 'SHA256SUMS.txt' } | Select-Object -First 1
    if ($sums) {
        $sumsPath = Join-Path $tmp 'SHA256SUMS.txt'
        Invoke-WebRequest -Uri $sums.browser_download_url -OutFile $sumsPath -UseBasicParsing

        $line = Select-String -Path $sumsPath -Pattern ([regex]::Escape($asset.name)) |
            Select-Object -First 1
        if (-not $line) {
            Write-Warn "$($asset.name) is not listed in SHA256SUMS.txt; skipping verification."
        } else {
            $expected = ($line.Line -split '\s+')[0]
            $actual = (Get-FileHash -Path $installer -Algorithm SHA256).Hash
            if ($actual -ne $expected.ToUpperInvariant() -and $actual -ne $expected) {
                Fail "Checksum mismatch for $($asset.name). Expected $expected, got $actual. Not installing."
            }
            Write-Step '  checksum verified'
        }
    } else {
        Write-Warn 'This release publishes no SHA256SUMS.txt; the download could not be verified.'
    }

    Write-Host ''
    Write-Warn 'Echo is not code-signed yet. Windows will warn that the publisher'
    Write-Warn 'is unknown — choose "More info" then "Run anyway" to continue.'
    Write-Host ''

    Write-Step '  launching the installer'
    if ($installer.EndsWith('.msi')) {
        $args = @('/i', "`"$installer`"")
        if ($Silent) { $args += '/quiet' }
        $proc = Start-Process -FilePath 'msiexec.exe' -ArgumentList $args -Wait -PassThru
    } else {
        # NSIS: /S is silent.
        $args = if ($Silent) { @('/S') } else { @() }
        $proc = Start-Process -FilePath $installer -ArgumentList $args -Wait -PassThru
    }

    if ($proc.ExitCode -ne 0) {
        Fail "The installer exited with code $($proc.ExitCode)."
    }

    Write-Host ''
    Write-Step 'Echo is installed. Launch it from the Start menu.'
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
