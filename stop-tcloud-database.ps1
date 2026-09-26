$ErrorActionPreference = "Stop"

$coreRoot = "D:\TCloud\core"

if ($null -eq (Get-Command docker -ErrorAction SilentlyContinue)) {
    Write-Host "[AVISO] Docker nao encontrado." -ForegroundColor Yellow
    exit 0
}

Push-Location $coreRoot

try {
    docker compose `
        --env-file .postgres.env `
        -f docker-compose.yml `
        stop

    if ($LASTEXITCODE -ne 0) {
        throw "Falha ao parar PostgreSQL."
    }

    Write-Host "[OK] PostgreSQL do TCloud parado." -ForegroundColor Green
}
finally {
    Pop-Location
}