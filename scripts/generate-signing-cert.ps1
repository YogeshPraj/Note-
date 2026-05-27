# ---------------------------------------------------------------------------
# generate-signing-cert.ps1
#
# Creates a self-signed code-signing certificate for Note++, exports it as
# a password-protected PFX (private key, NEVER commit) and a CER (public,
# safe to commit), and prints the values you need to upload as GitHub
# Actions secrets so CI can sign Windows installers automatically.
#
# Run from PowerShell (no admin required for CurrentUser store):
#   pwsh -File scripts/generate-signing-cert.ps1
#
# Optional parameters:
#   -Subject       Common name on the cert. Defaults to "CN=Yogesh Prajapati".
#   -OutDir        Where to write the .pfx + .cer. Defaults to ./.signing/.
#   -ValidYears    Cert validity. Defaults to 5.
#
# After running:
#   1. Commit build/Notepp-CodeSigning.cer (the public cert, copied by
#      this script).
#   2. In the GitHub repo: Settings -> Secrets and variables -> Actions
#      -> New repository secret. Add the two secrets the script prints:
#        WIN_CSC_LINK_BASE64    (base64 of the .pfx)
#        WIN_CSC_KEY_PASSWORD   (the PFX password)
#   3. Tag and push a release. The Windows runner will pick the secrets
#      up automatically via release.yml.
# ---------------------------------------------------------------------------

[CmdletBinding()]
param(
  [string] $Subject = "CN=Yogesh Prajapati",
  [string] $OutDir = (Join-Path $PSScriptRoot "..\.signing"),
  [int]    $ValidYears = 5
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$OutDir   = (Resolve-Path -LiteralPath (New-Item -ItemType Directory -Path $OutDir -Force)).Path
$buildDir = Join-Path $repoRoot "build"
if (-not (Test-Path $buildDir)) { New-Item -ItemType Directory -Path $buildDir | Out-Null }

$pfxPath = Join-Path $OutDir "Notepp-CodeSigning.pfx"
$cerPath = Join-Path $OutDir "Notepp-CodeSigning.cer"
$cerRepo = Join-Path $buildDir "Notepp-CodeSigning.cer"

Write-Host ""
Write-Host "Generating self-signed code-signing certificate" -ForegroundColor Cyan
Write-Host "  Subject     : $Subject"
Write-Host "  Valid for   : $ValidYears years"
Write-Host "  Output dir  : $OutDir"
Write-Host ""

$pwd = Read-Host -Prompt "Choose a PFX password (you will need this for the GitHub secret)" -AsSecureString
if ($pwd.Length -eq 0) { throw "Password cannot be empty." }

$cert = New-SelfSignedCertificate `
  -Type CodeSigningCert `
  -Subject $Subject `
  -KeyUsage DigitalSignature `
  -KeyAlgorithm RSA `
  -KeyLength 3072 `
  -HashAlgorithm SHA256 `
  -CertStoreLocation "Cert:\CurrentUser\My" `
  -NotAfter (Get-Date).AddYears($ValidYears) `
  -FriendlyName "Note++ Code Signing"

Write-Host "Created cert. Thumbprint: $($cert.Thumbprint)" -ForegroundColor Green

Export-PfxCertificate -Cert $cert -FilePath $pfxPath -Password $pwd | Out-Null
Export-Certificate    -Cert $cert -FilePath $cerPath | Out-Null
Copy-Item -Path $cerPath -Destination $cerRepo -Force

Write-Host ""
Write-Host "Wrote:"
Write-Host "  $pfxPath   (private key, DO NOT COMMIT)"
Write-Host "  $cerPath   (public cert)"
Write-Host "  $cerRepo   (public cert, copied into build/ for the repo)"
Write-Host ""

$pfxBytes = [System.IO.File]::ReadAllBytes($pfxPath)
$b64      = [System.Convert]::ToBase64String($pfxBytes)

$pwdPlain = [System.Net.NetworkCredential]::new("", $pwd).Password

Write-Host "---------- GitHub Actions secrets ----------" -ForegroundColor Yellow
Write-Host "Add these two repository secrets at:"
Write-Host "  https://github.com/YogeshPraj/Note-/settings/secrets/actions"
Write-Host ""
Write-Host "Secret name : WIN_CSC_LINK_BASE64"
Write-Host "Secret value:"
Write-Host $b64
Write-Host ""
Write-Host "Secret name : WIN_CSC_KEY_PASSWORD"
Write-Host "Secret value: $pwdPlain"
Write-Host "--------------------------------------------"
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Cyan
Write-Host "  1. Add both secrets in the GitHub repo settings (link above)."
Write-Host "  2. git add build/Notepp-CodeSigning.cer  &&  git commit"
Write-Host "  3. Tag and push a release. The Windows runner will sign automatically."
Write-Host ""
Write-Host "Verify locally (optional):"
Write-Host "  Get-AuthenticodeSignature path\to\notepp-win-x64.exe"
Write-Host ""
