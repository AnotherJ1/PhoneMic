package main

import (
	"image"
	"testing"
)

// urlToImage 应返回非 nil image、尺寸 > 0
func TestURLToImage_NonNilSized(t *testing.T) {
	img, err := urlToImage("https://192.168.1.10:8443/?code=ABC123")
	if err != nil {
		t.Fatalf("urlToImage error: %v", err)
	}
	if img == nil {
		t.Fatal("img is nil")
	}
	b := img.Bounds()
	if b.Dx() <= 0 || b.Dy() <= 0 {
		t.Fatalf("image size = %dx%d, want > 0", b.Dx(), b.Dy())
	}
}

// 不同 URL 应产生不同图像（像素有差异）
func TestURLToImage_DifferentURLsDiffer(t *testing.T) {
	a, err := urlToImage("https://192.168.1.10:8443/?code=AAAAAA")
	if err != nil {
		t.Fatal(err)
	}
	b, err := urlToImage("https://192.168.1.10:8443/?code=ZZZZZZ")
	if err != nil {
		t.Fatal(err)
	}
	if imagesEqual(a, b) {
		t.Error("expected different images for different URLs, got identical")
	}
}

// imagesEqual 逐像素比较两张图（尺寸不同即不等）
func imagesEqual(a, b image.Image) bool {
	ba, bb := a.Bounds(), b.Bounds()
	if ba != bb {
		return false
	}
	for y := ba.Min.Y; y < ba.Max.Y; y++ {
		for x := ba.Min.X; x < ba.Max.X; x++ {
			r1, g1, b1, a1 := a.At(x, y).RGBA()
			r2, g2, b2, a2 := b.At(x, y).RGBA()
			if r1 != r2 || g1 != g2 || b1 != b2 || a1 != a2 {
				return false
			}
		}
	}
	return true
}
