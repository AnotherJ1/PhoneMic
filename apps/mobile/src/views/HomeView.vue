<script setup lang="ts">
/**
 * 主页（语音输入）—— 任务 9.x 综合视图。
 *
 * 任务来源：tasks.md 9.6 / 9.8 / 9.10 / 9.12 / 9.27
 * 关联需求：R4.x、R5.x、R9.6
 * 设计来源：design.md §4.7
 *
 * 该视图组合：
 *  - 录音按钮（press / toggle 模式驱动 useRecorder）；
 *  - 实时 interim 预览 + 可编辑 draft；
 *  - 历史记录（容量 50）；
 *  - 状态徽标（statusLabel(s, lang)）；
 *  - 错误 toast。
 */
import { computed, ref } from 'vue'
import { useRouter } from 'vue-router'
import {
  useI18n,
  useConnection,
  useRecorder,
  useTranscript,
  useSettings,
} from '@/stores'
import { statusLabel } from '@/connection/status'
import ErrorToast from '@/components/ErrorToast.vue'
import type { AppError } from '@/protocol'

const router = useRouter()
const i18nStore = useI18n()
const t = i18nStore.t
const connection = useConnection()
const recorder = useRecorder()
const transcript = useTranscript()
const settings = useSettings()

const error = ref<AppError | null>(null)

const status = computed(() => statusLabel(connection.status, i18nStore.lang))
const isPaired = computed(() => connection.sessionToken != null)

function goPair(): void {
  router.push({ name: 'pair' })
}

function onPointerDown(ev: PointerEvent): void {
  if (settings.mode === 'press') {
    recorder.dispatch({ type: 'PointerDown', pointerId: ev.pointerId })
  }
}

function onPointerUp(ev: PointerEvent): void {
  if (settings.mode === 'press') {
    recorder.dispatch({ type: 'PointerUp', pointerId: ev.pointerId })
  } else {
    recorder.dispatch({ type: 'Tap' })
  }
}

function send(): void {
  if (!transcript.draft.trim()) return
  transcript.pushHistory(transcript.draft)
  transcript.clearDraft()
}

function clearDraft(): void {
  transcript.clearDraft()
}

function copyHistoryItem(item: string): void {
  void navigator.clipboard.writeText(item)
}

function resendHistoryItem(item: string): void {
  transcript.setDraft(item)
}
</script>

<template>
  <main class="mx-auto max-w-md px-4 py-6 flex flex-col gap-4">
    <header class="flex items-center justify-between">
      <h1 class="text-xl font-semibold text-brand-ink">{{ t('app.title') }}</h1>
      <button class="pm-btn-ghost text-xs" @click="router.push({ name: 'settings' })">
        ⚙ {{ t('nav.settings') }}
      </button>
    </header>

    <section v-if="!isPaired" class="pm-card">
      <h2 class="text-base font-medium mb-2">{{ t('home.untitled') }}</h2>
      <p class="text-sm text-slate-600 leading-relaxed">{{ t('home.untitled.desc') }}</p>
      <button class="pm-btn-primary mt-4 w-full" @click="goPair">{{ t('home.go_pair') }}</button>
    </section>

    <template v-else>
      <section class="pm-card">
        <div class="flex items-center justify-between">
          <span class="text-sm font-medium text-slate-700">{{ status }}</span>
          <span
            class="inline-block h-2 w-2 rounded-full"
            :class="connection.status === 'Connected' ? 'bg-emerald-500' : 'bg-amber-500'"
          />
        </div>

        <button
          class="mt-4 flex h-32 w-full items-center justify-center rounded-2xl text-white text-lg font-medium transition"
          :class="recorder.recording ? 'bg-red-500 animate-pulse' : 'bg-brand'"
          @pointerdown.passive="onPointerDown"
          @pointerup.passive="onPointerUp"
          @pointercancel.passive="onPointerUp"
        >
          {{
            recorder.recording
              ? t('home.record.recording')
              : settings.mode === 'press'
              ? t('home.record.press_hold')
              : t('home.record.tap_toggle')
          }}
        </button>
      </section>

      <section class="pm-card">
        <p v-if="transcript.interim" class="text-xs text-slate-500 italic">{{ transcript.interim }}</p>
        <textarea
          v-model="transcript.draft"
          rows="3"
          class="mt-2 w-full resize-none rounded-lg border border-slate-200 bg-white p-2 text-sm focus:outline-none focus:border-brand"
          :placeholder="t('home.draft.placeholder')"
        />
        <div class="mt-2 flex gap-2">
          <button class="pm-btn-primary flex-1" :disabled="!transcript.draft.trim()" @click="send">
            {{ t('home.draft.send') }}
          </button>
          <button class="pm-btn-ghost" @click="clearDraft">{{ t('home.draft.clear') }}</button>
        </div>
      </section>

      <section class="pm-card">
        <h2 class="text-base font-medium mb-2">{{ t('home.history.title') }}</h2>
        <p v-if="transcript.history.length === 0" class="text-sm text-slate-500">
          {{ t('home.history.empty') }}
        </p>
        <ul v-else class="flex flex-col-reverse gap-2">
          <li
            v-for="(item, idx) in transcript.history"
            :key="`${idx}-${item.length}`"
            class="flex items-start gap-2 rounded border border-slate-100 bg-slate-50 p-2 text-sm"
          >
            <p class="min-w-0 flex-1 break-words">{{ item }}</p>
            <button class="text-xs text-brand" @click="copyHistoryItem(item)">
              {{ t('home.history.copy') }}
            </button>
            <button class="text-xs text-brand" @click="resendHistoryItem(item)">
              {{ t('home.history.resend') }}
            </button>
          </li>
        </ul>
      </section>
    </template>

    <ErrorToast :error="error" :auto-dismiss-ms="6000" @dismiss="error = null" />

    <footer class="text-center text-xs text-slate-400 mt-6">PhoneMic Mobile</footer>
  </main>
</template>
