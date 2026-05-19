<script setup lang="ts">
/**
 * /desktop 视图 —— 任务 9.1 + Tauri command 桥接。
 *
 * 任务来源：worker-injector-desktop 的 handoff（.omc/handoffs/team-exec-injector-handoff.md）
 * 设计来源：design.md §4.1（桌面端 UI）
 *
 * 该视图加载在 Tauri WebView 内，组合连接面板、配对码 / QR、设备列表与设置。
 * 启动阶段（startup-stage 事件 ready=false）切换到 `/desktop/splash`。
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import {
  exportDiagnostics,
  getPairingCode,
  getRuntimeInfo,
  isTauri,
  listSessions,
  listenTauri,
  regenerateCode,
  revokeAllSessions,
  revokeSession,
  setInjectPaused,
  type PairingCodeView,
  type RuntimeInfo,
  type SessionView,
} from '@/desktop/tauri'
import { useI18n } from '@/stores'

const t = useI18n().t
const runtime = ref<RuntimeInfo | null>(null)
const pairing = ref<PairingCodeView | null>(null)
const sessions = ref<readonly SessionView[]>([])
const error = ref<string | null>(null)
const unlisten: Array<() => void> = []

const tauri = computed(() => isTauri())

async function refreshAll(): Promise<void> {
  if (!tauri.value) {
    error.value = '/desktop view requires the Tauri WebView'
    return
  }
  try {
    runtime.value = await getRuntimeInfo()
    pairing.value = await getPairingCode()
    sessions.value = await listSessions()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}

async function regenerate(): Promise<void> {
  pairing.value = await regenerateCode()
}

async function togglePause(): Promise<void> {
  if (!runtime.value) return
  const next = !runtime.value.paused
  await setInjectPaused(next)
  runtime.value = { ...runtime.value, paused: next }
}

async function revoke(deviceId: string): Promise<void> {
  await revokeSession(deviceId)
  sessions.value = await listSessions()
}

async function revokeAll(): Promise<void> {
  await revokeAllSessions()
  sessions.value = await listSessions()
}

async function exportLogs(): Promise<void> {
  // 桌面端：targetDir 由文件对话框选择；此处简单使用空串让后端使用默认目录。
  const r = await exportDiagnostics('')
  alert(`Diagnostics: ${r.path} (${r.bytes} bytes)`)
}

onMounted(async () => {
  await refreshAll()
  if (!tauri.value) return
  // 订阅事件，事件驱动更新。
  unlisten.push(
    await listenTauri('phonemic://pairing-code-changed', async () => {
      pairing.value = await getPairingCode()
    }),
    await listenTauri('phonemic://lan-changed', async () => {
      runtime.value = await getRuntimeInfo()
    }),
    await listenTauri('phonemic://session-changed', async () => {
      sessions.value = await listSessions()
    }),
  )
})

onBeforeUnmount(() => {
  for (const u of unlisten) u()
})
</script>

<template>
  <main class="mx-auto max-w-3xl px-6 py-6 grid gap-4 md:grid-cols-2">
    <section class="pm-card md:col-span-2">
      <h2 class="text-base font-medium mb-2">{{ t('panel.connect.title') }}</h2>
      <div v-if="runtime?.banner" class="mb-2 rounded bg-amber-50 p-2 text-xs text-amber-800">
        {{ runtime.banner }}
      </div>
      <ul class="text-sm text-slate-700">
        <li v-for="url in runtime?.urls ?? []" :key="url" class="font-mono">{{ url }}</li>
      </ul>
      <p class="mt-2 text-xs text-slate-500">
        v{{ runtime?.version }} · uptime {{ runtime?.uptimeSecs }}s
      </p>
    </section>

    <section class="pm-card">
      <h2 class="text-base font-medium mb-2">{{ t('pairing.code.label') }}</h2>
      <p class="font-mono text-3xl tracking-widest">{{ pairing?.code }}</p>
      <div v-if="pairing?.qrSvg" class="mt-2 max-w-xs" v-html="pairing.qrSvg" />
      <button class="pm-btn-primary mt-3" @click="regenerate">
        {{ t('pairing.code.regenerate') }}
      </button>
    </section>

    <section class="pm-card">
      <h2 class="text-base font-medium mb-2">{{ t('panel.devices.title') }}</h2>
      <ul class="flex flex-col gap-2 text-sm">
        <li
          v-for="s in sessions"
          :key="s.deviceId"
          class="flex items-center justify-between rounded border border-slate-100 bg-slate-50 p-2"
        >
          <div class="min-w-0">
            <p class="truncate font-medium">{{ s.deviceLabel }}</p>
            <p class="text-xs text-slate-500 font-mono">{{ s.fingerprintShort }}</p>
          </div>
          <button class="text-xs text-red-600 hover:underline" @click="revoke(s.deviceId)">
            Revoke
          </button>
        </li>
      </ul>
      <button v-if="sessions.length > 0" class="pm-btn mt-3 bg-red-50 text-red-700" @click="revokeAll">
        Revoke all
      </button>
    </section>

    <section class="pm-card md:col-span-2 flex items-center justify-between">
      <div>
        <p class="text-sm font-medium">{{ t('settings.title') }}</p>
        <p class="text-xs text-slate-500">paused: {{ runtime?.paused ? 'yes' : 'no' }}, delay: {{ runtime?.injectDelayMs }}ms</p>
      </div>
      <div class="flex gap-2">
        <button class="pm-btn-ghost" @click="togglePause">
          {{ runtime?.paused ? 'Resume' : 'Pause' }} inject
        </button>
        <button class="pm-btn-ghost" @click="exportLogs">Export logs</button>
      </div>
    </section>

    <section v-if="error" class="pm-card md:col-span-2 border-red-200 bg-red-50 text-red-800">
      {{ error }}
    </section>
  </main>
</template>
