#!/usr/bin/env pwsh
# push-all.ps1 — Envia todos os sub-repositórios para o GitHub
# Use: .\push-all.ps1
# Use: .\push-all.ps1 -Branch "feat/minha-feature"
# Use: .\push-all.ps1 -Message "fix: correcao urgente" (faz commit + push)

param(
    [string]$Branch  = "main",
    [string]$Message = ""        # Se informado, faz 'git add -A && git commit' antes do push
)

$ErrorActionPreference = "Stop"
$repos = @(
    @{ Path = "$PSScriptRoot";          Name = "root    "; Remote = "" },
    @{ Path = "$PSScriptRoot\app";      Name = "app     "; Remote = "" },
    @{ Path = "$PSScriptRoot\core";     Name = "core    "; Remote = "origin" },
    @{ Path = "$PSScriptRoot\mobile";   Name = "mobile  "; Remote = "" },
    @{ Path = "$PSScriptRoot\web";      Name = "web     "; Remote = "origin" }
)

function Write-Status($name, $msg, $color = "Cyan") {
    Write-Host "  [$name] $msg" -ForegroundColor $color
}

foreach ($repo in $repos) {
    $path   = $repo.Path
    $name   = $repo.Name
    $remote = $repo.Remote

    if (-not (Test-Path "$path\.git")) {
        Write-Status $name "Sem repositório git — ignorado" "DarkGray"
        continue
    }

    Push-Location $path

    # Commit automático se -Message fornecido
    if ($Message -ne "") {
        $dirty = git status --short 2>&1
        if ($dirty) {
            git add -A | Out-Null
            git commit -m $Message | Out-Null
            Write-Status $name "Commit: $Message" "Yellow"
        } else {
            Write-Status $name "Nada para commitar" "DarkGray"
        }
    }

    # Push
    if ($remote -eq "") {
        $remote = (git remote 2>&1 | Select-Object -First 1).Trim()
    }

    if ($remote -eq "") {
        Write-Status $name "Sem remote configurado — configure com: git remote add origin <url>" "DarkYellow"
    } else {
        try {
            $out = git push $remote $Branch 2>&1
            if ($LASTEXITCODE -eq 0 -or $out -match "up-to-date") {
                Write-Status $name "Push OK ($remote/$Branch)" "Green"
            } else {
                Write-Status $name "Erro: $out" "Red"
            }
        } catch {
            Write-Status $name "Falha no push: $_" "Red"
        }
    }

    Pop-Location
}

Write-Host ""
Write-Host "Concluído." -ForegroundColor Cyan
Write-Host ""
Write-Host "Para configurar remotes que faltam, use:" -ForegroundColor DarkGray
Write-Host "  git -C app    remote add origin https://github.com/SEU-USUARIO/TCloud-App.git" -ForegroundColor DarkGray
Write-Host "  git -C mobile remote add origin https://github.com/SEU-USUARIO/TCloud-Mobile.git" -ForegroundColor DarkGray
Write-Host "  git           remote add origin https://github.com/SEU-USUARIO/TCloud.git" -ForegroundColor DarkGray
