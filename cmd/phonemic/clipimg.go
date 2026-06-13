// 图片写入系统剪贴板。
//
// 用 golang.design/x/clipboard 实现跨平台图片剪贴板。选它而非手写 syscall 的原因：
//   - 本项目已通过 gioui 引入 x/image、x/sys、x/exp/shiny 等依赖，与本库高度重叠，
//     实测引入后二进制仅增大约 10KB（Windows / CGO=0 / strip）。
//   - API 跨平台统一，比每平台手写 CF_DIB / osascript / xclip 干净得多。
//
// 平台注意：
//   - Windows：CGO_ENABLED=0 即可编译运行，无需 GCC。
//   - macOS/Linux：本库在这两个平台依赖 CGO，若将来发布这两个平台且需要图片剪贴板，
//     需开启 CGO 编译。Windows 版不受影响。
//
// 格式约束：clipboard.FmtImage 只接受 PNG 字节。因此非 PNG 图片（JPEG/GIF/WebP 等）
// 先用标准库解码再编码成 PNG 后写入；解码失败则放弃写剪贴板（文件仍已存盘）。
package main

import (
	"bytes"
	"fmt"
	"image"
	"image/png"
	"sync"

	"golang.design/x/clipboard"

	// 注册常见图片格式的解码器，使 image.Decode 能识别 jpeg/gif；png/webp 见下。
	_ "image/gif"
	_ "image/jpeg"

	_ "golang.org/x/image/webp" // webp 解码（只读，已随 x/image 间接引入）
)

// clipboardInitOnce 惰性初始化剪贴板库。clipboard.Init 在不可用环境（无显示器的
// Linux 等）会返回错误；只初始化一次，错误缓存复用。
var (
	clipboardInitOnce sync.Once
	clipboardInitErr  error
)

func initClipboard() error {
	clipboardInitOnce.Do(func() {
		clipboardInitErr = clipboard.Init()
	})
	return clipboardInitErr
}

// writeImageToClipboard 把图片字节写入系统剪贴板。
//
// data 为原始图片字节，contentType 为其 MIME（如 image/png、image/jpeg）。
// 非 PNG 会先解码再转 PNG（clipboard.FmtImage 仅认 PNG）。
// 返回 nil 表示成功；任何失败都返回错误，调用方据此告知用户「已存盘但剪贴板写入失败」。
func writeImageToClipboard(data []byte, contentType string) error {
	if err := initClipboard(); err != nil {
		return fmt.Errorf("clipboard init: %w", err)
	}

	pngData, err := toPNG(data, contentType)
	if err != nil {
		return err
	}

	clipboard.Write(clipboard.FmtImage, pngData)
	return nil
}

// toPNG 确保返回 PNG 字节：已是 PNG 直接返回，否则解码后重新编码为 PNG。
func toPNG(data []byte, contentType string) ([]byte, error) {
	if contentType == "image/png" {
		return data, nil
	}
	img, _, err := image.Decode(bytes.NewReader(data))
	if err != nil {
		return nil, fmt.Errorf("decode image (%s): %w", contentType, err)
	}
	var buf bytes.Buffer
	if err := png.Encode(&buf, img); err != nil {
		return nil, fmt.Errorf("encode png: %w", err)
	}
	return buf.Bytes(), nil
}
