/**
 * 麦克风权限请求 —— 任务 9.26：MIC_PERMISSION_DENIED + 引导文案。
 *
 * 任务来源：tasks.md 9.26
 * 关联需求：R4.8、R5.1
 * 设计来源：design.md §4.7
 *
 * 该模块封装 `navigator.mediaDevices.getUserMedia`，把异常映射成统一
 * `AppError { code: 'MIC_PERMISSION_DENIED', ... }`。
 */

import type { AppError } from '@/protocol'

/**
 * 请求麦克风权限并返回 `MediaStream`。
 *
 * 失败时抛出形如 {@link AppError} 的对象（注意：仍是 throw，不是 return）。
 */
export async function requestMicrophone(): Promise<MediaStream> {
  if (typeof navigator === 'undefined' || !navigator.mediaDevices?.getUserMedia) {
    const err: AppError = {
      code: 'MIC_PERMISSION_DENIED',
      message: 'getUserMedia is not available in this browser',
      ts: new Date().toISOString(),
    }
    throw err
  }
  try {
    return await navigator.mediaDevices.getUserMedia({ audio: true, video: false })
  } catch (e) {
    const reason = e instanceof Error ? e.name : String(e)
    const message =
      reason === 'NotAllowedError' || reason === 'PermissionDeniedError'
        ? 'Microphone permission denied by the user'
        : `Failed to acquire microphone: ${reason}`
    const err: AppError = {
      code: 'MIC_PERMISSION_DENIED',
      message,
      detail: { reason },
      ts: new Date().toISOString(),
    }
    throw err
  }
}
