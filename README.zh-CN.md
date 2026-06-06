<div align="center">

<img src="cmd/phonemic/assets/logo.svg" width="96" height="96" alt="PhoneMic logo" />

# PhoneMic

**把手机变成无线麦克风，对着说话就能在电脑上打字。**

[English](README.md) · **简体中文**

</div>

对着手机说话 → 手机浏览器自带的 Web Speech API 把语音识别成文字 →
通过 WebSocket 推回桌面 → 桌面把文字粘贴到当前获得焦点的窗口里。

- **单文件二进制** —— 免安装、免运行时、无前端构建链。
- **手机零安装** —— 浏览器打开一个网址即可，不用装 App。
- **桌面图形窗口** —— 内嵌二维码、实时连接状态、滚动文字记录，集中在一个窗口。
- **中英文都能打**（以及 Web Speech API 支持的任意语言）。

---

## 软件截图

| 桌面窗口 | 手机网页 |
|:---:|:---:|
| <img src="docs/images/app-window.png" width="320" alt="PhoneMic 桌面窗口" /> | <img src="docs/images/web-page.png" width="240" alt="PhoneMic 手机网页" /> |
| 二维码 · 连接状态 · 配对码 · 实时文字记录 | 按住说话 · 编辑后发送 · 语言切换 |

---

## 工作原理

```
[ 手机浏览器 ]
    │  按住录音 → MediaStream
    │  Web Speech API 识别 → 最终文本
    ▼
[ WebSocket  /ws?code=XXXXXX ]   ← 配对码校验
    │
[ 桌面端（Go + gioui） ]
    │  写入剪贴板   （atotto/clipboard）
    │  模拟 Ctrl+V / Cmd+V （micmonay/keybd_event）
    ▼
[ 当前获得焦点的输入框 ]
```

桌面与手机在同一局域网内通过 **HTTPS + WebSocket** 通信。必须用 HTTPS，
因为 Web Speech API 只在「安全上下文」下才能使用麦克风。

---

## 快速开始（从源码运行）

需要 **Go 1.25+**。Windows 上无需 CGO 工具链。

```bash
git clone https://github.com/AnotherJ1/PhoneMic.git
cd PhoneMic/cmd/phonemic
go mod tidy
go run .
```

会弹出一个 **PhoneMic 窗口**（自动屏幕居中），界面包含：

| 元素 | 作用 |
|---|---|
| **二维码** | 用手机扫码直接打开连接页 |
| **连接地址** | `https://192.168.x.x:PORT/?code=ABCXYZ` —— 点「复制地址」复制 |
| **配对码** | 6 位 —— 点「换配对码」重新生成（会踢掉所有已连手机） |
| **状态胶囊** | 右上角；有手机连上时变绿并显示连接数 |
| **实时文字记录** | 最近注入的文本（含时间戳，新的在上，最多 50 条） |

在手机上（同一 Wi-Fi）：

1. 扫二维码，或打开复制的连接地址。
2. 浏览器提示「连接不安全」—— 这是本地自签证书的正常现象，点
   **高级 → 继续前往** 即可。
3. 按住「按住说话」开口讲。默认开启「编辑后发送」，识别结果会先进编辑框，
   你检查/修改后再点「发送到电脑」。
4. 文字出现在电脑光标处。

> 关闭窗口即退出整个程序（后台 HTTPS 服务随之停止）。

---

## 构建发布版二进制

编译出的二进制是自包含的（网页资源、TLS 证书逻辑、窗口图标都已嵌入），
拷到任意同架构机器双击即可运行。

### 当前平台（用 `build.sh`）

```bash
bash cmd/phonemic/build.sh
```

脚本会探测当前操作系统，编译出对应平台的二进制到 `cmd/phonemic/dist/`。

### Windows 手动构建

```bash
cd cmd/phonemic
CGO_ENABLED=0 go build -trimpath -ldflags "-s -w -H windowsgui" -o phonemic.exe .
```

`-H windowsgui` 去掉控制台黑窗；`-s -w` 去除调试信息。
当前 Windows amd64 发布版约 **13 MB**。

### 一次出齐所有平台（GitHub Actions）

> ⚠️ **无法在一台机器交叉编译所有平台。** gioui 的 Windows 后端是纯 Go
> （Direct3D），但 macOS（Metal/Cocoa）和 Linux（Vulkan/X11/Wayland）后端
> 都需要 **CGO 且只能在对应原生系统上编译**。

推一个 `v*` tag，
[`.github/workflows/release.yml`](.github/workflows/release.yml)
里的 CI 矩阵会在各自原生 runner 上编译全部四个目标并挂到 GitHub Release：

```bash
git tag v0.1.0
git push origin v0.1.0
```

| 目标 | Runner | CGO |
|---|---|---|
| `windows-amd64` | `windows-latest` | 关 |
| `linux-amd64` | `ubuntu-latest`（自动装 gioui 依赖） | 开 |
| `darwin-amd64` | `macos-13`（Intel） | 开 |
| `darwin-arm64` | `macos-14`（Apple Silicon） | 开 |

---

## 重新生成 LOGO / 图标

LOGO 源文件是
[`cmd/phonemic/assets/logo.svg`](cmd/phonemic/assets/logo.svg)。
要重新生成网页 favicon 和嵌入 Windows 的图标：

```bash
cd cmd/phonemic/assets
npm i sharp                       # 一次性，用于 SVG → PNG
node render.js                    # 生成网页 favicon 与 ico/*.png
cd winres && go-winres make --in winres.json --out ../../rsrc
# → 产出 rsrc_windows_*.syso，下次 go build 时自动嵌入
```

gioui 会从可执行文件加载资源 ID 为 `1` 的图标，因此 `.syso` 会让窗口和任务栏
自动显示 PhoneMic 图标。

---

## 手机端选项

| 选项 | 说明 |
|---|---|
| **语言 pill** | 点击在 `zh-CN` / `en-US` 之间循环切换 |
| **连续模式** | 改为「点一下开始 / 点一下停止」，长文本不用一直按住；静音自动停后会自动重启 |
| **编辑后发送**（默认开启） | 识别结果先进编辑框，改好后再点「发送到电脑」，避免识别错的内容直接打到电脑 |

---

## 安全模型

- 监听 `0.0.0.0:<随机端口>`；连接地址只显示 RFC1918 私网 IP
  （192.168 / 10.x / 172.16–31）。
- `/ws` **必须**带 `?code=XXXXXX`；不匹配返回 HTTP 403。
- 6 位配对码为 `[A-Z2-9]`（去掉 `0/O/1/I`），来自 `crypto/rand`。
- 「换配对码」会重新生成并强制关闭所有现有连接。
- 自签 HTTPS：证书 SAN 含 LAN IP、`127.0.0.1`、`localhost`，并缓存复用。
  权威 CA 无法给局域网 IP 签发证书，所以浏览器的「不安全」提示无法消除 ——
  在此场景下点一次「继续」是安全的。

---

## 已知限制

- **安卓 Chrome 的 Web Speech API 依赖 Google 服务器。** 它把音频上传到
  Google 做识别，网络连不到 Google 时会报 `sr error network`、识别不出内容。
  iOS Safari 走 Apple 引擎，不受影响。要彻底离线，需把语音识别搬到桌面端
  （如 whisper.cpp）—— 属于另一轮工作。
- **iOS Safari 需 14+** 才有 `webkitSpeechRecognition`。
- **Linux 按键注入需要 uinput 权限：**
  `sudo usermod -aG input $USER`，然后重新登录。
- **macOS 第一次按 Cmd+V 会弹「辅助功能」权限**：在 系统设置 → 隐私与安全性 →
  辅助功能 里允许 *phonemic*。
- 注入用剪贴板做载体，但桌面端会在粘贴后约 150ms 自动恢复你原来的剪贴板内容。

---

## 协议参考

WebSocket，客户端 → 服务端：

```json
{ "type": "text", "text": "今天天气不错" }
{ "type": "reset" }
{ "type": "ping" }
```

WebSocket，服务端 → 客户端：

```json
{ "type": "pong" }
```

HTTP：

- `GET /` → 静态前端（`index.html`）
- `GET /info` → `{ code, port, ip, version }`
- `GET /ws?code=XXXXXX` → WebSocket 升级（code 不匹配返回 403）

---

## 技术栈

桌面端是**纯 Go**；Windows 上构建**无需 CGO**。

| 依赖 | 作用 |
|---|---|
| `gioui.org` | 桌面图形窗口（Direct3D / Metal / Vulkan） |
| `github.com/gorilla/websocket` | WebSocket 传输 |
| `github.com/atotto/clipboard` | 剪贴板读写 |
| `github.com/micmonay/keybd_event` | 键盘（粘贴）模拟 |
| `github.com/skip2/go-qrcode` | 二维码生成 |

---

## 许可证

MIT 或 Apache-2.0，任选其一。
