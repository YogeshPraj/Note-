# JWT Decoder

A built-in decoder for **JSON Web Tokens** (JWT). Open it from **Tools → JWT Decoder…** or
the command palette (`JWT Decoder`). If the current editor selection looks like a token it is
pre-filled automatically.

> **Decode, not decrypt.** A standard JWT (JWS) is *signed*, not encrypted — its header and
> payload are Base64URL-encoded JSON that anyone can read. This tool decodes them locally and
> can optionally *verify* an HMAC signature. Truly encrypted tokens (JWE, 5 segments) can't be
> decoded without the decryption key, and the tool says so instead of failing silently.

## What it does

- **Decode** — splits `header.payload.signature`, Base64URL-decodes the header and payload,
  and pretty-prints the JSON. Live-updates as you type. A leading `Bearer ` prefix is stripped.
- **Humanized claims** — surfaces the common registered claims with friendly labels:
  - `iss` (issuer), `sub` (subject), `aud` (audience)
  - `iat` (issued at), `nbf` (not before), `exp` (expires) — each Unix timestamp is shown as a
    local date/time
  - `exp` gets a **✓ valid / ✗ EXPIRED** badge; `nbf` warns if the token isn't valid yet
  - the header `alg` is shown alongside
- **Signature verification (optional)** — for `HS256` / `HS384` / `HS512`, enter the shared
  secret and click **Verify Signature**. Uses the Web Crypto `HMAC` primitive to recompute the
  signature and compares it constant-length against the token's own. Reports
  **✓ valid** or **✗ does NOT match**. `RS*` / `ES*` (asymmetric) aren't verified here — they
  need a public key.
- **Copy Payload** — copies the decoded payload JSON to the clipboard.
- **Load from Editor** — pulls the current selection (or whole document) into the decoder.

## Safety / privacy

- 100% local. Nothing is sent anywhere — decoding and HMAC verification run in the renderer via
  standard browser APIs (`atob`, `TextDecoder`, `crypto.subtle`).
- No new dependencies.

## UI

- Docked **side panel**: `#jwt-panel` (with `#jwt-resize-handle`) inside `#editor-preview-row`
  in `src/index.html` — opens on the right like the Mermaid/Preview panel, not a modal popup.
  Drag the handle to resize; press **Esc** or the header **✕** to close.
- Logic: `decodeJwt()`, `verifyJwtSignature()`, `openJwtDecoder()` / `closeJwtPanel()` in
  `src/renderer.js` (near the other developer tools).
- Wired into the Tools menu (`menu-jwt-decoder`), the command palette, and the `m()` handler
  block, mirroring the Regex Tester.

## Error handling

- Wrong number of segments → clear "not a valid JWT" message.
- 5 segments → detected as **JWE (encrypted)**; explains it can't be decoded without the key.
- Malformed Base64URL or non-JSON header/payload → the offending part is reported without
  crashing the panel.
- Unsigned token (no signature segment) → noted; verification is skipped.
