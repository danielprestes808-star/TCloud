# TCloud

TCloud é uma nuvem pessoal que usa o Telegram como camada de armazenamento e oferece clientes para Windows, Web e Android.

## Projetos ativos

- `app/`: cliente Windows em Tauri, Rust e React; integra com o Explorador por CFAPI.
- `core/`: API em Rust/Axum, índice PostgreSQL e integração Telegram para a Web.
- `web/`: interface Next.js; encaminha chamadas ao Core exclusivamente pelo servidor.
- `mobile/`: aplicativo Android em Flutter com TDLib e cache local.

Cada pasta acima é construída e validada separadamente. `app`, `core` e `web` possuem seus próprios repositórios Git.

## Segurança da API publicada

O Core exige `TCLOUD_API_TOKEN` em ambiente publicado. O mesmo segredo deve ser configurado no servidor da Web, sem o prefixo `NEXT_PUBLIC_`. A rota `/health` permanece pública; as rotas `/api/v1/*` exigem Bearer token e possuem limites de requisição.

## Validação rápida

```powershell
# Validação unificada de todo o ecossistema (ou com -Quick para validação ágil)
powershell -ExecutionPolicy Bypass -File .\scripts\verify-all.ps1

# Ou individualmente:
cd D:\TCloud\core
cargo test

cd D:\TCloud\web
npm run lint
npm run build

cd D:\TCloud\mobile
flutter analyze
flutter test

cd D:\TCloud\app
npm run i18n:check
npm run desktop:verify
npm run build
cargo test --manifest-path src-tauri\Cargo.toml
```

Os diretórios `*-backups` e `logs` são históricos locais de segurança/auditoria e não fazem parte dos aplicativos publicados.
