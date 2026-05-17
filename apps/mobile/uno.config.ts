import { defineConfig, presetIcons, presetTypography, presetUno } from 'unocss'

// PhoneMic 移动端原子化样式入口。
// 主题与色板的最终落地由后续 UI 任务（9.x）补充，这里仅提供脚手架级别的占位主题。
export default defineConfig({
  presets: [
    presetUno(),
    presetIcons({
      scale: 1.2,
      warn: true,
    }),
    presetTypography(),
  ],
  theme: {
    colors: {
      // 占位品牌色，避免组件先依赖具体色值；正式色板见后续设计令牌任务。
      brand: {
        DEFAULT: '#2563eb',
        soft: '#dbeafe',
        ink: '#0f172a',
      },
      surface: {
        DEFAULT: '#ffffff',
        muted: '#f8fafc',
        sunken: '#f1f5f9',
      },
    },
    fontFamily: {
      sans: '"Inter", "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", system-ui, sans-serif',
    },
  },
  shortcuts: {
    'pm-card': 'rounded-xl bg-surface shadow-sm border border-slate-200 p-4',
    'pm-btn': 'inline-flex items-center justify-center gap-2 px-4 h-10 rounded-lg font-medium transition-colors duration-200 cursor-pointer',
    'pm-btn-primary': 'pm-btn bg-brand text-white hover:bg-blue-700',
    'pm-btn-ghost': 'pm-btn text-slate-700 hover:bg-slate-100',
  },
  content: {
    pipeline: {
      include: [
        /\.(vue|svelte|[jt]sx?|html)($|\?)/,
      ],
    },
  },
})
