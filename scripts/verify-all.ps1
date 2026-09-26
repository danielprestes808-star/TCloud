# ==============================================================================
# TCloud - Validador Unificado do Ecossistema
# ==============================================================================
# Executa as validações, análises estáticas, lints, builds e baterias de testes
# em todos os projetos ativos do ecossistema: core, web, mobile e app.
# ==============================================================================

[CmdletBinding()]
param(
    [ValidateSet("all", "core", "web", "mobile", "app")]
    [string]$Project = "all",

    [switch]$Quick
)

$ErrorActionPreference = "Stop"
$swTotal = [System.Diagnostics.Stopwatch]::StartNew()
$results = [System.Collections.Generic.List[PSCustomObject]]::new()

$rootDir = Split-Path -Parent $PSScriptRoot

function Run-Step {
    param(
        [string]$ProjectName,
        [string]$StepTitle,
        [string]$Directory,
        [scriptblock]$Action
    )

    Write-Host ""
    Write-Host "==============================================================================" -ForegroundColor DarkGray
    Write-Host " [$ProjectName] $StepTitle" -ForegroundColor Cyan
    Write-Host " Diretório: $Directory" -ForegroundColor DarkGray
    Write-Host "==============================================================================" -ForegroundColor DarkGray

    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $status = "PASS"
    $errorMsg = ""

    Push-Location $Directory
    try {
        & $Action
        $sw.Stop()
        Write-Host "  -> Sucesso em $([Math]::Round($sw.Elapsed.TotalSeconds, 1))s" -ForegroundColor Green
    }
    catch {
        $sw.Stop()
        $status = "FAIL"
        $errorMsg = $_.Exception.Message
        Write-Host "  -> Falha após $([Math]::Round($sw.Elapsed.TotalSeconds, 1))s: $errorMsg" -ForegroundColor Red
    }
    finally {
        Pop-Location
    }

    $results.Add([PSCustomObject]@{
        Projeto  = $ProjectName
        Etapa    = $StepTitle
        Status   = $status
        TempoSec = [Math]::Round($sw.Elapsed.TotalSeconds, 1)
        Erro     = $errorMsg
    })

    if ($status -eq "FAIL") {
        throw "A etapa [$StepTitle] do projeto [$ProjectName] falhou."
    }
}

Write-Host "==============================================================================" -ForegroundColor Yellow
Write-Host " TCloud - Verificação do Ecossistema" -ForegroundColor Yellow
Write-Host " Modo: $(if ($Quick) { 'Rápido (sem testes longos)' } else { 'Completo' })" -ForegroundColor Yellow
Write-Host " Escopo: $Project" -ForegroundColor Yellow
Write-Host "==============================================================================" -ForegroundColor Yellow

try {
    # --------------------------------------------------------------------------
    # 1. CORE (Rust / Axum / PostgreSQL)
    # --------------------------------------------------------------------------
    if ($Project -in @("all", "core")) {
        $coreDir = Join-Path $rootDir "core"
        if ($Quick) {
            Run-Step -ProjectName "Core" -StepTitle "Cargo Check" -Directory $coreDir -Action {
                cargo check --color always
            }
        } else {
            Run-Step -ProjectName "Core" -StepTitle "Cargo Test" -Directory $coreDir -Action {
                cargo test --color always
            }
        }
    }

    # --------------------------------------------------------------------------
    # 2. WEB (Next.js 16 / React 19)
    # --------------------------------------------------------------------------
    if ($Project -in @("all", "web")) {
        $webDir = Join-Path $rootDir "web"
        Run-Step -ProjectName "Web" -StepTitle "ESLint" -Directory $webDir -Action {
            npm run lint
        }
        Run-Step -ProjectName "Web" -StepTitle "Next.js Build" -Directory $webDir -Action {
            npm run build
        }
    }

    # --------------------------------------------------------------------------
    # 3. MOBILE (Flutter / Android)
    # --------------------------------------------------------------------------
    if ($Project -in @("all", "mobile")) {
        $mobileDir = Join-Path $rootDir "mobile"
        Run-Step -ProjectName "Mobile" -StepTitle "Flutter Analyze" -Directory $mobileDir -Action {
            flutter analyze
        }
        if (-not $Quick) {
            Run-Step -ProjectName "Mobile" -StepTitle "Flutter Test" -Directory $mobileDir -Action {
                flutter test
            }
        }
    }

    # --------------------------------------------------------------------------
    # 4. APP (Desktop Tauri / Windows)
    # --------------------------------------------------------------------------
    if ($Project -in @("all", "app")) {
        $appDir = Join-Path $rootDir "app"
        Run-Step -ProjectName "App" -StepTitle "Desktop Only Integrity" -Directory $appDir -Action {
            npm run desktop:verify
        }
        Run-Step -ProjectName "App" -StepTitle "i18n Validation" -Directory $appDir -Action {
            npm run i18n:validate
        }
        Run-Step -ProjectName "App" -StepTitle "Vite & TypeScript Build" -Directory $appDir -Action {
            npm run build
        }
        if ($Quick) {
            Run-Step -ProjectName "App" -StepTitle "Cargo Check (Tauri)" -Directory $appDir -Action {
                cargo check --manifest-path src-tauri\Cargo.toml
            }
        } else {
            Run-Step -ProjectName "App" -StepTitle "Cargo Test (Tauri)" -Directory $appDir -Action {
                cargo test --manifest-path src-tauri\Cargo.toml
            }
        }
    }
}
catch {
    Write-Host ""
    Write-Host "Execução interrompida devido a uma falha." -ForegroundColor Red
}
finally {
    $swTotal.Stop()
    Write-Host ""
    Write-Host "==============================================================================" -ForegroundColor Cyan
    Write-Host " RESUMO FINAL DA VALIDAÇÃO" -ForegroundColor Cyan
    Write-Host "==============================================================================" -ForegroundColor Cyan

    $results | Format-Table -Property Projeto, Etapa, Status, TempoSec

    $failedCount = ($results | Where-Object { $_.Status -eq "FAIL" }).Count
    $totalTime = [Math]::Round($swTotal.Elapsed.TotalSeconds, 1)

    if ($failedCount -eq 0) {
        Write-Host "Tudo pronto! Todas as etapas ($($results.Count)) passaram com sucesso em ${totalTime}s." -ForegroundColor Green
        exit 0
    } else {
        Write-Host "Atenção: $failedCount etapa(s) falharam. Verifique os logs acima." -ForegroundColor Red
        exit 1
    }
}
