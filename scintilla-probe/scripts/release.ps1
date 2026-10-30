<#
.SYNOPSIS
  Package dist\NotePP as the two files the in-app updater downloads, ready to attach to a GitHub release.

.DESCRIPTION
  Run scripts\package.ps1 first. Then publish a GitHub release whose tag is native-v<version>
  (the version in Cargo.toml) and attach both files written to dist\release:
    NotePP-native-windows-x64.zip
    NotePP-native-windows-x64.zip.sha256
  The app only offers releases that are newer than itself, not drafts or prereleases, and have both files.
#>
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$app = Join-Path $root 'dist\NotePP'
if (-not (Test-Path (Join-Path $app 'sci_probe.exe'))) { throw 'Run scripts\package.ps1 first.' }
$out = Join-Path $root 'dist\release'
New-Item $out -ItemType Directory -Force | Out-Null
$zip = Join-Path $out 'NotePP-native-windows-x64.zip'
Remove-Item $zip -Force -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $app '*') -DestinationPath $zip
$hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
"$hash  NotePP-native-windows-x64.zip" | Set-Content "$zip.sha256" -Encoding ascii
$ver = (Select-String (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
Write-Host "Ready: $zip"
Write-Host "Tag the release native-v$ver and attach both files in $out"
