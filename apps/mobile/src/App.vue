<script setup lang="ts">
// 应用根组件：仅承载 router-view 与全局壳层。
// 真正的 UI 视图、状态管理、i18n 等由后续 9.x 任务在 views/components/stores 中落地。
</script>

<template>
  <div class="min-h-screen bg-surface-muted text-slate-900 font-sans antialiased">
    <router-view />
  </div>
</template>
