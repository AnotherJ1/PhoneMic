/**
 * Tauri 命令 / 事件桥接 —— 任务 9.1（/desktop 视图所需）。
 *
 * 任务来源：worker-injector-desktop 的 handoff（.omc/handoffs/team-exec-injector-handoff.md）
 * 设计来源：design.md §4.1（桌面端 UI）+ §4.7（共享 Vue SPA）
 *
 * 该模块统一封装与 Tauri 后端的通信：
 *  - 13 个 `Result<T, String>` 异步命令；
 *  - 6 个 `phonemic://*` 事件订阅。
 *
 * 在浏览器（手机）环境下，`window.__TAURI_INTERNALS__` 不存在，
 * {@link isTauri} 返回 `false`。Mobile 视图不应直接调用本模块；本模块
 * 仅服务于 `/desktop` 与 `/desktop/splash` 这两个桌面专属视图。
 */

import type { AppConfig, UiLanguage } from '@/protocol'

// ----------------------------------------------------------------------------
// Type definitions mirroring D:/work/cc/PhoneMic/apps/desktop/src-tauri/src/commands.rs
// ----------------------------------------------------------------------------

/** `get_runtime_info` 返回。 */
export interface RuntimeInfo {
  scheme: 'http' | 'https'
  port: number
  ips: readonly string[]
  urls: readonly string[]
  version: string
  uptimeSecs: number
  lanDisabled: boolean
  banner: string | null
  paused: boolean
  injectDelayMs: number
}

/** `get_pairing_code` / `regenerate_code` 返回。 */
export interface PairingCodeView {
  code: string
  qrSvg: string
}

/** `list_sessions` 返回的单条记录。 */
export interface SessionView {
  deviceId: string
  deviceLabel: string
  fingerprintShort: string
  lastUsedAt: string
  createdAt: string
}

/** `get_logs_tail` 返回。 */
export interface LogsTail {
  lines: readonly string[]
  totalBytes: number
}

/** `get_i18n_dict` 返回。 */
export interface I18nDict {
  lang: 'zh-CN' | 'en-US'
  entries: Record<string, string>
}

/** `export_diagnostics` 返回。 */
export interface DiagnosticsBundle {
  path: string
  bytes: number
}

/** Tauri 事件 payload 形态。 */
export type TauriEventName =
  | 'phonemic://startup-stage'
  | 'phonemic://inject-error'
  | 'phonemic://inject-ack'
  | 'phonemic://pairing-code-changed'
  | 'phonemic://lan-changed'
  | 'phonemic://session-changed'

export interface StartupStagePayload {
  stage: string
  message: string
  ready: boolean
}

export interface InjectErrorPayloadEvt {
  code: string
  message: string
  requestId?: string
}

export interface InjectAckPayloadEvt {
  requestId?: string
  chars: number
}

export interface PairingCodeChangedPayload {
  code: string
}

export interface LanChangedPayload {
  disabled: boolean
  banner?: string
  ips: readonly string[]
}

export interface SessionChangedPayload {
  kind: 'added' | 'revoked'
  deviceId: string
}

// ----------------------------------------------------------------------------
// Runtime detection + thin wrappers
// ----------------------------------------------------------------------------

/**
 * 判断当前是否运行在 Tauri WebView 内。
 *
 * 仅作为路由层 / 视图层选择视图（mobile vs desktop）的依据，*不是* fallback：
 * mobile 浏览器与 Tauri WebView 的视图集合在设计上就是分离的（design §4.7），
 * 我们用此函数把请求路由到正确的视图，而不是假装两者同源。
 */
export function isTauri(): boolean {
  if (typeof window === 'undefined') return false
  // Tauri 2.x 注入 `__TAURI_INTERNALS__`；1.x 是 `__TAURI__`。同时检测两者。
  const w = window as unknown as Record<string, unknown>
  return Boolean(w.__TAURI_INTERNALS__ ?? w.__TAURI__)
}

interface TauriCoreApi {
  invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T>
}

interface TauriEventApi {
  listen<T>(
    event: string,
    handler: (e: { payload: T }) => void,
  ): Promise<() => void>
}

/**
 * 取得 Tauri `core.invoke`。
 *
 * 该函数在非 Tauri 环境下抛出而不是返回 stub —— 调用方应在调用前检查
 * {@link isTauri}，把"该不该调用 Tauri"的决策上提到视图层（更好排查）。
 */
async function getInvoke(): Promise<TauriCoreApi['invoke']> {
  if (!isTauri()) {
    throw new Error('tauri/core.invoke called outside Tauri WebView')
  }
  // 动态 import：vite 不会把 @tauri-apps/api 内联进 mobile 入口。
  const mod = (await import(
    /* @vite-ignore */ '@tauri-apps/api/core'
  )) as TauriCoreApi
  return mod.invoke
}

async function getListen(): Promise<TauriEventApi['listen']> {
  if (!isTauri()) {
    throw new Error('tauri/event.listen called outside Tauri WebView')
  }
  const mod = (await import(
    /* @vite-ignore */ '@tauri-apps/api/event'
  )) as TauriEventApi
  return mod.listen
}

// ----------------------------------------------------------------------------
// Commands
// ----------------------------------------------------------------------------

export async function getRuntimeInfo(): Promise<RuntimeInfo> {
  const invoke = await getInvoke()
  return invoke<RuntimeInfo>('get_runtime_info')
}

export async function getPairingCode(): Promise<PairingCodeView> {
  const invoke = await getInvoke()
  return invoke<PairingCodeView>('get_pairing_code')
}

export async function regenerateCode(): Promise<PairingCodeView> {
  const invoke = await getInvoke()
  return invoke<PairingCodeView>('regenerate_code')
}

export async function listSessions(): Promise<readonly SessionView[]> {
  const invoke = await getInvoke()
  return invoke<readonly SessionView[]>('list_sessions')
}

export async function revokeSession(deviceId: string): Promise<void> {
  const invoke = await getInvoke()
  await invoke<void>('revoke_session', { deviceId })
}

export async function revokeAllSessions(): Promise<{ revoked: number }> {
  const invoke = await getInvoke()
  return invoke<{ revoked: number }>('revoke_all_sessions')
}

export async function saveConfig(config: AppConfig): Promise<void> {
  const invoke = await getInvoke()
  await invoke<void>('save_config', { config })
}

export async function getConfig(): Promise<AppConfig> {
  const invoke = await getInvoke()
  return invoke<AppConfig>('get_config')
}

export async function getLogsTail(maxBytes?: number): Promise<LogsTail> {
  const invoke = await getInvoke()
  return invoke<LogsTail>('get_logs_tail', maxBytes != null ? { maxBytes } : {})
}

export async function setInjectPaused(paused: boolean): Promise<void> {
  const invoke = await getInvoke()
  await invoke<void>('set_inject_paused', { paused })
}

export async function setInjectDelayMs(delayMs: number): Promise<void> {
  const invoke = await getInvoke()
  await invoke<void>('set_inject_delay_ms', { delayMs })
}

export async function getI18nDict(lang: UiLanguage): Promise<I18nDict> {
  const invoke = await getInvoke()
  return invoke<I18nDict>('get_i18n_dict', { lang })
}

export async function exportDiagnostics(targetDir: string): Promise<DiagnosticsBundle> {
  const invoke = await getInvoke()
  return invoke<DiagnosticsBundle>('export_diagnostics', { targetDir })
}

// ----------------------------------------------------------------------------
// Events
// ----------------------------------------------------------------------------

/** 事件名到 payload 类型的映射，便于在 listen 调用点享受类型推断。 */
export interface TauriEventPayloadMap {
  'phonemic://startup-stage': StartupStagePayload
  'phonemic://inject-error': InjectErrorPayloadEvt
  'phonemic://inject-ack': InjectAckPayloadEvt
  'phonemic://pairing-code-changed': PairingCodeChangedPayload
  'phonemic://lan-changed': LanChangedPayload
  'phonemic://session-changed': SessionChangedPayload
}

/** 订阅事件并返回 unlisten 函数。 */
export async function listenTauri<E extends TauriEventName>(
  event: E,
  handler: (payload: TauriEventPayloadMap[E]) => void,
): Promise<() => void> {
  const listen = await getListen()
  return listen<TauriEventPayloadMap[E]>(event, (e) => handler(e.payload))
}
