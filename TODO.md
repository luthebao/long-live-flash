# Chrome Web Store publishing — outstanding work

Publishing this extension is harder than usual because the Web Store distributes only the extension, not the native host binary — the user has to install that separately. Below is what changes vs. the working local dev setup.

## The blockers (in order)

### 1. Extension ID changes when published

Right now `allowed_origins` in the native manifest is hard-pinned to `mkicablifdfonppdpcggakcmncfkfjbk` — the dev ID derived from the unpacked path. When the Web Store assigns a production ID, the host stops accepting connections from the published extension.

Two ways to handle:

- **Lock the ID before publishing**: pin a public RSA key into `manifest.json` as the `"key"` field. The resulting extension ID is deterministic from that key — same ID in dev, in CI, and on the store. Generate once, commit, and the installer can target it from day one. Standard approach.
- **List both IDs** in `allowed_origins` post-publish. Less clean; means publish, find out the assigned ID, update installer, re-release. Works but creates a chicken-and-egg moment.

Recommendation: `"key"` approach. Quirk: on the store the `key` field must be *removed* before uploading (the store sets it based on the publisher account) — but the resulting ID matches. Easiest pattern is a separate `manifest.json5` field stripped at packaging time.

### 2. Native host needs its own installer per OS

`install.sh` only handles macOS and Linux today. For real distribution:

- **macOS**: ship the binary inside a `.pkg` or `.dmg`. Must be **codesigned + notarized** with Apple Developer ID, otherwise Gatekeeper blocks first-run with "cannot be opened because Apple cannot check it for malicious software." Notarization adds ~5 minutes to release pipeline.
- **Windows**: native messaging hosts register via the **registry**, not a JSON file path — `HKEY_CURRENT_USER\Software\Google\Chrome\NativeMessagingHosts\com.longliveflash.rtmp_host` with the default value being a path to a manifest JSON. Windows installer creates that key. Binary needs Authenticode signing or SmartScreen warns.
- **Linux**: keep the current path-based approach. Ship `.deb` + `.rpm` (or just a tarball with an install script) that drops both the binary and the JSON into the right place.
- **Architectures**: macOS needs arm64 + x86_64 (universal binary or two pkgs), Linux + Windows need x86_64 (arm64 increasingly relevant). `cargo build --target aarch64-apple-darwin` etc.

### 3. UX when the host isn't installed

Today, opening an RTMP SWF on a machine without the host gives a `failed to spawn native host` log in the SW console and a silent `Connect.Failed` event in the SWF. Before publishing, add a user-visible path:

- In `background.ts`, on `chrome.runtime.lastError.message` containing `"not found"` or `"forbidden"`, open a tab (or popup) pointing at the install page.
- Or: surface the state via `chrome.storage.session` so the popup UI shows "Native host not installed — [Install]".

### 4. Web Store review on `nativeMessaging`

This permission gets scrutinized. The listing needs an explicit justification field. Draft:

> RTMP/RTMPE is a TCP-based protocol used by Flash multiplayer games and streaming SWFs. Browsers cannot open raw TCP sockets from JavaScript or WebAssembly, so this extension communicates with a separately-installed native binary that performs the actual socket I/O. The native host only handles RTMP traffic initiated by Flash content the user has chosen to view; no user data is transmitted outside the RTMP server the SWF itself targets.

Single-purpose, narrow data flow, and "user-initiated" framing get through review faster than vague "for performance" hand-waves.

### 5. Privacy policy

Required because `host_permissions: ["<all_urls>"]` is set. The privacy policy needs to disclose: SWF URLs read, RTMP destinations contacted via the native host, no telemetry (confirm none). The policy is a URL provided in the store listing.

## Suggested release pipeline

```
release/
  llflash-extension-1.0.0.zip                  # what gets uploaded to the Web Store
  llflash-rtmp-host-1.0.0-macos-arm64.pkg      # signed + notarized
  llflash-rtmp-host-1.0.0-macos-x86_64.pkg
  llflash-rtmp-host-1.0.0-linux-x86_64.tar.gz
  llflash-rtmp-host-1.0.0-windows-x86_64.exe   # signed installer
  install/                                     # static landing page
    index.html                                 # OS-detect, download right host installer
```

The extension popup links to the install page when it sees no host.

## Checklist

### Before any submission

- [ ] Add a `"key"` field to `manifest.json5` so the ID is stable across dev/store. (~15 min)
- [ ] Wire a "native host not installed" UX hook in `background.ts` (open install page on `lastError` containing "not found"). (~30 min)
- [ ] Write the Web Store permission justification draft for `nativeMessaging` and `host_permissions`. (~15 min)

### Before the public submission

- [ ] Cross-platform host builds (CI matrix: macos-arm64, macos-x86_64, linux-x86_64, windows-x86_64). (Few hours)
- [ ] macOS signing + notarization (Apple Developer ID required).
- [ ] Windows registry-based installer (replaces `install.sh` on that OS).
- [ ] Privacy policy URL hosted somewhere.
- [ ] Install landing page with OS detection + download links.

### Don't worry about until v2

- [ ] Native host auto-update — the `version` field in the `ready` event already exists; let it diverge and only deal with breaking-protocol bumps via a version check in `background.ts`.
- [ ] Cross-extension messaging (one host serving multiple LLFlash variants / branches).
