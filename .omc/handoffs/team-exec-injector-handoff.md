## Handoff: worker-injector-desktop → worker-mobile-e2e (Tauri command surface)

### Decided
- Tauri command surface uses `Result<T, String>` async commands; types live at `D:/work/cc/PhoneMic/apps/desktop/src-tauri/src/commands.rs`.
- Tauri events use the `phonemic://...` namespace.
- Vue routes for desktop viewport: `/desktop`, `/desktop/splash`. Mobile routes unchanged: `/pair`, `/`, `/settings`.
- `InjectError` variants map 1:1 to ErrorCode names: `INJECT_NO_FOCUS_TARGET`, `INJECT_PERMISSION_DENIED`, `INJECT_PAUSED`, `INJECT_BACKEND_ERROR`.

### Tauri commands

| Command | Args | Returns |
|---|---|---|
| `get_runtime_info` | — | `RuntimeInfo { scheme, port, ips, urls, version, uptime_secs, lan_disabled, banner, paused, inject_delay_ms }` |
| `get_pairing_code` | — | `PairingCodeView { code, qr_svg }` |
| `regenerate_code` | — | `PairingCodeView` |
| `list_sessions` | — | `Vec<SessionView { device_id, device_label, fingerprint_short, last_used_at, created_at }>` |
| `revoke_session` | `{ device_id }` | `()` |
| `revoke_all_sessions` | — | `{ revoked }` |
| `save_config` | `{ config: AppConfig }` | `()` |
| `get_config` | — | `AppConfig` |
| `get_logs_tail` | `{ max_bytes? }` | `{ lines, total_bytes }` |
| `set_inject_paused` | `{ paused }` | `()` |
| `set_inject_delay_ms` | `{ delay_ms }` (0..=500) | `()` |
| `get_i18n_dict` | `{ lang: "zh-CN"\|"en-US"\|"auto" }` | `{ lang, entries }` |
| `export_diagnostics` | `{ target_dir }` | `{ path, bytes }` |

### Tauri events

| Event | Payload |
|---|---|
| `phonemic://startup-stage` | `{ stage, message, ready }` |
| `phonemic://inject-error` | `{ code, message, request_id? }` |
| `phonemic://inject-ack` | `{ request_id?, chars }` |
| `phonemic://pairing-code-changed` | `{ code }` |
| `phonemic://lan-changed` | `{ disabled, banner?, ips }` |
| `phonemic://session-changed` | `{ kind: "added"\|"revoked", device_id }` |

### InjectorEventSink trait
`phonemic_injector::InjectorEventSink` at `crates/phonemic-injector/src/lib.rs`:
```rust
pub trait InjectorEventSink: Send + Sync {
    fn on_inject_error(&self, code: &str, message: &str, request_id: Option<&str>);
    fn on_inject_ack(&self, request_id: Option<&str>, chars: usize);
}
```

### Files
- `crates/phonemic-injector/src/lib.rs` (traits + VirtualBackend + planner)
- `apps/desktop/src-tauri/src/commands.rs` (in progress)

### Remaining for worker-injector-desktop
- 7.11 inject.error sink wiring
- Section 10 Tauri commands + tray + splash (10.5/10.6/10.7/10.8/10.9)
- Section 13 errors/logs (13.1–13.5)
- Wait for worker-backend BridgeEvents trait before completing 13.3 ASR error forwarding
