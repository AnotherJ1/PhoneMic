// gioui 桌面窗口：把 appState 渲染成界面，承载二维码 / 连接状态 / 文字记录 /
// 配对码操作。纯 Go、无 CGO（gioui 走系统 API / Direct3D / Vulkan）。
//
// 视觉风格：深色主题，呼应手机端网页（深蓝灰底 #0f172a + sky 蓝主色 #0ea5e9），
// 卡片化分区、圆角、状态胶囊，桌面/手机品牌统一。
//
// 关键约定（见设计文档「数据流」）：
//   - UI 单向只读 state（snapshot / connCount / recentTexts），不反向驱动 server。
//   - 用「定时 Invalidate + 读快照」每 ~500ms 重绘一帧，规避事件总线的并发复杂度。
//   - 二维码仅在 URL 变化时重新编码并缓存 paint.ImageOp，避免每帧重编码。
//   - 窗口关闭 = 退出整个进程（server goroutine 随之结束），无后台常驻形态。
package main

import (
	"image"
	"image/color"
	"log"
	"os"
	"os/exec"
	"runtime"
	"strconv"
	"strings"
	"time"

	"gioui.org/app"
	"gioui.org/font"
	"gioui.org/font/gofont"
	"gioui.org/io/system"
	"gioui.org/layout"
	"gioui.org/op"
	"gioui.org/op/clip"
	"gioui.org/op/paint"
	"gioui.org/text"
	"gioui.org/unit"
	"gioui.org/widget"
	"gioui.org/widget/material"

	"github.com/atotto/clipboard"
)

// uiRefreshInterval 是定时重绘间隔：连接状态 / 文字记录靠它跟上 server 侧变化。
const uiRefreshInterval = 500 * time.Millisecond

// ---- 深色主题调色板（与手机端 index.html 同源）----
var (
	colBg      = color.NRGBA{R: 0x0f, G: 0x17, B: 0x2a, A: 0xff} // 窗口底：slate-900
	colCard    = color.NRGBA{R: 0x1e, G: 0x29, B: 0x3b, A: 0xff} // 卡片：slate-800
	colCardAlt = color.NRGBA{R: 0x0b, G: 0x11, B: 0x20, A: 0xff} // 更深的内嵌块
	colAccent  = color.NRGBA{R: 0x0e, G: 0xa5, B: 0xe9, A: 0xff} // 主色：sky-500
	colText    = color.NRGBA{R: 0xe2, G: 0xe8, B: 0xf0, A: 0xff} // 主文字：slate-200
	colMuted   = color.NRGBA{R: 0x94, G: 0xa3, B: 0xb8, A: 0xff} // 次要文字：slate-400
	colFaint   = color.NRGBA{R: 0x64, G: 0x74, B: 0x8b, A: 0xff} // 更弱：slate-500
	colDotOn   = color.NRGBA{R: 0x10, G: 0xb9, B: 0x81, A: 0xff} // 绿：有连接 emerald-500
	colDotOff  = color.NRGBA{R: 0x47, G: 0x55, B: 0x69, A: 0xff} // 灰：无连接 slate-600
	colPillOn  = color.NRGBA{R: 0x06, G: 0x5f, B: 0x46, A: 0xff} // 状态胶囊底（连接）
	colWhite   = color.NRGBA{R: 0xff, G: 0xff, B: 0xff, A: 0xff}
)

// runUI 启动 gioui 主循环。占据主线程，替代原 systray.Run。
func runUI(state *appState) {
	go func() {
		w := new(app.Window)
		w.Option(
			app.Title("PhoneMic"),
			app.Size(unit.Dp(460), unit.Dp(640)),
			app.MinSize(unit.Dp(420), unit.Dp(560)),
		)
		// 打开时在当前显示器居中（Windows/macOS/X11 支持；纯 Wayland 由合成器
		// 决定窗口位置，此调用被忽略，不影响其余功能）。
		w.Perform(system.ActionCenter)
		// 定时触发重绘，让连接数 / 文字记录跟上后台变化。
		go func() {
			t := time.NewTicker(uiRefreshInterval)
			defer t.Stop()
			for range t.C {
				w.Invalidate()
			}
		}()
		if err := loopUI(w, state); err != nil {
			log.Fatalf("[ui] %v", err)
		}
		// 窗口关闭即退出整个程序
		os.Exit(0)
	}()
	app.Main()
}

// uiState 持有 gioui widget 与跨帧缓存（二维码图）。
type uiState struct {
	th         *material.Theme
	copyBtn    widget.Clickable
	rotateBtn  widget.Clickable
	copyLogBtn widget.Clickable // 复制全部历史消息到剪贴板
	openLogBtn    widget.Clickable // 用系统默认程序打开历史日志文件
	copyRecordBtns []widget.Clickable // 逐条消息复制按钮
	list          widget.List

	// 复制成功后的视觉反馈：记录各复制按钮「最近一次成功复制」的时刻，
	// 渲染时若距今 < copiedFeedbackDur 就把按钮文案临时换成「已复制」态。
	// 不另开 ticker —— 复用既有 500ms 定时 Invalidate 让文案在 deadline 后自动恢复。
	copyBtnCopiedAt    time.Time
	copyLogBtnCopiedAt time.Time
	copyRecordCopiedAt []time.Time // 与 copyRecordBtns 等长，逐条对应

	// 二维码缓存：仅当 URL 变化时重新编码
	qrURL string
	qrOp  paint.ImageOp
	qrOK  bool
}

// copiedFeedbackDur 是复制成功后按钮显示「已复制」态的时长（约 1.5s 后恢复原文案）。
const copiedFeedbackDur = 1500 * time.Millisecond

// isCopied 判断某复制时刻是否仍在反馈窗口内（用于渲染时切换按钮文案）。
func isCopied(at time.Time) bool {
	return !at.IsZero() && time.Since(at) < copiedFeedbackDur
}

func newUIState() *uiState {
	// 默认 Go 字体不含 CJK；追加系统中文字体（失败则降级，仅记 warning）。
	collection := gofont.Collection()
	if cjk, err := loadCJKFont(); err != nil {
		log.Printf("[ui] CJK font not loaded (Chinese may show as tofu): %v", err)
	} else {
		collection = append(collection, cjk...)
	}
	th := material.NewTheme()
	th.Shaper = text.NewShaper(text.WithCollection(collection))
	th.Palette.Fg = colText
	th.Palette.Bg = colBg
	th.Palette.ContrastBg = colAccent
	th.Palette.ContrastFg = colWhite

	u := &uiState{th: th}
	u.list.Axis = layout.Vertical
	return u
}

// loopUI 是 gioui 事件循环：处理窗口事件、每帧调用 layout 渲染。
func loopUI(w *app.Window, state *appState) error {
	u := newUIState()
	var ops op.Ops
	for {
		switch e := w.Event().(type) {
		case app.DestroyEvent:
			return e.Err
		case app.FrameEvent:
			gtx := app.NewContext(&ops, e)
			// 整窗深色背景
			paint.Fill(gtx.Ops, colBg)
			u.layout(gtx, state)
			e.Frame(gtx.Ops)
		}
	}
}

// layout 渲染整个窗口：纵向 Flex 分三区（顶部状态 / 中部配对卡 / 底部文字记录卡）。
func (u *uiState) layout(gtx layout.Context, state *appState) layout.Dimensions {
	// ---- 先处理本帧的按钮点击 ----
	if u.copyBtn.Clicked(gtx) {
		url := state.connectURL()
		if err := clipboard.WriteAll(url); err != nil {
			log.Printf("[ui] copy URL failed: %v", err)
		} else {
			log.Printf("[ui] copied URL %s", url)
			u.copyBtnCopiedAt = time.Now() // 触发「已复制」态视觉反馈
		}
	}
	if u.rotateBtn.Clicked(gtx) {
		state.rotateCode()
	}
	if u.copyLogBtn.Clicked(gtx) {
		// 复制窗口内全部历史消息（最近 50 条，新的在前）到剪贴板
		if text := formatRecords(state.recentTexts()); text != "" {
			if err := clipboard.WriteAll(text); err != nil {
				log.Printf("[ui] copy history failed: %v", err)
			} else {
				log.Printf("[ui] copied %d history lines", len(state.recentTexts()))
				u.copyLogBtnCopiedAt = time.Now() // 触发「已复制」态视觉反馈
			}
		}
	}
	if u.openLogBtn.Clicked(gtx) {
		// 用系统默认程序打开完整历史日志文件
		openPath(historyLogPath())
	}

	// 监听逐条复制按钮点击
	for i := range u.copyRecordBtns {
		if u.copyRecordBtns[i].Clicked(gtx) {
			recs := state.recentTexts()
			if i < len(recs) {
				if err := clipboard.WriteAll(recs[i].text); err != nil {
					log.Printf("[ui] copy record %d failed: %v", i, err)
				} else {
					log.Printf("[ui] copied record %d", i)
					if i < len(u.copyRecordCopiedAt) {
						u.copyRecordCopiedAt[i] = time.Now() // 触发「已复制」态视觉反馈
					}
				}
			}
		}
	}

	// ---- 读取本帧快照（单向只读）----
	code, _, _ := state.snapshot()
	url := state.connectURL()
	conns := state.connCount()
	records := state.recentTexts()

	inset := layout.UniformInset(unit.Dp(20))
	return inset.Layout(gtx, func(gtx layout.Context) layout.Dimensions {
		return layout.Flex{Axis: layout.Vertical}.Layout(gtx,
			// 顶部：标题 + 状态胶囊
			layout.Rigid(func(gtx layout.Context) layout.Dimensions {
				return u.layoutHeader(gtx, conns)
			}),
			layout.Rigid(layout.Spacer{Height: unit.Dp(16)}.Layout),
			// 中部：配对卡（二维码 + URL/配对码 + 按钮）
			layout.Rigid(func(gtx layout.Context) layout.Dimensions {
				return card(gtx, colCard, unit.Dp(16), func(gtx layout.Context) layout.Dimensions {
					return u.layoutPairing(gtx, url, code)
				})
			}),
			layout.Rigid(layout.Spacer{Height: unit.Dp(16)}.Layout),
			// 底部：文字记录卡（占据剩余空间，可滚动）
			layout.Flexed(1, func(gtx layout.Context) layout.Dimensions {
				return card(gtx, colCard, unit.Dp(16), func(gtx layout.Context) layout.Dimensions {
					return u.layoutRecordsSection(gtx, records)
				})
			}),
		)
	})
}

// layoutHeader 顶部：左标题、右状态胶囊（圆点 + 文案）
func (u *uiState) layoutHeader(gtx layout.Context, conns int) layout.Dimensions {
	th := u.th
	dotColor := colDotOff
	pillBg := colCard
	label := "未连接"
	if conns == 1 {
		dotColor, pillBg, label = colDotOn, colPillOn, "1 台已连接"
	} else if conns > 1 {
		dotColor, pillBg, label = colDotOn, colPillOn, strconv.Itoa(conns)+" 台已连接"
	}
	return layout.Flex{Axis: layout.Horizontal, Alignment: layout.Middle}.Layout(gtx,
		layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			t := material.H5(th, "PhoneMic")
			t.Color = colText
			t.Font.Weight = font.Bold
			return t.Layout(gtx)
		}),
		// 版本号：紧跟标题，弱化为次要灰字，与标题基线对齐
		layout.Rigid(layout.Spacer{Width: unit.Dp(8)}.Layout),
		layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			return layout.Inset{Top: unit.Dp(10)}.Layout(gtx, func(gtx layout.Context) layout.Dimensions {
				l := material.Label(th, unit.Sp(13), "v"+version)
				l.Color = colMuted
				return l.Layout(gtx)
			})
		}),
		layout.Flexed(1, layout.Spacer{}.Layout),
		// 状态胶囊
		layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			return pill(gtx, pillBg, func(gtx layout.Context) layout.Dimensions {
				return layout.Flex{Axis: layout.Horizontal, Alignment: layout.Middle}.Layout(gtx,
					layout.Rigid(func(gtx layout.Context) layout.Dimensions {
						return drawDot(gtx, dotColor, 9)
					}),
					layout.Rigid(layout.Spacer{Width: unit.Dp(7)}.Layout),
					layout.Rigid(func(gtx layout.Context) layout.Dimensions {
						l := material.Label(th, unit.Sp(13), label)
						l.Color = colText
						return l.Layout(gtx)
					}),
				)
			})
		}),
	)
}

// layoutPairing 配对卡内容：左二维码、右 URL/配对码/按钮
func (u *uiState) layoutPairing(gtx layout.Context, url, code string) layout.Dimensions {
	th := u.th
	return layout.Flex{Axis: layout.Horizontal, Alignment: layout.Start}.Layout(gtx,
		// 二维码（白底卡，扫码对比度更好）
		layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			return card(gtx, colWhite, unit.Dp(8), func(gtx layout.Context) layout.Dimensions {
				return u.layoutQR(gtx, url)
			})
		}),
		layout.Rigid(layout.Spacer{Width: unit.Dp(16)}.Layout),
		// 右侧信息列
		layout.Flexed(1, func(gtx layout.Context) layout.Dimensions {
			return layout.Flex{Axis: layout.Vertical}.Layout(gtx,
				layout.Rigid(captionLabel(th, "连接地址")),
				layout.Rigid(layout.Spacer{Height: unit.Dp(2)}.Layout),
				layout.Rigid(func(gtx layout.Context) layout.Dimensions {
					l := material.Label(th, unit.Sp(13), url)
					l.Color = colText
					return l.Layout(gtx)
				}),
				layout.Rigid(layout.Spacer{Height: unit.Dp(12)}.Layout),
				layout.Rigid(captionLabel(th, "配对码")),
				layout.Rigid(layout.Spacer{Height: unit.Dp(2)}.Layout),
				layout.Rigid(func(gtx layout.Context) layout.Dimensions {
					l := material.Label(th, unit.Sp(24), code)
					l.Color = colAccent
					l.Font.Weight = font.Bold
					return l.Layout(gtx)
				}),
				layout.Rigid(layout.Spacer{Height: unit.Dp(16)}.Layout),
				// 按钮行
				layout.Rigid(func(gtx layout.Context) layout.Dimensions {
					return layout.Flex{Axis: layout.Horizontal}.Layout(gtx,
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							label := "复制地址"
							if isCopied(u.copyBtnCopiedAt) {
								label = "已复制 ✓"
							}
							b := material.Button(th, &u.copyBtn, label)
							b.Background = colAccent
							b.Color = colWhite
							b.CornerRadius = unit.Dp(10)
							return b.Layout(gtx)
						}),
						layout.Rigid(layout.Spacer{Width: unit.Dp(8)}.Layout),
						layout.Rigid(func(gtx layout.Context) layout.Dimensions {
							b := material.Button(th, &u.rotateBtn, "换配对码")
							b.Background = colCardAlt
							b.Color = colText
							b.CornerRadius = unit.Dp(10)
							return b.Layout(gtx)
						}),
					)
				}),
			)
		}),
	)
}

// layoutQR 绘制二维码；URL 变化时才重新编码并缓存。失败时显示降级文案。
func (u *uiState) layoutQR(gtx layout.Context, url string) layout.Dimensions {
	if url != u.qrURL {
		u.qrURL = url
		if img, err := urlToImage(url); err != nil {
			log.Printf("[ui] QR encode failed: %v", err)
			u.qrOK = false
		} else {
			u.qrOp = paint.NewImageOp(img)
			u.qrOK = true
		}
	}
	side := gtx.Dp(unit.Dp(148))
	if !u.qrOK {
		// 降级：占位框 + 文案，URL 文本仍可复制
		gtx.Constraints.Min = image.Pt(side, side)
		l := material.Label(u.th, unit.Sp(12), "二维码不可用")
		l.Color = colFaint
		return layout.Center.Layout(gtx, l.Layout)
	}
	img := widget.Image{
		Src:   u.qrOp,
		Fit:   widget.Contain,
		Scale: 1,
	}
	gtx.Constraints.Min = image.Pt(side, side)
	gtx.Constraints.Max = image.Pt(side, side)
	return img.Layout(gtx)
}

// layoutRecordsSection 文字记录卡：标题行（标题 + 复制全部 / 打开日志按钮）+ 列表
func (u *uiState) layoutRecordsSection(gtx layout.Context, records []textRecord) layout.Dimensions {
	th := u.th
	return layout.Flex{Axis: layout.Vertical}.Layout(gtx,
		// 标题行：左标题，右两个小按钮
		layout.Rigid(func(gtx layout.Context) layout.Dimensions {
			return layout.Flex{Axis: layout.Horizontal, Alignment: layout.Middle}.Layout(gtx,
				layout.Rigid(func(gtx layout.Context) layout.Dimensions {
					l := material.Label(th, unit.Sp(15), "实时文字记录")
					l.Color = colText
					l.Font.Weight = font.Bold
					return l.Layout(gtx)
				}),
				layout.Flexed(1, layout.Spacer{}.Layout),
				layout.Rigid(func(gtx layout.Context) layout.Dimensions {
					label := "复制全部"
					if isCopied(u.copyLogBtnCopiedAt) {
						label = "已复制 ✓"
					}
					return smallButton(gtx, th, &u.copyLogBtn, label, colCardAlt)
				}),
				layout.Rigid(layout.Spacer{Width: unit.Dp(6)}.Layout),
				layout.Rigid(func(gtx layout.Context) layout.Dimensions {
					return smallButton(gtx, th, &u.openLogBtn, "打开日志", colCardAlt)
				}),
			)
		}),
		layout.Rigid(layout.Spacer{Height: unit.Dp(10)}.Layout),
		layout.Flexed(1, func(gtx layout.Context) layout.Dimensions {
			return u.layoutRecords(gtx, records)
		}),
	)
}

// layoutRecords 文字记录列表：最近 N 条（新的在上），可纵向滚动。
func (u *uiState) layoutRecords(gtx layout.Context, records []textRecord) layout.Dimensions {
	th := u.th
	if len(records) == 0 {
		l := material.Label(th, unit.Sp(13), "（等待手机端语音…）")
		l.Color = colFaint
		return l.Layout(gtx)
	}
	// 确保逐条复制按钮数量与记录数一致；copyRecordCopiedAt 与其等长同步增减。
	for len(u.copyRecordBtns) < len(records) {
		u.copyRecordBtns = append(u.copyRecordBtns, widget.Clickable{})
		u.copyRecordCopiedAt = append(u.copyRecordCopiedAt, time.Time{})
	}
	for len(u.copyRecordBtns) > len(records) {
		u.copyRecordBtns = u.copyRecordBtns[:len(records)]
		u.copyRecordCopiedAt = u.copyRecordCopiedAt[:len(records)]
	}
	return material.List(th, &u.list).Layout(gtx, len(records), func(gtx layout.Context, i int) layout.Dimensions {
		rec := records[i]
		return layout.Inset{Bottom: unit.Dp(8)}.Layout(gtx, func(gtx layout.Context) layout.Dimensions {
			// 每条一个内嵌深色小块
			return card(gtx, colCardAlt, unit.Dp(10), func(gtx layout.Context) layout.Dimensions {
				return layout.Flex{Axis: layout.Horizontal, Alignment: layout.Baseline}.Layout(gtx,
					layout.Rigid(func(gtx layout.Context) layout.Dimensions {
						ts := material.Label(th, unit.Sp(12), rec.t.Format("15:04:05"))
						ts.Color = colFaint
						return ts.Layout(gtx)
					}),
					layout.Rigid(layout.Spacer{Width: unit.Dp(12)}.Layout),
					layout.Flexed(1, func(gtx layout.Context) layout.Dimensions {
						l := material.Label(th, unit.Sp(14), rec.text)
						l.Color = colText
						return l.Layout(gtx)
					}),
					layout.Rigid(layout.Spacer{Width: unit.Dp(6)}.Layout),
					layout.Rigid(func(gtx layout.Context) layout.Dimensions {
						label := "复制"
						if i < len(u.copyRecordCopiedAt) && isCopied(u.copyRecordCopiedAt[i]) {
							label = "✓"
						}
						return smallButton(gtx, th, &u.copyRecordBtns[i], label, colCard)
					}),
				)
			})
		})
	})
}

// ---- 复用组件 ----

// card 画一个带圆角背景的容器，内部 padding 统一。
func card(gtx layout.Context, bg color.NRGBA, pad unit.Dp, w layout.Widget) layout.Dimensions {
	macro := op.Record(gtx.Ops)
	dims := layout.UniformInset(pad).Layout(gtx, w)
	call := macro.Stop()
	// 先画圆角底，再回放内容
	rr := gtx.Dp(unit.Dp(14))
	defer clip.RRect{Rect: image.Rectangle{Max: dims.Size}, SE: rr, SW: rr, NW: rr, NE: rr}.Push(gtx.Ops).Pop()
	paint.Fill(gtx.Ops, bg)
	call.Add(gtx.Ops)
	return dims
}

// pill 画一个圆角胶囊（用于状态指示）。
func pill(gtx layout.Context, bg color.NRGBA, w layout.Widget) layout.Dimensions {
	macro := op.Record(gtx.Ops)
	dims := layout.Inset{Top: unit.Dp(6), Bottom: unit.Dp(6), Left: unit.Dp(12), Right: unit.Dp(12)}.Layout(gtx, w)
	call := macro.Stop()
	rr := dims.Size.Y / 2
	defer clip.RRect{Rect: image.Rectangle{Max: dims.Size}, SE: rr, SW: rr, NW: rr, NE: rr}.Push(gtx.Ops).Pop()
	paint.Fill(gtx.Ops, bg)
	call.Add(gtx.Ops)
	return dims
}

// captionLabel 返回一个次要灰色小标题 widget。
func captionLabel(th *material.Theme, txt string) layout.Widget {
	return func(gtx layout.Context) layout.Dimensions {
		l := material.Label(th, unit.Sp(12), txt)
		l.Color = colMuted
		return l.Layout(gtx)
	}
}

// drawDot 画一个实心圆（状态点）。d 为直径（Dp）。
func drawDot(gtx layout.Context, c color.NRGBA, d int) layout.Dimensions {
	size := gtx.Dp(unit.Dp(d))
	defer clip.Ellipse{Max: image.Pt(size, size)}.Push(gtx.Ops).Pop()
	paint.ColorOp{Color: c}.Add(gtx.Ops)
	paint.PaintOp{}.Add(gtx.Ops)
	return layout.Dimensions{Size: image.Pt(size, size)}
}

// smallButton 画一个紧凑的次要按钮（用于文字记录卡标题行）。
func smallButton(gtx layout.Context, th *material.Theme, btn *widget.Clickable, label string, bg color.NRGBA) layout.Dimensions {
	b := material.Button(th, btn, label)
	b.Background = bg
	b.Color = colText
	b.CornerRadius = unit.Dp(8)
	b.TextSize = unit.Sp(12)
	b.Inset = layout.Inset{Top: unit.Dp(6), Bottom: unit.Dp(6), Left: unit.Dp(12), Right: unit.Dp(12)}
	return b.Layout(gtx)
}

// formatRecords 把文字记录拼成可复制的多行文本：每行 "时间戳<Tab>文本"。
func formatRecords(records []textRecord) string {
	if len(records) == 0 {
		return ""
	}
	var sb strings.Builder
	for _, r := range records {
		sb.WriteString(r.t.Format("15:04:05"))
		sb.WriteByte('\t')
		sb.WriteString(r.text)
		sb.WriteByte('\n')
	}
	return sb.String()
}

// openPath 用系统默认程序打开文件 / 文件夹。失败只记 warning。
func openPath(target string) {
	if target == "" {
		return
	}
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "windows":
		cmd = exec.Command("cmd", "/c", "start", "", target)
	case "darwin":
		cmd = exec.Command("open", target)
	default:
		cmd = exec.Command("xdg-open", target)
	}
	if err := cmd.Start(); err != nil {
		log.Printf("[ui] open %s failed: %v", target, err)
	}
}
