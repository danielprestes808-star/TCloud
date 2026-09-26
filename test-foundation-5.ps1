param([string]$Root = "D:\TCloud")
$ErrorActionPreference = "Stop"
function Check($ok,[string]$message) {
    if (-not $ok) { throw $message }
    Write-Host "[OK] $message" -ForegroundColor Green
}
$status = Invoke-RestMethod "http://127.0.0.1:8787/api/v1/status" -TimeoutSec 10
Check ($status.version -eq "0.7.0") "Core 0.7.0"
Check ($status.databaseConnected) "PostgreSQL conectado"
Check ($status.telegramConnected) "Telegram conectado"

$cap = Invoke-RestMethod "http://127.0.0.1:8787/api/v1/platform/capabilities" -TimeoutSec 10
Check ($cap.foundation -eq "5.0") "Foundation 5.0"
Check ($cap.forumManagement) "Forum management"
Check ($cap.folderManagement) "Folder management"
Check ($cap.telegramDelta) "Telegram delta"
Check ($cap.credentialBridge) "Credential bridge"

$forums = @(Invoke-RestMethod "http://127.0.0.1:8787/api/v1/forums" -TimeoutSec 10)
Check ($forums.Count -ge 1) "Forums listados"

Write-Host ""
Write-Host "TCLOUD FOUNDATION 5.0 - SMOKE APROVADO" -ForegroundColor Green