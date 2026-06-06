# PhoneMic 桌面端图形窗口 — 设计文档

- 日期：2026-06-06
- 分支：`0.2.0`
- 状态：已批准，待实现

## 背景

PhoneMic 当前桌面端是「零 UI」设计（见 `cmd/phonemic/main.go:9-12`），只有
系统托盘菜单：复制 Connect URL、复制配对码、用看图器打开二维码 PNG、轮换配对码、
退出。语音识别在手机浏览器侧用 Web Speech API 完成，桌面端通过 HTTPS+WebSocket
收到文字后写剪贴板并模拟 Ctrl/Cmd+V 注入当前焦点窗口。

`apps/` 目录是早期废弃的 Tauri+Vue 版本残留（未被 git 跟踪，仅剩 `node_modules`
与陈旧构建产物），与本设计无关，不在改动范围。

## 目标

给桌面端增加一个真正的图形窗口，承载以下能力：

1. **内嵌二维码** —— 窗口内直接绘制配对二维码，不再用外部看图器打开 PNG。
2. **实时连接状态** —— 显示当前是否有手机连接、连接数。
3. **文字记录** —— 显示最近收到并注入的文本（含时间戳）。
4. **配对码操作** —— 窗口内复制 URL、轮换配对码（搬走托盘的全部能力）。

## 关键决策

| 决策点 | 选择 | 理由 |
|---|---|---|
| 界面对象 | 桌面端窗口 | 用户明确选择（非手机端网页美化） |
| GUI 技术 | 纯 Go `gioui`（gioui.org） | 跨平台 win/mac/linux 且**无 CGO**，契合「单 binary、小体积、无前端构建链」约束。fyne 在 Windows 需 CGO，walk 仅 Windows，均不满足。 |
| 窗口与托盘关系 | **窗口替代 systray 托盘** | gioui 与 systray 都要独占主线程（macOS 强制 UI 在主线程），无法干净共存。四项窗口功能已完整覆盖原托盘能力，托盘无存在必要。 |
| 中文字体 | 运行时加载系统 CJK 字体 | gioui 默认 Go 字体不含 CJK 字形，文字记录会显示豆腐块。运行时加载系统字体不增加 binary 体积。 |

## 架构

### 主线程归属

```
main()
  ├─ 初始化 appState（配对码 / IP / 端口 / 连接集合 / 文字记录环形缓冲）
  ├─ startServer(state)         ← 后台 goroutine，HTTPS+WS（完全不变）
  └─ ui.Run(state)              ← 占据主线程，gioui 事件循环（替代 systray.Run）
```

- `startServer` 及其下的 HTTPS/WS/证书逻辑**完全不动**。
- 删除 `systray.Run` / `onReady` / `onExit` 及 `getlantern/systray` 依赖。
- 删除 `showQR` / `openInBrowser`（二维码改为窗口内绘制；不再需要外部看图器）。
- `main()` 末尾由 `systray.Run(...)` 改为 `ui.Run(state)`。

### 组件划分

| 文件 | 职责 | 主要依赖 |
|---|---|---|
| `main.go` | 启动编排：建 state、起 server、进 UI 主循环；保留 appState、injector、server、detectLanIP 等既有逻辑 | ui, appState |
| `ui.go`（新增） | gioui 窗口：布局、事件循环、把 state 渲染成界面；按钮事件调 state 方法 | gioui, appState |
| `qr.go`（新增） | 二维码：`urlToImage(url) image.Image`，供 gioui 当 widget 绘制 | go-qrcode |
| `font.go`（新增） | `findCJKFont()` 按平台探测系统中文字体路径并加载为 `text.FontFace` | gioui/font, os |

### appState 扩展（新增只读/写方法，供 UI 单向读取）

- `connCount() int` —— 基于已有 `conns` map，带锁返回当前连接数。
- 文字记录环形缓冲：固定容量（50 条），元素含 `{ time, text }`。
  - `inject.write(text)` 成功注入后追加一条（需让 injector 能访问 state，或由
    `/ws` handler 在调用 `inject.write` 后追加——实现时择一，倾向后者以保持
    injector 纯粹）。
  - `recentTexts() []textRecord` —— 带锁返回快照副本（新的在前）。
  - 满容量时丢弃最旧。

UI **只读** state，不反向驱动 server。轮换配对码按钮调既有 `state.rotateCode()`；
Copy URL 调既有 `clipboard.WriteAll`。

## 界面布局

单窗口，纵向 `layout.Flex` 分区：

```
┌─────────────────────────────────────┐
│  PhoneMic            ● 1 phone connected │  顶部：标题 + 状态点（绿=有连接/灰=无）
├─────────────────────────────────────┤
│   ┌─────────┐    Connect URL:        │
│   │ ▓▓ QR ▓▓ │    https://192.168../  │  二维码 + URL + 配对码
│   │ ▓▓▓▓▓▓▓ │    Pair code: ABCXYZ   │
│   └─────────┘    [Copy URL] [Rotate] │  按钮
├─────────────────────────────────────┤
│  Recent text                          │
│   12:30:01  今天天气不错               │  文字记录（最近 N 条，新的在上）
│   12:29:50  hello world               │
│   …                                   │
└─────────────────────────────────────┘
```

## 数据流

```
手机连上/断开  → registerConn / unregister 改 conns map
手机发文字     → /ws handler 调 inject.write 后，追加一条到 state 文字记录环形缓冲
                          │
窗口侧：window.Invalidate() 每 ~500ms 触发一帧重绘
                          │ 读 snapshot() / connCount() / recentTexts()
                          ▼
   gioui 用最新的 连接数 / URL / 配对码 / recent[] 重绘
```

- 二维码在 URL 或配对码变化时才重新生成 `image.Image` 并缓存，避免每帧重编码。
- 采用「定时 Invalidate + 读快照」而非事件总线，简单、足够，规避并发复杂度。

## 中文字体

gioui 默认字体（Go Regular）不含 CJK 字形，文字记录里的中文会显示为缺字方块。

- **主方案**：启动时 `findCJKFont()` 按平台探测系统字体文件并注册：
  - Windows：`C:\Windows\Fonts\msyh.ttc`（微软雅黑）等
  - macOS：`/System/Library/Fonts/PingFang.ttc` 等
  - Linux：常见 `Noto Sans CJK` / `wqy` 路径
- **降级**：找不到任何 CJK 字体时记录一条 warning，UI 照常启动（英文/数字/界面
  正常，CJK 显示缺字），不崩溃。
- **不嵌入字体**（体积优先）；如未来要保证一定可显示，可作为可选编译项再议。

## 错误处理

- **窗口关闭 = 退出整个程序**（server goroutine 随进程结束）。无后台常驻形态。
- 字体加载失败：`log` warning，UI 照常起，不崩溃。
- 二维码生成失败：对应区域显示 "QR unavailable"，URL 文本仍可复制。
- `startServer` 失败：维持既有 `log.Fatalf` 直接退出（此时尚未进入 UI）。
- 文字记录环形缓冲固定容量（50），满则丢最旧，防止长时间运行内存增长。

## 测试策略

不对 gioui 像素渲染做端到端测试（成本高、收益低）。聚焦可单测的纯逻辑：

- 文字记录环形缓冲：追加、容量上限（满则丢最旧）、并发读写（`-race`）。
- `connCount()`：register/unregister 后计数正确。
- `qr.go` 的 `urlToImage(url)`：返回非 nil `image.Image`、尺寸 > 0、不同 URL 产生
  不同图像。
- `findCJKFont()`：用临时目录模拟有/无字体路径，分别返回路径或空。

**手动验收清单（跨平台）**：窗口能开 → 二维码扫得出 → 手机连上后状态点变绿、
计数 +1 → 说话后文字出现在记录区且中文非豆腐 → Copy URL 可粘贴 → Rotate 后旧手机
被踢、需新码重连。

## 体积影响（预估）

| 项 | 现在 | 加 gioui 后 |
|---|---|---|
| Windows amd64 | 7.4 MB | ~12–16 MB |
| 依赖 CGO | 否 | **仍然否**（gioui 走系统 API / Direct3D / Vulkan） |
| 前端构建链 | 无 | **仍然无** |

移除 systray 及其 `getlantern/*` 间接依赖可抵掉部分增量。

## 不做（YAGNI）

- 不保留 systray 托盘（窗口替代之）。
- 不做手机端网页美化（本次范围只在桌面端窗口）。
- 不嵌入字体文件。
- 不做 gioui 像素级端到端 UI 测试。
- 不引入设置持久化 / 主题切换 / 多语言界面等额外特性。
