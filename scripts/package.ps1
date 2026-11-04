<#
.SYNOPSIS
  Build the Windows release and assemble the folder to distribute (dist\NotePP).

.DESCRIPTION
  Builds the wxWidgets shell (wx\) in release mode and lays out:
    notepp.exe          the app (the updater and the installer look for this name)
    WebView2Loader.dll  Microsoft's loader: wxWidgets loads it when the program runs, and without it every
                        web view falls back to Internet Explorer
    whiteboard\         the Excalidraw page
  The exe links its C++ runtime statically, so nothing else is needed beside it. The script checks the exe's
  imports to make sure of that, and that assets\app.manifest pins each one Windows does not protect itself.
  On a build server (CI is set) it also starts the package with a minimal PATH and checks which engine loads.
  Run  node scripts\setup-assets.js  first. Needs the toolchain described in wx\README.md.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\package.ps1
#>
$ErrorActionPreference = 'Stop'
$proj = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$dist = Join-Path $proj 'dist\NotePP'

Push-Location (Join-Path $proj 'wx')
try {
    cargo build -p notepp-wx --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'cargo build --release failed' }
} finally { Pop-Location }

$exe = Join-Path $proj 'wx\target\release\notepp-wx.exe'
$wbSrc = Join-Path $proj 'assets\whiteboard'
if (-not (Test-Path $exe)) { throw "Missing $exe" }
if (-not (Test-Path (Join-Path $wbSrc 'whiteboard.bundle.js'))) { throw 'assets\whiteboard is incomplete - run: node scripts\setup-assets.js' }

if (Test-Path $dist) { Remove-Item $dist -Recurse -Force }
New-Item -ItemType Directory $dist | Out-Null
Copy-Item $exe (Join-Path $dist 'notepp.exe')
Copy-Item $wbSrc (Join-Path $dist 'whiteboard') -Recurse

$loader = Join-Path $proj 'wx\target\release\WebView2Loader.dll'
if (-not (Test-Path $loader)) { throw "Missing $loader" }
$signature = Get-AuthenticodeSignature $loader
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'CN=Microsoft Corporation') { throw "$loader is not signed by Microsoft" }
Copy-Item $loader (Join-Path $dist 'WebView2Loader.dll')

# Every DLL the exe imports must be a Windows system DLL: the only one shipped beside it is the loader, which is
# not imported.
$system = 'kernel32|user32|gdi32|gdiplus|advapi32|shell32|shlwapi|ole32|oleaut32|oleacc|comctl32|comdlg32|dwmapi|msimg32|imm32|ws2_32|userenv|ntdll|bcrypt|bcryptprimitives|winspool|wtsapi32|winmm|version|crypt32|secur32|ncrypt|combase|propsys|rpcrt4|uxtheme|opengl32'
# What Windows loads from System32 by itself: its known DLLs, and comctl32, which the manifest picks by version.
$protected = 'kernel32|user32|gdi32|gdiplus|advapi32|shell32|shlwapi|ole32|oleaut32|comdlg32|ws2_32|ntdll|comctl32'
$pinned = [regex]::Matches((Get-Content (Join-Path $proj 'assets\app.manifest') -Raw), '<file name="([^"]+)"') | ForEach-Object { $_.Groups[1].Value.ToLower() }
$objdump = Get-Command objdump -ErrorAction SilentlyContinue
if (-not $objdump) { throw 'objdump not found (it ships with MinGW): cannot check the exe imports' }
$imports = & $objdump.Source -p (Join-Path $dist 'notepp.exe') | Select-String 'DLL Name:' | ForEach-Object { ($_ -split ':')[1].Trim() } | Sort-Object -Unique
foreach ($dll in $imports) {
    if ($dll -match '^api-ms-win-') { continue }
    if ($dll -notmatch "^($system)\.(dll|drv)$") { throw "notepp.exe needs $dll, which is neither a Windows system DLL nor in the package" }
    if ($dll -notmatch "^($protected)\.dll$" -and $pinned -notcontains $dll.ToLower()) { throw "assets\app.manifest does not pin $dll to System32" }
}
Write-Host 'Import check passed: nothing is loaded from outside System32' -ForegroundColor Green

if ($env:CI) {
    # What a clean machine gets: nothing on PATH that could supply the loader.
    $start = [Diagnostics.ProcessStartInfo]::new((Join-Path $dist 'notepp.exe'))
    $start.UseShellExecute = $false
    $start.ArgumentList.Add('--new-instance')
    $start.Environment['PATH'] = "$env:SystemRoot\System32;$env:SystemRoot"
    $start.Environment['APPDATA'] = Join-Path $env:RUNNER_TEMP 'notepp-check'
    $app = [Diagnostics.Process]::Start($start)
    Start-Sleep 8
    $modules = (Get-Process -Id $app.Id).Modules.FileName
    Stop-Process -Id $app.Id -Force
    if ($modules -notcontains (Join-Path $dist 'WebView2Loader.dll')) { throw 'The package does not load its WebView2 loader' }
    if ($modules -match 'ieframe|mshtml') { throw 'The package fell back to Internet Explorer' }
    Write-Host 'Clean start passed: WebView2' -ForegroundColor Green
}
Write-Host "Package ready: $dist" -ForegroundColor Green
