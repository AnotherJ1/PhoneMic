// 协议 fingerprint 工具库
//
// 任务来源：.kiro/specs/phone-mic-voice-input/tasks.md 2.5
// 设计来源：.kiro/specs/phone-mic-voice-input/design.md §5
//
// 本模块只做一件事：以确定性方式（字典序遍历 + 字节级 SHA-256）计算
// 「Rust 协议源文件集合」与「TS 镜像源文件集合」各自的 fingerprint。
// 由 scripts/gen-ts-types.mjs 调用；stamp 文件中两个 fingerprint 不匹配
// 即视为协议漂移。
//
// 仅依赖 Node 20+ 内置模块，无第三方包。

import { createHash } from 'node:crypto'
import { readFile, stat } from 'node:fs/promises'
import path from 'node:path'

/**
 * 仓库根的相对路径常量。所有 fingerprint 计算都基于这些路径，避免
 * 因 CWD 不同而导致结果漂移。
 *
 * @typedef {{ rust: string[], ts: string[] }} ProtocolSources
 */

/** @type {ProtocolSources} */
export const PROTOCOL_SOURCES = {
  rust: [
    'crates/phonemic-protocol/src/lib.rs',
    'crates/phonemic-protocol/src/ws.rs',
    'crates/phonemic-protocol/src/http.rs',
    'crates/phonemic-protocol/src/error.rs',
    'crates/phonemic-protocol/src/error_obj.rs',
    'crates/phonemic-protocol/src/config.rs',
  ],
  ts: [
    'apps/mobile/src/protocol/index.ts',
    'apps/mobile/src/protocol/ws.ts',
    'apps/mobile/src/protocol/http.ts',
    'apps/mobile/src/protocol/error.ts',
    'apps/mobile/src/protocol/error_obj.ts',
    'apps/mobile/src/protocol/config.ts',
  ],
}

/**
 * 把仓库相对路径转换为以仓库根为基准的绝对路径。
 *
 * @param {string} repoRoot
 * @param {string} relPath
 * @returns {string}
 */
export function resolveFromRoot(repoRoot, relPath) {
  return path.resolve(repoRoot, relPath)
}

/**
 * 规范化路径分隔符，使 Windows 与 POSIX 上的 fingerprint 完全一致。
 *
 * @param {string} relPath
 * @returns {string}
 */
function normalizePathForHash(relPath) {
  return relPath.split(path.sep).join('/')
}

/**
 * 计算一组文件的确定性 fingerprint。
 *
 * 算法：按 `normalizePathForHash` 后字典序排序文件路径；逐文件喂入
 * `<rel-path>\n<byte-len>\n<file-bytes>\n`；最终取 SHA-256 十六进制。
 * 这样无论运行平台如何，只要文件内容（按字节）与逻辑路径不变，
 * fingerprint 就保持一致。
 *
 * @param {string} repoRoot 仓库根的绝对路径
 * @param {string[]} relPaths 仓库相对路径列表
 * @returns {Promise<string>} 64 字符的小写十六进制 SHA-256
 */
export async function fingerprintFiles(repoRoot, relPaths) {
  const sortedPaths = [...relPaths]
    .map((p) => normalizePathForHash(p))
    .sort((a, b) => (a < b ? -1 : a > b ? 1 : 0))

  const hash = createHash('sha256')
  for (const rel of sortedPaths) {
    const abs = resolveFromRoot(repoRoot, rel)
    const buf = await readFile(abs)
    hash.update(rel, 'utf8')
    hash.update('\n')
    hash.update(String(buf.byteLength), 'utf8')
    hash.update('\n')
    hash.update(buf)
    hash.update('\n')
  }
  return hash.digest('hex')
}

/**
 * 一次性计算 Rust 与 TS 两侧的 fingerprint。
 *
 * @param {string} repoRoot
 * @returns {Promise<{ rust: string, ts: string }>}
 */
export async function computeProtocolFingerprints(repoRoot) {
  const [rust, ts] = await Promise.all([
    fingerprintFiles(repoRoot, PROTOCOL_SOURCES.rust),
    fingerprintFiles(repoRoot, PROTOCOL_SOURCES.ts),
  ])
  return { rust, ts }
}

/**
 * 校验所有源文件存在；缺失会立即抛错并指出具体路径，避免 fingerprint
 * 在缺文件场景下被误算。
 *
 * @param {string} repoRoot
 * @returns {Promise<void>}
 */
export async function ensureProtocolSourcesExist(repoRoot) {
  const all = [...PROTOCOL_SOURCES.rust, ...PROTOCOL_SOURCES.ts]
  for (const rel of all) {
    const abs = resolveFromRoot(repoRoot, rel)
    try {
      await stat(abs)
    } catch (cause) {
      const err = new Error(`Protocol source missing: ${rel}`)
      err.cause = cause
      throw err
    }
  }
}
