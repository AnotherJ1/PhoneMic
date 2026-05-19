#!/usr/bin/env bash
# PhoneMic 跨平台冒烟脚本（macOS / Linux）—— 任务 12.3
#
# 任务来源：tasks.md 12.3
# 关联需求：R1.1, R1.2, R1.3, R2.5, R2.6, R2.7
# 设计来源：design.md §9.5
#
# 与 scripts/smoke.ps1 对称：启动桌面二进制 → 健康检查（≤5s）→ GET / →
# POST /api/pair（valid + invalid）→ 优雅退出（≤3s）。
#
# 退出码：0 = 全部通过；非 0 = 某条断言失败。

set -euo pipefail

DESKTOP_BIN="${PHONEMIC_DESKTOP_BIN:-}"
PAIR_CODE="${PHONEMIC_TEST_PAIR_CODE:-}"
PORT="${PHONEMIC_TEST_PORT:-18080}"
STARTUP_BUDGET="${PHONEMIC_STARTUP_BUDGET:-5}"
SHUTDOWN_BUDGET="${PHONEMIC_SHUTDOWN_BUDGET:-3}"

BASE_URL="http://127.0.0.1:${PORT}"

if [[ -z "$DESKTOP_BIN" ]]; then
  for cand in \
    "$(dirname "$0")/../target/release/phonemic-app" \
    "$(dirname "$0")/../target/release/phonemic" \
    "$(dirname "$0")/../apps/desktop/src-tauri/target/release/phonemic"; do
    if [[ -x "$cand" ]]; then
      DESKTOP_BIN="$cand"
      break
    fi
  done
fi
if [[ -z "$DESKTOP_BIN" || ! -x "$DESKTOP_BIN" ]]; then
  echo "[smoke] PHONEMIC_DESKTOP_BIN not set and no default binary found" >&2
  exit 2
fi
if [[ -z "$PAIR_CODE" ]]; then
  echo "[smoke] PHONEMIC_TEST_PAIR_CODE must be provided" >&2
  exit 2
fi

echo "[smoke] launching $DESKTOP_BIN on port $PORT"
"$DESKTOP_BIN" >/tmp/phonemic-smoke.stdout.log 2>/tmp/phonemic-smoke.stderr.log &
PID=$!
trap 'cleanup' EXIT
cleanup() {
  if kill -0 "$PID" 2>/dev/null; then
    local stop_ts now elapsed
    stop_ts=$(date +%s)
    kill -TERM "$PID" 2>/dev/null || true
    for _ in $(seq 1 $((SHUTDOWN_BUDGET * 10))); do
      if ! kill -0 "$PID" 2>/dev/null; then break; fi
      sleep 0.1
    done
    if kill -0 "$PID" 2>/dev/null; then
      kill -KILL "$PID" 2>/dev/null || true
      now=$(date +%s)
      elapsed=$((now - stop_ts))
      echo "[smoke] desktop did not exit within ${SHUTDOWN_BUDGET}s; killed after ${elapsed}s" >&2
      exit 4
    fi
    now=$(date +%s)
    elapsed=$((now - stop_ts))
    echo "[smoke] desktop shut down in ${elapsed}s"
  fi
}

# Wait for /api/health
START_TS=$(date +%s)
HEALTH_OK=false
for _ in $(seq 1 $((STARTUP_BUDGET * 10))); do
  if curl -fsS --max-time 1 "${BASE_URL}/api/health" >/dev/null 2>&1; then
    HEALTH_OK=true
    break
  fi
  sleep 0.1
done
NOW=$(date +%s)
ELAPSED=$((NOW - START_TS))
if [[ "$HEALTH_OK" != true ]]; then
  echo "[smoke] /api/health did not become ready in ${STARTUP_BUDGET}s" >&2
  exit 3
fi
echo "[smoke] /api/health ready in ${ELAPSED}s"

# GET /
T0=$(date +%s%3N 2>/dev/null || date +%s000)
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" --max-time 2 "${BASE_URL}/")
T1=$(date +%s%3N 2>/dev/null || date +%s000)
DELTA_MS=$((T1 - T0))
if [[ "$HTTP_CODE" != "200" ]]; then
  echo "[smoke] GET / returned $HTTP_CODE" >&2
  exit 5
fi
if [[ $DELTA_MS -gt 2000 ]]; then
  echo "[smoke] GET / took ${DELTA_MS}ms (> 2s)" >&2
  exit 6
fi
echo "[smoke] GET / ok in ${DELTA_MS}ms"

# POST /api/pair valid
VALID_BODY=$(printf '{"pairingCode":"%s","fingerprint":"smoke","deviceLabel":"smoke-sh"}' "$PAIR_CODE")
HTTP_CODE=$(curl -s -o /tmp/phonemic-smoke.pair.json -w "%{http_code}" \
  -X POST -H 'Content-Type: application/json' -d "$VALID_BODY" "${BASE_URL}/api/pair")
if [[ "$HTTP_CODE" != "200" ]]; then
  echo "[smoke] valid pair returned $HTTP_CODE" >&2
  exit 7
fi
echo "[smoke] POST /api/pair (valid) ok"

# POST /api/pair invalid
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
  -X POST -H 'Content-Type: application/json' \
  -d '{"pairingCode":"BADCODE0","fingerprint":"smoke","deviceLabel":"smoke-sh"}' \
  "${BASE_URL}/api/pair")
if [[ "$HTTP_CODE" -lt 400 || "$HTTP_CODE" -ge 500 ]]; then
  echo "[smoke] invalid pair propagated unexpected status $HTTP_CODE" >&2
  exit 8
fi
echo "[smoke] POST /api/pair (invalid) rejected with $HTTP_CODE (expected 4xx)"

echo "[smoke] PASS"
exit 0
