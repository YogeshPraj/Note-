<#
.SYNOPSIS
  Build the Windows installer and the update manifest for the 1.x updater.

.DESCRIPTION
  Run scripts\package.ps1 first. Writes to dist\release:
    NotePP-Setup-<version>.exe   the installer (per-user; replaces an installed 1.x release)
    latest.yml                   the manifest the 1.x app's updater reads (version + SHA-512 of the installer)
  Attach both to the GitHub release v<version> together with the zip from scripts\release.ps1.
  Needs NSIS (makensis): winget install NSIS.NSIS
  -Define passes extra /D switches to makensis (used by tests).
#>
param(
    [string[]]$Define = @(),
    [string]$OutDir
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$app = Join-Path $root 'dist\NotePP'
if (-not (Test-Path (Join-Path $app 'notepp.exe'))) { throw 'Run scripts\package.ps1 first.' }
if (-not $OutDir) { $OutDir = Join-Path $root 'dist\release' }
New-Item $OutDir -ItemType Directory -Force | Out-Null

$ver = (Select-String (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$makensis = (Get-Command makensis -ErrorAction SilentlyContinue).Source
if (-not $makensis) {
    $makensis = @("${env:ProgramFiles(x86)}\NSIS\makensis.exe", "$env:ProgramFiles\NSIS\makensis.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $makensis) { throw 'makensis not found. Install NSIS:  winget install NSIS.NSIS' }

$name = "NotePP-Setup-$ver.exe"
$out = Join-Path $OutDir $name
Remove-Item $out -Force -ErrorAction SilentlyContinue
$nsisArgs = @('/V2', "/DVERSION=$ver", "/DAPP_DIR=$app", "/DOUT_FILE=$out") + $Define + (Join-Path $root 'installer\NotePP.nsi')
& $makensis @nsisArgs
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $out)) { throw 'makensis failed' }

# electron-updater (used by the 1.x app) wants the SHA-512 as base64 and the size.
$sha = [Convert]::ToBase64String([Security.Cryptography.SHA512]::Create().ComputeHash([IO.File]::ReadAllBytes($out)))
$size = (Get-Item $out).Length
$date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-dd'T'HH:mm:ss.fff'Z'")
$yml = @"
version: $ver
files:
  - url: $name
    sha512: $sha
    size: $size
path: $name
sha512: $sha
releaseDate: '$date'
"@
[IO.File]::WriteAllText((Join-Path $OutDir 'latest.yml'), ($yml -replace "`r`n", "`n") + "`n", (New-Object Text.UTF8Encoding $false))
Write-Host "Installer: $out ($([math]::Round($size / 1MB, 1)) MB)"
Write-Host "Manifest:  $(Join-Path $OutDir 'latest.yml')"
