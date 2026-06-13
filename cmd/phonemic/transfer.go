// 电脑 → 手机 的下行传输（第二期）。
//
// 两条下行：
//   - 文本：PC 发送区输入 → broadcast({type:"push-text"}) → 手机接收区显示
//   - 文件：PC 选文件 → 存入内存暂存区 → broadcast({type:"push-file", id}) →
//     手机点击从 GET /download?id=&code= 拉取
//
// 设计取舍：
//   - 下行文件仅存内存（pendingFiles），进程退出自然清空——契合「随手传一下」场景，
//     不留磁盘垃圾。设软上限防内存膨胀：超出数量/总字节就丢最旧。
//   - /download 复用配对码鉴权（与 /ws、/upload 同一套）。
//   - 下载后不立即删除，允许多设备/重试重复下载；靠软上限淘汰与进程退出兜底。
package main

import (
	"crypto/rand"
	"encoding/hex"
	"log"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"sync"
	"time"
)

// 下行文件暂存的软上限：数量与总字节，任一超出就从最旧开始淘汰。
const (
	pendingMaxCount = 20
	pendingMaxBytes = 200 << 20 // 200MB
)

// sentLogCap 是「发送给手机」记录环形缓冲容量（与接收侧 textLogCap 对称）。
const sentLogCap = 50

// pendingFile 是一份等待手机下载的内存文件。
type pendingFile struct {
	id          string
	name        string
	data        []byte
	contentType string
	t           time.Time
}

// sentRecord 是一条「发送到手机」的记录，供 PC 发送 tab 展示。
// kind 区分文本/文件，text 为文本内容或文件名。
type sentRecord struct {
	t    time.Time
	kind string // "text" | "file"
	text string
}

// transferState 持有下行相关状态：内存文件暂存区 + 发送记录环形缓冲。
// 独立于 appState 的锁，避免与连接/接收记录的锁纠缠。
type transferState struct {
	mu sync.Mutex
	// 内存文件暂存：id -> pendingFile。order 记录插入顺序用于淘汰最旧。
	pending    map[string]pendingFile
	order      []string
	totalBytes int64
	// 发送记录环形缓冲（最近 sentLogCap 条，新的在后）。
	sent     []sentRecord
	sentHead int
	sentSize int
}

var transfer = &transferState{pending: make(map[string]pendingFile)}

// newTransferID 生成 16 hex 字符的随机 id（crypto/rand，防猜测）。
func newTransferID() string {
	b := make([]byte, 8)
	if _, err := rand.Read(b); err != nil {
		// 极罕见；退化用时间戳纳秒兜底（仍单调唯一，足够）
		return hex.EncodeToString([]byte(time.Now().Format("150405.000000000")))
	}
	return hex.EncodeToString(b)
}

// addPendingFile 把一份文件存入内存暂存区，返回其下载 id。
// 超过软上限（数量或总字节）时从最旧开始淘汰。
func (ts *transferState) addPendingFile(name string, data []byte, contentType string) string {
	id := newTransferID()
	ts.mu.Lock()
	defer ts.mu.Unlock()
	pf := pendingFile{id: id, name: name, data: data, contentType: contentType, t: time.Now()}
	ts.pending[id] = pf
	ts.order = append(ts.order, id)
	ts.totalBytes += int64(len(data))
	// 淘汰最旧，直到满足两个软上限
	for (len(ts.order) > pendingMaxCount || ts.totalBytes > pendingMaxBytes) && len(ts.order) > 1 {
		oldest := ts.order[0]
		ts.order = ts.order[1:]
		if old, ok := ts.pending[oldest]; ok {
			ts.totalBytes -= int64(len(old.data))
			delete(ts.pending, oldest)
			log.Printf("[transfer] evicted pending file %s (%s)", oldest, old.name)
		}
	}
	return id
}

// getPendingFile 按 id 取暂存文件；不删除（允许重复下载）。
func (ts *transferState) getPendingFile(id string) (pendingFile, bool) {
	ts.mu.Lock()
	defer ts.mu.Unlock()
	pf, ok := ts.pending[id]
	return pf, ok
}

// addSent 追加一条发送记录到环形缓冲（满则覆盖最旧）。
func (ts *transferState) addSent(kind, text string) {
	ts.mu.Lock()
	defer ts.mu.Unlock()
	if ts.sent == nil {
		ts.sent = make([]sentRecord, sentLogCap)
	}
	rec := sentRecord{t: time.Now(), kind: kind, text: text}
	if ts.sentSize < sentLogCap {
		idx := (ts.sentHead + ts.sentSize) % sentLogCap
		ts.sent[idx] = rec
		ts.sentSize++
	} else {
		ts.sent[ts.sentHead] = rec
		ts.sentHead = (ts.sentHead + 1) % sentLogCap
	}
}

// recentSent 返回发送记录快照副本，新的在前（供 UI 只读）。
func (ts *transferState) recentSent() []sentRecord {
	ts.mu.Lock()
	defer ts.mu.Unlock()
	out := make([]sentRecord, ts.sentSize)
	for i := 0; i < ts.sentSize; i++ {
		idx := (ts.sentHead + ts.sentSize - 1 - i + sentLogCap) % sentLogCap
		out[i] = ts.sent[idx]
	}
	return out
}

// sendTextToPhone 把一段文本广播给所有手机（接收区显示），并记一条发送记录。
// 返回是否有活跃连接（无连接时 UI 可提示「没有已连接的手机」）。
func sendTextToPhone(state *appState, text string) bool {
	if text == "" {
		return false
	}
	state.broadcast(map[string]any{
		"type": "push-text",
		"id":   newTransferID(),
		"text": text,
		"t":    time.Now().UnixMilli(),
	})
	transfer.addSent("text", text)
	log.Printf("[transfer] pushed text (%d chars) to %d phone(s)", len([]rune(text)), state.connCount())
	return state.connCount() > 0
}

// sendFileToPhone 把一份文件存入暂存区并广播下载通知，记一条发送记录。
func sendFileToPhone(state *appState, name string, data []byte, contentType string) bool {
	if len(data) == 0 {
		return false
	}
	id := transfer.addPendingFile(name, data, contentType)
	state.broadcast(map[string]any{
		"type": "push-file",
		"id":   id,
		"name": name,
		"size": len(data),
		"t":    time.Now().UnixMilli(),
	})
	transfer.addSent("file", name)
	log.Printf("[transfer] pushed file %q (%d bytes, id=%s) to %d phone(s)", name, len(data), id, state.connCount())
	return state.connCount() > 0
}

// pickAndSendFile 弹原生选文件框 → 读字节 → 推断 MIME → 发送到手机。
// 返回 (文件名, ok)。用户取消返回 ("", false)；读取失败返回 (名, false) 并 log。
// 供 PC 发送 tab 的「选文件发送」按钮调用。
func pickAndSendFile(state *appState) (string, bool) {
	path := pickFile()
	if path == "" {
		return "", false // 用户取消 / 平台不支持
	}
	name := filepath.Base(path)
	data, err := os.ReadFile(path)
	if err != nil {
		log.Printf("[transfer] read picked file %s failed: %v", path, err)
		return name, false
	}
	// 推断 MIME：标准库嗅探前 512 字节，无需第三方库
	ct := http.DetectContentType(data)
	sendFileToPhone(state, name, data, ct)
	return name, true
}

// handleDownload 处理 GET /download?id=&code=：鉴权 → 查暂存 → 回字节。
func handleDownload(state *appState) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		code, _, _ := state.snapshot()
		if r.URL.Query().Get("code") != code {
			http.Error(w, "PAIR_CODE_INVALID", http.StatusForbidden)
			return
		}
		id := r.URL.Query().Get("id")
		pf, ok := transfer.getPendingFile(id)
		if !ok {
			http.Error(w, "NOT_FOUND", http.StatusNotFound)
			return
		}
		ct := pf.contentType
		if ct == "" {
			ct = "application/octet-stream"
		}
		w.Header().Set("Content-Type", ct)
		// 用 RFC5987 编码文件名，兼容中文/特殊字符
		w.Header().Set("Content-Disposition", "attachment; filename*=UTF-8''"+url.PathEscape(pf.name))
		w.Header().Set("Content-Length", strconv.Itoa(len(pf.data)))
		_, _ = w.Write(pf.data)
		log.Printf("[transfer] served download %s (%s, %d bytes)", id, pf.name, len(pf.data))
	}
}
