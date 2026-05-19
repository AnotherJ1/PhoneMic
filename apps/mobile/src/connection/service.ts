/**
 * ConnectionService —— 任务 9.3：WebSocket 客户端 + 连接状态机。
 *
 * 任务来源：tasks.md 9.3
 * 关联需求：R4.5、R7.4、R9.6
 * 设计来源：design.md §4.7、§6.4
 * 协议：design.md §5.1（Sec-WebSocket-Protocol: phonemic.<token> —— 由
 *      worker-backend 在团队协调中确认）
 *
 * 状态机：
 *   Disconnected → Connecting → Connected → Reconnecting → Connecting → ...
 *
 * 转移规则：
 *   - connect()           : Disconnected → Connecting；Connected/Connecting 则 no-op；Reconnecting 时立刻发起新连接。
 *   - 收到 onopen         : Connecting → Connected；其它状态被忽略（防御）。
 *   - 收到 onclose/onerror: Connected → Reconnecting（如未达上限）；Connecting 同样。
 *   - close()             : 任意 → Disconnected（手动断开，禁止自动重连）。
 *
 * 鉴权：握手时通过 `Sec-WebSocket-Protocol: phonemic.<sessionToken>` 携带 token。
 * 浏览器 WebSocket 构造函数把 protocol 列表当作字符串数组，本模块把
 * 这条 sub-protocol 直接放进去。
 */

import type { ClientMessage, ServerMessage } from '@/protocol'
import type { ConnectionStatus } from './status'

/** WebSocket 状态机事件，仅供调试 / store 同步使用。 */
export type ConnectionEvent =
  | { kind: 'status'; status: ConnectionStatus }
  | { kind: 'message'; message: ServerMessage }
  | { kind: 'error'; reason: string }

/** ConnectionService 构造参数。 */
export interface ConnectionConfig {
  /** WS 完整 URL，例如 `ws://192.168.1.5:18080/ws`（design §5.1）。 */
  readonly url: string
  /** 256 位 sessionToken（base64url）。 */
  readonly sessionToken: string
  /** 事件订阅（status / message / error）。 */
  readonly onEvent: (e: ConnectionEvent) => void
  /**
   * WebSocket 构造器注入：默认使用全局 `WebSocket`。
   * 测试和 SSR 场景下可注入自定义实现，避免直接 mock 全局对象。
   */
  readonly webSocketCtor?: typeof WebSocket
}

/** 把 sessionToken 拼成 sub-protocol 字符串。 */
export function buildSubprotocol(sessionToken: string): string {
  return `phonemic.${sessionToken}`
}

/**
 * 创建 ConnectionService。返回可控制实例。
 *
 * 设计：以工厂函数 + 闭包代替 class，避免 `this` 绑定问题；与 Pinia 协作时
 * 把 `onEvent` 直接写成 store action。
 */
export interface ConnectionService {
  /** 当前状态机状态（同步读取）。 */
  status(): ConnectionStatus
  /** 发起或重连一次。Disconnected/Reconnecting → Connecting。 */
  connect(): void
  /** 主动关闭连接（不会触发自动重连）。 */
  close(): void
  /** 发送消息；非 Connected 状态返回 false（调用方应入离线队列）。 */
  send(msg: ClientMessage): boolean
  /** 把 ws.onclose 当作"被动断开"通知给状态机，用于 9.16 重连调度。 */
  notifyDisconnected(reason: string): void
}

export function createConnectionService(cfg: ConnectionConfig): ConnectionService {
  const Ctor = cfg.webSocketCtor ?? (typeof WebSocket !== 'undefined' ? WebSocket : undefined)
  if (!Ctor) {
    throw new Error('WebSocket is not available in this runtime')
  }

  let status: ConnectionStatus = 'Disconnected'
  let ws: WebSocket | null = null

  function setStatus(next: ConnectionStatus): void {
    if (status === next) return
    status = next
    cfg.onEvent({ kind: 'status', status: next })
  }

  function attach(socket: WebSocket): void {
    socket.onopen = () => {
      // 防御：仅在 Connecting 时进入 Connected。
      if (status === 'Connecting' || status === 'Reconnecting') setStatus('Connected')
    }
    socket.onmessage = (ev: MessageEvent) => {
      let parsed: ServerMessage
      try {
        parsed = JSON.parse(String(ev.data)) as ServerMessage
      } catch (e) {
        cfg.onEvent({ kind: 'error', reason: `Malformed JSON from server: ${String(e)}` })
        return
      }
      cfg.onEvent({ kind: 'message', message: parsed })
    }
    socket.onerror = () => {
      cfg.onEvent({ kind: 'error', reason: 'websocket error' })
    }
    socket.onclose = () => {
      ws = null
      // 状态机：Connected/Connecting → Reconnecting。
      if (status === 'Connected' || status === 'Connecting') setStatus('Reconnecting')
    }
  }

  return {
    status: () => status,
    connect(): void {
      if (status === 'Connected' || status === 'Connecting') return
      setStatus('Connecting')
      try {
        const socket = new Ctor(cfg.url, [buildSubprotocol(cfg.sessionToken)])
        ws = socket
        attach(socket)
      } catch (e) {
        cfg.onEvent({ kind: 'error', reason: `WebSocket construction failed: ${String(e)}` })
        setStatus('Reconnecting')
      }
    },
    close(): void {
      if (ws) {
        try {
          ws.close(1000, 'client closed')
        } catch {
          // OK to ignore — close called twice or before open.
        }
        ws = null
      }
      setStatus('Disconnected')
    },
    send(msg: ClientMessage): boolean {
      if (status !== 'Connected' || !ws) return false
      try {
        ws.send(JSON.stringify(msg))
        return true
      } catch (e) {
        cfg.onEvent({ kind: 'error', reason: `send failed: ${String(e)}` })
        return false
      }
    },
    notifyDisconnected(reason: string): void {
      cfg.onEvent({ kind: 'error', reason })
      if (status === 'Connected' || status === 'Connecting') setStatus('Reconnecting')
    },
  }
}

/**
 * 把 LAN 中的 origin (`http(s)://host:port`) 转成 WebSocket URL (`ws(s)://host:port/ws`)。
 *
 * 仅做 scheme 替换 + path 追加；输入若不合法（例如非 http/https）返回输入本身，
 * 由调用方早期失败。
 */
export function deriveWsUrl(httpOrigin: string): string {
  const trimmed = httpOrigin.replace(/\/+$/, '')
  if (trimmed.startsWith('https://')) return `${trimmed.replace(/^https:/, 'wss:')}/ws`
  if (trimmed.startsWith('http://')) return `${trimmed.replace(/^http:/, 'ws:')}/ws`
  return trimmed
}
