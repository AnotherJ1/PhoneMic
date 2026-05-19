#!/usr/bin/env bash
# PhoneMic macOS 代码签名 + 公证脚本 —— 任务 14.4
#
# 任务来源：tasks.md 14.4
# 关联需求：R1.1
# 设计来源：design.md §9.7
#
# 行为：
#   - 当 CODESIGN_APPLE_CERT_P12_BASE64 + CODESIGN_APPLE_CERT_PASSWORD 与
#     APPLE_ID_USERNAME / APPLE_ID_TEAM_ID / APPLE_ID_APP_PASSWORD 都存在时：
#       1. 把 .p12 导入临时 keychain
#       2. codesign 对所有 .app / .dmg 产物
#       3. notarytool submit + staple
#   - 当任一 secret 缺失：退出 0 并写 GITHUB_STEP_SUMMARY 警告（设计上允许
#     贡献者本地构建跳过签名，详见 design §9.7）。

set -euo pipefail

ARTIFACTS_DIR="${1:-}"
if [[ -z "$ARTIFACTS_DIR" || ! -d "$ARTIFACTS_DIR" ]]; then
  echo "Usage: $0 <artifacts-dir>" >&2
  exit 2
fi

append_summary() {
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf '%s\n' "$1" >> "$GITHUB_STEP_SUMMARY"
  fi
  printf '%s\n' "$1"
}

P12_BASE64="${CODESIGN_APPLE_CERT_P12_BASE64:-}"
P12_PASSWORD="${CODESIGN_APPLE_CERT_PASSWORD:-}"
APPLE_ID="${APPLE_ID_USERNAME:-}"
APPLE_TEAM="${APPLE_ID_TEAM_ID:-}"
APPLE_APP_PASSWORD="${APPLE_ID_APP_PASSWORD:-}"

if [[ -z "$P12_BASE64" || -z "$P12_PASSWORD" || -z "$APPLE_ID" || -z "$APPLE_TEAM" || -z "$APPLE_APP_PASSWORD" ]]; then
  append_summary "## ⚠ macOS code signing skipped"
  append_summary "One or more required secrets absent (CODESIGN_APPLE_CERT_*, APPLE_ID_*)."
  append_summary "This is the documented behaviour for contributor builds (design §9.7)."
  exit 0
fi

KEYCHAIN_PATH="${RUNNER_TEMP:-/tmp}/phonemic-codesign.keychain-db"
KEYCHAIN_PASSWORD="$(openssl rand -hex 16)"

cleanup() {
  security delete-keychain "$KEYCHAIN_PATH" 2>/dev/null || true
}
trap cleanup EXIT

P12_PATH="${RUNNER_TEMP:-/tmp}/phonemic-codesign.p12"
echo "$P12_BASE64" | base64 --decode > "$P12_PATH"

security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
security set-keychain-settings -lut 21600 "$KEYCHAIN_PATH"
security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
security import "$P12_PATH" -P "$P12_PASSWORD" -A -t cert -f pkcs12 -k "$KEYCHAIN_PATH"
security list-keychains -d user -s "$KEYCHAIN_PATH" $(security list-keychains -d user | tr -d '"')
security set-key-partition-list -S apple-tool:,apple: -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN_PATH"
rm -f "$P12_PATH"

IDENTITY=$(security find-identity -v -p codesigning "$KEYCHAIN_PATH" \
  | awk -F'"' '/Developer ID Application/ {print $2; exit}')
if [[ -z "$IDENTITY" ]]; then
  echo "[sign-macos] no Developer ID Application identity found in keychain" >&2
  exit 1
fi
append_summary "## ✅ macOS signing identity loaded: $IDENTITY"

# Sign every .app and .dmg under the artefacts dir
shopt -s nullglob
APPS=( $(find "$ARTIFACTS_DIR" -type d -name '*.app') )
DMGS=( $(find "$ARTIFACTS_DIR" -type f -name '*.dmg') )

for app in "${APPS[@]}"; do
  echo "[sign-macos] codesigning $app"
  codesign --force --deep --options runtime --timestamp --sign "$IDENTITY" "$app"
done

for dmg in "${DMGS[@]}"; do
  echo "[sign-macos] codesigning $dmg"
  codesign --force --options runtime --timestamp --sign "$IDENTITY" "$dmg"
  echo "[sign-macos] notarising $dmg"
  xcrun notarytool submit "$dmg" \
    --apple-id "$APPLE_ID" \
    --team-id "$APPLE_TEAM" \
    --password "$APPLE_APP_PASSWORD" \
    --wait
  xcrun stapler staple "$dmg"
done

append_summary "## ✅ macOS code signing + notarisation succeeded"
append_summary "Signed ${#APPS[@]} app bundle(s); notarised ${#DMGS[@]} dmg(s)."
exit 0
