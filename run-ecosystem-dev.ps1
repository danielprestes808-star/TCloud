# ==============================================================================
# TCloud - Orquestrador de Desenvolvimento Local do Ecossistema
# ==============================================================================
# Inicializa os serviços de desenvolvimento conforme o escopo desejado:
# - Banco de dados (PostgreSQL local / Docker)
# - Core (API Rust / Axum)
# - Web (Next.js 16)
# - App Desktop (Tauri + Vite dev)
# ==============================================================================

[CmdletBinding()]
param(
    [ValidateSet("All", "Desktop", "Web", "Core")]
    [string]$Scope = "All"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$root = $PSScriptRoot
$coreRoot = Join-Path $root "core"
$webRoot  = Join-Path $root "web"
$appRoot  = Join-Path $root "app"
$dbScript = Join-Path $root "start-tcloud-database.ps1"

Write-Host "==============================================================================" -ForegroundColor Cyan
Write-Host " TCloud - Ambiente de Desenvolvimento Local" -ForegroundColor Cyan
Write-Host " Escopo selecionado: $Scope" -ForegroundColor Yellow
Write-Host "==============================================================================" -ForegroundColor Cyan

# 1. Banco de Dados
if (Test-Path $dbScript -PathType Leaf) {
    Write-Host "[1] Verificando / Inicializando PostgreSQL local..." -ForegroundColor Cyan
    try {
        powershell -NoLogo -NoProfile -ExecutionPolicy Bypass -File $dbScript
    } catch {
        Write-Host "Aviso: Banco de dados local não inicializou. O Core usará fallback em memória." -ForegroundColor Yellow
    }
}

# 2. Core (API Axum)
Write-Host "[2] Inicializando TCloud Core API (porta 8787)..." -ForegroundColor Cyan
Start-Process powershell -ArgumentList @(
    "-NoLogo",
    "-NoExit",
    "-Command",
    "Set-Location '$coreRoot'; Write-Host '--- TCloud Core API ---' -ForegroundColor Green; cargo run"
)

# 3. Web (Next.js)
if ($Scope -in @("All", "Web")) {
    Write-Host "[3] Inicializando TCloud Web (porta 3000)..." -ForegroundColor Cyan
    Start-Process powershell -ArgumentList @(
        "-NoLogo",
        "-NoExit",
        "-Command",
        "Set-Location '$webRoot'; Write-Host '--- TCloud Web ---' -ForegroundColor Green; npm run dev"
    )
}

# 4. App Desktop (Tauri)
if ($Scope -in @("All", "Desktop")) {
    Write-Host "[4] Inicializando TCloud Desktop App (Tauri dev)..." -ForegroundColor Cyan
    Start-Process powershell -ArgumentList @(
        "-NoLogo",
        "-NoExit",
        "-Command",
        "Set-Location '$appRoot'; Write-Host '--- TCloud Desktop (Tauri) ---' -ForegroundColor Green; npm run dev"
    )
}

Write-Host ""
Write-Host "==============================================================================" -ForegroundColor Green
Write-Host " Ecossistema em execução:" -ForegroundColor Green
Write-Host "==============================================================================" -ForegroundColor Green
Write-Host " * Core API:   http://127.0.0.1:8787/health" -ForegroundColor White
if ($Scope -in @("All", "Web")) {
    Write-Host " * Web Client: http://localhost:3000" -ForegroundColor White
}
if ($Scope -in @("All", "Desktop")) {
    Write-Host " * Desktop:    Janela Tauri nativa do Windows aberta em modo Dev" -ForegroundColor White
}
Write-Host ""
Write-Host "Para encerrar o banco após o uso: .\stop-tcloud-database.ps1" -ForegroundColor DarkGray
Write-Host "==============================================================================" -ForegroundColor Green
