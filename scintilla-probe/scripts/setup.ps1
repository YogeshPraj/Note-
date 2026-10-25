<#
.SYNOPSIS
  One-time (and repeatable) setup for the native Note++ build on Windows.

.DESCRIPTION
  Gets everything the repository deliberately does not store:
    1. Scintilla + Lexilla  - downloaded at pinned versions and built with MinGW.
    2. Mermaid + the Excalidraw whiteboard bundle and fonts - restored from the
       Electron app's own build (npm install / build-whiteboard.js).
  Safe to re-run: finished steps are skipped unless -Force is given.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\setup.ps1
  cargo build
#>
param(
    [switch]$Force,
    [string]$ScintillaVersion = '5.5.2',
    [string]$LexillaVersion = '5.5.4'
)

$ErrorActionPreference = 'Stop'
$proj = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)   # scintilla-probe\
$root = Split-Path -Parent $proj                                                # repo root
function Step($m) { Write-Host "==> $m" -ForegroundColor Cyan }
function Ok($m) { Write-Host "    $m" -ForegroundColor Green }

# ---- 1. toolchain -----------------------------------------------------------
Step 'Checking toolchain'
if (-not (Get-Command g++ -ErrorAction SilentlyContinue)) {
    $winget = Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages'
    $gpp = Get-ChildItem $winget -Recurse -Filter g++.exe -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $gpp) { throw 'MinGW-w64 (g++) not found. Install it with:  winget install BrechtSanders.WinLibs.POSIX.UCRT' }
    $env:Path = "$($gpp.DirectoryName);$env:Path"
}
$make = Get-Command mingw32-make -ErrorAction SilentlyContinue
if (-not $make) { $make = Get-Command make -ErrorAction SilentlyContinue }
if (-not $make) { throw 'mingw32-make not found (it ships with the WinLibs package).' }
Ok "g++ and $($make.Name) found"

# ---- 2. Scintilla + Lexilla -------------------------------------------------
# Download a release zip into scintilla-probe\<name> unless the right version is already there.
function Get-Component($name, $version) {
    $dir = Join-Path $proj $name
    $compact = $version.Replace('.', '')
    $versionFile = Join-Path $dir 'version.txt'
    $current = (Test-Path $versionFile) -and ((Get-Content $versionFile -Raw).Trim() -eq $compact)
    if ($current -and -not $Force) { return }
    Step "Downloading $name $version"
    $zip = Join-Path $env:TEMP "$name$compact.zip"
    Invoke-WebRequest -UseBasicParsing "https://www.scintilla.org/$name$compact.zip" -OutFile $zip
    $expected = $PinnedHashes["$name$compact"]
    if (-not $expected) { Remove-Item $zip; throw "No pinned SHA-256 for $name$compact. Add it to `$PinnedHashes in setup.ps1 after checking the download." }
    $actual = (Get-FileHash $zip -Algorithm SHA256).Hash
    if ($actual -ne $expected) { Remove-Item $zip; throw "$name$compact.zip failed its integrity check (expected $expected, got $actual). Not extracting it." }
    if (Test-Path $dir) { Remove-Item $dir -Recurse -Force }
    Expand-Archive $zip -DestinationPath $proj -Force
    Remove-Item $zip
    if (-not (Test-Path $versionFile)) { throw "$name download looks wrong: $versionFile missing" }
}

$env:CXXFLAGS = '-include cstdint'   # GCC 16 no longer includes <cstdint> transitively

# SHA-256 of the release zips, also confirmed against the SourceForge mirror. A different
# version needs its own hash added here, so a tampered or swapped download is refused.
$PinnedHashes = @{
    'scintilla552' = '1A9257C27AA827090726AF813E8E02FC856D62ABB397D1AEB4F6B1029CA11C17'
    'lexilla554'   = '5E0A21356E4E2F19D6A38ECD1C8172D5A2B7B4542940568B023386080DB90FE0'
}

Get-Component 'scintilla' $ScintillaVersion
if ($Force -or -not (Test-Path "$proj\scintilla\bin\Scintilla.dll")) {
    Step "Building Scintilla $ScintillaVersion"
    & $make.Source -C "$proj\scintilla\win32" -j8
    if ($LASTEXITCODE -ne 0) { throw 'Scintilla build failed' }
}
Get-Component 'lexilla' $LexillaVersion
if ($Force -or -not (Test-Path "$proj\lexilla\bin\liblexilla.a")) {
    Step "Building Lexilla $LexillaVersion"
    & $make.Source -C "$proj\lexilla\src" -j8
    if ($LASTEXITCODE -ne 0) { throw 'Lexilla build failed' }
}
foreach ($f in 'scintilla\bin\Scintilla.dll', 'lexilla\bin\liblexilla.a') {
    if (-not (Test-Path "$proj\$f")) { throw "Expected build output is missing: $f" }
}
Ok "Scintilla $ScintillaVersion and Lexilla $LexillaVersion are ready"

# ---- 3. Mermaid + whiteboard (from the Electron build) ---------------------
Step 'Restoring Mermaid and whiteboard assets'
$mermaid = "$root\node_modules\mermaid\dist\mermaid.min.js"
$bundle = "$root\src\whiteboard.bundle.js"
if (-not (Test-Path $mermaid) -or -not (Test-Path $bundle)) {
    if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { throw 'npm is required to fetch Mermaid and Excalidraw (install Node.js 18+).' }
    Push-Location $root
    try {
        if (-not (Test-Path "$root\node_modules\mermaid")) {
            Step 'npm ci (exact versions and hashes from package-lock.json; lifecycle scripts skipped)'
            npm ci --ignore-scripts --no-audit --no-fund
            if ($LASTEXITCODE -ne 0) { throw 'npm install failed' }
        }
        if (-not (Test-Path $bundle)) {
            Step 'Building the Excalidraw bundle'
            node build-whiteboard.js
            if ($LASTEXITCODE -ne 0) { throw 'build-whiteboard.js failed' }
        }
    } finally { Pop-Location }
}

$wb = "$proj\assets\whiteboard"
New-Item -ItemType Directory -Force $wb | Out-Null
Copy-Item $mermaid "$proj\assets\mermaid.min.js" -Force
Copy-Item "$root\src\whiteboard.bundle.js", "$root\src\whiteboard.bundle.css" $wb -Force
if (Test-Path "$root\src\excalidraw-fonts") {
    if (Test-Path "$wb\excalidraw-fonts") { Remove-Item "$wb\excalidraw-fonts" -Recurse -Force }
    Copy-Item "$root\src\excalidraw-fonts" "$wb\excalidraw-fonts" -Recurse
}
foreach ($f in 'assets\mermaid.min.js', 'assets\whiteboard\whiteboard.bundle.js', 'assets\whiteboard\excalidraw-fonts') {
    if (-not (Test-Path "$proj\$f")) { throw "Expected asset is missing: $f" }
}
Ok 'Mermaid and whiteboard assets restored'

Write-Host ''
Write-Host 'Setup complete. Build and run with:' -ForegroundColor Green
Write-Host '    cargo build'
Write-Host '    cargo run'
