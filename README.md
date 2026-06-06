# PhoneMic

把手机当成无线麦克风给电脑打字。手机录音 → 浏览器自带的 Web Speech API 识别成文字 → WebSocket 推回桌面 → 桌面用剪贴板 + Ctrl/Cmd+V 把文字打到当前光标处。

- 单 binary，~7 MB（release）
- 桌面零 UI，只有系统托盘菜单
- 手机零安装，浏览器打开 URL 就能用
- 中文/英文都能打（Web Speech API 支持的语言均可）

## 快速开始

```bash
cd cmd/phonemic
go run .
```

控制台会打印一行：

```
[main] listening on http://192.168.x.x:PORT  pair=ABCXYZ
```

右下角任务栏托盘出现 PhoneMic 蓝色图标。右键看到：

| 菜单项 | 行为 |
|---|---|
| `Connect URL: http://...` | 点击复制完整 URL（含配对码） |
| `Pair code: ABCXYZ` | 点击复制配对码 |
| `Show QR code…` | 用默认看图器打开二维码 PNG |
| `Regenerate pair code` | 轮换配对码，所有现有连接被踢，必须用新码重连 |
| `Quit` | 退出 |

手机用同一 Wi-Fi 浏览器（**Chrome / Edge / Safari**，Firefox 不支持 Web Speech API）打开复制的 URL → 浏览器请求麦克风权限点允许 → 按住"按住说话"→ 松开停止。识别出的文字会通过 WebSocket 推回桌面，由桌面端写剪贴板 + 模拟 Ctrl+V 打到当前焦点输入框。

### 手机端选项

| 选项 | 说明 |
|---|---|
| 语言 pill | 点击在 `zh-CN` / `en-US` 之间循环切换 |
| 连续模式 | 勾选后改为"点一下开始 / 点一下停止"，长文本不用一直按住；浏览器静音自动停止时会自动重启录音 |
| 编辑后发送 | 勾选后识别结果先进编辑框，可手动修改后点"发送到电脑"，避免识别错的内容直接打到电脑 |

## Release 构建

```bash
bash cmd/phonemic/build.sh
```

输出到 `cmd/phonemic/dist/`，包含：
- `phonemic-windows-amd64.exe`（GUI 子系统，无控制台窗口）
- `phonemic-darwin-amd64`、`phonemic-darwin-arm64`
- `phonemic-linux-amd64`

体积参考（release，未 UPX 压缩）：

| 平台 | 体积 |
|---|---|
| Windows amd64 | 7.4 MB |
| macOS amd64 | ~7 MB |
| macOS arm64 | ~7 MB |
| Linux amd64 | ~7 MB |

## 架构

```
[手机浏览器]
   |  按住录音 → MediaStream
   |  Web Speech API 识别 → final text
   v
[WebSocket /ws?code=XXX]   ← 配对码校验
   |
[Go 桌面端]
   |  写剪贴板（atotto/clipboard）
   |  按 Ctrl+V / Cmd+V（micmonay/keybd_event）
   v
[当前焦点的输入框]
```

依赖包（全部纯 Go，**无 CGO**）：

- `github.com/getlantern/systray` — 系统托盘
- `github.com/gorilla/websocket` — WebSocket
- `github.com/atotto/clipboard` — 剪贴板读写
- `github.com/micmonay/keybd_event` — 键盘按键模拟
- `github.com/skip2/go-qrcode` — 二维码

## 安全

- 监听 `0.0.0.0:` 随机端口；菜单仅显示 RFC1918 私网 IP（192.168 / 10.x / 172.16-31）
- `/ws` 必须带 `?code=XXX` 查询参数；`code` 不匹配返回 403
- 配对码 6 位 [A-Z2-9]（去除 0/O/1/I），来自 `crypto/rand`
- 通过 `Regenerate pair code` 菜单可随时轮换

## 已知限制

- **Web Speech API 在 iOS Safari 上需要 14+**；旧版 Safari 没有 `webkitSpeechRecognition`。
- **Linux 上键盘模拟需要 uinput 权限**：把当前用户加入 `input` 组：`sudo usermod -aG input $USER` 后重登录。
- **macOS 第一次按 Cmd+V 会弹"辅助功能"权限**：在系统设置 → 隐私与安全 → 辅助功能 中允许 phonemic。
- **剪贴板**：注入用剪贴板做载体，但桌面端会在粘贴后 ~150ms 自动恢复用户原剪贴板内容，正常使用不会丢失。

## 协议

WebSocket 消息（client → server）：

```json
{ "type": "text", "text": "今天天气不错" }
{ "type": "ping" }
```

WebSocket 消息（server → client）：

```json
{ "type": "pong" }
```

HTTP：

- `GET /` → 静态前端 `index.html`
- `GET /info` → `{ code, port, ip, version }` JSON
- `GET /ws?code=XXX` → WebSocket 升级，403 if code mismatch

## License

MIT or Apache-2.0, at your option.
