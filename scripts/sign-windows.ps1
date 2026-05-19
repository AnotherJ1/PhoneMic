# PhoneMic Windows 代码签名脚本 —— 任务 14.4
#
# 任务来源：tasks.md 14.4
# 关联需求：R1.1
# 设计来源：design.md §9.7
#
# 行为：
#   - 当 CODESIGN_CERT_PFX_BASE64 + CODESIGN_CERT_PASSWORD 存在时，对所有 .msi
#     与 .exe 产物执行 signtool 签名（SHA256 + RFC3161 时间戳）。
#   - 当 secrets 缺失时退出码 0（design §9.7 documented behavior：贡献者本地
#     构建无 secrets 时不应阻断，但需要在 GITHUB_STEP_SUMMARY 留痕）。
#
# 必须由 GitHub Actions release.yml job 在 windows-latest runner 上调用。

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $ArtifactsDir
)

$ErrorActionPreference = 'Stop'

function Append-Summary($text) {
    if ($env:GITHUB_STEP_SUMMARY) {
        Add-Content -Path $env:GITHUB_STEP_SUMMARY -Value $text -Encoding utf8
    }
    Write-Host $text
}

if (-not (Test-Path $ArtifactsDir)) {
    throw "Artifacts dir not found: $ArtifactsDir"
}

$pfxBase64 = $env:CODESIGN_CERT_PFX_BASE64
$pfxPwd    = $env:CODESIGN_CERT_PASSWORD
$tsaUrl    = $env:CODESIGN_TIMESTAMP_URL
if (-not $tsaUrl) { $tsaUrl = 'http://timestamp.digicert.com' }

if (-not $pfxBase64 -or -not $pfxPwd) {
    Append-Summary "## ⚠ Windows code signing skipped"
    Append-Summary "Secrets ``CODESIGN_CERT_PFX_BASE64`` / ``CODESIGN_CERT_PASSWORD`` are absent."
    Append-Summary "This is the documented behaviour for contributor builds (design §9.7)."
    exit 0
}

# Materialise PFX from base64
$pfxPath = Join-Path $env:RUNNER_TEMP 'phonemic-codesign.pfx'
[System.IO.File]::WriteAllBytes($pfxPath, [Convert]::FromBase64String($pfxBase64))

# Locate signtool.exe (Windows SDK)
$signtool = (Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Recurse -Filter 'signtool.exe' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\signtool.exe$' } |
    Sort-Object FullName -Descending |
    Select-Object -First 1).FullName
if (-not $signtool) { throw "signtool.exe not found; install Windows SDK." }

$targets = Get-ChildItem -Path $ArtifactsDir -Recurse -Include *.msi, *.exe
if (-not $targets) {
    Append-Summary "## ⚠ No .msi/.exe artefacts under $ArtifactsDir"
    exit 0
}

foreach ($t in $targets) {
    Write-Host "[sign-windows] signing $($t.FullName)"
    & $signtool sign /fd SHA256 /td SHA256 /tr $tsaUrl `
        /f $pfxPath /p $pfxPwd $t.FullName
    if ($LASTEXITCODE -ne 0) { throw "signtool failed for $($t.FullName)" }
}

Remove-Item $pfxPath -Force
Append-Summary "## ✅ Windows code signing succeeded"
Append-Summary "Signed $($targets.Count) artefact(s)."
exit 0
