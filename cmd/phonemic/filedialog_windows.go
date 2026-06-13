//go:build windows

// Windows 原生「打开文件」对话框：syscall 调 comdlg32 GetOpenFileNameW。
//
// 为什么走 syscall 而非拖放：实测 gioui v0.10.0 无 OS 文件拖放 API（transfer.DataEvent
// 仅来自剪贴板文本）。GetOpenFileNameW 是 Win32 经典文件选择框，纯 syscall、无 CGO，
// 体积无感。详见 docs/superpowers/specs 第二期设计的「技术约束」。
package main

import (
	"syscall"
	"unsafe"
)

var (
	modComdlg32          = syscall.NewLazyDLL("comdlg32.dll")
	procGetOpenFileNameW = modComdlg32.NewProc("GetOpenFileNameW")
)

// openFileNameW 对应 Win32 OPENFILENAMEW 结构体。字段顺序/类型必须与系统定义一致。
type openFileNameW struct {
	lStructSize       uint32
	hwndOwner         uintptr
	hInstance         uintptr
	lpstrFilter       *uint16
	lpstrCustomFilter *uint16
	nMaxCustFilter    uint32
	nFilterIndex      uint32
	lpstrFile         *uint16
	nMaxFile          uint32
	lpstrFileTitle    *uint16
	nMaxFileTitle     uint32
	lpstrInitialDir   *uint16
	lpstrTitle        *uint16
	flags             uint32
	nFileOffset       uint16
	nFileExtension    uint16
	lpstrDefExt       *uint16
	lCustData         uintptr
	lpfnHook          uintptr
	lpTemplateName    *uint16
	pvReserved        uintptr
	dwReserved        uint32
	flagsEx           uint32
}

// OFN flags：文件必须存在、路径必须存在、不变更工作目录、隐藏只读勾选框。
const (
	ofnFileMustExist = 0x00001000
	ofnPathMustExist = 0x00000800
	ofnNoChangeDir   = 0x00000008
	ofnHideReadOnly  = 0x00000004
	ofnExplorer      = 0x00080000
)

// pickFile 弹出系统「打开文件」对话框，返回用户选择的完整路径。
// 用户取消或出错时返回空字符串（调用方据此跳过发送）。
func pickFile() string {
	buf := make([]uint16, 4096) // 路径缓冲（足够长路径）

	title, _ := syscall.UTF16PtrFromString("选择要发送到手机的文件")
	// 过滤器："所有文件 (*.*)\0*.*\0\0" —— 双 null 结尾，用切片精确构造
	filter := utf16Filter("所有文件 (*.*)", "*.*")

	ofn := openFileNameW{
		lStructSize:  uint32(unsafe.Sizeof(openFileNameW{})),
		lpstrFile:    &buf[0],
		nMaxFile:     uint32(len(buf)),
		lpstrFilter:  &filter[0],
		nFilterIndex: 1,
		lpstrTitle:   title,
		flags:        ofnFileMustExist | ofnPathMustExist | ofnNoChangeDir | ofnHideReadOnly | ofnExplorer,
	}

	ret, _, _ := procGetOpenFileNameW.Call(uintptr(unsafe.Pointer(&ofn)))
	if ret == 0 {
		// 返回 0：用户取消或出错（CommDlgExtendedError 可查，但这里无需区分）
		return ""
	}
	return syscall.UTF16ToString(buf)
}

// utf16Filter 构造 GetOpenFileName 需要的过滤器串：每段以 null 分隔，整体双 null 结尾。
func utf16Filter(desc, pattern string) []uint16 {
	var out []uint16
	appendNull := func(s string) {
		u, _ := syscall.UTF16FromString(s) // 含结尾 null
		out = append(out, u...)
	}
	appendNull(desc)     // "...\0"
	appendNull(pattern)  // "...\0"
	out = append(out, 0) // 额外一个 null 收尾（总体双 null）
	return out
}
