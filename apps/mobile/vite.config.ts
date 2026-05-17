import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import UnoCSS from 'unocss/vite'

// 输出至桌面端 Tauri 资源目录，便于 Web_Server 直接以静态资源形式分发（Design §3.4、§4.7）。
// 使用相对路径以保证仓库迁移时仍然有效。
const TAURI_WEB_RESOURCE_DIR = '../desktop/src-tauri/resources/web'

// 关联需求：
// - Requirement 2.5：首次访问 2 秒内返回页面，因此构建产物以静态资源形式由 Web_Server 直接分发。
// - Requirement 4.1：手机浏览器无需安装即可访问页面，dev server 默认绑定 0.0.0.0 便于真机联调。
export default defineConfig(({ mode }) => ({
  base: './',
  plugins: [
    vue(),
    UnoCSS(),
  ],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    host: '0.0.0.0',
    port: 5173,
    strictPort: false,
  },
  preview: {
    host: '0.0.0.0',
    port: 4173,
  },
  build: {
    outDir: TAURI_WEB_RESOURCE_DIR,
    emptyOutDir: true,
    sourcemap: mode !== 'production',
    target: 'es2020',
    rollupOptions: {
      output: {
        // 与 Tauri 静态分发保持稳定的资源命名，便于 ServeDir + ETag 缓存策略。
        assetFileNames: 'assets/[name]-[hash][extname]',
        chunkFileNames: 'assets/[name]-[hash].js',
        entryFileNames: 'assets/[name]-[hash].js',
      },
    },
  },
}))
