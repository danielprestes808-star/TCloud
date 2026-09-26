# TCloud Foundation 4.3

## Objetivo

Completar o ciclo de fóruns do TCloud com operações reais no Telegram e gerenciamento pela interface Web.

## Entregue

- GET `/api/v1/forums` para listar fóruns principais.
- POST `/api/v1/forums` cria supergrupo com modo fórum no Telegram.
- O novo fórum persiste `channel_id` e `access_hash` em `telegram_peer_credentials`.
- POST `/api/v1/forums/rename` usa `channels.editTitle`.
- POST `/api/v1/forums/delete` usa `channels.deleteChannel`.
- `Meus Arquivos` é protegido contra exclusão permanente.
- Por segurança, a exclusão Web só é habilitada para fóruns criados pelo próprio TCloud.
- A exclusão só é aceita se o fórum não tiver nenhum arquivo registrado, inclusive histórico/Lixeira.
- Web possui um gerenciador único de fóruns para criar, renomear e excluir.
- Web usa o mesmo TCloud Core; nenhuma lógica Telegram foi duplicada no frontend.
- `resolve_peer_with_credentials` antigo foi removido; mutações seguem por `InputPeer`/`InputChannel` diretos.

## Teste funcional

O instalador da Foundation 4.3:

1. registra o baseline;
2. compila Core e Web;
3. cria um fórum temporário real no Telegram;
4. confirma `channel_id` e `access_hash` sem exibi-los;
5. confirma listagem via API;
6. renomeia o fórum no Telegram;
7. cria uma pasta/tópico dentro dele;
8. faz upload real de arquivo;
9. executa indexação delta e confirma o vínculo;
10. exclui o arquivo e a pasta;
11. exclui o fórum real;
12. executa delta final;
13. exige retorno exato ao baseline de fóruns, tópicos, arquivos e credenciais.