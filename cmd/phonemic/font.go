// 中文字体：gioui 默认字体（Go Regular）不含 CJK 字形，文字记录里的中文会显示
// 为缺字方块。启动时按平台探测系统字体文件，加载为 gioui 可用的 FontFace。
//
// 设计取舍：
//   - 运行时加载系统字体，不嵌入字体文件（体积优先）。
//   - 找不到任何 CJK 字体时返回错误，由 UI 侧记 warning 并照常启动（英文/数字/
//     界面正常，CJK 显示缺字），不崩溃。
package main

import (
	"fmt"
	"os"
	"runtime"

	"gioui.org/font"
	"gioui.org/font/opentype"
)

// cjkFontCandidates 返回当前平台常见的 CJK 字体文件路径（按优先级）。
func cjkFontCandidates() []string {
	switch runtime.GOOS {
	case "windows":
		root := os.Getenv("WINDIR")
		if root == "" {
			root = `C:\Windows`
		}
		base := root + `\Fonts\`
		return []string{
			base + "msyh.ttc",   // 微软雅黑
			base + "msyh.ttf",   // 旧版微软雅黑
			base + "simhei.ttf", // 黑体
			base + "simsun.ttc", // 宋体
			base + "Deng.ttf",   // 等线
		}
	case "darwin":
		return []string{
			"/System/Library/Fonts/PingFang.ttc",
			"/System/Library/Fonts/STHeiti Medium.ttc",
			"/System/Library/Fonts/Hiragino Sans GB.ttc",
			"/Library/Fonts/Arial Unicode.ttf",
		}
	default: // linux / others
		return []string{
			"/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
			"/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
			"/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
			"/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
			"/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
			"/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
		}
	}
}

// findCJKFontPath 在给定候选列表中返回第一个存在的文件路径；都不存在返回 ""。
// 拆成独立函数便于单测（注入临时目录构造的候选）。
func findCJKFontPath(candidates []string) string {
	for _, p := range candidates {
		if fi, err := os.Stat(p); err == nil && !fi.IsDir() {
			return p
		}
	}
	return ""
}

// loadCJKFont 探测并加载系统 CJK 字体，返回可供 gioui 注册的 FontFace 集合。
//
// 找不到任何字体或解析失败时返回错误，调用方应记 warning 后用默认字体继续。
func loadCJKFont() ([]font.FontFace, error) {
	path := findCJKFontPath(cjkFontCandidates())
	if path == "" {
		return nil, fmt.Errorf("no system CJK font found")
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read font %s: %w", path, err)
	}
	// .ttc/.otc 是字体集合（多个 face），用 ParseCollection 一次拿全。
	faces, err := opentype.ParseCollection(data)
	if err != nil {
		return nil, fmt.Errorf("parse font %s: %w", path, err)
	}
	return faces, nil
}
