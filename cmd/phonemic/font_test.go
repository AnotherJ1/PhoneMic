package main

import (
	"os"
	"path/filepath"
	"testing"
)

// findCJKFontPath：候选中有存在文件时返回第一个存在者
func TestFindCJKFontPath_ReturnsFirstExisting(t *testing.T) {
	dir := t.TempDir()
	missing := filepath.Join(dir, "missing.ttc")
	present := filepath.Join(dir, "present.ttc")
	if err := os.WriteFile(present, []byte("fake-font"), 0o600); err != nil {
		t.Fatal(err)
	}
	got := findCJKFontPath([]string{missing, present})
	if got != present {
		t.Errorf("got %q, want %q", got, present)
	}
}

// findCJKFontPath：候选全不存在时返回空串
func TestFindCJKFontPath_NoneReturnsEmpty(t *testing.T) {
	dir := t.TempDir()
	got := findCJKFontPath([]string{
		filepath.Join(dir, "a.ttc"),
		filepath.Join(dir, "b.ttf"),
	})
	if got != "" {
		t.Errorf("got %q, want empty", got)
	}
}

// findCJKFontPath：目录路径不算字体文件（须是文件）
func TestFindCJKFontPath_SkipsDirectories(t *testing.T) {
	dir := t.TempDir()
	subdir := filepath.Join(dir, "fonts.ttc")
	if err := os.Mkdir(subdir, 0o700); err != nil {
		t.Fatal(err)
	}
	got := findCJKFontPath([]string{subdir})
	if got != "" {
		t.Errorf("got %q for a directory, want empty", got)
	}
}
