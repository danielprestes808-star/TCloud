#!/usr/bin/env pwsh
# push-all.ps1 — Commit + Push automático para todos os sub-repos do TCloud
# Uso simples  : .\push-all.ps1 -Message "feat: descrição"
# Só push      : .\push-all.ps1
# Branch custom: .\push-all.ps1 -Branch "feat/x" -Message "feat: x"

param(
    [string]$Branch  = "main",
    [string]$Message = ""
)

$ErrorActionPreference = "Continue"
$root = $PSScriptRoot

$repos = @(
    @{ Path = "$root";          Name = "root   "; Remote = "origin" },
    @{ Path = "$root\app";      Name = "app    "; Remote = "origin" },
    @{ Path = "$root\core";     Name = "core   "; Remote = "origin" },
    @{ Path = "$root\mobile";   Name = "mobile "; Remote = "origin" },
    @{ Path = "$root\web";      Name = "web    "; Remote = "origin" }
)

$pushed = 0
$skipped = 0
$failed = 0

Write-Host ""
Write-Host "  TCloud — push-all.ps1" -ForegroundColor Cyan
Write-Host "  ─────────────────────────────────────────" -ForegroundColor DarkGray
Write-Host ""

foreach ($repo in $repos) {
    $path   = $repo.Path
    $name   = $repo.Name

    if (-not (Test-Path "$path\.git")) {
        Write-Host "  [$name] sem .git — ignorado" -ForegroundColor DarkGray
        $skipped++
        continue
    }

    Push-Location $path

    # --- Commit automático se -Message foi passado ---
    if ($Message -ne "") {
        $dirty = (git status --short 2>&1) | Where-Object { $_ -match "^\s*[MADRCU?]" }
        if ($dirty) {
            git add -A | Out-Null
            git commit -m $Message 2>&1 | Out-Null
            Write-Host "  [$name] commit: $Message" -ForegroundColor Yellow
        }
    }

    # --- Verifica se há commits para enviar ---
    $ahead = git rev-list --count "origin/$Branch..HEAD" 2>&1
    if ($ahead -match '^\d+$' -and [int]$ahead -eq 0) {
        Write-Host "  [$name] nada a enviar" -ForegroundColor DarkGray
        $skipped++
        Pop-Location
        continue
    }

    # --- Push ---
    $out = git push origin $Branch 2>&1
    if ($LASTEXITCODE -eq 0 -or ($out -join "") -match "up-to-date|new branch") {
        Write-Host "  [$name] ✓ enviado para origin/$Branch" -ForegroundColor Green
        $pushed++
    } else {
        Write-Host "  [$name] ✗ falha: $($out -join ' ')" -ForegroundColor Red
        $failed++
    }

    Pop-Location
}

Write-Host ""
Write-Host "  ─────────────────────────────────────────" -ForegroundColor DarkGray
Write-Host "  ✓ $pushed enviados   ~ $skipped sem mudanças   ✗ $failed com erro" -ForegroundColor Cyan
Write-Host ""
