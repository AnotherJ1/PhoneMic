// 二维码：把配对 URL 编码成 image.Image，供 gioui 当 widget 直接绘制，
// 不再写临时 PNG、不再调用外部看图器。
package main

import (
	"image"

	qrcode "github.com/skip2/go-qrcode"
)

// qrPixelSize 是生成二维码位图的边长（像素）。够清晰能扫，又不至于太大。
const qrPixelSize = 320

// urlToImage 把给定 URL 编码为二维码 image.Image。
//
// 出错时返回 (nil, err)，由调用方决定降级显示（如 "QR unavailable"）。
func urlToImage(url string) (image.Image, error) {
	q, err := qrcode.New(url, qrcode.Medium)
	if err != nil {
		return nil, err
	}
	// go-qrcode 的 Image 返回 *image.RGBA / image.Image，尺寸为 size×size。
	return q.Image(qrPixelSize), nil
}
