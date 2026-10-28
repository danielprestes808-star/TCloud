# TCloud Foundation 3A

## Entregue

- credenciais Telegram centralizadas no TCloud Core
- valores mantidos fora do codigo e do Git
- Core 0.3.0
- tabela telegram_accounts
- tabela auth_challenges
- tabela telegram_dialogs
- tabela index_runs
- GET /api/v1/auth/status
- POST /api/v1/index/telegram
- GET /api/v1/index/status
- Web mostra se as credenciais Telegram estao prontas

## Importante

A Foundation 3A NAO cria uma segunda sessao Telegram ainda.

Isso e proposital: Desktop e Mobile ja possuem autenticacao funcional.
A Foundation 3B adicionara grammers ao Core e fara o primeiro login
central controlado, com sessao persistente propria do Core.

Depois disso:
Desktop -> Core
Mobile -> Core
Web -> Core
Core -> Telegram