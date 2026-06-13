//go:build !windows

// 非 Windows 平台的选文件桩：第二期仅实现 Windows 原生对话框。
// macOS/Linux 后续再补（NSOpenPanel / xdg-desktop-portal / zenity）。
// 返回空字符串，UI 据此提示「当前平台暂不支持选文件发送」。
package main

func pickFile() string {
	return ""
}
