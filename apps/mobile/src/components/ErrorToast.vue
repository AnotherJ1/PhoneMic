<script setup lang="ts">
/**
 * 错误展示组件 —— 任务 9.27（Toast/Banner）。
 *
 * 任务来源：tasks.md 9.27
 * 关联需求：R9.6
 * 设计来源：design.md §8.1
 *
 * 该组件渲染 `AppError { code, message }`，不展示堆栈或敏感字段。
 * `detail` 字段折叠在 `<details>` 内，便于用户主动展开排查。
 */
import { computed, onUnmounted, ref, watch } from 'vue'
import type { AppError } from '@/protocol'
import { useI18n } from '@/stores'

const props = defineProps<{
  /** 错误对象；为空表示无错误。 */
  error: AppError | null
  /** 自动消失（毫秒）；0 / undefined 表示常驻。 */
  autoDismissMs?: number
}>()

const emit = defineEmits<{
  (e: 'dismiss'): void
}>()

const { t } = useI18n()
const visible = ref(false)
let dismissTimer: number | null = null

const i18nMessage = computed(() => {
  if (!props.error) return ''
  // 错误码对应的字典 key 形如 "error.PAIR_INVALID"
  const key = `error.${props.error.code}`
  return t(key)
})

const detailJson = computed(() => {
  if (!props.error?.detail) return ''
  try {
    return JSON.stringify(props.error.detail, null, 2)
  } catch {
    return String(props.error.detail)
  }
})

function dismiss(): void {
  visible.value = false
  if (dismissTimer != null) {
    window.clearTimeout(dismissTimer)
    dismissTimer = null
  }
  emit('dismiss')
}

watch(
  () => props.error,
  (next) => {
    if (next) {
      visible.value = true
      if (dismissTimer != null) window.clearTimeout(dismissTimer)
      if (props.autoDismissMs && props.autoDismissMs > 0) {
        dismissTimer = window.setTimeout(dismiss, props.autoDismissMs)
      }
    } else {
      visible.value = false
    }
  },
  { immediate: true },
)

onUnmounted(() => {
  if (dismissTimer != null) window.clearTimeout(dismissTimer)
})
</script>

<template>
  <transition name="fade">
    <div
      v-if="visible && error"
      role="alert"
      class="fixed inset-x-3 bottom-4 z-50 mx-auto max-w-md rounded-xl border border-red-200 bg-red-50 p-3 text-sm text-red-900 shadow-lg"
    >
      <div class="flex items-start gap-2">
        <span class="mt-0.5 inline-block h-2 w-2 flex-none rounded-full bg-red-500" />
        <div class="min-w-0 flex-1">
          <p class="font-medium break-words">{{ i18nMessage || error.message }}</p>
          <p class="mt-0.5 text-xs text-red-700 opacity-80">{{ error.code }}</p>
          <details v-if="detailJson" class="mt-1">
            <summary class="cursor-pointer text-xs text-red-700">详细信息</summary>
            <pre class="mt-1 overflow-auto rounded bg-white p-2 text-xs">{{ detailJson }}</pre>
          </details>
        </div>
        <button
          type="button"
          class="ml-2 flex-none rounded px-2 py-1 text-xs text-red-700 hover:bg-red-100"
          @click="dismiss"
        >
          ✕
        </button>
      </div>
    </div>
  </transition>
</template>

<style scoped>
.fade-enter-active,
.fade-leave-active {
  transition: opacity 200ms ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}
</style>
