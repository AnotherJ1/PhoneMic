/**
 * Server_ASR 推流 —— 任务 9.9：MediaRecorder → audio.chunk / audio.end。
 *
 * 任务来源：tasks.md 9.9
 * 关联需求：R5.4
 * 设计来源：design.md §4.7、§6.3、§5.1
 *
 * 设计：包装 MediaRecorder，监听 `dataavailable` 事件，把每个 Blob 编码为
 * base64 后通过 `sendChunk` 回调投递；停止时调用 `sendEnd` 投递 `audio.end`。
 *
 * 选择 base64 而非 ArrayBuffer 的原因：
 *  - 协议层 `audio.chunk.data` 字段是 base64 字符串（design §5.1.1、TS 协议
 *    镜像 `MAX_AUDIO_CHUNK_BASE64_BYTES`）。
 *  - WebSocket JSON 通道天生不能携带二进制；改成二进制帧需要协议层重写，
 *    暂未列入任务范围。
 */

import type { AudioChunkPayload, AudioEndPayload, AudioCodec } from '@/protocol'

/** 把 Uint8Array 编码为 base64 字符串（标准编码，与 Rust base64 默认 alphabet 对齐）。 */
function uint8ToBase64(bytes: Uint8Array): string {
  if (typeof btoa === 'undefined') {
    // Node 环境（仅测试）：用 Buffer。运行时浏览器永远走 btoa。
    return Buffer.from(bytes).toString('base64')
  }
  let binary = ''
  const chunkSize = 0x8000
  for (let i = 0; i < bytes.length; i += chunkSize) {
    const slice = bytes.subarray(i, i + chunkSize)
    binary += String.fromCharCode(...slice)
  }
  return btoa(binary)
}

/** ServerAsrStreamer 对外接口。 */
export interface ServerAsrStreamer {
  start(stream: MediaStream): Promise<void>
  stop(): Promise<void>
}

/** 选择编码格式：`pcm16k` / `opus`（与协议 `AudioCodec` 对齐）。 */
function pickMimeType(codec: AudioCodec): string {
  return codec === 'pcm16k' ? 'audio/webm;codecs=pcm' : 'audio/webm;codecs=opus'
}

export interface ServerAsrConfig {
  /** 协议层音频编码字段。 */
  readonly codec: AudioCodec
  /** 每隔多少毫秒切一片（建议 200–500）。 */
  readonly timesliceMs: number
  /** 投递 audio.chunk —— 由 ConnectionService 转译为 WebSocket 报文。 */
  readonly sendChunk: (payload: AudioChunkPayload) => void
  /** 投递 audio.end。 */
  readonly sendEnd: (payload: AudioEndPayload) => void
}

/** 创建 ServerAsrStreamer。 */
export function createServerAsrStreamer(cfg: ServerAsrConfig): ServerAsrStreamer {
  let recorder: MediaRecorder | null = null
  let seq = 0

  return {
    async start(stream: MediaStream): Promise<void> {
      seq = 0
      const mimeType = pickMimeType(cfg.codec)
      // 浏览器对 mimeType 不支持时构造会抛 NotSupportedError；调用方应在
      // 调用前用 `MediaRecorder.isTypeSupported` 探测，并退化到 codec='opus'。
      const r = new MediaRecorder(stream, { mimeType })
      recorder = r
      r.ondataavailable = async (ev: BlobEvent) => {
        if (!ev.data || ev.data.size === 0) return
        const buf = new Uint8Array(await ev.data.arrayBuffer())
        cfg.sendChunk({
          seq: seq++,
          codec: cfg.codec,
          data: uint8ToBase64(buf),
          tsMs: Date.now(),
        })
      }
      r.start(cfg.timesliceMs)
    },
    async stop(): Promise<void> {
      const r = recorder
      if (!r) return
      await new Promise<void>((resolve) => {
        r.addEventListener('stop', () => resolve(), { once: true })
        r.stop()
      })
      cfg.sendEnd({ seq })
      recorder = null
    },
  }
}

/** 选择 ASR 引擎（与桌面端 `pick_asr_engine` 决策一致）。 */
export function pickAsrEngine(args: {
  supportsBrowserAsr: boolean
  preferServerAsr: boolean
}): 'browser' | 'server' {
  return args.supportsBrowserAsr && !args.preferServerAsr ? 'browser' : 'server'
}
