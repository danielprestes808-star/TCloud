# TCloud Unified Platform — Foundation 2

## Camadas

Desktop ─┐
Mobile  ─┼── TCloud Core ── PostgreSQL
Web     ─┘       │
                 └── Telegram (Foundation 3)

## Banco canonico criado

- users
- devices
- sessions
- folders
- files
- file_versions
- favorites
- device_file_state
- sync_events
- sync_queue

## Regras

- PostgreSQL guarda metadados e estados.
- Arquivos grandes continuarao armazenados no Telegram.
- Nenhuma credencial real e commitada.
- Desktop e Mobile ainda nao foram migrados para o Core.
- Foundation 3 conectara autenticacao Telegram e indexacao real.

## Desenvolvimento local

Com Docker:
  D:\TCloud\start-tcloud-database.ps1

Plataforma:
  D:\TCloud\run-web-core-dev.ps1

Sem Docker:
  Core inicia em fallback para a interface continuar testavel.