/**
 * Browser_ASR 适配 —— 任务 9.8：Web Speech API + interim 实时显示。
 *
 * 任务来源：tasks.md 9.8
 * 关联需求：R4.4、R5.1、R5.2、R5.3
 * 设计来源：design.md §4.7
 *
 * 该模块把浏览器原生 SpeechRecognition 包装为 `BrowserAsr`，暴露：
 *  - {@link supportsBrowserASR}：判断当前环境是否支持原生 ASR；
 *  - {@link createBrowserAsr}：构造可启动 / 停止的实例，事件回调驱动 store。
 */

import type { AsrEvent } from '@/transcript/auto_send'

/** 当 `SpeechRecognition` 或厂商前缀 `webkitSpeechRecognition` 存在即视为可用。 */
export function supportsBrowserASR(): boolean {
  if (typeof window === 'undefined') return false
  const w = window as unknown as Record<string, unknown>
  return Boolean(w.SpeechRecognition ?? w.webkitSpeechRecognition)
}

/** SpeechRecognition 构造器的最小签名（避免 dom-lib 类型冲突）。 */
interface SpeechRecognitionLike extends EventTarget {
  lang: string
  continuous: boolean
  interimResults: boolean
  maxAlternatives: number
  start(): void
  stop(): void
  abort(): void
  onresult: ((this: SpeechRecognitionLike, ev: SpeechRecognitionEventLike) => void) | null
  onerror: ((this: SpeechRecognitionLike, ev: { error: string }) => void) | null
  onend: ((this: SpeechRecognitionLike, ev: Event) => void) | null
}

interface SpeechRecognitionEventLike {
  readonly resultIndex: number
  readonly results: ArrayLike<{
    isFinal: boolean
    [index: number]: { transcript: string; confidence: number }
    length: number
  }>
}

interface SpeechRecognitionCtor {
  new (): SpeechRecognitionLike
}

function getCtor(): SpeechRecognitionCtor | null {
  if (typeof window === 'undefined') return null
  const w = window as unknown as Record<string, unknown>
  return (w.SpeechRecognition as SpeechRecognitionCtor | undefined)
    ?? (w.webkitSpeechRecognition as SpeechRecognitionCtor | undefined)
    ?? null
}

/** BrowserAsr 实例对外接口。 */
export interface BrowserAsr {
  start(lang: string): void
  stop(): void
}

/** 创建 BrowserAsr 实例。事件回调订阅 interim / final / error。 */
export function createBrowserAsr(handlers: {
  onEvent: (e: AsrEvent) => void
  onError: (code: string, message: string) => void
}): BrowserAsr {
  const Ctor = getCtor()
  if (!Ctor) {
    throw new Error('SpeechRecognition is not available in this runtime')
  }
  let rec: SpeechRecognitionLike | null = null

  function attach(r: SpeechRecognitionLike): void {
    r.continuous = true
    r.interimResults = true
    r.maxAlternatives = 1
    r.onresult = (ev) => {
      // 累计所有从 resultIndex 起的结果。每个 result 含 isFinal 标记。
      let interim = ''
      const finals: string[] = []
      for (let i = ev.resultIndex; i < ev.results.length; i += 1) {
        const r = ev.results[i]
        const transcript = r[0]?.transcript ?? ''
        if (r.isFinal) finals.push(transcript)
        else interim += transcript
      }
      if (interim) handlers.onEvent({ type: 'interim', text: interim })
      for (const f of finals) handlers.onEvent({ type: 'final', text: f })
    }
    r.onerror = (ev) => {
      handlers.onError('ASR_TIMEOUT', `BrowserAsr error: ${ev.error}`)
    }
  }

  return {
    start(lang: string): void {
      if (rec) return
      const r = new Ctor()
      attach(r)
      r.lang = lang
      rec = r
      r.start()
    },
    stop(): void {
      if (!rec) return
      try {
        rec.stop()
      } finally {
        rec = null
      }
    },
  }
}
