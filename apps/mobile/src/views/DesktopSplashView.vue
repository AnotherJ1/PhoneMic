<script setup lang="ts">
/**
 * /desktop/splash 视图 —— 任务 10.8（桌面启动画面）。
 *
 * 任务来源：worker-injector-desktop handoff，事件 phonemic://startup-stage
 * 设计来源：design.md §6.1
 *
 * 在桌面端进程未 `ready=true` 之前展示进度；`ready=true` 时跳转到 `/desktop`。
 */
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { listenTauri, isTauri } from '@/desktop/tauri'

const router = useRouter()
const stage = ref<string>('starting')
const message = ref<string>('Initialising…')
let unlisten: (() => void) | null = null

onMounted(async () => {
  if (!isTauri()) {
    // Browser preview: skip splash so dev can iterate on /desktop directly.
    void router.replace({ name: 'desktop' })
    return
  }
  unlisten = await listenTauri('phonemic://startup-stage', (payload) => {
    stage.value = payload.stage
    message.value = payload.message
    if (payload.ready) {
      void router.replace({ name: 'desktop' })
    }
  })
})

onBeforeUnmount(() => {
  unlisten?.()
})
</script>

<template>
  <main class="flex h-full flex-col items-center justify-center gap-4 bg-surface-muted">
    <h1 class="text-2xl font-semibold text-brand-ink">PhoneMic</h1>
    <p class="text-sm text-slate-500">{{ message }}</p>
    <p class="text-xs text-slate-400 font-mono">[{{ stage }}]</p>
    <div class="h-1 w-48 overflow-hidden rounded-full bg-slate-200">
      <div class="h-full w-1/3 animate-pulse bg-brand" />
    </div>
  </main>
</template>
