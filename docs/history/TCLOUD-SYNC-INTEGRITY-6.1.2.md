# TCloud Sync Integrity 6.1.2

Data: 2026-08-22 20:12:08

- query_scalar removido do auto-delta.
- COUNT(*) usa sqlx_core::query::query.
- Resultado usa Row::try_get.
- Em erro da consulta, busy=1 impede indexacao concorrente.
- cargo fmt/check/build aprovados.
- PostgreSQL/Core/Telegram online.
- Delta manual aceita.
- Mobile/Desktop/Web 6.1.1 preservados.

Backup: D:\TCloud\platform-backups\sync-integrity-6.1.2-20260822-201132