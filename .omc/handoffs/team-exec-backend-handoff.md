## Handoff: worker-backend → worker-injector-desktop (BridgeEvents + WsOutbound)

### Decided
- Cross-subsystem event bus is `phonemic_core::bridge_events::BridgeEvent`, plumbed via tokio mpsc.
- Discovery (Section 6) reuses the same BridgeEvents channel as Web Server (Section 5) — single channel for all cross-subsystem events.
- WsOutbound trait at `phonemic_core::ws_outbound::WsOutbound` is the canonical interface for Injector + ASR to push messages back to Mobile.
- `Sec-WebSocket-Protocol: phonemic.<sessionToken>` is the ONLY auth path on `/ws`. No Authorization header fallback.

### BridgeEvents
- Module: `phonemic_core::bridge_events`
- Channel: `channel()` returns `(BridgeEventTx, BridgeEventRx)`. Capacity default 256.
- Sender: `BridgeEventTx::send` (async), `try_send` (sync). Clone-able. `raw()` exposes underlying `tokio::sync::mpsc::Sender`.
- Receiver: `BridgeEventRx::recv`, `close`.
- Variants:
  - `DevicePaired { device_id, device_label, peer_ip, paired_at }` — emitted on /api/pair success
  - `DeviceRevoked { device_ids, revoked_at }` — emitted by Tauri revoke command
  - `InjectError { submit_id, code: ErrorCode, message }` — emitted by Injector (7.11)
  - `AsrTimeout { segment_id }` — emitted by ASR watchdog (8.5)
  - `PortUnavailable { preferred_port }` — emitted on bind failure (5.14)
  - `LanLost` / `LanRestored` — unit variants from Discovery (6.2)

### WsOutbound
- Module: `phonemic_core::ws_outbound`
- Trait: `WsOutbound: Send + Sync + 'static` (uses `async_trait`)
- Core: `async fn send(&self, target: SessionTarget, msg: ServerMessage) -> Result<(), WsOutboundError>`
- Convenience: `send_inject_ack`, `send_inject_error`, `send_transcript_final`
- `SessionTarget = One(SessionToken) | Broadcast`
- `WsOutboundError = NotConnected | SendFailed(String) | ServerStopped`

### Web protocol details (already published)
- HTTP types: `phonemic_protocol::http::{PairRequest, PairResponse, HealthResponse}`. Wire format camelCase: `pairingCode`, `fingerprint`, `deviceLabel`, `sessionToken`, `expiresAt`.
- Error format: `phonemic_protocol::AppError { code, message, detail?, ts }`.
- Error codes used: PAIR_INVALID (401), PAIR_RATELIMIT (429 + Retry-After: 300), FORBIDDEN_SUBNET (403), AUTH_REQUIRED (401), MSG_BAD_FORMAT (400).
- /ws server echoes the client-offered subprotocol on the 101 response.

### Pre-existing fixes merged by worker-backend
- TranscriptFinalPayload no longer derives `Eq` (Option<f32> blocked it)
- property_17_pairing_verify.rs: removed inline format-args capture inside nested macro
- Workspace Cargo.toml: added axum 0.7, tower-http 0.5, tokio-tungstenite 0.21, hyper 1, rustls 0.23, rcgen 0.13 (with `ring`), mdns-sd 0.11, if-watch 3, qrcode 0.14, async-trait 0.1, futures-util 0.3, bytes 1

### Section status (per worker-backend)
- Section 3 leftovers: 132+ tests passing
- Section 5: 5.1 + 5.5 + 5.6 + 5.10 cert helper done; 5.15 integration tests pending
- Section 6: qr.rs + mdns.rs done; integration test pending
- Section 8: next

### Outstanding bugs flagged by worker-injector-desktop
- `crates/phonemic-core/src/web/handlers/pair.rs` previously used `phonemic_protocol::PairRequest` (wrong path) — confirm fixed
- `crates/phonemic-core/src/web/tls.rs` `KeyPair::generate` API mismatch with rcgen 0.13 — confirm fixed
