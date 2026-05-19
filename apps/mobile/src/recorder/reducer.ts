/**
 * 录音状态机 —— 任务 9.6 + Property 6。
 *
 * 任务来源：tasks.md 9.6
 * 关联需求：R4.2、R4.3
 * 设计来源：design.md §4.7
 *
 * 两种录音模式：
 *  - `press`：按住说话；按下时录音、抬起停止；同时支持多指按下时只要还有
 *    任意一指未抬起，仍然处于录音状态（这是 React-Native 等触屏 UI 的常见
 *    语义，可避免在多指场景下提前停止）。
 *  - `toggle`：点击切换；`Tap` 计数奇偶决定 `isRecording`。
 *
 * 两种模式下 `Blur`（窗口失焦 / 切后台 / 系统打断）一律强制停止：清空所有
 * 内部计数与按下集合，回到 `isRecording = false`。
 *
 * 该 reducer 是纯函数：相同 `(state, event)` 恒返回相同新 state，无副作用。
 */

/** 录音模式（用户在设置中选择）。 */
export type RecorderMode = 'press' | 'toggle'

/** 状态机内部状态。`pressedIds` 用 number[] 存储以保证可序列化与稳定快照。 */
export interface RecorderState {
  /** 当前模式。 */
  readonly mode: RecorderMode
  /** 已按下但尚未抬起的指针 ID 集合（press 模式专用，多指容错）。 */
  readonly pressedIds: readonly number[]
  /** 累计 Tap 次数（toggle 模式专用，奇数→录音，偶数→停止）。 */
  readonly tapCount: number
}

/** 状态机事件。 */
export type RecorderEvent =
  | { type: 'PointerDown'; pointerId: number }
  | { type: 'PointerUp'; pointerId: number }
  | { type: 'Tap' }
  | { type: 'Blur' }

/** 用户设置的初始状态。 */
export function initialRecorderState(mode: RecorderMode = 'press'): RecorderState {
  return { mode, pressedIds: [], tapCount: 0 }
}

/**
 * 状态机 reducer。
 *
 * Property 6 不变量：
 *  - `press` 模式下 `isRecording` ⇔ `pressedIds.length > 0`；
 *  - `toggle` 模式下 `isRecording` ⇔ `tapCount % 2 === 1`；
 *  - `Blur` 后 `isRecording === false` 且内部计数清零；
 *  - `PointerUp` 对未按下的 pointerId 是幂等空操作；
 *  - 重复 `PointerDown` 同一 pointerId 不重复入队。
 */
export function recorderReduce(state: RecorderState, event: RecorderEvent): RecorderState {
  switch (event.type) {
    case 'PointerDown': {
      if (state.mode !== 'press') return state
      if (state.pressedIds.includes(event.pointerId)) return state
      return { ...state, pressedIds: [...state.pressedIds, event.pointerId] }
    }
    case 'PointerUp': {
      if (state.mode !== 'press') return state
      if (!state.pressedIds.includes(event.pointerId)) return state
      return {
        ...state,
        pressedIds: state.pressedIds.filter((id) => id !== event.pointerId),
      }
    }
    case 'Tap': {
      if (state.mode !== 'toggle') return state
      return { ...state, tapCount: state.tapCount + 1 }
    }
    case 'Blur': {
      // 任何模式下都强制清零。
      return { ...state, pressedIds: [], tapCount: 0 }
    }
    default: {
      // 穷尽性检查；运行时不会到这里。
      const _exhaustive: never = event
      void _exhaustive
      return state
    }
  }
}

/** 派生属性：当前是否正在录音（视觉绑定 / 主线程 ASR 启停判断都使用此函数）。 */
export function isRecording(state: RecorderState): boolean {
  if (state.mode === 'press') return state.pressedIds.length > 0
  return state.tapCount % 2 === 1
}

/** 模式切换辅助：清空模式相关字段，避免泄漏旧状态。`_state` 仅保留签名一致。 */
export function switchMode(_state: RecorderState, mode: RecorderMode): RecorderState {
  return { mode, pressedIds: [], tapCount: 0 }
}
