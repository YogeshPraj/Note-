<#
.SYNOPSIS
  Build a release and assemble a self-contained folder (dist\NotePP) to distribute.

.DESCRIPTION
  The exe restricts DLL loading to System32 and its own folder (a defence against DLL
  planting), so every library it needs must sit beside it:
    sci_probe.exe, Scintilla.dll, WebView2Loader.dll, whiteboard\ (the Excalidraw page).
  Run scripts\setup.ps1 first. The script fails if a required file is missing, and
  checks the exe's imports so no stray MinGW runtime DLL is needed.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\package.ps1
#>
$ErrorActionPreference = 'Stop'
$proj = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$dist = Join-Path $proj 'dist\NotePP'

Push-Location $proj
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'cargo build --release failed' }
} finally { Pop-Location }

$rel = Join-Path $proj 'target\release'
$loader = Get-ChildItem (Join-Path $rel 'build') -Recurse -Filter WebView2Loader.dll -ErrorAction SilentlyContinue |
    Where-Object { $_.Directory.Name -eq 'x64' } | Select-Object -First 1
$parts = @{
    'sci_probe.exe'       = Join-Path $rel 'sci_probe.exe'
    'Scintilla.dll'       = Join-Path $proj 'scintilla\bin\Scintilla.dll'
    'WebView2Loader.dll'  = if ($loader) { $loader.FullName } else { Join-Path $rel 'WebView2Loader.dll' }
}
foreach ($k in $parts.Keys) { if (-not (Test-Path $parts[$k])) { throw "Missing $k ($($parts[$k]))" } }
$wbSrc = Join-Path $proj 'assets\whiteboard'
if (-not (Test-Path (Join-Path $wbSrc 'whiteboard.html'))) { throw 'assets\whiteboard is missing - run scripts\setup.ps1' }

if (Test-Path $dist) { Remove-Item $dist -Recurse -Force }
New-Item -ItemType Directory $dist | Out-Null
foreach ($k in $parts.Keys) { Copy-Item $parts[$k] (Join-Path $dist $k) }
Copy-Item $wbSrc (Join-Path $dist 'whiteboard') -Recurse

# Every non-system DLL the exe or Scintilla imports must be in the folder.
$system = 'kernel32|user32|gdi32|gdiplus|advapi32|shell32|shlwapi|ole32|oleaut32|comctl32|comdlg32|dwmapi|msimg32|imm32|ws2_32|userenv|ntdll|bcryptprimitives|combase|propsys|rpcrt4|uxtheme|api-ms-win-.*'
$objdump = Get-Command objdump -ErrorAction SilentlyContinue
if ($objdump) {
    foreach ($bin in 'sci_probe.exe', 'Scintilla.dll') {
        $imports = & $objdump.Source -p (Join-Path $dist $bin) | Select-String 'DLL Name:' | ForEach-Object { ($_ -split ':')[1].Trim() } | Sort-Object -Unique
        foreach ($dll in $imports) {
            if ($dll -match "^($system)\.dll$" -or $dll -match '^api-ms-win-') { continue }
            if (-not (Test-Path (Join-Path $dist $dll))) { throw "$bin needs $dll, which is neither a Windows system DLL nor in the package" }
        }
    }
    Write-Host 'Import check passed: nothing is loaded from outside System32 or the app folder' -ForegroundColor Green
} else {
    Write-Warning 'objdump not found: skipped the DLL import check'
}
# The visual-styles manifest must have made it into the exe (modern controls and menus).
$exeText = [Text.Encoding]::ASCII.GetString([IO.File]::ReadAllBytes((Join-Path $dist 'sci_probe.exe')))
if ($exeText -notmatch 'Microsoft\.Windows\.Common-Controls') { throw 'The exe has no comctl32 v6 manifest' }
Write-Host "Package ready: $dist" -ForegroundColor Green
