# Card 05 打包前置(Windows):把 onchina 二进制 + 前端产物 + china.sqlite +
# PostgreSQL 官方二进制组装到 node\{binaries,resources}。冻结 plain chainspec 已由
# node 二进制内嵌；安装包不复制创世 RocksDB，首启按同一 chainspec 本地物化并校验块 0。
# 之后在 node\ 跑 `npm run tauri build` 产安装包。
#
# 用法:
#   $env:CITIZENCHAIN_PG_DIST = "<postgresql.org 官方二进制解压目录(含 bin\lib\share)>"
#   citizenchain\scripts\prepack.ps1
$ErrorActionPreference = "Stop"

$Root = (Resolve-Path "$PSScriptRoot\..").Path          # citizenchain\
$Here = (Join-Path $Root "node")                        # citizenchain\node

Write-Host "[prepack] build onchina (release)"
Push-Location $Root; cargo build -p onchina --release --config "$Root\config.toml"; Pop-Location

Write-Host "[prepack] build onchina frontend"
Push-Location "$Root\onchina\frontend"; npm ci; npm run build; Pop-Location

Write-Host "[prepack] assemble node\resources"
New-Item -ItemType Directory -Force -Path "$Here\resources\onchina-bin", "$Here\resources\onchina-frontend", "$Here\resources\postgres" | Out-Null
# onchina 二进制随包(Tauri resources\onchina-bin),onchina_proc 从资源目录解析。
Copy-Item "$Root\target\release\onchina.exe" "$Here\resources\onchina-bin\onchina.exe" -Force
if (Test-Path "$Here\resources\onchina-frontend\dist") { Remove-Item -Recurse -Force "$Here\resources\onchina-frontend\dist" }
Copy-Item -Recurse "$Root\onchina\frontend\dist" "$Here\resources\onchina-frontend\dist"

# PostgreSQL 官方二进制(postgresql.org):CITIZENCHAIN_PG_DIST 指向已解压目录(含 bin\lib\share)。
if ($env:CITIZENCHAIN_PG_DIST -and (Test-Path "$($env:CITIZENCHAIN_PG_DIST)\bin")) {
  $dst = "$Here\resources\postgres\windows"
  if (Test-Path $dst) { Remove-Item -Recurse -Force $dst }
  New-Item -ItemType Directory -Force -Path $dst | Out-Null
  Copy-Item -Recurse "$($env:CITIZENCHAIN_PG_DIST)\*" $dst
  Write-Host "[prepack] PostgreSQL 已组装(windows)"
} else {
  Write-Host "[prepack][warn] 未提供 CITIZENCHAIN_PG_DIST。请从 https://www.postgresql.org/download/windows/"
  Write-Host "                取官方二进制(含 bin\lib\share),解压后设 CITIZENCHAIN_PG_DIST 再重跑;否则安装包不含内嵌 PG。"
}

$dst = "$Here\resources\genesis-state"
if (Test-Path $dst) { Remove-Item -Recurse -Force $dst }
# 中文注释：release 状态包只作为正式创世审计制品保留，不进入任一平台安装包。
Write-Host "[prepack] 已确认安装包不携带 genesis-state；首启按冻结 plain chainspec 本地物化"

Write-Host "[prepack] done. 接着在 node\ 执行: npm run tauri build"
