# TCloud Foundation 4 — Web Mutations

Core 0.7.0 e Web operacional.

## Entregue

- Nova pasta: cria tópico real em um fórum Telegram.
- Enviar: envia arquivos do navegador para o Telegram e registra no PostgreSQL.
- Novo fórum: cria fórum/supergrupo real no Telegram.
- Renomear: edita o nome/caption da mensagem no Telegram e atualiza o índice.
- Mover: altera a localização canônica no TCloud e protege essa escolha de reindexações.
- Lixeira: remoção lógica e reversível no índice central.
- Restaurar: devolve o arquivo à pasta anterior.
- Atualização automática da Web a cada 5 segundos e ao retornar para a aba.
- Fóruns criados pela Web passam a integrar o escopo gerenciado do indexador.
- A regra da Foundation 3 RC2.9 continua preservando tópico conhecido quando uma leitura do Telegram vier com topic_id 0.

## Segurança operacional

Mover e Lixeira nesta fase são operações canônicas do Core/PostgreSQL.
O arquivo remoto do Telegram não é destruído nessas duas ações.
Renomear, upload, criação de pasta e criação de fórum usam Telegram real.

## Limite temporário do upload Web

O upload Web desta fase aceita até 512 MB por arquivo porque o proxy atual recebe o corpo inteiro antes de encaminhar ao Core.
Uma etapa futura poderá trocar esse caminho por streaming/chunks sem alterar o contrato visual.

## Rotas Core

- POST /api/v1/forums
- POST /api/v1/folders
- POST /api/v1/files/upload
- POST /api/v1/files/rename
- POST /api/v1/files/move
- POST /api/v1/files/trash
- POST /api/v1/files/restore
- GET  /api/v1/trash

## Validação

O instalador da Foundation 4 executa uma única validação consolidada:
- migration PostgreSQL;
- cargo fmt/check/build;
- Next build;
- Core 0.7.0;
- PostgreSQL + Telegram;
- contratos das novas rotas sem criar recursos de teste;
- indexação delta;
- confirmação de que os vínculos de tópicos da RC2.9 não diminuíram.