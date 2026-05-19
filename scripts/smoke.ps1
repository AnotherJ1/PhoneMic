# PhoneMic 跨平台冒烟脚本（PowerShell）—— 任务 12.3
#
# 任务来源：tasks.md 12.3
# 关联需求：R1.1、R1.2、R1.3、R2.5、R2.6、R2.7
# 设计来源：design.md §9.5
#
# 流程：
#   1. 启动桌面二进制（默认从 target/release 寻找）
#   2. 等待 /api/health 就绪（≤ 5s）
#   3. GET / 校验 HTML 可加载（≤ 2s）
#   4. POST /api/pair 一次有效配对码 + 一次无效配对码，断言响应
#   5. 优雅终止进程并断言 ≤ 3s 退出
#
# 退出码：0 = 全部通过；非 0 = 某条断言失败。

[CmdletBinding()]
param(
    [string] $DesktopBin = $env:PHONEMIC_DESKTOP_BIN,
    [string] $PairingCode = $env:PHONEMIC_TEST_PAIR_CODE,
    [int]    $Port = [int]($env:PHONEMIC_TEST_PORT ?? 18080),
    [int]    $StartupBudgetSec = 5,
    [int]    $ShutdownBudgetSec = 3
)

$ErrorActionPreference = 'Stop'
$baseUrl = "http://127.0.0.1:$Port"

if (-not $DesktopBin) {
    $candidates = @(
        "$PSScriptRoot/../target/release/phonemic-app.exe",
        "$PSScriptRoot/../target/release/phonemic.exe",
        "$PSScriptRoot/../apps/desktop/src-tauri/target/release/phonemic.exe"
    )
    $DesktopBin = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $DesktopBin -or -not (Test-Path $DesktopBin)) {
    throw "Desktop binary not found. Set PHONEMIC_DESKTOP_BIN or build first."
}
if (-not $PairingCode) {
    throw "Set PHONEMIC_TEST_PAIR_CODE to a known-good pairing code (or arrange for the binary to print it)."
}

Write-Host "[smoke] launching $DesktopBin on port $Port"
$proc = Start-Process -FilePath $DesktopBin -PassThru -WindowStyle Hidden
$startTs = Get-Date
try {
    # Step 1+2: wait for health
    $healthOk = $false
    for ($i = 0; $i -lt ($StartupBudgetSec * 10); $i += 1) {
        try {
            $r = Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/api/health" -TimeoutSec 1
            if ($r.StatusCode -eq 200) {
                $healthOk = $true
                break
            }
        } catch {
            Start-Sleep -Milliseconds 100
        }
    }
    $startupSec = (Get-Date) - $startTs
    if (-not $healthOk) {
        throw "[smoke] /api/health did not become ready in $StartupBudgetSec s"
    }
    Write-Host "[smoke] /api/health ready in $($startupSec.TotalSeconds.ToString('0.000')) s"

    # Step 3: GET /
    $idxStart = Get-Date
    $idx = Invoke-WebRequest -UseBasicParsing -Uri "$baseUrl/"
    $idxSec = (Get-Date) - $idxStart
    if ($idx.StatusCode -ne 200) { throw "[smoke] GET / returned $($idx.StatusCode)" }
    if ($idxSec.TotalSeconds -gt 2) { throw "[smoke] GET / took $($idxSec.TotalSeconds) s (> 2s)" }
    Write-Host "[smoke] GET / ok in $($idxSec.TotalSeconds.ToString('0.000')) s"

    # Step 4: POST /api/pair (valid + invalid)
    $body = @{ pairingCode = $PairingCode; fingerprint = 'smoke'; deviceLabel = 'smoke-ps1' } | ConvertTo-Json
    $okResp = Invoke-WebRequest -UseBasicParsing -Method Post -Uri "$baseUrl/api/pair" -Body $body -ContentType 'application/json'
    if ($okResp.StatusCode -ne 200) { throw "[smoke] valid pair returned $($okResp.StatusCode)" }
    Write-Host "[smoke] POST /api/pair (valid) ok"

    $bad = @{ pairingCode = 'BADCODE0'; fingerprint = 'smoke'; deviceLabel = 'smoke-ps1' } | ConvertTo-Json
    try {
        $badResp = Invoke-WebRequest -UseBasicParsing -Method Post -Uri "$baseUrl/api/pair" -Body $bad -ContentType 'application/json' -ErrorAction Stop
        if ($badResp.StatusCode -ne 401 -and $badResp.StatusCode -ne 400) {
            throw "[smoke] invalid pair returned $($badResp.StatusCode), expected 4xx"
        }
    } catch {
        $code = $_.Exception.Response.StatusCode.value__
        if ($code -lt 400 -or $code -ge 500) {
            throw "[smoke] invalid pair propagated unexpected status $code"
        }
        Write-Host "[smoke] POST /api/pair (invalid) rejected with $code (expected)"
    }
}
finally {
    # Step 5: shutdown budget
    $stopTs = Get-Date
    if ($proc -and -not $proc.HasExited) {
        $proc.CloseMainWindow() | Out-Null
        if (-not $proc.WaitForExit($ShutdownBudgetSec * 1000)) {
            $proc.Kill()
            $shutdownSec = (Get-Date) - $stopTs
            throw "[smoke] desktop did not exit within $ShutdownBudgetSec s; killed after $($shutdownSec.TotalSeconds) s"
        }
        $shutdownSec = (Get-Date) - $stopTs
        Write-Host "[smoke] desktop shut down in $($shutdownSec.TotalSeconds.ToString('0.000')) s"
    }
}

Write-Host "[smoke] PASS"
exit 0
