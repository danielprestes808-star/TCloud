# TCloud Foundation 4.3.2

## Resultado esperado

- `channels.createChannel` usa a resposta `Updates` para obter o `Channel` recém-criado.
- `channel_id` e `access_hash` são persistidos sem depender da propagação de `iter_dialogs`.
- `iter_dialogs` permanece apenas como fallback.
- O delta registra credenciais técnicas dos fóruns encontrados no Telegram.
- Fóruns criados pelo TCloud preservam `source = web-forum` após delta.
- A tentativa incompleta da Foundation 4.3.1 é recuperada e limpa automaticamente.
- Criar, listar, renomear e excluir fórum continua disponível no Web.
- O ciclo funcional cria fórum, pasta, arquivo, sincroniza, remove tudo e retorna ao baseline.