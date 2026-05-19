<script setup lang="ts">
/**
 * Settings 视图 —— 任务 9.1（持久化）+ 9.22（i18n 切换）+ 9.27（错误展示）。
 *
 * 任务来源：tasks.md 9.1, 9.22
 * 关联需求：R4.x, R8.5, R8.6
 * 设计来源：design.md §4.7
 */
import { useRouter } from 'vue-router'
import { useI18n, useSettings } from '@/stores'
import { UI_LANGS, type UiLang } from '@/i18n'
import type { RecorderMode } from '@/recorder/reducer'

const router = useRouter()
const i18nStore = useI18n()
const t = i18nStore.t
const settings = useSettings()

function setUiLang(l: UiLang): void {
  i18nStore.setLang(l)
}
function setAsrLang(l: UiLang): void {
  settings.setAsrLang(l)
}
function setMode(m: RecorderMode): void {
  settings.setMode(m)
}
</script>

<template>
  <main class="mx-auto max-w-md px-4 py-6 flex flex-col gap-4">
    <header class="flex items-center justify-between">
      <button class="pm-btn-ghost" :aria-label="t('nav.back')" @click="router.back()">← {{ t('nav.back') }}</button>
      <h1 class="text-lg font-semibold text-brand-ink">{{ t('settings.title') }}</h1>
      <span aria-hidden="true" class="w-12" />
    </header>

    <section class="pm-card">
      <label class="block text-sm font-medium text-slate-700">{{ t('settings.ui_lang') }}</label>
      <div class="mt-2 flex gap-2">
        <button
          v-for="l in UI_LANGS"
          :key="`ui-${l}`"
          class="pm-btn"
          :class="i18nStore.lang === l ? 'bg-brand-soft text-brand-ink' : 'bg-slate-100 text-slate-700'"
          @click="setUiLang(l)"
        >
          {{ l === 'zh-CN' ? t('settings.lang.zh_cn') : t('settings.lang.en_us') }}
        </button>
      </div>
    </section>

    <section class="pm-card">
      <label class="block text-sm font-medium text-slate-700">{{ t('settings.asr_lang') }}</label>
      <div class="mt-2 flex gap-2">
        <button
          v-for="l in UI_LANGS"
          :key="`asr-${l}`"
          class="pm-btn"
          :class="settings.asrLang === l ? 'bg-brand-soft text-brand-ink' : 'bg-slate-100 text-slate-700'"
          @click="setAsrLang(l)"
        >
          {{ l === 'zh-CN' ? t('settings.lang.zh_cn') : t('settings.lang.en_us') }}
        </button>
      </div>
    </section>

    <section class="pm-card">
      <label class="block text-sm font-medium text-slate-700">{{ t('settings.mode') }}</label>
      <div class="mt-2 flex gap-2">
        <button
          class="pm-btn"
          :class="settings.mode === 'press' ? 'bg-brand-soft text-brand-ink' : 'bg-slate-100 text-slate-700'"
          @click="setMode('press')"
        >
          {{ t('settings.mode.press') }}
        </button>
        <button
          class="pm-btn"
          :class="settings.mode === 'toggle' ? 'bg-brand-soft text-brand-ink' : 'bg-slate-100 text-slate-700'"
          @click="setMode('toggle')"
        >
          {{ t('settings.mode.toggle') }}
        </button>
      </div>
    </section>

    <section class="pm-card flex items-center justify-between">
      <label for="autoSend" class="text-sm font-medium text-slate-700">{{ t('settings.auto_send') }}</label>
      <input
        id="autoSend"
        type="checkbox"
        :checked="settings.autoSend"
        @change="settings.setAutoSend(($event.target as HTMLInputElement).checked)"
      />
    </section>

    <section class="pm-card flex items-center justify-between">
      <label for="preferServer" class="text-sm font-medium text-slate-700">{{ t('settings.prefer_server_asr') }}</label>
      <input
        id="preferServer"
        type="checkbox"
        :checked="settings.preferServerAsr"
        @change="settings.setPreferServerAsr(($event.target as HTMLInputElement).checked)"
      />
    </section>
  </main>
</template>
