<#
.SYNOPSIS
    One command, one Setup.exe: copies the release build of ridge.exe into release\Ridge and compiles
    installer\Ridge.iss with Inno Setup into release\Ridge-Setup-<version>.exe.

.DESCRIPTION
    The version comes from Cargo.toml and from nowhere else: it is stamped into the Setup filename, the installer's
    Add/Remove Programs entry and the Setup's version resource.

    Code signing is not a switch: when the Azure Artifact Signing settings are present (tools\CodeSigning.ps1)
    ridge.exe, then the Setup and its uninstaller, are signed with a timestamp and checked; when absent the build
    is unsigned and the build log says so.

.PARAMETER Exe
    The ridge.exe to package (default: the x86_64-pc-windows-msvc release build).
#>
param(
    [string]$Version,
    [string]$Exe
)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent

if (-not $Version) {
    $line = Select-String -Path (Join-Path $root "Cargo.toml") -Pattern '^version = "(.+)"$' | Select-Object -First 1
    if (-not $line) { throw "no version found in Cargo.toml" }
    $Version = $line.Matches[0].Groups[1].Value
}
if ($Version -cnotmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
    throw "version must be strict SemVer MAJOR.MINOR.PATCH (e.g. 0.1.4), got: '$Version'"
}
if (-not $Exe) { $Exe = Join-Path $root "target\x86_64-pc-windows-msvc\release\ridge.exe" }
if (-not (Test-Path $Exe)) { throw "ridge.exe not found at $Exe (run: cargo build --release --target x86_64-pc-windows-msvc)" }

function Get-Iscc {
    $onPath = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($onPath) { return $onPath.Source }
    foreach ($candidate in @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
        "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    )) {
        if (Test-Path $candidate) { return $candidate }
    }
    throw "Inno Setup 6 not found. Install it with: winget install --id JRSoftware.InnoSetup"
}
$iscc = Get-Iscc
Write-Host "Build of $Version, Inno Setup compiler: $iscc"

# code signing, when its settings are present (never committed: environment or the git-ignored tools\signing.local.json)
. (Join-Path $PSScriptRoot "CodeSigning.ps1")
$signing = Get-ridgeSigningConfig
if ($signing) {
    $signTool = Find-ridgeSignTool $signing
    if (-not (Test-Path $signing.Dlib)) { throw "Azure.CodeSigning.Dlib.dll not found: $($signing.Dlib)" }
    Write-Host "Code signing: ON (Azure Artifact Signing, account $($signing.Account), profile $($signing.Profile)); signtool $signTool"
} else {
    Write-Host "Code signing: OFF -- no signing settings found. This build is UNSIGNED." -ForegroundColor Yellow
}

$payload = Join-Path $root "release\Ridge"
if (Test-Path $payload) { Remove-Item $payload -Recurse -Force }
New-Item -ItemType Directory -Path $payload | Out-Null
Copy-Item $Exe (Join-Path $payload "ridge.exe")

$signMeta = $null
try {
    $isccArgs = @("/DAppVersion=$Version")
    if ($signing) {
        $signMeta = New-ridgeSigningMetadata $signing
        # sign the program before Inno embeds it, then let Inno sign the Setup and the uninstaller ($f = the file)
        Invoke-ridgeSign -Files @(Join-Path $payload "ridge.exe") -Config $signing -SignTool $signTool -Metadata $signMeta
        $isccArgs += "/DridgeSign=1"
        $isccArgs += ("/Sridgesign=" + (Get-ridgeSignCommand $signing $signTool $signMeta -ForInno) + ' $f')
    }
    & $iscc @isccArgs (Join-Path $root "installer\Ridge.iss")
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed with exit code $LASTEXITCODE" }
}
finally { if ($signMeta) { Remove-Item $signMeta -Force -ErrorAction SilentlyContinue } }

$setup = Join-Path $root "release\Ridge-Setup-$Version.exe"
if (-not (Test-Path $setup)) { throw "ISCC reported success but $setup is missing" }
if ($signing) { Assert-ridgeSigned -Files @($setup) }

$mb = [math]::Round((Get-Item $setup).Length / 1MB, 1)
Write-Host ""
Write-Host "Installer: $setup ($mb MB)"
Write-Host ("signed   : " + $(if ($signing) { "yes (Azure Artifact Signing)" } else { "NO -- unsigned build" }))
