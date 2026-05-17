<script setup lang="ts">
import { useRouter } from 'vue-router'

// HomeView 占位：对应 design.md §4.7 MainView。
// 录音按钮、波形、转写预览、历史等由后续 9.6 / 9.8 / 9.10 / 9.12 等任务落地。
// 这里使用静态字符串占位，i18n 字典将在任务 9.22 接入 useI18n() 后替换。
const router = useRouter()

function goPair(): void {
  router.push({ name: 'pair' })
}
</script>

<template>
  <main class="mx-auto max-w-md px-4 py-6 flex flex-col gap-4">
    <header class="flex items-center justify-between">
      <h1 class="text-xl font-semibold text-brand-ink">PhoneMic</h1>
      <span class="text-xs px-2 py-1 rounded-full bg-brand-soft text-brand">脚手架</span>
    </header>

    <section class="pm-card">
      <h2 class="text-base font-medium mb-2">未连接</h2>
      <p class="text-sm text-slate-600 leading-relaxed">
        请先与桌面端完成配对，扫描桌面应用显示的二维码或手动输入 8 位配对码。
      </p>
      <button class="pm-btn-primary mt-4 w-full" @click="goPair">前往配对</button>
    </section>

    <footer class="text-center text-xs text-slate-400 mt-6">PhoneMic Mobile · v0.0.0</footer>
  </main>
</template>
