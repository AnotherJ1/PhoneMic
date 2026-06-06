// 历史消息落盘：把每条成功注入的文本追加写入日志文件，供用户事后查看完整历史
// （窗口内的环形缓冲只留最近 50 条，日志文件则保留全部）。
//
// 设计取舍：
//   - 文件放在用户配置目录下的 phonemic/ 子目录（与证书缓存同处），跨平台一致。
//   - 追加写、每条带时间戳；写失败只记 warning，不影响主流程（历史是辅助功能）。
//   - 单独的 mutex，避免和 appState 的锁纠缠；I/O 不该持 state 锁。
package main

import (
	"fmt"
	"log"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// historyLogName 是历史日志文件名（位于 certCacheDir() 同目录）。
const historyLogName = "history.log"

// historyLog 管理历史日志文件的追加写入，并发安全。
type historyLog struct {
	mu   sync.Mutex
	path string
}

// 全局单例：在 main 启动时初始化。
var history = &historyLog{}

// historyLogPath 返回历史日志文件的完整路径。
func historyLogPath() string {
	return filepath.Join(certCacheDir(), historyLogName)
}

// initHistory 准备日志文件路径并确保目录存在。失败只记 warning，不致命。
func (h *historyLog) init() {
	h.mu.Lock()
	defer h.mu.Unlock()
	dir := certCacheDir()
	if err := os.MkdirAll(dir, 0o700); err != nil {
		log.Printf("[history] mkdir %s failed: %v", dir, err)
		return
	}
	h.path = filepath.Join(dir, historyLogName)
	// 启动时写一行分隔，便于区分不同运行会话。
	h.appendLine(fmt.Sprintf("==== PhoneMic session started %s ====",
		time.Now().Format("2006-01-02 15:04:05")))
}

// append 记录一条注入文本（带时间戳）。供 /ws handler 在 addText 后调用。
func (h *historyLog) append(now time.Time, text string) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.appendLine(fmt.Sprintf("%s\t%s", now.Format("2006-01-02 15:04:05"), text))
}

// appendLine 把一行写入日志文件（调用方需持锁）。失败只 warning。
func (h *historyLog) appendLine(line string) {
	if h.path == "" {
		return // init 失败，跳过落盘
	}
	f, err := os.OpenFile(h.path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o600)
	if err != nil {
		log.Printf("[history] open %s failed: %v", h.path, err)
		return
	}
	defer f.Close()
	if _, err := fmt.Fprintln(f, line); err != nil {
		log.Printf("[history] write failed: %v", err)
	}
}
