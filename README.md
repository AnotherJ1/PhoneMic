# PhoneMic — Phone-Mic Voice Input

PhoneMic turns any phone on the same Wi-Fi network into a hands-free
voice-to-text microphone for your desktop. It runs as a small native
desktop app (Windows / macOS / Linux) plus a zero-install web client
that loads in your phone's browser.

> Spec source: `.kiro/specs/phone-mic-voice-input/{requirements,design,tasks}.md`

---

## System requirements

| Platform | Minimum version | Notes |
| --- | --- | --- |
| Windows | 10 (1809) | x64 only |
| macOS   | 12 (Monterey) | Universal binary (Apple Silicon + Intel) |
| Linux   | Ubuntu 20.04 / Fedora 36 / equivalent | Requires `libgtk-3`, `libwebkit2gtk-4.1`, `libsoup-3.0`, `librsvg2`. Wayland is supported via XWayland. |
| Phone   | iOS 14+ Safari, Android 9+ Chromium | LAN-only (RFC1918 IPv4). |
| Network | Same Wi-Fi LAN | The desktop must be reachable on a private subnet. |

## First pair walkthrough

1. **Launch the desktop app.** It binds to a free port (default `18080`)
   on every private LAN interface. The connect panel shows the URLs you
   can reach from your phone (e.g. `http://192.168.1.5:18080`).
2. **Grant microphone permission on your phone.** Open the URL in your
   phone's browser. Tap "Allow" when prompted for microphone access.
   Without it, recording is disabled with a `MIC_PERMISSION_DENIED` error.
3. **Pair.** Either scan the QR code shown on the desktop or type the
   8-character code into the manual entry field. The desktop validates
   the code in constant time; after 5 failures the rate limiter pauses
   pairing for 5 minutes and surfaces `PAIR_RATELIMIT`.
4. **Speak.** With the phone showing the **Connected** status, hold (or
   tap, configurable) the record button and speak. The recogniser
   produces interim text live and finalises after a short pause. With
   *Auto-send* on, every final transcript is injected directly into the
   currently focused application on the desktop. With *Auto-send* off,
   you can review/edit the text on the phone first.

The connection is sticky: if the phone briefly loses Wi-Fi or the
browser tab goes to the background, PhoneMic reconnects with a
`[1, 2, 4, 8, 16]`-second backoff and replays any messages that were
queued while disconnected.

## Permissions checklist

- **Phone microphone** (browser prompt). Required to record audio for
  Browser_ASR and Server_ASR alike.
- **macOS Accessibility permission** (System Settings → Privacy &
  Security → Accessibility). Required for the desktop to inject
  keystrokes into the focused app. PhoneMic guides you to the prompt
  on first run; without this permission `INJECT_PERMISSION_DENIED` is
  reported and injection is disabled until granted.
- **Windows / Linux:** no extra permission is required for keyboard
  injection. On Linux under pure Wayland (no XWayland) the injector
  reports `INJECT_BACKEND_ERROR` with `wayland_unsupported`; the app
  still serves the LAN microphone surface so you can troubleshoot.
- **Network**: PhoneMic binds only to RFC1918 / loopback interfaces; it
  refuses requests from public IPs (`FORBIDDEN_SUBNET`).

## Build & test from source

```bash
# rust toolchain, pnpm 9, node 20 are required (rust-toolchain.toml pins the version)
pnpm install
cargo test --workspace --all-features
pnpm -C apps/mobile typecheck
pnpm -C apps/mobile test
pnpm -C apps/mobile build
cargo tauri build  # requires `cargo install tauri-cli@^2`
```

Smoke test (after a release build):

```bash
# Linux / macOS
PHONEMIC_DESKTOP_BIN=./target/release/phonemic-app \
  PHONEMIC_TEST_PAIR_CODE=ABCDEFGH \
  ./scripts/smoke.sh

# Windows
$env:PHONEMIC_DESKTOP_BIN = ".\target\release\phonemic-app.exe"
$env:PHONEMIC_TEST_PAIR_CODE = 'ABCDEFGH'
./scripts/smoke.ps1
```

Mobile end-to-end (Playwright):

```bash
pnpm -C apps/mobile e2e
```

## Repo layout

| Path | Description |
| --- | --- |
| `apps/desktop/src-tauri/` | Tauri 2.x desktop shell (owned by worker-injector-desktop). |
| `apps/mobile/` | Vue 3 SPA bundled into `src-tauri/resources/web/`. |
| `crates/phonemic-core` | Pure functions, state machines, i18n. |
| `crates/phonemic-protocol` | WebSocket / HTTP / config schema (single source of truth). |
| `crates/phonemic-injector` | Cross-platform keyboard injection. |
| `crates/phonemic-discovery` | mDNS + QR. |
| `crates/phonemic-asr` | Server_ASR (whisper.cpp adapter). |
| `scripts/` | gen-ts-types, smoke, sign, release notes. |
| `.kiro/specs/phone-mic-voice-input/` | Source-of-truth requirements / design / tasks. |

## Releases & signing

`v*` tags trigger `.github/workflows/release.yml`. Builds on Windows /
macOS (Apple Silicon + Intel) / Linux (Ubuntu 22.04) run in parallel;
artefacts are signed (Windows: signtool with PFX; macOS: codesign +
notarytool) when secrets are configured. Contributor builds without
signing secrets succeed and emit a warning to `GITHUB_STEP_SUMMARY` —
this is the documented behaviour from design §9.7, not a workaround.

## License

Dual-licensed under MIT or Apache-2.0, at your option.
