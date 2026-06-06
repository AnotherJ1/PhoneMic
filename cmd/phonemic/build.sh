#!/usr/bin/env bash
# 本地 release 构建。
#
# 重要：跨平台打包不能在一台机器交叉编译完成。
#   gioui 仅 Windows 后端是纯 Go（Direct3D）；macOS（Metal/Cocoa）与
#   Linux（Vulkan/X11/Wayland）后端都依赖系统 C 库，必须 CGO=1 且只能在
#   对应原生平台编译。因此：
#     - 本脚本只编译「当前所在平台」。
#     - 要一次出齐三平台包，请用 GitHub Actions（见 .github/workflows/release.yml，
#       推 v* tag 自动构建并发 Release）。
#
# 依赖：
#   - Windows：无需 CGO（gioui 走 Direct3D，keybd_event 纯 Go）。
#   - Linux：需 gcc 及 gioui 系统库，见 https://gioui.org/doc/install/linux
#   - macOS：需 Xcode 命令行工具（CGO）。
set -euo pipefail

cd "$(dirname "$0")"

GOPROXY=${GOPROXY:-https://goproxy.cn,direct}
LDFLAGS="-s -w"
mkdir -p dist

# 探测当前平台
GOOS_CUR=$(go env GOOS)
GOARCH_CUR=$(go env GOARCH)

case "$GOOS_CUR" in
  windows)
    # Windows 用 GUI 子系统：去掉黑色控制台窗口（生产）。
    # 调试时手动 go run . 可看到日志。
    OUT="dist/phonemic-${GOOS_CUR}-${GOARCH_CUR}.exe"
    echo "==> building $OUT (CGO off)"
    CGO_ENABLED=0 GOPROXY="$GOPROXY" \
      go build -trimpath -ldflags "$LDFLAGS -H windowsgui" -o "$OUT" .
    ;;
  linux|darwin)
    # mac/linux 必须 CGO，且只能原生编译
    OUT="dist/phonemic-${GOOS_CUR}-${GOARCH_CUR}"
    echo "==> building $OUT (CGO on — native platform required)"
    CGO_ENABLED=1 GOPROXY="$GOPROXY" \
      go build -trimpath -ldflags "$LDFLAGS" -o "$OUT" .
    ;;
  *)
    echo "unsupported GOOS: $GOOS_CUR" >&2
    exit 1
    ;;
esac

echo
echo "Done: $OUT"
ls -la "$OUT"
echo
echo "Cross-platform packaging: push a v* tag to trigger .github/workflows/release.yml"
