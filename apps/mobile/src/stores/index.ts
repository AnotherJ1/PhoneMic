/**
 * Pinia stores —— 任务 9.1。
 *
 * 任务来源：tasks.md 9.1（路由 + Pinia store + 持久化）
 * 关联需求：R4.1
 * 设计来源：design.md §4.7
 *
 * 暴露 5 个 store：useConnection / useRecorder / useTranscript / useSettings / useI18n。
 *
 * 持久化策略（design §4.7 + 任务 9.1）：仅以下 5 项写入 localStorage，
 * key 前缀 `phonemic.`：
 *   - uiLang
 *   - asrLang
 *   - mode
 *   - autoSend
 *   - preferServerAsr
 *
 * Session_Token 不在此处持久化（应放 sessionStorage，9.2 实现），原因是
 * sessionToken 一旦关闭浏览器就应过期（design §5.2）。
 */

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import {
  initialRecorderState,
  recorderReduce,
  isRecording as derivIsRecording,
  type RecorderState,
  type RecorderEvent,
  type RecorderMode,
} from '@/recorder/reducer'
import {
  decideLang,
  translate,
  UI_LANGS,
  type UiLang,
} from '@/i18n'
import {
  pushBounded,
  HISTORY_CAPACITY,
} from '@/transcript/history'
import type { ConnectionStatus } from '@/connection/status'
import type { AsrEvent } from '@/transcript/auto_send'
import type { AppError } from '@/protocol'

// ----------------------------------------------------------------------------
// Persistence helpers
// ----------------------------------------------------------------------------

/** 命名空间避免与同源应用的 localStorage 冲突。 */
const STORAGE_PREFIX = 'phonemic.'

function readJson<T>(key: string, def: T): T {
  if (typeof localStorage === 'undefined') return def
  try {
    const raw = localStorage.getItem(STORAGE_PREFIX + key)
    if (raw == null) return def
    return JSON.parse(raw) as T
  } catch {
    return def
  }
}

function writeJson(key: string, value: unknown): void {
  if (typeof localStorage === 'undefined') return
  try {
    localStorage.setItem(STORAGE_PREFIX + key, JSON.stringify(value))
  } catch {
    // QuotaExceeded / private mode — 非致命，忽略。
  }
}

// ----------------------------------------------------------------------------
// useI18n
// ----------------------------------------------------------------------------

/** UI 语言 store —— 仅持久化 `uiLang`。 */
export const useI18n = defineStore('i18n', () => {
  const initialLang: UiLang = (() => {
    const saved = readJson<UiLang | null>('uiLang', null)
    if (saved && UI_LANGS.includes(saved)) return saved
    if (typeof navigator !== 'undefined') return decideLang(navigator.language)
    return 'en-US'
  })()
  const lang = ref<UiLang>(initialLang)

  function setLang(next: UiLang): void {
    lang.value = next
    writeJson('uiLang', next)
  }

  function t(key: string, vars?: Readonly<Record<string, string | number>>): string {
    return translate(lang.value, key, vars)
  }

  return { lang, setLang, t }
})

// ----------------------------------------------------------------------------
// useSettings
// ----------------------------------------------------------------------------

export const useSettings = defineStore('settings', () => {
  const asrLang = ref<UiLang>(readJson<UiLang>('asrLang', 'zh-CN'))
  const mode = ref<RecorderMode>(readJson<RecorderMode>('mode', 'press'))
  const autoSend = ref<boolean>(readJson<boolean>('autoSend', true))
  const preferServerAsr = ref<boolean>(readJson<boolean>('preferServerAsr', false))

  function setAsrLang(v: UiLang): void {
    asrLang.value = v
    writeJson('asrLang', v)
  }
  function setMode(v: RecorderMode): void {
    mode.value = v
    writeJson('mode', v)
  }
  function setAutoSend(v: boolean): void {
    autoSend.value = v
    writeJson('autoSend', v)
  }
  function setPreferServerAsr(v: boolean): void {
    preferServerAsr.value = v
    writeJson('preferServerAsr', v)
  }

  return {
    asrLang,
    mode,
    autoSend,
    preferServerAsr,
    setAsrLang,
    setMode,
    setAutoSend,
    setPreferServerAsr,
  }
})

// ----------------------------------------------------------------------------
// useConnection
// ----------------------------------------------------------------------------

export const useConnection = defineStore('connection', () => {
  const status = ref<ConnectionStatus>('Disconnected')
  const lastError = ref<AppError | null>(null)
  const sessionToken = ref<string | null>(null)
  const reconnectAttempt = ref<number>(0)

  function setStatus(s: ConnectionStatus): void {
    status.value = s
  }
  function setError(e: AppError | null): void {
    lastError.value = e
  }
  function setSessionToken(token: string | null): void {
    sessionToken.value = token
    if (typeof sessionStorage !== 'undefined') {
      if (token == null) sessionStorage.removeItem(STORAGE_PREFIX + 'sessionToken')
      else sessionStorage.setItem(STORAGE_PREFIX + 'sessionToken', token)
    }
  }
  function bumpReconnect(): void {
    reconnectAttempt.value += 1
  }
  function resetReconnect(): void {
    reconnectAttempt.value = 0
  }

  // Hydrate sessionToken from sessionStorage on creation
  if (typeof sessionStorage !== 'undefined') {
    const saved = sessionStorage.getItem(STORAGE_PREFIX + 'sessionToken')
    if (saved) sessionToken.value = saved
  }

  return {
    status,
    lastError,
    sessionToken,
    reconnectAttempt,
    setStatus,
    setError,
    setSessionToken,
    bumpReconnect,
    resetReconnect,
  }
})

// ----------------------------------------------------------------------------
// useRecorder
// ----------------------------------------------------------------------------

export const useRecorder = defineStore('recorder', () => {
  // 初始 mode 来自 settings（同样持久化）。读取时用 defaultMode 兜底，避免
  // 循环引用 useSettings（store 可能尚未初始化）。
  const initialMode: RecorderMode = readJson<RecorderMode>('mode', 'press')
  const state = ref<RecorderState>(initialRecorderState(initialMode))

  const recording = computed(() => derivIsRecording(state.value))

  function dispatch(event: RecorderEvent): void {
    state.value = recorderReduce(state.value, event)
  }

  function setMode(mode: RecorderMode): void {
    state.value = initialRecorderState(mode)
  }

  return { state, recording, dispatch, setMode }
})

// ----------------------------------------------------------------------------
// useTranscript
// ----------------------------------------------------------------------------

export const useTranscript = defineStore('transcript', () => {
  const draft = ref<string>('')
  const interim = ref<string>('')
  const history = ref<readonly string[]>([])
  const events = ref<readonly AsrEvent[]>([])

  function setDraft(text: string): void {
    draft.value = text
  }
  function setInterim(text: string): void {
    interim.value = text
  }
  function clearDraft(): void {
    draft.value = ''
    interim.value = ''
  }
  function pushHistory(text: string): void {
    history.value = pushBounded(history.value, text, HISTORY_CAPACITY)
  }
  function pushEvent(e: AsrEvent): void {
    events.value = events.value.concat([e])
    if (e.type === 'interim') interim.value = e.text
    if (e.type === 'final') draft.value = e.text
  }
  function clearEvents(): void {
    events.value = []
  }

  return {
    draft,
    interim,
    history,
    events,
    setDraft,
    setInterim,
    clearDraft,
    pushHistory,
    pushEvent,
    clearEvents,
  }
})
