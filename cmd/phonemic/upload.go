// 手机 → 电脑 的文件/图片上传（第一期：上行）。
//
// 数据流：
//   手机网页选图/拍照 → POST /upload (multipart/form-data, ?code=配对码) →
//   电脑：① 始终存盘到 <exe目录>/file/<时间戳>.<原扩展名>
//         ② 若内容是图片，额外写入系统剪贴板（可直接 Ctrl/Cmd+V 粘贴）
//         ③ 记一条历史 + UI 文字记录区显示「📷 文件名」
//
// 设计取舍：
//   - 走独立 HTTP 端点而非复用 WS：大文件不该塞进 WS JSON，且 multipart 流式落盘
//     省内存、不阻塞 WS 心跳。鉴权复用现有配对码（与 /ws 同一套）。
//   - 不限文件类型：任何类型都存盘；仅当 MIME 为 image/* 时才尝试写剪贴板。
//   - 上限 100MB：防止恶意大文件耗尽内存/磁盘。
package main

import (
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

// maxUploadBytes 单文件上传上限：100MB。
const maxUploadBytes = 100 << 20

// uploadDirName 是存盘子目录名，位于可执行文件所在目录下。
const uploadDirName = "file"

// uploadSeq 给同一秒内的多次上传追加序号，避免时间戳文件名冲突。
var (
	uploadSeqMu sync.Mutex
	uploadSeq   int
)

// exeDir 返回可执行文件所在目录；取不到时退化到当前工作目录。
// 注意：`go run` 下 exe 在临时目录，存盘会落在那里——测试请用 `go build` 出的二进制。
func exeDir() string {
	exe, err := os.Executable()
	if err != nil {
		if wd, werr := os.Getwd(); werr == nil {
			return wd
		}
		return "."
	}
	// 解析符号链接，确保拿到真实目录
	if resolved, rerr := filepath.EvalSymlinks(exe); rerr == nil {
		exe = resolved
	}
	return filepath.Dir(exe)
}

// uploadDir 返回存盘目录 <exe目录>/file。
func uploadDir() string {
	return filepath.Join(exeDir(), uploadDirName)
}

// nextUploadName 基于当前时间生成不冲突的文件名：20060102-150405[-N].<ext>。
// 同一秒内多次上传用递增序号区分。ext 含点号（如 ".png"）；无扩展名时为空。
func nextUploadName(now time.Time, ext string) string {
	uploadSeqMu.Lock()
	seq := uploadSeq
	uploadSeq++
	uploadSeqMu.Unlock()

	base := now.Format("20060102-150405")
	// 首次（seq 起始 0）不加后缀；之后加 -1、-2… 防同秒上传文件名冲突
	name := base
	if seq != 0 {
		name = fmt.Sprintf("%s-%d", base, seq)
	}
	return name + ext
}

// sanitizeExt 从原始文件名提取一个安全的扩展名（仅字母数字，限长），防路径穿越/怪字符。
func sanitizeExt(filename string) string {
	ext := filepath.Ext(filename)
	if ext == "" {
		return ""
	}
	ext = strings.ToLower(ext)
	// 只保留 .xxx 形式的字母数字扩展名，最长 10 字符（含点）
	if len(ext) > 10 {
		return ""
	}
	for _, r := range ext[1:] {
		if !((r >= 'a' && r <= 'z') || (r >= '0' && r <= '9')) {
			return ""
		}
	}
	return ext
}

// handleUpload 处理 /upload：鉴权 → 接收文件 → 存盘 → 图片写剪贴板 → 记录。
func handleUpload(state *appState) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
			return
		}
		// 配对码鉴权：与 /ws 同一套
		code, _, _ := state.snapshot()
		if r.URL.Query().Get("code") != code {
			http.Error(w, "PAIR_CODE_INVALID", http.StatusForbidden)
			return
		}

		// 限制请求体大小，防恶意大文件；超限时 ReadForm/Copy 会报错
		r.Body = http.MaxBytesReader(w, r.Body, maxUploadBytes)

		// 流式取出表单文件部分（字段名 file）
		file, header, err := r.FormFile("file")
		if err != nil {
			// MaxBytesReader 超限也会走到这里
			log.Printf("[upload] read form file failed: %v", err)
			writeUploadJSON(w, http.StatusBadRequest, uploadResp{
				OK: false, Error: "读取文件失败或超过 100MB 上限",
			})
			return
		}
		defer file.Close()

		// 确保存盘目录存在
		dir := uploadDir()
		if err := os.MkdirAll(dir, 0o755); err != nil {
			log.Printf("[upload] mkdir %s failed: %v", dir, err)
			writeUploadJSON(w, http.StatusInternalServerError, uploadResp{
				OK: false, Error: "电脑端创建存储目录失败",
			})
			return
		}

		now := time.Now()
		ext := sanitizeExt(header.Filename)
		name := nextUploadName(now, ext)
		dstPath := filepath.Join(dir, name)

		// 存盘：流式写，省内存。同时把内容缓存进内存仅当需要写剪贴板时。
		dst, err := os.OpenFile(dstPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o644)
		if err != nil {
			log.Printf("[upload] create %s failed: %v", dstPath, err)
			writeUploadJSON(w, http.StatusInternalServerError, uploadResp{
				OK: false, Error: "电脑端写文件失败",
			})
			return
		}

		// 判断是否图片：以 Content-Type 为准。
		contentType := header.Header.Get("Content-Type")
		isImage := strings.HasPrefix(contentType, "image/")

		var written int64
		var imgBuf []byte
		if isImage {
			// 图片需完整字节写剪贴板：先全部读入内存（受 100MB 上限保护），再写盘。
			data, rerr := io.ReadAll(file)
			if rerr != nil {
				dst.Close()
				os.Remove(dstPath)
				log.Printf("[upload] read image failed: %v", rerr)
				writeUploadJSON(w, http.StatusBadRequest, uploadResp{OK: false, Error: "读取图片失败或超过 100MB 上限"})
				return
			}
			n, werr := dst.Write(data)
			if werr != nil {
				dst.Close()
				os.Remove(dstPath)
				log.Printf("[upload] write image failed: %v", werr)
				writeUploadJSON(w, http.StatusInternalServerError, uploadResp{OK: false, Error: "写图片失败"})
				return
			}
			written = int64(n)
			imgBuf = data
		} else {
			// 非图片：纯流式拷贝，不占内存
			written, err = io.Copy(dst, file)
			if err != nil {
				dst.Close()
				os.Remove(dstPath)
				log.Printf("[upload] copy failed: %v", err)
				writeUploadJSON(w, http.StatusBadRequest, uploadResp{
					OK: false, Error: "保存文件失败或超过 100MB 上限",
				})
				return
			}
		}
		if cerr := dst.Close(); cerr != nil {
			log.Printf("[upload] close %s failed: %v", dstPath, cerr)
		}

		// 图片：尝试写剪贴板（失败不影响存盘成功）
		clipOK := false
		if isImage && imgBuf != nil {
			if cerr := writeImageToClipboard(imgBuf, contentType); cerr != nil {
				log.Printf("[upload] clipboard write failed: %v", cerr)
			} else {
				clipOK = true
			}
		}

		// 记录：UI 文字记录区 + 历史日志，用「📷/📎 文件名」表示
		icon := "📎"
		if isImage {
			icon = "📷"
		}
		label := fmt.Sprintf("%s %s", icon, name)
		state.addText(now, label)
		history.append(now, label)

		log.Printf("[upload] saved %s (%d bytes, image=%v, clipboard=%v)", dstPath, written, isImage, clipOK)

		writeUploadJSON(w, http.StatusOK, uploadResp{
			OK:        true,
			Name:      name,
			IsImage:   isImage,
			Clipboard: clipOK,
		})
	}
}

// uploadResp 是 /upload 的 JSON 响应。
type uploadResp struct {
	OK        bool   `json:"ok"`
	Name      string `json:"name,omitempty"`
	IsImage   bool   `json:"isImage,omitempty"`
	Clipboard bool   `json:"clipboard,omitempty"`
	Error     string `json:"error,omitempty"`
}

func writeUploadJSON(w http.ResponseWriter, status int, resp uploadResp) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(resp)
}
