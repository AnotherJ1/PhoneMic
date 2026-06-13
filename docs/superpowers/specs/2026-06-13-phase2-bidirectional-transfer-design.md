# 第二期设计：电脑→手机下行传输 + 接收/发送双 Tab UI 重构

日期：2026-06-13
版本目标：v0.1.4

## 背景与目标

第一期已实现手机→电脑的图片/文件上传。第二期补齐**反方向**（电脑→手机）的文本与文件
推送，并把单向工具升级为**双向中转站**。同时重新规划 UI：用「接收 / 发送」两个 Tab
分别承载两个传输方向，各自带独立的传输记录；并替换旧的扁平深色风格为**暖调极简浅色**。

## 范围

做：
- 电脑→手机推送**文本**（WS 广播，手机接收区显示 + 一键复制）
- 电脑→手机推送**文件**（PC 拖拽文件进窗口 → WS 通知 → 手机从 /download 拉取）
- PC 窗口与手机网页重构为「接收 / 发送」双 Tab
- 全新暖调极简浅色视觉风格（双端一致）
- 版本号升至 v0.1.4

不做（已明确排除）：
- 手机→电脑方向的新增（第一期已覆盖）
- 双向剪贴板自动同步
- 下行文件持久化（进程退出即清）

## 关键决策（已与用户确认）

| 决策点 | 结论 |
|---|---|
| 下行文件通道 | WS 通知 + `GET /download?id=&code=` 拉取 |
| 下行文件暂存 | 内存 `map[id]→{name,bytes,contentType}`，进程退出自然清空 |
| 下行文本 | WS 广播，手机接收区显示 + 一键复制（不自动写手机剪贴板） |
| PC 选文件方式 | Windows syscall 调 comdlg32 GetOpenFileName 弹原生文件选择框（见下方技术约束） |
| UI 结构 | 配对卡固定顶部 + 接收/发送双 Tab |
| 视觉风格 | 暖调极简浅色 |

## 技术约束（实测确认）

**gioui v0.10.0 不支持从操作系统拖文件进窗口。** 实测查证：
`app/os_windows.go` 的 `transfer.DataEvent` 仅由 `readClipboard()`（CF_UNICODETEXT）
产生；全平台均无 `WM_DROPFILES` / `registerForDraggedTypes` / file-drop 公开 API。
故第二期 PC 发文件改用 **Windows syscall 调 comdlg32 `GetOpenFileNameW`** 弹原生
文件选择对话框，纯 Go + syscall、无 CGO。仅实现 Windows；mac/Linux 选文件后续再补
（届时各自调 NSOpenPanel / zenity 或 portal）。非 Windows 平台该按钮可降级为禁用 +
提示，不阻断编译。

## 配色（双端统一）

| 用途 | 色值 |
|---|---|
| 背景 | `#FAF7F2` 暖米白 |
| 卡片 | `#FFFFFF` 纯白 |
| 主文字 | `#1A1A1A` 墨黑 |
| 次要文字 | `#6B6B6B` 暖灰 |
| 更弱文字 | `#9B9B94` |
| 强调色 | `#E8743B` 琥珀橙 |
| 强调按下 | `#D15F28` |
| 连接绿点 | `#3DAA6D` |
| 分隔/边框 | `#ECE7DF` |
| 内嵌浅块 | `#F4F1EA` |

字体：桌面端沿用现有字体加载（gofont + CJK fallback）；网页端用系统字体栈，
标题可选 Space Grotesk fallback（不强制下载，保持零资源原则）。

## 架构

```
┌─ PC (gioui 窗口) ──────────┐         ┌─ 手机 (网页) ──────────┐
│ 顶部: 配对卡(QR/码/按钮)    │         │ 顶部: 配对卡 + 状态     │
│ ┌ 接收 tab ┐ ┌ 发送 tab ┐  │  WS     │ ┌ 接收 ┐ ┌ 发送 ┐      │
│ │语音文字  │ │文本输入框│  │ ←推送→  │ │收到的│ │语音+ │      │
│ │+上传记录 │ │[选文件]  │  │         │ │文本/ │ │上传  │      │
│ │          │ │发送记录  │  │         │ │文件  │ │      │      │
│ └──────────┘ └──────────┘  │         │ └──────┘ └──────┘      │
└────────────────────────────┘         └────────────────────────┘
         │                                        ↑
   文件暂存内存 ←──── GET /download?id=&code= ─────┘
```

Tab 方向语义（站在用户视角，两端镜像）：
- PC「接收」= 收手机来的（语音文字、上传文件记录）
- PC「发送」= 发给手机（文本输入框、文件拖拽区、发送记录）
- 手机「接收」= 收电脑来的（推送文本、推送文件）
- 手机「发送」= 发给电脑（语音录入、图片/文件上传——即原有全部功能）

## 数据流

### 下行文本
1. PC 发送 tab 输入框输入 → 点「发送到手机」
2. 服务端 `state.broadcast({type:"push-text", id, text, t})` 遍历所有 conns 写 WS
3. 手机 `ws.onmessage` 收到 push-text → 接收区追加文本卡（带复制按钮）
4. PC 发送 tab 记录区追加一条「→ 文本」

### 下行文件
1. PC 发送 tab 点「选文件」→ comdlg32 GetOpenFileNameW 弹原生对话框 → 读取文件字节
2. 服务端存入 `pendingFiles[id] = {name, bytes, contentType}`
3. `state.broadcast({type:"push-file", id, name, size, t})`
4. 手机收到 push-file → 接收区追加文件卡，链接指向 `/download?id=<id>&code=<pair>`
5. 手机点击 → 浏览器原生下载
6. PC 发送 tab 记录区追加一条「→ 文件名」

### 鉴权与生命周期
- `/download` 复用配对码校验（与 /ws、/upload 同）
- 文件仅存内存，进程退出清空；不设 TTL（用户确认「随手传一下」场景足够）
- 内存上限：单文件复用 100MB 限制；pendingFiles 总量软上限（如 20 个/200MB），
  超出丢最旧，log 提示

## 组件分解

### 后端

**新文件 `transfer.go`**（下行核心）
- `pendingFile{ name string; data []byte; contentType string; t time.Time }`
- `appState` 扩展：`pending map[string]pendingFile` + `pendingMu` + 头插式淘汰
- `addPendingFile(name, data, ct) (id string)`：生成 id、存储、淘汰最旧
- `takePendingFile(id) (pendingFile, bool)`：读取（下载用，保留以便多设备重复下载）
- `handleDownload(state) http.HandlerFunc`：鉴权 → 查 id → 写响应头（Content-Disposition）+ 字节
- 下行记录环形缓冲：复用第一期 textRecord 思路，新增 `sentTexts`（发送 tab 记录）

**`main.go` 扩展**
- `appState.broadcast(v any)`：遍历 conns 用各自 writeJSON 串行写（已有写锁）
- 注册 `/download`
- clientMsg 新增类型：无（下行是服务端→手机，手机不主动发新类型；
  PC 发送由 UI 直接调 state 方法，不经 WS 客户端消息）

**`ui.go` 重构**
- 顶部配对卡保留
- 新增 tab 切换状态 `activeTab int`（0 接收 / 1 发送）+ 两个 tab 按钮 widget
- 接收 tab：现有「实时文字记录」列表（语音 + 上传记录）
- 发送 tab：`widget.Editor` 文本框 + 「发送文本」按钮 + 「选文件发送」按钮 + 发送记录列表
- 选文件：新文件 `filedialog_windows.go`（syscall comdlg32 GetOpenFileNameW）+
  `filedialog_other.go`（非 Windows 降级桩，返回 unsupported）。build tag 分平台。
- 全量替换调色板常量为暖调浅色

### 前端 `index.html`
- 顶部配对卡 + 状态（沿用）
- Tab 切换：发送 / 接收（默认「发送」，因主用途仍是手机发电脑）
- 发送 tab：现有全部（语音、编辑框、上传、回车、发送历史）
- 接收 tab：接收记录列表，push-text 显示文本卡（复制按钮），
  push-file 显示文件卡（下载链接 `/download?id=&code=`）
- WS onmessage 扩展解析 push-text / push-file
- 全量替换 CSS 配色为暖调浅色

## 错误处理
- 下行文件超 100MB：PC 端拒绝并 log（拖拽前难拿大小，落字节后判断）
- pendingFiles 超量：丢最旧 + log
- /download id 不存在：404
- /download 配对码错：403
- broadcast 时某连接写失败：log，不影响其他连接（容错广播）
- 手机自动写剪贴板不做（用户确认用手动复制按钮）

## 测试策略
- 后端：`/download` 鉴权（403/404/200）、pendingFiles 淘汰、broadcast 多连接
- 端到端：PC 发文本 → curl 模拟手机 WS 收到 push-text；PC「发」文件 →
  curl GET /download 拿回原字节
- UI：构建二进制实跑，确认 tab 切换、拖拽接收、发送记录
- 回归：第一期上传 + 语音功能不受影响；现有 go test 通过

## 实施顺序
1. 后端 transfer.go（pendingFiles + /download + broadcast）
2. 后端 main.go 接线
3. filedialog_windows.go + filedialog_other.go（comdlg32 选文件对话框，分平台）
4. 前端 index.html 双 tab + 接收区 + 新配色
5. PC ui.go 双 tab + 发送区（文本框 + 选文件） + 新配色
6. 版本号 → 0.1.4
7. 构建 + 端到端验证 + 回归
```
