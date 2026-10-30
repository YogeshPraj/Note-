name: Build & Release

on:
  push:
    tags:
      - 'v*'

permissions:
  contents: write
  actions: read

jobs:
  # ───────────────────────────────────────────────────────────────────────────
  # JOB 1 — BUILD + PUBLISH (matrix across Windows + macOS + Linux)
  # Each runner compiles its own native artifacts and publishes them to the
  # GitHub release via `electron-builder --publish always`. electron-updater
  # needs the per-platform manifest files (latest.yml, latest-mac.yml,
  # latest-linux.yml) to live alongside the binaries on the same release —
  # `--publish always` handles both in one step.
  # ───────────────────────────────────────────────────────────────────────────
  build:
    name: Build (${{ matrix.os }})
    strategy:
      fail-fast: false   # one platform's failure shouldn't block the others
      matrix:
        include:
          - os: windows-latest
            target_flag: --win
            artifact_glob: |
              dist/*.exe
              dist/*.zip
              dist/latest.yml
              dist/*.blockmap
          - os: macos-latest
            target_flag: --mac
            artifact_glob: |
              dist/*.dmg
              dist/*.zip
              dist/latest-mac.yml
              dist/*.blockmap
          - os: ubuntu-latest
            target_flag: --linux
            artifact_glob: |
              dist/*.AppImage
              dist/*.deb
              dist/latest-linux.yml
              dist/*.blockmap
    runs-on: ${{ matrix.os }}
    outputs:
      version: ${{ steps.version.outputs.VERSION }}
    # Lift signing secrets into job-level env so step `if:` conditions can
    # reference them via `env.*` — GitHub Actions doesn't allow `secrets.*`
    # in step-level if expressions.
    env:
      WIN_CSC_LINK_BASE64:  ${{ secrets.WIN_CSC_LINK_BASE64 }}
      WIN_CSC_KEY_PASSWORD: ${{ secrets.WIN_CSC_KEY_PASSWORD }}

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Set up Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: 'npm'

      # Linux-only: AppImage builds need libfuse2 on the GH runner image.
      - name: Install libfuse2 (Linux only)
        if: matrix.os == 'ubuntu-latest'
        run: sudo apt-get update && sudo apt-get install -y libfuse2

      # Electron's postinstall downloads a ~100 MB binary from GitHub which
      # flakes on CI runners with "socket hang up" once in a while. Retry
      # up to 3 times with backoff.
      - name: Install dependencies (with retry on network flakes)
        shell: bash
        run: |
          for attempt in 1 2 3; do
            echo "::group::npm ci attempt $attempt"
            if npm ci; then
              echo "::endgroup::"
              echo "npm ci succeeded on attempt $attempt"
              exit 0
            fi
            echo "::endgroup::"
            echo "npm ci failed on attempt $attempt; cleaning and retrying"
            rm -rf node_modules
            sleep $((attempt * 10))
          done
          echo "npm ci failed after 3 attempts"
          exit 1

      - name: Build whiteboard iframe bundle
        run: npm run build:wb

      # Decode the self-signed Windows code-signing PFX out of repo secrets
      # so electron-builder can sign the NSIS installer + .exe. If the
      # secret is missing the step is a no-op and the build ships unsigned
      # — keeps the workflow green before the cert is provisioned.
      # See scripts/generate-signing-cert.ps1 and features/SIGNING.md.
      - name: Decode Windows signing certificate (Windows only)
        if: matrix.os == 'windows-latest' && env.WIN_CSC_LINK_BASE64 != ''
        shell: pwsh
        run: |
          $pfxPath = Join-Path $env:RUNNER_TEMP "notepp-codesign.pfx"
          [System.IO.File]::WriteAllBytes(
            $pfxPath,
            [System.Convert]::FromBase64String($env:WIN_CSC_LINK_BASE64)
          )
          "CSC_LINK=$pfxPath" | Out-File -FilePath $env:GITHUB_ENV -Append -Encoding utf8
          Write-Host "Decoded signing PFX to $pfxPath"

      - name: Build & publish installer (electron-builder publishes to GitHub release)
        run: npx electron-builder ${{ matrix.target_flag }} --publish always
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          # Windows signing: CSC_LINK is set by the decode step above when
          # WIN_CSC_LINK_BASE64 is configured. CSC_KEY_PASSWORD comes
          # straight from the secret. If either is empty, electron-builder
          # silently skips signing (= unsigned build, current behaviour).
          CSC_KEY_PASSWORD: ${{ secrets.WIN_CSC_KEY_PASSWORD }}
          # Skip Mac code-signing in CI — self-signed certs are useless on
          # macOS (Gatekeeper requires an Apple Developer cert + notarization).
          CSC_IDENTITY_AUTO_DISCOVERY: false

      - name: Upload platform artifacts (for the release job)
        uses: actions/upload-artifact@v4
        with:
          name: dist-${{ matrix.os }}
          path: ${{ matrix.artifact_glob }}
          if-no-files-found: warn

      - name: Get version from tag
        id: version
        shell: bash
        run: echo "VERSION=${GITHUB_REF#refs/tags/}" >> $GITHUB_OUTPUT

  # ───────────────────────────────────────────────────────────────────────────
  # JOB 2 — FINALISE RELEASE
  # electron-builder --publish always has already uploaded every artifact.
  # This job's only purpose is to set a friendly title, generate the
  # auto-changelog, and flip the 'Latest' badge.
  #
  # IMPORTANT: we do NOT pass `files:` to softprops. If we did, it would
  # re-upload everything in dist/ with its OWN filename sanitization
  # (which strips '+' chars and bloats the release with duplicates).
  # Electron-builder is the sole authoritative uploader.
  # ───────────────────────────────────────────────────────────────────────────
  release:
    name: Finalise GitHub Release
    needs: build
    runs-on: ubuntu-latest

    steps:
      - name: Finalise release (title + changelog + 'Latest' badge)
        uses: softprops/action-gh-release@v2
        with:
          name: Note++ ${{ needs.build.outputs.VERSION }}
          tag_name: ${{ needs.build.outputs.VERSION }}
          draft: false
          prerelease: false
          make_latest: 'true'
          generate_release_notes: true
          # no `files:` — electron-builder is the sole uploader
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
