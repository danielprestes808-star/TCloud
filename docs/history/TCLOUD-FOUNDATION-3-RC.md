# TCloud Foundation 3 RC

Versão consolidada da integração Core + Telegram + PostgreSQL + Web.

## Entregue

- Core 0.5.0.
- Sessão Telegram persistente no Core.
- Login dedicado: telefone, código e 2FA.
- PostgreSQL via sqlx-core + sqlx-postgres.
- Grammers 0.8.1.
- Índice incremental Telegram.
- Pastas reais a partir de Mensagens Salvas e diálogos TCloud.
- Documentos e fotos indexados com message_id, peer_id, nome, tamanho, MIME e data.
- Cursor incremental por diálogo.
- Web sem arquivos de demonstração.
- Web com navegação por pasta.
- Atualização Telegram pelo Web.
- Estado claro quando PostgreSQL estiver offline.

## Escopo seguro padrão

O indexador lê apenas:

- Mensagens Salvas.
- Diálogos cujo nome começa com TCloud.

Para permitir todos os diálogos, definir:

TCLOUD_INDEX_ALL_DIALOGS=1

O padrão permanece desligado.

## Limite inicial

TCLOUD_INDEX_MAX_MESSAGES_PER_DIALOG pode ajustar o limite.
O padrão é 20000 mensagens por diálogo.

## Próxima fase

- mutações Web reais: criar pasta, enviar e excluir;
- tópicos/fóruns detalhados;
- WebSocket/delta realtime;
- conectar Desktop e Mobile ao Core.