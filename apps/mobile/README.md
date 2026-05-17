# PhoneMic Mobile（手机端 Web SPA）

PhoneMic 项目的移动端 Web 客户端，对应 `design.md §3.4`、`§4.7` 与 `requirements.md` R4 / R5 / R8 / R9。

技术栈：

- Vue 3.4 + Vite 5（`@vitejs/plugin-vue`）
- TypeScript 5（严格模式 + path alias `@/*` → `src/*`）
- UnoCSS 0.61（`presetUno` + `presetIcons` + `presetTypography`）
- Vue Router 4（Hash 模式，便于 Tauri 静态分发）

## 目录结构

```
apps/mobile/
├── index.html              # 入口 HTML，含移动端 viewport 元信息
├── package.json            # pnpm workspaces 成员（任务 1.4 接入工作区）
├── vite.config.ts          # 构建产物输出至 ../desktop/src-tauri/resources/web
├── uno.config.ts           # UnoCSS 主题与预设
├── tsconfig.json           # 应用 TS 配置
├── tsconfig.node.json      # 工具链（vite / uno / vitest）TS 配置
├── eslint.config.js        # ESLint 9 flat config
├── .prettierrc             # Prettier 配置（2 空格 / 单引号 / no semi）
└── src/
    ├── main.ts             # 应用入口，挂载 router 与 UnoCSS
    ├── App.vue             # 根组件
    ├── env.d.ts            # Vite / Vue / SFC 类型声明
    ├── router/             # 路由：/ → HomeView, /pair → PairView
    ├── views/              # 视图占位（HomeView / PairView）
    └── styles/main.css     # 基础全局样式
```

## 构建产物路径

`vite.config.ts` 中将 `build.outDir` 指向 `../desktop/src-tauri/resources/web`，
这样桌面端 Tauri 打包时可以直接通过 `tower-http::services::ServeDir`
（详见 `design.md §4.2`）以静态资源分发，满足 `Requirement 2.5` 首次访问 ≤ 2s。

构建时会清空目标目录，因此任何手动放入该目录的文件都会被覆盖。

## 开发指引

> 本仓库使用 pnpm workspaces，依赖安装由根目录统一管理（任务 1.4 配置）。
> 当前任务（1.3）只完成脚手架的文件草稿，**不执行 `pnpm install`**。

待 1.4 任务建立 workspace 之后，可在仓库根目录运行：

```bash
pnpm install                # 安装全部 workspace 依赖
pnpm --filter @phonemic/mobile dev
pnpm --filter @phonemic/mobile build
pnpm --filter @phonemic/mobile lint
```

`vite dev` 默认监听 `0.0.0.0:5173`，便于开发者用真机扫码访问开发预览页。

## 后续任务

- 任务 1.4：在仓库根目录配置 `pnpm-workspace.yaml`，并接入 CI。
- 任务 9.x：在 `src/` 下落地 Pinia 状态、i18n 字典、扫码 / PIN 输入、录音状态机、心跳等。
- 任务 2.5：通过 `scripts/gen-ts-types.ts` 生成 `src/protocol/*.ts`，与 `phonemic-protocol` 保持一致。
