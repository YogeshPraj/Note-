# Note++ — Encrypted Pad Feature Spec

> **Status: v1 implementation complete (all 6 phases).**
> See "Implementation Status" at the bottom for what shipped and what's deferred.

## Goal

Let users encrypt note files with strong, password-based encryption so that the file on disk is unreadable without the key. Support an extensible auth model (password today; PIN, Windows Hello, recovery key, etc. later) without breaking existing encrypted files when new methods are added.

## Threat Model

**In scope (what we protect against):**
- Someone with read access to the file on disk (lost laptop, leaked backup, cloud-sync mishap) reading the content
- Brute-force attacks against the password (mitigated by KDF cost)
- File tampering (mitigated by AEAD auth tag)

**Out of scope (not protected):**
- Attacker who has unlocked Note++ running on the user's machine (in-memory keys are visible)
- Coercion / shoulder-surfing the password
- Plausible deniability that an encrypted file exists (the JSON envelope is self-describing)
- Side-channel attacks
- Whiteboard / non-text tabs (text only for v1)

## Architecture: Envelope Encryption with Multiple Auth Methods

Standard pattern used by BitLocker, FileVault, LUKS, 1Password, Age:

```
                   ┌──────────────────────────────┐
                   │ Data Encryption Key (DEK)    │ ◄─── encrypts every file
                   │ random 256-bit AES key       │
                   └─────────────┬────────────────┘
                                 │
                  wrapped by one or more KEKs
                                 │
            ┌────────────────────┼────────────────────┐
            │                    │                    │
    ┌───────▼────────┐  ┌────────▼─────────┐  ┌───────▼────────┐
    │ KEK_password   │  │ KEK_recovery     │  │ KEK_hello (v2) │
    │ PBKDF2(pw)     │  │ HKDF(recoveryKey)│  │ Windows Hello  │
    └────────────────┘  └──────────────────┘  └────────────────┘
```

**Consequences:**
- One unlock unlocks all encrypted files in the profile
- Change password = re-wrap DEK; files don't need re-encryption
- Add new auth method = append a new wrapped-key entry; old files still work
- Recovery key = just another KEK that wraps the same DEK

## Crypto Primitives (locked)

| Component | Choice | Notes |
|---|---|---|
| Symmetric cipher | **AES-256-GCM** | Authenticated; built into Web Crypto API |
| Password KDF | **PBKDF2-HMAC-SHA-256, 600 000 iterations** | OWASP 2023 minimum |
| Recovery key KDF | **HKDF-SHA-256** | Recovery key is already 256-bit, no PBKDF2 needed |
| Random source | `crypto.getRandomValues()` | Native, CSPRNG |
| Salt size | 16 bytes per wrapped key | Stored in profile, not secret |
| IV / nonce size | 12 bytes per encryption | Stored alongside ciphertext |
| Compression | **gzip** before encryption | Big size savings for typical text/code |

All available in Electron's renderer via `window.crypto.subtle` — no extra dependencies needed. Compression via Node `zlib` over IPC, or pako in renderer.

## File Format

### Encrypted note file (`.md`, `.txt`, etc. — extension unchanged)

The entire on-disk file content IS this JSON:

```json
{
  "_notepp_encrypted": true,
  "version": 1,
  "alg": "AES-256-GCM",
  "compression": "gzip",
  "profile": "5f4dcc3b",
  "iv":  "<base64, 12 bytes>",
  "ct":  "<base64, ciphertext + 16-byte auth tag>",
  "originalExt": "md",
  "createdAt": "2026-05-13T20:30:00Z"
}
```

- `profile` is the fingerprint of the encryption profile that owns the DEK (first 8 hex chars of SHA-256(DEK))
- `originalExt` lets us restore syntax highlighting on open
- Detection on open: `JSON.parse()` first N KB → check `_notepp_encrypted === true`

### Encryption Profile (`%AppData%\notepp\encryption\profile.json`)

```json
{
  "_notepp_profile": true,
  "version": 1,
  "fingerprint": "5f4dcc3b",
  "createdAt": "2026-05-13T20:30:00Z",
  "wrappedKeys": [
    {
      "method": "password",
      "kdf": "PBKDF2-SHA256",
      "iter": 600000,
      "salt": "<base64, 16 bytes>",
      "wrappedDek": "<base64>",
      "wrapIv": "<base64, 12 bytes>"
    },
    {
      "method": "recovery",
      "kdf": "HKDF-SHA256",
      "salt": "<base64>",
      "wrappedDek": "<base64>",
      "wrapIv": "<base64>"
    }
  ]
}
```

- `wrappedKeys` is an array → trivially extensible for future PIN, Hello, YubiKey, etc.
- Plaintext DEK is **never** written to disk
- Profile file is small and safe to sync via cloud-sync

### Recovery Output (shown to user once, never stored)

**Human-readable code (always shown):**

```
Note++ Recovery Key
Profile fingerprint: 5f4d-cc3b
=========================================
ABCD-EFGH-IJKL-MNOP-QRST-UVWX-YZ23-4567
=========================================

⚠ Anyone with this code can decrypt your protected notes.
   Store it in a password manager or fireproof safe.
   Note++ cannot generate a new one without your password.
```

256-bit random value, Crockford base32, dash-grouped for readability (~32 chars).

**Optional downloadable JSON (`recovery.json`):**

```json
{
  "_notepp_recovery": true,
  "version": 1,
  "fingerprint": "5f4dcc3b",
  "createdAt": "2026-05-13T20:30:00Z",
  "recoveryKey": "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
}
```

User chooses: copy the printed code, save the JSON, or both.

## File Size Analysis

| Plaintext | After gzip | After encrypt + base64 | Net change |
|---|---|---|---|
| 100 KB markdown | 25 KB | 34 KB | **−66%** |
| 1 MB source code | 200 KB | 272 KB | **−73%** |
| 10 MB log | 1 MB | 1.36 MB | **−86%** |
| 100 KB random/binary | 95 KB | 129 KB | +29% |

Practical ceiling: **~50 MB plaintext**, governed by Monaco's own large-file limits, not the encryption pipeline. Chunked format for huge files is deferred to v2.

## UX Flows

### Settings → Encryption page (new prefs panel)

```
Encryption Profile      ⚪ Disabled    ●  Enabled

Authentication methods:
  ☑ Password (required)            [ Change password ]
  ☐ PIN (v2 — needs Windows Hello)
  ☐ Windows Hello (v2)

Recovery key:                      [ Show recovery key ]
                                   [ Download recovery.json ]
                                   [ Regenerate (requires password) ]

Profile fingerprint: 5f4d-cc3b
Created: 2026-05-13 20:30:00

Session:                           [ 🔓 Unlocked — Lock now ]
                                   or
                                   [ 🔒 Locked    — Unlock… ]
```

### Per-document UX

- Toolbar 🔒 button on any tab → "Encrypt this file" (uses the active profile, no per-file password)
- 🔒 badge prepended to tab name when encrypted
- Status bar shows `🔒 Protected` pill (clickable to show profile fingerprint)
- Save → routes through encrypt pipeline automatically; no extra prompt

### First-time setup (Enable Encryption)

1. User clicks Enable → password dialog
2. Password strength meter + "type again to confirm"
3. Big red warning: **"If you forget your password AND lose your recovery key, your encrypted notes are gone forever. Note++ has no backdoor."**
4. Generate DEK, generate recovery key, wrap DEK with both
5. Show recovery key — user must tick **"I've saved my recovery key"** before continuing
6. Optional [Download recovery.json] button
7. Write `profile.json`. DEK stays in memory → session is unlocked

### Encrypting an existing file

1. Tab is open with plaintext content
2. Click 🔒 on toolbar → confirm dialog "Encrypt this file using the active Note++ profile?"
3. If profile not yet enabled → walk through first-time setup flow first
4. If profile locked → unlock dialog first
5. On confirm: set `tab.encrypted = true`, mark dirty, next save writes encrypted envelope

### Opening an encrypted file

1. Read file → `JSON.parse()` → see `_notepp_encrypted: true`
2. Look up `profile` fingerprint
   - **Match + DEK in memory:** decrypt immediately, no prompt
   - **Match but locked:** show Unlock dialog (password / recovery)
   - **No profile installed:** "This file was encrypted with profile XXXX. Restore the profile or use the recovery key."
   - **Different profile fingerprint:** "This file belongs to a different profile (XXXX). You can't unlock it with the current profile."
3. After unlock: DEK cached on `app.encryption.dek` until app quits or "Lock now" clicked
4. Decrypt → ungzip → set Monaco model content; tab marked `encrypted = true` so save re-encrypts

### Change password

1. Settings → Change password
2. Prompt: old password
3. Prompt: new password (+ confirm + strength meter)
4. Decrypt DEK with old KEK → wrap with new KEK → replace `wrappedKeys[password]` entry in profile
5. Existing encrypted files **don't need to be touched** — the DEK is unchanged

### Lost password, have recovery key

1. Settings → "Use recovery key" link on locked screen, or first-launch when profile missing
2. User pastes the 32-char code or uploads `recovery.json`
3. Decrypt DEK with recovery KEK
4. Force-set a new password → wrap DEK with new password KEK → save profile
5. Optionally regenerate a fresh recovery key (replacing the used one)

### Lost password AND lost recovery key

No-op. Files are unrecoverable. Settings page shows: "Files encrypted with this profile cannot be recovered. Create a new profile to encrypt new files (existing encrypted files will remain unreadable)."

## Locked Decisions

| Question | Answer |
|---|---|
| Per-document vs. settings-based | **Hybrid:** settings-based profile + per-file opt-in lock toggle |
| PIN in v1 | **Skip.** Only safe with Windows Hello binding (v2) |
| Face scan in v1 | **Skip.** Requires Windows Hello / OS biometrics (v2) |
| Recovery format | **Both:** human-readable code + optional `recovery.json` download |
| Multiple profiles per install | **No.** One profile in v1; multi-profile is a v2 feature |
| File extension on encrypted save | **Keep original** (`notes.md` stays `.md`). JSON header is reliable for detection |
| Where to store the marker | **UI only** (tab badge, status bar). No `Security Key` block in document body |
| Compression | **gzip before encrypt.** Saves 60–80% size on typical text/code |
| Password retention | **Keep DEK in memory until app close or "Lock now".** No per-save re-prompt |
| Auto-save with encryption | **Yes**, cached DEK makes this ~5 ms |
| Backup interaction | **Always backup encrypted form**, never plaintext |
| Find-in-files | **v1: skip encrypted files.** Decrypt-on-search is v2 |
| Whiteboard / non-text tabs | **v1: text only.** Whiteboard encryption deferred |
| Profile lost but files exist | **Recovery flow** (paste recovery key → rebuild profile) |
| Wrong password | **Toast + retry.** No lockout (local file, brute-force is the threat, not interactive attempts) |

## Implementation Plan

### Phase 1 — Crypto core (no UI)

- [ ] `src/crypto.js` new module — pure functions, no DOM:
  - `randomBytes(n)`, `b64encode`, `b64decode`
  - `pbkdf2DeriveKey(password, salt, iter)` → AES-GCM CryptoKey
  - `hkdfDeriveKey(rawKey, salt, info)` → AES-GCM CryptoKey
  - `aesGcmEncrypt(key, plaintext)` → `{iv, ct}`
  - `aesGcmDecrypt(key, iv, ct)` → plaintext bytes
  - `gzipCompress(buf)`, `gzipDecompress(buf)` (via Node `zlib` IPC or pako)
  - `wrapDek(dek, kek)` / `unwrapDek(wrapped, kek)`
  - `fingerprint(dek)` — first 8 hex chars of SHA-256(rawDek)
  - `generateRecoveryKey()` — 256 bits → base32 string
- [ ] Add `electronAPI.gzip` / `gunzip` IPC handlers in `main.js` + `preload.js` (or import pako)

### Phase 2 — Profile manager (no UI yet)

- [ ] `src/encryption-profile.js` (or section of renderer.js):
  - `loadProfile()` — read `%AppData%\notepp\encryption\profile.json`
  - `saveProfile(profile)` — atomic write
  - `createProfile(password)` — generates DEK + wraps with password + recovery key
  - `addWrappedKey(profile, method, kek)` — appends new auth method
  - `unlockWithPassword(profile, password)` — returns DEK
  - `unlockWithRecoveryKey(profile, recoveryKey)` — returns DEK
  - `changePassword(profile, oldPw, newPw)`
- [ ] IPC for the profile file (or just `electronAPI.readFile` / `writeFile` with the known path)
- [ ] In-memory state: `app.encryption = { profile, dek, isUnlocked }`

### Phase 3 — File I/O integration

- [ ] `openFile()` extension: detect `_notepp_encrypted: true` → route through unlock + decrypt
- [ ] `saveTabFile()` extension: if `tab.encrypted` → gzip → encrypt → wrap in envelope → write
- [ ] Tab fields: `tab.encrypted: boolean`, `tab.protectedBy: fingerprint`
- [ ] Auto-save path also goes through encryption pipeline
- [ ] Reload-from-disk also goes through decryption

### Phase 4 — UI: Encryption settings page

- [ ] New `prefs-item` "🔒 Encryption" in `index.html`
- [ ] Pref page with: Enable toggle, Change password btn, Show/Download recovery btn, Lock/Unlock btn, fingerprint display
- [ ] Modals:
  - First-time setup wizard (password → confirm → recovery display → confirm-saved checkbox)
  - Unlock dialog
  - Change password dialog
  - Show recovery key dialog
  - Recovery-flow dialog (paste code → set new password)

### Phase 5 — UI: per-document

- [ ] Toolbar 🔒 button (`data-action="encrypt-toggle"`)
- [ ] 🔒 prefix in tab name (similar to existing `wb` pill badge for whiteboard)
- [ ] Status bar `🔒 Protected` indicator (click → show fingerprint)
- [ ] Dark mode styles for all new dialogs

### Phase 6 — Edge cases & polish

- [ ] Profile file missing on startup → grey out encryption UI; existing encrypted files trigger recovery flow on open
- [ ] Wrong-profile file detection (`profile` fingerprint mismatch)
- [ ] "Lock now" clears `app.encryption.dek` from memory
- [ ] Session restore: encrypted files re-prompt for unlock (don't restore decrypted content from `session.json`)
- [ ] Find-in-files: skip files with `_notepp_encrypted: true` (cheap detection without decrypt)
- [ ] Strength meter for password input (zxcvbn, or simple length-based)

## Out of Scope (Deferred to v2+)

- PIN authentication (needs Windows Hello binding)
- Windows Hello / Touch ID / biometric authentication
- YubiKey / hardware key support
- Multiple profiles per install (e.g., shared computers)
- Encrypted whiteboard tabs
- Chunked file format for >50 MB files
- Find-in-files for encrypted documents
- Encrypted session restore (decrypted in-memory content survives restart)
- Plausible-deniability mode (no metadata leak on disk)
- Key escrow / corporate recovery

## Open Items to Discuss Before Coding

(Nothing blocking — listed for completeness in case anything surfaces during implementation:)

1. Where to put `crypto.js` — separate module or section in `renderer.js`?
2. Compression via Node `zlib` IPC (~zero deps) vs. pako (in renderer, no IPC)?
3. Should "Encrypt this file" on a tab tied to a real disk path (not yet encrypted) require a save-as flow, or overwrite the existing plaintext file?
4. Should `tab.dirty` flag flip the moment a user toggles encryption, even before edits?
5. Recovery key character set — Crockford base32 (no ambiguous 0/O/1/I/L) is my preference; confirm OK?

## Implementation Status (v1 shipped)

| Phase | What landed | Files touched |
|---|---|---|
| 1. Crypto core | `src/crypto.js` — pure module exposing `window.NotePPCrypto`. AES-256-GCM, PBKDF2 (600k iter), HKDF, gzip via native `CompressionStream`, base64, Crockford base32, key wrap/unwrap. **No new dependencies.** | `src/crypto.js`, `src/index.html` |
| 2. Profile manager | `appEnc` state + `loadEncryptionProfile`/`saveEncryptionProfile`/`createEncryptionProfile`/`unlockEncryptionWithPassword`/`unlockEncryptionWithRecoveryKey`/`changeEncryptionPassword`/`regenerateEncryptionRecoveryKey`/`lockEncryption`. Profile lives at `%AppData%\notepp\encryption\profile.json`. Loaded once at startup. | `src/renderer.js` |
| 3. File I/O integration | `openFile()` detects `_notepp_encrypted: true` → routes to `openEncryptedFile()` which validates fingerprint, prompts unlock, decrypts, creates tab with `tab.encrypted = true`. `saveTabFile()` encrypts → gzip → AES-GCM → JSON envelope when `tab.encrypted`. `toggleTabEncryption()` flips the flag. `originalExt` preserved across encrypt/decrypt for syntax highlighting. | `src/renderer.js` |
| 4. Settings UI | New "🔒 Encryption" prefs page with status / setup / unlock / lock / change-password / regenerate-recovery / restore-via-recovery. Five new modals (setup, unlock, change-password, recovery-display, recovery-entry). All wired up. | `src/index.html`, `src/renderer.js` |
| 5. Per-document UI | 🔒 toolbar button (active when current tab is encrypted), 🔒 badge on tab name, 🔒 status-bar indicator (click to lock/unlock). | `src/index.html`, `src/renderer.js`, `src/style.css` |
| 6. Edge cases | Session restore re-routes encrypted tabs through `openFile()` so they re-prompt unlock (never persists plaintext). Auto-backup of an encrypted tab copies the encrypted form on disk, never the plaintext. `detectEncrypted()` is fast-path (string-search before full JSON parse). | `src/renderer.js` |

### Remaining open items (not blocking)

- Find-in-files: UI tab exists but no implementation in `renderer.js` — when implemented, it should skip files where `detectEncrypted()` returns truthy.
- Password strength meter (`enc-setup-strength`) is wired into the DOM but has no logic yet — could plug in a simple length/entropy check or zxcvbn.
- Dark-mode polish on the warning banners (`#fff4e6` background) — currently fine but could be themed.

### Deferred to v2 (per spec)

PIN auth, Windows Hello, biometric methods, multi-profile, encrypted whiteboard tabs, chunked >50 MB format, decrypt-during-find-in-files, no-metadata "binary" mode, key escrow.

### Testing checklist for next session

1. Open Preferences → 🔒 Encryption → "Set up encryption…" → enter password twice → should display recovery key dialog.
2. Copy + Save recovery.json → verify file written.
3. Tick "I've saved my recovery key" → Done.
4. Open any text file → click 🔒 toolbar → "Encrypt" → save → reopen file in another text editor; should be the JSON envelope.
5. Close and reopen Note++ → file appears in session → unlock prompt → enter password → file decrypts and opens.
6. Click 🔒 in status bar to lock; click again → unlock prompt.
7. Settings → Change password → enter old + new × 2 → done.
8. Settings → New recovery key → confirm warning → new key displayed.
9. Lock the profile, then "Reset using recovery key" → paste old recovery key → set new password → unlocks.
