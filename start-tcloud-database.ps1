$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$coreRoot = "D:\TCloud\core"

if (-not (Test-Path $coreRoot -PathType Container)) {
    throw "TCloud Core nao encontrado."
}

if ($null -eq (Get-Command docker -ErrorAction SilentlyContinue)) {
    Write-Host "[AVISO] Docker nao instalado." -ForegroundColor Yellow
    Write-Host "[INFO] Core usara modo fallback." -ForegroundColor DarkCyan
    exit 0
}

$oldPref = $ErrorActionPreference
$ErrorActionPreference = "Continue"

docker info --format "{{.ServerVersion}}" *> $null
$dockerReady = ($LASTEXITCODE -eq 0)

$ErrorActionPreference = $oldPref

if (-not $dockerReady) {
    Write-Host "[AVISO] Docker esta instalado, mas o Docker Desktop/daemon nao esta rodando." -ForegroundColor Yellow
    Write-Host "[INFO] PostgreSQL nao sera iniciado agora." -ForegroundColor DarkCyan
    Write-Host "[INFO] Core usara modo fallback automaticamente." -ForegroundColor DarkCyan
    exit 0
}

Push-Location $coreRoot

try {
    docker compose `
        --env-file .postgres.env `
        -f docker-compose.yml `
        up -d

    if ($LASTEXITCODE -ne 0) {
        Write-Host "[AVISO] PostgreSQL nao iniciou; Core usara fallback." -ForegroundColor Yellow
        exit 0
    }

    Write-Host "[OK] PostgreSQL solicitado." -ForegroundColor Green

    for ($attempt = 1; $attempt -le 45; $attempt++) {
        $oldPref = $ErrorActionPreference
        $ErrorActionPreference = "Continue"

        docker compose `
            --env-file .postgres.env `
            -f docker-compose.yml `
            exec -T postgres `
            pg_isready -U tcloud -d tcloud *> $null

        $ready = ($LASTEXITCODE -eq 0)
        $ErrorActionPreference = $oldPref

        if ($ready) {
            Write-Host "[OK] PostgreSQL pronto em 127.0.0.1:54329." -ForegroundColor Green
            exit 0
        }

        Start-Sleep -Milliseconds 500
    }

    Write-Host "[AVISO] PostgreSQL ainda nao respondeu; Core usara fallback." -ForegroundColor Yellow
    exit 0
}
finally {
    Pop-Location
}