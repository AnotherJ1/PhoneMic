<script setup lang="ts">
/**
 * Pair 视图 —— 任务 9.2：扫码 / 手输 PIN。
 *
 * 任务来源：tasks.md 9.2
 * 关联需求：R7.2、R7.3、R7.5
 * 设计来源：design.md §4.7、§6.2
 *
 * 行为：
 *  - 优先使用原生 BarcodeDetector（Chrome / 部分 Edge 支持）；
 *    不可用环境下使用 `jsQR`（这是设计层的库选择，不是运行时回退）。
 *  - 调用 `POST /api/pair`，成功后保存 `sessionToken` 至 sessionStorage；
 *  - PAIR_INVALID 提示重输；PAIR_RATELIMIT 启动倒计时禁用提交。
 *
 * 注意：PairView 仅在手机浏览器中使用；桌面 Tauri webview 走 `/desktop` 路由。
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import jsQR from 'jsqr'
import { useI18n, useConnection } from '@/stores'
import type { AppError, PairRequest, PairResponse } from '@/protocol'
import ErrorToast from '@/components/ErrorToast.vue'

const router = useRouter()
const { t } = useI18n()
const connection = useConnection()

const code = ref<string>('')
const submitting = ref<boolean>(false)
const error = ref<AppError | null>(null)
const cooldownSec = ref<number>(0)
const scanning = ref<boolean>(false)
const videoEl = ref<HTMLVideoElement | null>(null)
const canvasEl = ref<HTMLCanvasElement | null>(null)
let stream: MediaStream | null = null
let scanRaf: number | null = null
let cooldownInterval: number | null = null

const submitDisabled = computed(
  () => submitting.value || cooldownSec.value > 0 || code.value.trim().length !== 8,
)

function goHome(): void {
  router.push({ name: 'home' })
}

interface BarcodeDetectorLike {
  detect(input: HTMLVideoElement | HTMLCanvasElement | ImageBitmap): Promise<{
    rawValue: string
    format: string
  }[]>
}
interface BarcodeDetectorCtor {
  new (opts: { formats: string[] }): BarcodeDetectorLike
}

function getBarcodeDetector(): BarcodeDetectorLike | null {
  const w = window as unknown as Record<string, unknown>
  const C = w.BarcodeDetector as BarcodeDetectorCtor | undefined
  if (!C) return null
  return new C({ formats: ['qr_code'] })
}

async function startScan(): Promise<void> {
  scanning.value = true
  try {
    stream = await navigator.mediaDevices.getUserMedia({
      video: { facingMode: 'environment' },
      audio: false,
    })
    if (videoEl.value) {
      videoEl.value.srcObject = stream
      await videoEl.value.play()
    }
    const detector = getBarcodeDetector()
    const tick = async (): Promise<void> => {
      if (!scanning.value || !videoEl.value || !canvasEl.value) return
      const v = videoEl.value
      const c = canvasEl.value
      c.width = v.videoWidth || 640
      c.height = v.videoHeight || 480
      const ctx = c.getContext('2d', { willReadFrequently: true })
      if (!ctx) return
      ctx.drawImage(v, 0, 0, c.width, c.height)
      let raw = ''
      if (detector) {
        const codes = await detector.detect(v)
        raw = codes[0]?.rawValue ?? ''
      } else {
        const img = ctx.getImageData(0, 0, c.width, c.height)
        const result = jsQR(img.data, img.width, img.height)
        raw = result?.data ?? ''
      }
      if (raw) {
        const m = raw.match(/[?&]c=([A-Z0-9]{6,16})/i)
        if (m) {
          code.value = m[1].toUpperCase()
          stopScan()
          await submit()
          return
        }
      }
      scanRaf = requestAnimationFrame(() => void tick())
    }
    scanRaf = requestAnimationFrame(() => void tick())
  } catch (e) {
    error.value = {
      code: 'MIC_PERMISSION_DENIED',
      message: t('pair.scan.permission_denied'),
      detail: { reason: e instanceof Error ? e.message : String(e) },
      ts: new Date().toISOString(),
    }
    scanning.value = false
  }
}

function stopScan(): void {
  scanning.value = false
  if (scanRaf != null) cancelAnimationFrame(scanRaf)
  scanRaf = null
  stream?.getTracks().forEach((t) => t.stop())
  stream = null
}

async function submit(): Promise<void> {
  if (submitDisabled.value) return
  submitting.value = true
  error.value = null
  try {
    const body: PairRequest = {
      pairingCode: code.value.trim().toUpperCase(),
      fingerprint: deriveFingerprint(),
      deviceLabel: deriveDeviceLabel(),
    }
    const resp = await fetch(`${window.location.origin}/api/pair`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    })
    if (resp.ok) {
      const data = (await resp.json()) as PairResponse
      connection.setSessionToken(data.sessionToken)
      router.push({ name: 'home' })
      return
    }
    if (resp.status === 429) {
      const ra = parseInt(resp.headers.get('retry-after') ?? '300', 10)
      startCooldown(Number.isNaN(ra) ? 300 : ra)
      const payload = (await resp.json().catch(() => ({}))) as Partial<AppError>
      error.value = {
        code: 'PAIR_RATELIMIT',
        message: t('pair.ratelimit', { seconds: ra }),
        detail: payload.detail,
        ts: new Date().toISOString(),
      }
    } else {
      const payload = (await resp.json().catch(() => ({}))) as Partial<AppError>
      error.value = {
        code: 'PAIR_INVALID',
        message: t('pair.invalid'),
        detail: payload.detail,
        ts: new Date().toISOString(),
      }
    }
  } catch (e) {
    error.value = {
      code: 'PAIR_INVALID',
      message: t('pair.invalid'),
      detail: { reason: e instanceof Error ? e.message : String(e) },
      ts: new Date().toISOString(),
    }
  } finally {
    submitting.value = false
  }
}

function startCooldown(seconds: number): void {
  cooldownSec.value = seconds
  if (cooldownInterval != null) window.clearInterval(cooldownInterval)
  cooldownInterval = window.setInterval(() => {
    cooldownSec.value -= 1
    if (cooldownSec.value <= 0) {
      cooldownSec.value = 0
      if (cooldownInterval != null) window.clearInterval(cooldownInterval)
      cooldownInterval = null
    }
  }, 1000)
}

function deriveFingerprint(): string {
  // 极简稳定指纹（足够用于 LAN 内识别；不用于密码学场景）。
  const ua = (navigator.userAgent ?? '').slice(0, 64)
  const screen = `${window.screen?.width ?? 0}x${window.screen?.height ?? 0}`
  const seed = `${ua}|${screen}|${navigator.language ?? ''}`
  // 简单哈希 djb2 → hex（设计层指纹仅用于设备识别，不要求加密强度）。
  let h = 5381
  for (let i = 0; i < seed.length; i += 1) h = ((h << 5) + h + seed.charCodeAt(i)) >>> 0
  return h.toString(16).padStart(8, '0')
}

function deriveDeviceLabel(): string {
  const ua = navigator.userAgent ?? 'Unknown'
  const m = ua.match(/\((.*?)\)/)
  return m ? m[1].slice(0, 60) : ua.slice(0, 60)
}

onMounted(() => {
  // 自动尝试启动扫码；失败时用户可改用手动输入。
  void startScan()
})

onBeforeUnmount(() => {
  stopScan()
  if (cooldownInterval != null) window.clearInterval(cooldownInterval)
})
</script>

<template>
  <main class="mx-auto max-w-md px-4 py-6 flex flex-col gap-4">
    <header class="flex items-center justify-between">
      <button class="pm-btn-ghost" :aria-label="t('nav.back')" @click="goHome">← {{ t('nav.back') }}</button>
      <h1 class="text-lg font-semibold text-brand-ink">{{ t('pair.title') }}</h1>
      <span aria-hidden="true" class="w-12" />
    </header>

    <section class="pm-card">
      <h2 class="text-base font-medium mb-2">{{ t('pair.scan.title') }}</h2>
      <p class="text-sm text-slate-600">{{ t('pair.scan.desc') }}</p>
      <div class="relative mt-3 aspect-square overflow-hidden rounded-lg bg-slate-100">
        <video ref="videoEl" class="absolute inset-0 h-full w-full object-cover" muted playsinline />
        <canvas ref="canvasEl" class="hidden" />
      </div>
    </section>

    <section class="pm-card">
      <h2 class="text-base font-medium mb-2">{{ t('pair.manual.title') }}</h2>
      <p class="text-sm text-slate-600">{{ t('pair.manual.desc') }}</p>
      <input
        v-model="code"
        type="text"
        inputmode="text"
        autocomplete="one-time-code"
        maxlength="8"
        :placeholder="t('pair.manual.placeholder')"
        class="mt-3 w-full h-11 px-3 rounded-lg border border-slate-200 bg-white tracking-[0.4em] text-center uppercase focus:outline-none focus:border-brand"
        :disabled="submitting"
      />
      <button
        class="pm-btn-primary mt-3 w-full"
        :disabled="submitDisabled"
        @click="submit"
      >
        <span v-if="cooldownSec > 0">{{ t('pair.ratelimit', { seconds: cooldownSec }) }}</span>
        <span v-else>{{ t('pair.manual.submit') }}</span>
      </button>
    </section>

    <ErrorToast :error="error" :auto-dismiss-ms="0" @dismiss="error = null" />
  </main>
</template>
