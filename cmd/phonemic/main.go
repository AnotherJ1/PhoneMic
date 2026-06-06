// PhoneMic — 手机当无线麦克风给电脑打字（方案 A：Go + gioui + Web Speech API）
//
// 启动后：
//  1. 在 LAN 上随机端口起一个 HTTPS+WebSocket 服务
//  2. 弹出一个 gioui 桌面窗口：内嵌配对二维码 / 连接状态 / 文字记录 / 配对码操作
//  3. 手机浏览器访问 https://<电脑IP>:<port>/?code=XXXX 后，能录音并通过 WS
//     把识别出的文字发回桌面端，桌面端写剪贴板并模拟 Ctrl/Cmd+V 注入当前焦点窗口
//
// 设计取舍：
//   - 桌面端用 gioui 单窗口（纯 Go、无 CGO、无前端构建链），替代早期的 systray 托盘
//   - 语音识别完全交给手机浏览器自带的 Web Speech API，免模型免下载
//   - 单 binary，体积目标 < 16 MB
package main

import (
	"crypto/rand"
	"crypto/tls"
	"embed"
	"encoding/json"
	"fmt"
	"io/fs"
	"log"
	"net"
	"net/http"
	"os"
	"runtime"
	"strings"
	"sync"
	"time"
	"unicode"

	"github.com/atotto/clipboard"
	"github.com/gorilla/websocket"
	"github.com/micmonay/keybd_event"
)

//go:embed web/*
var webFS embed.FS

// 配对码：6 位大写字母+数字，足够防误连，又便于手输
const pairCodeLen = 6
const pairAlphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789" // 去掉易混淆字符 0/O/1/I

// 文字记录环形缓冲容量：满则丢最旧，防止长时间运行内存增长
const textLogCap = 50

// textRecord 是一条注入成功的文字记录，供 UI 文字记录区展示
type textRecord struct {
	t    time.Time
	text string
}

type appState struct {
	mu       sync.RWMutex
	pairCode string
	port     int
	lanIP    string
	// 当前所有活跃的 WebSocket 连接；轮换配对码时需要全部踢掉
	conns map[*websocket.Conn]struct{}
	// 文字记录环形缓冲：固定容量，最近 textLogCap 条注入文本（含时间戳）。
	// 用切片 + 头指针实现的环形队列；texts[head] 是下一个写入位（即最旧）。
	texts []textRecord
	head  int
	size  int
}

func (s *appState) snapshot() (string, int, string) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.pairCode, s.port, s.lanIP
}

// connCount 返回当前活跃 WebSocket 连接数，供 UI 状态点 / 计数显示
func (s *appState) connCount() int {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return len(s.conns)
}

// addText 在成功注入一段文本后追加一条记录到环形缓冲；满容量时覆盖最旧。
func (s *appState) addText(now time.Time, text string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.texts == nil {
		s.texts = make([]textRecord, textLogCap)
	}
	rec := textRecord{t: now, text: text}
	if s.size < textLogCap {
		// 缓冲未满：写到逻辑队尾
		idx := (s.head + s.size) % textLogCap
		s.texts[idx] = rec
		s.size++
	} else {
		// 已满：覆盖最旧（head），head 前移
		s.texts[s.head] = rec
		s.head = (s.head + 1) % textLogCap
	}
}

// recentTexts 返回文字记录快照副本，新的在前（供 UI 单向只读，不持有内部切片）。
func (s *appState) recentTexts() []textRecord {
	s.mu.RLock()
	defer s.mu.RUnlock()
	out := make([]textRecord, s.size)
	for i := 0; i < s.size; i++ {
		// 物理索引从最新往最旧遍历：最新是 (head+size-1)
		idx := (s.head + s.size - 1 - i + textLogCap) % textLogCap
		out[i] = s.texts[idx]
	}
	return out
}

// 注册一条新的 ws 连接，并返回 unregister 函数
func (s *appState) registerConn(c *websocket.Conn) func() {
	s.mu.Lock()
	if s.conns == nil {
		s.conns = make(map[*websocket.Conn]struct{})
	}
	s.conns[c] = struct{}{}
	s.mu.Unlock()
	return func() {
		s.mu.Lock()
		delete(s.conns, c)
		s.mu.Unlock()
	}
}

// 轮换配对码：生成新码、踢掉所有用旧码的连接
func (s *appState) rotateCode() string {
	newCode := genPairCode()
	s.mu.Lock()
	s.pairCode = newCode
	// 关闭所有旧连接，逼客户端重连（重连时如果还带旧 code 会被 403 拒绝）
	for c := range s.conns {
		_ = c.WriteMessage(websocket.CloseMessage,
			websocket.FormatCloseMessage(websocket.ClosePolicyViolation, "PAIR_CODE_ROTATED"))
		_ = c.Close()
	}
	s.conns = make(map[*websocket.Conn]struct{})
	s.mu.Unlock()
	log.Printf("[pair] rotated -> %s", newCode)
	return newCode
}

// 生成配对码：用 crypto/rand 防可预测
func genPairCode() string {
	b := make([]byte, pairCodeLen)
	if _, err := rand.Read(b); err != nil {
		panic(err)
	}
	for i := range b {
		b[i] = pairAlphabet[int(b[i])%len(pairAlphabet)]
	}
	return string(b)
}

// 找一个 RFC1918 私网 IPv4，用作配对 URL 的主机
//
// 排除规则：
//   - DOWN / loopback / point-to-point / virtual 网卡
//   - Docker (172.17-31.x 中的 docker0、br-*、vEthernet 等)
//   - WSL (172.18-19.x 通常)
//
// 选择优先级：192.168.x.x > 10.x > 真实物理 172.16-31
func detectLanIP() string {
	ifaces, err := net.Interfaces()
	if err != nil {
		return "127.0.0.1"
	}
	type cand struct {
		ip       string
		priority int // 数字越小越优先
	}
	var cands []cand
	for _, iface := range ifaces {
		if iface.Flags&net.FlagUp == 0 || iface.Flags&net.FlagLoopback != 0 {
			continue
		}
		// 跳过常见虚拟网卡：名字含 docker / veth / vEthernet / WSL / Loopback Pseudo-Interface
		nameLower := strings.ToLower(iface.Name)
		if containsAny(nameLower, []string{"docker", "veth", "vethernet", "wsl", "vmware", "virtualbox", "hyper-v", "loopback"}) {
			continue
		}
		addrs, _ := iface.Addrs()
		for _, addr := range addrs {
			ipnet, ok := addr.(*net.IPNet)
			if !ok {
				continue
			}
			ip := ipnet.IP.To4()
			if ip == nil {
				continue
			}
			// 优先级：192.168 > 10.x > 172.16-31（真物理）
			switch {
			case ip[0] == 192 && ip[1] == 168:
				cands = append(cands, cand{ip.String(), 1})
			case ip[0] == 10:
				cands = append(cands, cand{ip.String(), 2})
			case ip[0] == 172 && ip[1] >= 16 && ip[1] <= 31:
				cands = append(cands, cand{ip.String(), 3})
			}
		}
	}
	if len(cands) == 0 {
		return "127.0.0.1"
	}
	best := cands[0]
	for _, c := range cands[1:] {
		if c.priority < best.priority {
			best = c
		}
	}
	return best.ip
}

// containsAny 判断 s 是否包含 subs 中任意一个子串
func containsAny(s string, subs []string) bool {
	for _, sub := range subs {
		if strings.Contains(s, sub) {
			return true
		}
	}
	return false
}

// WebSocket 升级器；同源内网用，不做严格 Origin 校验
var wsUpgrader = websocket.Upgrader{
	ReadBufferSize:  1024,
	WriteBufferSize: 1024,
	CheckOrigin:     func(*http.Request) bool { return true },
}

// 客户端发来的 WS 消息
type clientMsg struct {
	Type string `json:"type"` // "text" | "ping" | "reset"
	Text string `json:"text,omitempty"`
}

// 启动 HTTP + WS 服务
func startServer(state *appState) (int, error) {
	mux := http.NewServeMux()

	// 静态前端：embed 进 binary
	subFS, err := fs.Sub(webFS, "web")
	if err != nil {
		return 0, err
	}
	mux.Handle("/", http.FileServer(http.FS(subFS)))

	// /info：返回当前 IP / 配对码（前端开机时拉一次）
	mux.HandleFunc("/info", func(w http.ResponseWriter, r *http.Request) {
		code, port, ip := state.snapshot()
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{
			"code": code, "port": port, "ip": ip,
			"version": "0.1.0",
		})
	})

	// /ws：WebSocket，必须带 ?code=XXXX 通过配对校验后才接受文本
	mux.HandleFunc("/ws", func(w http.ResponseWriter, r *http.Request) {
		code, _, _ := state.snapshot()
		if r.URL.Query().Get("code") != code {
			http.Error(w, "PAIR_CODE_INVALID", http.StatusForbidden)
			return
		}
		conn, err := wsUpgrader.Upgrade(w, r, nil)
		if err != nil {
			return
		}
		defer conn.Close()
		unregister := state.registerConn(conn)
		defer unregister()
		log.Printf("[ws] client connected from %s", r.RemoteAddr)
		for {
			_, raw, err := conn.ReadMessage()
			if err != nil {
				log.Printf("[ws] disconnect: %v", err)
				return
			}
			var msg clientMsg
			if err := json.Unmarshal(raw, &msg); err != nil {
				continue
			}
			switch msg.Type {
			case "text":
				if msg.Text != "" {
					inject.write(msg.Text)
					// 注入后追加一条文字记录，供窗口的文字记录区展示
					state.addText(time.Now(), msg.Text)
				}
			case "reset":
				// 新一段录音开始：清空补空格状态，避免与上段黏连
				inject.resetSpacing()
			case "ping":
				_ = conn.WriteJSON(map[string]string{"type": "pong"})
			}
		}
	})

	// 自签证书：SAN 必须含当前 LAN IP，否则手机 Chrome 仍拒绝麦克风
	_, _, ip := state.snapshot()
	cert, err := loadOrCreateCert(ip)
	if err != nil {
		return 0, fmt.Errorf("load cert: %w", err)
	}

	// 监听 0.0.0.0:0 让系统挑空闲端口
	ln, err := net.Listen("tcp", "0.0.0.0:0")
	if err != nil {
		return 0, err
	}
	port := ln.Addr().(*net.TCPAddr).Port

	srv := &http.Server{
		Handler:   mux,
		TLSConfig: &tls.Config{Certificates: []tls.Certificate{cert}},
	}
	go func() {
		// 证书已在 TLSConfig 内，ServeTLS 的文件参数留空
		if err := srv.ServeTLS(ln, "", ""); err != nil && err != http.ErrServerClosed {
			log.Printf("[https] serve error: %v", err)
		}
	}()
	return port, nil
}

// injector 负责把识别出的文本注入当前活动窗口。
//
// 跨平台 + 支持中文的最稳路径：写剪贴板 → 模拟 Ctrl+V（macOS 是 Cmd+V）。
// 不依赖 CGO，体积小，不需要 GCC 工具链。
//
// 两个体验优化：
//   - save/restore：注入前保存用户原剪贴板，paste 后恢复，避免污染。
//   - 智能补空格：跨消息记录上一段尾字符，若上段结尾与本段开头均为
//     ASCII 字母/数字（典型英文听写），自动补一个空格防止单词黏连；
//     中文/标点之间不补。
type injector struct {
	mu       sync.Mutex
	lastRune rune // 上一次注入文本的最后一个字符（0 表示尚无）
}

var inject = &injector{}

// shouldPrependSpace 判断本段开头是否需要补空格：仅当上段尾与本段首都是
// ASCII 字母或数字时补（避免把英文单词黏在一起）。
func shouldPrependSpace(prev, next rune) bool {
	isWordChar := func(r rune) bool {
		return r < unicode.MaxASCII && (unicode.IsLetter(r) || unicode.IsDigit(r))
	}
	return isWordChar(prev) && isWordChar(next)
}

func (in *injector) write(text string) {
	if text == "" {
		return
	}
	in.mu.Lock()
	// 智能补空格：英文片段之间补，中文之间不补
	first := []rune(text)[0]
	if in.lastRune != 0 && shouldPrependSpace(in.lastRune, first) {
		text = " " + text
	}
	r := []rune(text)
	in.lastRune = r[len(r)-1]
	in.mu.Unlock()

	log.Printf("[inject] %q", text)

	// 1. 保存用户原剪贴板（失败不致命，只是不恢复）
	prevClip, prevErr := clipboard.ReadAll()

	// 2. 写入待注入文本并粘贴
	if err := clipboard.WriteAll(text); err != nil {
		log.Printf("[inject] clipboard write failed: %v", err)
		return
	}
	time.Sleep(30 * time.Millisecond) // 给前台窗口让点时间稳定
	if err := pasteHotkey(); err != nil {
		log.Printf("[inject] paste hotkey failed: %v", err)
	}

	// 3. 延迟恢复旧剪贴板：粘贴是异步的，太快恢复会粘到旧内容
	if prevErr == nil {
		go func(old string) {
			time.Sleep(150 * time.Millisecond)
			if err := clipboard.WriteAll(old); err != nil {
				log.Printf("[inject] clipboard restore failed: %v", err)
			}
		}(prevClip)
	}
}

// resetSpacing 在一段录音开始时清空尾字符状态，避免与上一段口述黏连补空格
func (in *injector) resetSpacing() {
	in.mu.Lock()
	in.lastRune = 0
	in.mu.Unlock()
}

// 发送 Ctrl+V（macOS 上为 Cmd+V）
func pasteHotkey() error {
	kb, err := keybd_event.NewKeyBonding()
	if err != nil {
		return err
	}
	// Linux 上 NewKeyBonding 后需要 sleep 让 uinput 就绪
	if runtime.GOOS == "linux" {
		time.Sleep(2 * time.Second)
	}
	kb.SetKeys(keybd_event.VK_V)
	if runtime.GOOS == "darwin" {
		kb.HasSuper(true) // Cmd
	} else {
		kb.HasCTRL(true)
	}
	return kb.Launching()
}

// connectURL 拼出当前配对 URL（含 code），供 UI 复制按钮与二维码共用
func (s *appState) connectURL() string {
	code, port, ip := s.snapshot()
	return fmt.Sprintf("https://%s:%d/?code=%s", ip, port, code)
}

func main() {
	log.SetOutput(os.Stdout)

	state := &appState{
		pairCode: genPairCode(),
		lanIP:    detectLanIP(),
	}

	port, err := startServer(state)
	if err != nil {
		log.Fatalf("[main] startServer: %v", err)
	}
	state.mu.Lock()
	state.port = port
	state.mu.Unlock()

	log.Printf("[main] listening on https://%s:%d  pair=%s", state.lanIP, port, state.pairCode)

	// 进入 gioui 事件循环（占据主线程，替代原 systray.Run）。
	// 窗口关闭即退出整个进程，server goroutine 随之结束。
	runUI(state)
}
