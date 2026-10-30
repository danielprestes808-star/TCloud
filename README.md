# TCloud

TCloud é uma nuvem pessoal distribuída que utiliza o Telegram como camada primária de armazenamento seguro e redundante, oferecendo clientes nativos para Windows, Web e Android com sincronização em tempo real.

## Matriz de Versões do Ecossistema

| Componente | Diretório | Versão Atual | Stack Tecnológica | Papel no Ecossistema |
| :--- | :--- | :--- | :--- | :--- |
| **App Desktop** | `app/` | **13.0.61** | Tauri v2, Rust, React 19, TypeScript | Cliente Windows com integração nativa ao Windows Explorer via Cloud Files API (CFAPI). |
| **Core API** | `core/` | **0.9.0** | Rust, Axum 0.8, PostgreSQL, Grammers | API REST/Streaming central, catálogo de arquivos, autenticação por dispositivo e ponte Telegram. |
| **Web** | `web/` | **0.3.9** | Next.js 16, React 19, Fluent Design | Portal web com suporte a PWA e visualização de compartilhamentos públicos. |
| **Mobile** | `mobile/` | **1.1.38+119** | Flutter 3.12, Dart, TDLib nativo | Aplicativo Android com backup de galeria, filtros de imagens/vídeos e agrupamento facial local, scanner OCR, biometria, cast e gerenciador de armazenamento/cache. |

Contrato oficial sincronizado: [`contracts/tcloud-ecosystem-contract.json`](file:///d:/TCloud/contracts/tcloud-ecosystem-contract.json).

---

## Estrutura do Repositório

```text
TCloud/
├── app/               # Cliente Desktop Windows (Tauri v2 + React)
├── core/              # Backend Rust / Axum / PostgreSQL
├── web/               # Interface Web Next.js 16 (App Router)
├── mobile/            # Aplicativo Android em Flutter
├── contracts/         # Contratos e especificações do ecossistema
├── docs/              # Documentação organizada
│   ├── history/       # Documentos de evolução de arquitetura (12.x -> 13.x)
│   ├── contracts/     # Contratos e snapshots históricos
│   └── audits/        # Logs e relatórios de auditoria técnica
├── scripts/           # Scripts utilitários e orquestradores de CI/Dev
└── brand-assets/      # Ícones e identidades visuais da marca
```

---

## Ambiente de Desenvolvimento Local

Para inicializar todo o ecossistema ou partes dele:

```powershell
# Executa todos os serviços (PostgreSQL, Core, Web e App Desktop)
powershell -ExecutionPolicy Bypass -File .\run-ecosystem-dev.ps1 -Scope All

# Ou escolha escopos individuais: Desktop, Web ou Core
powershell -ExecutionPolicy Bypass -File .\run-ecosystem-dev.ps1 -Scope Desktop
```

### Inicialização do Banco de Dados
```powershell
# Inicia container PostgreSQL via Docker
.\start-tcloud-database.ps1

# Encerra o container após o uso
.\stop-tcloud-database.ps1
```

---

## Validação e Qualidade (CI / Verificação)

```powershell
# Validação unificada de todo o ecossistema
powershell -ExecutionPolicy Bypass -File .\scripts\verify-all.ps1

# Validação rápida (sem suíte pesada de testes pontuais)
powershell -ExecutionPolicy Bypass -File .\scripts\verify-all.ps1 -Quick

# Ou validar componente individual:
powershell -ExecutionPolicy Bypass -File .\scripts\verify-all.ps1 -Project core
powershell -ExecutionPolicy Bypass -File .\scripts\verify-all.ps1 -Project web
powershell -ExecutionPolicy Bypass -File .\scripts\verify-all.ps1 -Project app
powershell -ExecutionPolicy Bypass -File .\scripts\verify-all.ps1 -Project mobile
```

---

## Segurança da API Publicada

O Core exige `TCLOUD_API_TOKEN` em ambiente publicado. O mesmo segredo deve ser configurado no servidor da Web, sem o prefixo `NEXT_PUBLIC_`. 
* A rota `/health` permanece pública para sondas de orquestração.
* As rotas `/api/v1/*` exigem autenticação Bearer e possuem limites de taxa contra ataques de força bruta.
* Compartilhamentos públicos usam rotas protegidas com tokens únicos de acesso `/api/v1/public/shares/*`.
