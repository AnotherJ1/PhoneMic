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

// version 是应用版本号，作为 /info 的单一可信来源。
// 声明为 var（而非 const）以便发布时用 -ldflags "-X main.version=..." 覆盖。
var version = "0.1.0"

// 配对码：6 位大写字母+数字，足够防误连，又便于手输
const pairCodeLen = 6
const pairAlphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789" // 去掉易混淆字符 0/O/1/I

// maxConns 限制同时活跃的 WebSocket 连接数；超过则拒绝新连接，防止资源耗尽
const maxConns = 8

// 文字记录环形缓冲容量：满则丢最旧，防止长时间运行内存增长
const textLogCap = 50

// textRecord 是一条注入成功的文字记录，供 UI 文字记录区展示
type textRecord struct {
	t    time.Time
	text string
}

// wsClient 包裹一条 WebSocket 连接及其专属写锁。
// gorilla/websocket 不允许并发写同一连接；服务端的协议级 ping goroutine 与
// 读循环里的 pong 回复可能同时写，因此每条连接配一把写锁串行化所有写操作。
type wsClient struct {
	conn   *websocket.Conn
	writeM sync.Mutex
}

// writeMessage 串行化对该连接的写，供 ping goroutine 与读循环共用。
func (c *wsClient) writeMessage(messageType int, data []byte) error {
	c.writeM.Lock()
	defer c.writeM.Unlock()
	return c.conn.WriteMessage(messageType, data)
}

// writeJSON 串行化对该连接的 JSON 写（应用层 pong 等）。
func (c *wsClient) writeJSON(v any) error {
	c.writeM.Lock()
	defer c.writeM.Unlock()
	return c.conn.WriteJSON(v)
}

type appState struct {
	mu       sync.RWMutex
	pairCode string
	port     int
	lanIP    string
	// 当前所有活跃的 WebSocket 连接；轮换配对码时需要全部踢掉。
	// 值是带写锁的包裹，保证对同一连接的写串行（见 wsClient）。
	conns map[*websocket.Conn]*wsClient
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
func (s *appState) registerConn(conn *websocket.Conn) func() {
	_, un, _ := s.registerClient(conn)
	return un
}

// registerClient 登记一条连接并返回带写锁的包裹与 unregister 函数。
// 已达 maxConns 时返回 ok=false，调用方应拒绝该连接。
func (s *appState) registerClient(conn *websocket.Conn) (*wsClient, func(), bool) {
	s.mu.Lock()
	if s.conns == nil {
		s.conns = make(map[*websocket.Conn]*wsClient)
	}
	if len(s.conns) >= maxConns {
		s.mu.Unlock()
		return nil, nil, false
	}
	c := &wsClient{conn: conn}
	s.conns[conn] = c
	s.mu.Unlock()
	return c, func() {
		s.mu.Lock()
		delete(s.conns, conn)
		s.mu.Unlock()
	}, true
}

// 轮换配对码：生成新码、踢掉所有用旧码的连接
func (s *appState) rotateCode() string {
	newCode := genPairCode()
	s.mu.Lock()
	s.pairCode = newCode
	// 关闭所有旧连接，逼客户端重连（重连时如果还带旧 code 会被 403 拒绝）。
	// 写 CloseMessage 走连接写锁，避免与该连接的 ping goroutine / pong 回复并发写。
	for conn, c := range s.conns {
		_ = c.writeMessage(websocket.CloseMessage,
			websocket.FormatCloseMessage(websocket.ClosePolicyViolation, "PAIR_CODE_ROTATED"))
		_ = conn.Close()
	}
	s.conns = make(map[*websocket.Conn]*wsClient)
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

// WebSocket 保活参数：服务端每 pingPeriod 发一次协议级 ping，读侧 readWait 内
// 必须收到任意消息（pong / 应用层 ping / text），否则判定连接已死并断开。
// pingPeriod 必须明显小于 readWait，给网络往返留余量。
const (
	readWait   = 60 * time.Second
	pingPeriod = 30 * time.Second
)

// WebSocket 升级器；同源内网用，不做严格 Origin 校验
var wsUpgrader = websocket.Upgrader{
	ReadBufferSize:  1024,
	WriteBufferSize: 1024,
	CheckOrigin:     func(*http.Request) bool { return true },
}

// 客户端发来的 WS 消息
type clientMsg struct {
	Type string `json:"type"` // "text" | "ping" | "reset" | "enter"
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
			"version": version,
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
		// 连接上限：升级成功后立即登记，已满则礼貌关闭并退出
		client, unregister, ok := state.registerClient(conn)
		if !ok {
			_ = conn.WriteMessage(websocket.CloseMessage,
				websocket.FormatCloseMessage(websocket.ClosePolicyViolation, "MAX_CONNECTIONS"))
			log.Printf("[ws] rejected %s: max connections (%d) reached", r.RemoteAddr, maxConns)
			return
		}
		defer unregister()
		log.Printf("[ws] client connected from %s", r.RemoteAddr)

		// 死连接检测：设初始读超时，收到协议级 pong 时顺延。
		// 掉 Wi-Fi 而无 close 帧的僵尸连接会在 readWait 内触发 ReadMessage 超时退出。
		_ = conn.SetReadDeadline(time.Now().Add(readWait))
		conn.SetPongHandler(func(string) error {
			return conn.SetReadDeadline(time.Now().Add(readWait))
		})

		// 服务端定时发协议级 ping；写经连接写锁，与读循环里的 pong 回复串行。
		// 读循环退出时关闭 done 通知本 goroutine 收尾。
		done := make(chan struct{})
		defer close(done)
		go func() {
			ticker := time.NewTicker(pingPeriod)
			defer ticker.Stop()
			for {
				select {
				case <-ticker.C:
					if err := client.writeMessage(websocket.PingMessage, nil); err != nil {
						return
					}
				case <-done:
					return
				}
			}
		}()

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
					now := time.Now()
					inject.write(msg.Text)
					// 注入后追加一条文字记录，供窗口的文字记录区展示
					state.addText(now, msg.Text)
					// 同时落盘到历史日志文件，保留完整历史（窗口只留最近 50 条）
					history.append(now, msg.Text)
				}
			case "reset":
				// 新一段录音开始：清空补空格状态，避免与上段黏连
				inject.resetSpacing()
			case "enter":
				// 手机端按「回车」：在当前焦点窗口模拟一次 Enter 键
				inject.pressEnter()
			case "ping":
				// 应用层 ping：除回 pong 外也顺延读超时，与协议级 pong 等效
				_ = conn.SetReadDeadline(time.Now().Add(readWait))
				_ = client.writeJSON(map[string]string{"type": "pong"})
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
	// 整段注入是一个不可分割的临界区：补空格状态 + 剪贴板 保存→写→粘贴→恢复
	// 全程持锁。多台手机同时连接（UI 支持 N 台已连接）时，若不串行化，两次注入的
	// 剪贴板保存/恢复会相互覆盖、补空格状态也会错乱。因此整段持锁直到旧剪贴板恢复
	// 完成才释放，让每段文本的注入彻底原子化。单连接场景行为不变。
	in.mu.Lock()
	defer in.mu.Unlock()

	// 智能补空格：英文片段之间补，中文之间不补
	first := []rune(text)[0]
	if in.lastRune != 0 && shouldPrependSpace(in.lastRune, first) {
		text = " " + text
	}
	r := []rune(text)
	in.lastRune = r[len(r)-1]

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

	// 3. 同步恢复旧剪贴板：粘贴是异步的，太快恢复会粘到旧内容。
	//    必须在持锁期间完成（而非另起 goroutine），否则下一段注入会与本段恢复
	//    交叉，串行化形同虚设。代价是每段注入多 150ms，换取并发下的正确性。
	if prevErr == nil {
		time.Sleep(150 * time.Millisecond)
		if err := clipboard.WriteAll(prevClip); err != nil {
			log.Printf("[inject] clipboard restore failed: %v", err)
		}
	}
}

// resetSpacing 在一段录音开始时清空尾字符状态，避免与上一段口述黏连补空格
func (in *injector) resetSpacing() {
	in.mu.Lock()
	in.lastRune = 0
	in.mu.Unlock()
}

// pressEnter 在当前焦点窗口模拟一次 Enter 键。
// 走 injector.mu，与粘贴注入串行，避免与正在进行的注入并发按键。
// 同时清空补空格状态：回车后下一段文本属于新一行，不应与上段黏连补空格。
func (in *injector) pressEnter() {
	in.mu.Lock()
	defer in.mu.Unlock()
	in.lastRune = 0
	log.Printf("[inject] <enter>")
	if err := enterHotkey(); err != nil {
		log.Printf("[inject] enter hotkey failed: %v", err)
	}
}

// pasteKB 是复用的按键模拟器：只创建一次，避免每次粘贴都重建 + 在 Linux 上
// 重复等待 uinput 就绪（旧实现每次粘贴 sleep 2s，使 Linux 几乎不可用）。
//
// 并发安全说明：keybd_event.KeyBonding 不保证可并发使用，但本程序所有粘贴都
// 经 injector.mu 串行（见 injector.write 的整段临界区），故复用同一实例安全。
var (
	pasteKBOnce sync.Once
	pasteKB     keybd_event.KeyBonding
	pasteKBErr  error
)

// initPasteKB 惰性初始化按键模拟器：Linux 上的 2s uinput 就绪等待只在首次发生一次。
func initPasteKB() {
	pasteKBOnce.Do(func() {
		kb, err := keybd_event.NewKeyBonding()
		if err != nil {
			pasteKBErr = err
			return
		}
		// Linux 上 NewKeyBonding 后需要 sleep 让 uinput 就绪；只需一次，不必每次粘贴都等
		if runtime.GOOS == "linux" {
			time.Sleep(2 * time.Second)
		}
		kb.SetKeys(keybd_event.VK_V)
		if runtime.GOOS == "darwin" {
			kb.HasSuper(true) // Cmd
		} else {
			kb.HasCTRL(true)
		}
		pasteKB = kb
	})
}

// 发送 Ctrl+V（macOS 上为 Cmd+V），复用初始化好的模拟器
func pasteHotkey() error {
	initPasteKB()
	if pasteKBErr != nil {
		return pasteKBErr
	}
	return pasteKB.Launching()
}

// enterKB 是复用的「回车」按键模拟器：与 pasteKB 分开两个实例，因为它们绑定的
// 按键 / 修饰键不同（pasteKB 是 Ctrl/Cmd+V，enterKB 是裸 Enter），复用同一实例
// 会相互覆盖配置。同样只初始化一次，Linux 的 2s uinput 等待只发生在首次。
//
// 并发安全：所有按键都经 injector.mu 串行（见 pressEnter / write），故复用安全。
var (
	enterKBOnce sync.Once
	enterKB     keybd_event.KeyBonding
	enterKBErr  error
)

// initEnterKB 惰性初始化回车按键模拟器。
func initEnterKB() {
	enterKBOnce.Do(func() {
		kb, err := keybd_event.NewKeyBonding()
		if err != nil {
			enterKBErr = err
			return
		}
		// Linux 上 NewKeyBonding 后需要 sleep 让 uinput 就绪；只需一次。
		// 若 pasteKB 已先初始化过，这里仍需各自等待自己实例的 uinput 就绪。
		if runtime.GOOS == "linux" {
			time.Sleep(2 * time.Second)
		}
		kb.SetKeys(keybd_event.VK_ENTER)
		enterKB = kb
	})
}

// 发送 Enter 键，复用初始化好的模拟器
func enterHotkey() error {
	initEnterKB()
	if enterKBErr != nil {
		return enterKBErr
	}
	return enterKB.Launching()
}

// connectURL 拼出当前配对 URL（含 code），供 UI 复制按钮与二维码共用
func (s *appState) connectURL() string {
	code, port, ip := s.snapshot()
	return fmt.Sprintf("https://%s:%d/?code=%s", ip, port, code)
}

func main() {
	log.SetOutput(os.Stdout)

	// 初始化历史日志文件（落盘完整历史；失败只 warning，不影响主流程）。
	history.init()

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
