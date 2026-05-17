<script setup lang="ts">
import { useRouter } from 'vue-router'

// PairView 占位：对应 design.md §4.7 OnboardingView。
// 真实的扫码 / PIN 输入逻辑由任务 9.2 实现，此处仅提供路由可达的最小骨架。
const router = useRouter()

function goHome(): void {
  router.push({ name: 'home' })
}
</script>

<template>
  <main class="mx-auto max-w-md px-4 py-6 flex flex-col gap-4">
    <header class="flex items-center justify-between">
      <button class="pm-btn-ghost" aria-label="返回" @click="goHome">← 返回</button>
      <h1 class="text-lg font-semibold text-brand-ink">配对</h1>
      <span aria-hidden="true" class="w-12" />
    </header>

    <section class="pm-card">
      <h2 class="text-base font-medium mb-2">扫描二维码</h2>
      <p class="text-sm text-slate-600">
        请将手机摄像头对准桌面应用主界面显示的二维码。完整扫码流程将在后续任务中接入。
      </p>
    </section>

    <section class="pm-card">
      <h2 class="text-base font-medium mb-2">手动输入配对码</h2>
      <p class="text-sm text-slate-600">输入桌面端显示的 8 位配对码以完成配对。</p>
      <input
        type="text"
        inputmode="latin"
        autocomplete="one-time-code"
        maxlength="8"
        placeholder="ABCDEFGH"
        class="mt-3 w-full h-11 px-3 rounded-lg border border-slate-200 bg-white tracking-[0.4em] text-center uppercase focus:outline-none focus:border-brand"
        disabled
      />
    </section>
  </main>
</template>
