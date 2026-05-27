# Windows Code Signing (self-signed)

Note++ Windows installers are signed in CI with a self-signed code-signing
certificate. This document explains the one-time setup, how CI consumes the
cert, and what users see.

> Self-signed certs **do not** bypass SmartScreen — the cert chain doesn't
> reach a trusted CA, so Windows still warns "unrecognized app". What signing
> does buy you:
>
> - Installer shows **Yogesh Prajapati** as publisher instead of "Unknown
>   publisher".
> - Artifacts are tamper-evident — modification breaks the signature.
> - Users who manually trust the public `.cer` (one-time import into Trusted
>   Publishers) get zero SmartScreen prompts on future installs.
> - When we eventually buy a real OV/EV cert, the wiring is already in place.

---

## Files in this repo

| Path | Role | Committed? |
|---|---|---|
| `scripts/generate-signing-cert.ps1` | Creates a new self-signed cert, exports PFX + CER, prints CI secret values | Yes |
| `build/Notepp-CodeSigning.cer` | Public certificate. Users can import this into Trusted Publishers to silence SmartScreen | Yes (after first run) |
| `.signing/Notepp-CodeSigning.pfx` | **Private** key. Generated locally, never committed | No (`.gitignore`) |

## One-time setup

1. From a Windows PowerShell prompt at the repo root:

   ```powershell
   pwsh -File scripts/generate-signing-cert.ps1
   ```

   The script prompts for a PFX password, generates a 5-year SHA-256
   code-signing cert in `Cert:\CurrentUser\My`, exports both the PFX and CER
   to `.signing/`, and copies the public CER into `build/`.

2. The script prints two values when it finishes. Add both as GitHub Actions
   secrets at <https://github.com/YogeshPraj/Note-/settings/secrets/actions>:

   | Secret name | Value |
   |---|---|
   | `WIN_CSC_LINK_BASE64` | Base64 of the PFX (printed by the script) |
   | `WIN_CSC_KEY_PASSWORD` | The PFX password you chose |

3. Commit the public cert and push:

   ```bash
   git add build/Notepp-CodeSigning.cer
   git commit -m "build(signing): add public code-signing certificate"
   git push
   ```

4. Tag a release (`vX.Y.Z`). The Windows runner decodes the PFX, electron-builder
   sees `CSC_LINK` + `CSC_KEY_PASSWORD`, and signs `notepp-win-x64.exe`
   automatically.

## How CI consumes the cert

`.github/workflows/release.yml`, Windows job:

1. `Decode Windows signing certificate` step writes the base64 secret to a
   `.pfx` under `$RUNNER_TEMP` and exports its path as `CSC_LINK` via
   `$GITHUB_ENV`.
2. The `electron-builder` step picks up `CSC_LINK` + `CSC_KEY_PASSWORD`
   automatically — no `package.json` change required.
3. If `WIN_CSC_LINK_BASE64` isn't set, the decode step is skipped and
   electron-builder ships an unsigned build. The workflow stays green.

macOS and Linux runners ignore these secrets — see "Why not Mac/Linux?" below.

## Verifying a signed build

After downloading `notepp-win-x64.exe` from a release:

```powershell
Get-AuthenticodeSignature .\notepp-win-x64.exe
```

`Status` should be `Valid`, signer should be `CN=Yogesh Prajapati`. (Windows
will list it as `UnknownError` if the cert isn't yet in any trust store on
the verifying machine — that's expected for self-signed.)

## Users: silencing SmartScreen permanently (optional)

A motivated user can pre-trust the cert once:

1. Download `Notepp-CodeSigning.cer` from the repo's `build/` folder.
2. Double-click → **Install Certificate** → **Local Machine** → **Place all
   certificates in the following store** → **Trusted Publishers** → Finish.
3. From then on, every signed Note++ installer launches without the
   "Windows protected your PC" dialog.

Document this in the GitHub release notes if you want users to discover it.

## Rotating the cert

The cert is valid for 5 years. To rotate:

1. Re-run `scripts/generate-signing-cert.ps1` (it creates a brand-new cert).
2. Update both GitHub secrets with the new values it prints.
3. Replace `build/Notepp-CodeSigning.cer` with the new public cert and
   commit.
4. Note: `electron-updater` verifies that an update's signing publisher
   matches the currently-installed publisher. As long as the new cert has
   the same `CN`, auto-updates from old installs keep working.

## Why not Mac/Linux?

- **macOS** — Gatekeeper validates against an Apple-issued Developer ID cert
  chain. A self-signed cert is no better than no cert; both produce the same
  "unidentified developer" prompt. Mac signing requires a paid Apple
  Developer account ($99/yr) + notarization.
- **Linux** — `.deb` and AppImage don't have a desktop-level signing model
  comparable to Authenticode. The closest equivalent is a detached GPG
  signature alongside the artifact, which we can add later if there's
  demand.
