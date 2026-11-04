<#
.SYNOPSIS
  Build the Windows release and assemble the folder to distribute (dist\NotePP).

.DESCRIPTION
  Builds the wxWidgets shell (wx\) in release mode and lays out:
    notepp.exe     the app (the updater and the installer look for this name)
    whiteboard\    the Excalidraw page
  The exe links its C++ runtime statically and has the WebView2 loader built in, so nothing else is needed
  beside it. The script checks the exe's imports to make sure of that.
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

# Every DLL the exe imports must be a Windows system DLL: none is shipped beside it.
$system = 'kernel32|user32|gdi32|gdiplus|advapi32|shell32|shlwapi|ole32|oleaut32|oleacc|comctl32|comdlg32|dwmapi|msimg32|imm32|ws2_32|userenv|ntdll|bcrypt|bcryptprimitives|winspool|wtsapi32|winmm|version|crypt32|secur32|ncrypt|combase|propsys|rpcrt4|uxtheme'
$objdump = Get-Command objdump -ErrorAction SilentlyContinue
if (-not $objdump) { throw 'objdump not found (it ships with MinGW): cannot check the exe imports' }
$imports = & $objdump.Source -p (Join-Path $dist 'notepp.exe') | Select-String 'DLL Name:' | ForEach-Object { ($_ -split ':')[1].Trim() } | Sort-Object -Unique
foreach ($dll in $imports) {
    if ($dll -match "^($system)\.(dll|drv)$" -or $dll -match '^api-ms-win-') { continue }
    throw "notepp.exe needs $dll, which is neither a Windows system DLL nor in the package"
}
Write-Host 'Import check passed: nothing is loaded from outside System32' -ForegroundColor Green
Write-Host "Package ready: $dist" -ForegroundColor Green
