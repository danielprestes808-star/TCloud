# TCloud — Modelo de armazenamento

Entidades principais previstas:

- folders: árvore de pastas lógica do TCloud
- files: metadados e referência ao arquivo no Telegram
- sync_jobs: fila de sincronização
- sync_conflicts: conflitos entre cópia local e remota

Campos importantes de files:
id, name, path, size, mime_type, hash, telegram_file_id,
telegram_message_id, telegram_chat_id, folder_id, sync_status,
local_status, created_at, updated_at.
