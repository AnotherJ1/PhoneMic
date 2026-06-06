# PhoneMic (方案 A — Go + gioui)

把手机当无线麦克风，对桌面打字。

## 运行

```bash
cd cmd/phonemic
go mod tidy
go run .
```

启动后弹出一个 PhoneMic 桌面窗口：

- **二维码**：手机扫码直接打开连接页
- **连接地址**：`https://192.168.x.x:PORT/?code=ABCXYZ`，点「复制地址」可粘贴
- **配对码**：6 位，点「换配对码」可轮换（旧连接会被踢，需用新码重连）
- **连接状态**：右上角胶囊，有手机连上变绿并显示连接数
- **实时文字记录**：最近收到并注入的文本（含时间戳，最多 50 条）

手机用同一 Wi-Fi 浏览器扫码（或访问上面的连接地址）→ 首次会提示证书不安全，
点「高级 → 继续前往」即可 → 按住「按住说话」（或默认的「编辑后发送」改完再发）→
识别出的文本通过 WS 推回桌面，写剪贴板并模拟 Ctrl/Cmd+V 打到当前光标处。

> 关闭窗口即退出整个程序（后台 HTTPS 服务随进程结束）。

## 打包发布

单文件、无外部依赖、无 CGO，编译出的 exe 可拷到任意同架构电脑直接双击运行：

```bash
# Windows（GUI 子系统，无控制台黑窗）
CGO_ENABLED=0 go build -trimpath -ldflags "-s -w -H windowsgui" -o phonemic.exe .

# macOS / Linux 见 build.sh
bash build.sh
```

当前 Windows amd64 release 约 **12–13 MB**。

## 设计要点

- **零前端工具链**：单 HTML，浏览器自带 Web Speech API，免下载模型
- **桌面 GUI 用 gioui**：纯 Go、无 CGO、无前端构建链，单窗口承载二维码 / 状态 / 记录 / 配对操作（替代早期 systray 托盘）
- **单 binary**：HTML 与图标用 `embed` 烤进 exe
- **中文显示**：运行时探测并加载系统 CJK 字体（微软雅黑 / PingFang / Noto），不嵌入字体（体积优先）；找不到时降级为缺字，不崩溃
- **配对码**：6 位避混淆字符，`/ws` 必须带 `?code=` 校验
- **自签 HTTPS**：Web Speech API 需安全上下文；SAN 含 LAN IP，证书缓存复用。
  局域网 IP 无法由权威 CA 签发，故首次访问浏览器必报"不安全"，点继续即可
- **LAN-only**：监听 `0.0.0.0` 但只在私网 IP 提供 connect URL（不做反向代理）

## 已知限制 / TODO

- **Web Speech 依赖 Google 语音服务**：安卓 Chrome 的识别走 Google 云端，网络
  连不到 Google 时报 `sr error network`（无法识别）。iOS Safari 走 Apple，不受此限。
  彻底离线需把识别搬到桌面端（如 whisper.cpp），属另一轮工作
- mDNS 发现（v2，不影响第一版）
