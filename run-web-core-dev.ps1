$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$root = "D:\TCloud"
$coreRoot = Join-Path $root "core"
$webRoot = Join-Path $root "web"
$dbScript = Join-Path $root "start-tcloud-database.ps1"

if (-not (Test-Path $coreRoot -PathType Container)) {
    throw "TCloud Core nao encontrado: $coreRoot"
}

if (-not (Test-Path $webRoot -PathType Container)) {
    throw "TCloud Web nao encontrado: $webRoot"
}

if (Test-Path $dbScript -PathType Leaf) {
    powershell `
        -NoLogo `
        -NoProfile `
        -ExecutionPolicy Bypass `
        -File $dbScript
}

Write-Host ""
Write-Host "Abrindo TCloud Core..." -ForegroundColor Cyan

Start-Process powershell -ArgumentList @(
    "-NoLogo",
    "-NoExit",
    "-Command",
    "Set-Location '$coreRoot'; cargo run"
)

Write-Host "Abrindo TCloud Web..." -ForegroundColor Cyan

Start-Process powershell -ArgumentList @(
    "-NoLogo",
    "-NoExit",
    "-Command",
    "Set-Location '$webRoot'; npm run dev"
)

Write-Host ""
Write-Host "TCloud Web:" -ForegroundColor Green
Write-Host "http://localhost:3000"
Write-Host ""
Write-Host "TCloud Core:" -ForegroundColor Green
Write-Host "http://127.0.0.1:8787/health"
Write-Host ""
Write-Host "PostgreSQL:" -ForegroundColor Green
Write-Host "127.0.0.1:54329 quando Docker Desktop estiver rodando"
Write-Host ""
Write-Host "Sem Docker Desktop, o Core inicia em fallback automaticamente." -ForegroundColor DarkCyan