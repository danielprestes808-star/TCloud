# TCloud 12.9.5 — Startup & Search Efficiency

Esta rodada reduz trabalho pesado na abertura do TCloud e durante buscas, sem alterar o modelo de sincronização ou a integração com o Explorer.

## Melhorias principais

- O CFAPI registrado deixa de bloquear a thread de `setup` em uma abertura normal: o WebView recebe o primeiro frame e, 350 ms depois, o provider é reconectado em worker de fundo.
- Inicializações `--background` continuam conectando o CFAPI imediatamente, preservando o comportamento de login do Windows e a unidade T: em segundo plano.
- A busca global passa a ser **local-first**: resultados do índice SQLite aparecem antes da busca remota do Telegram.
- A busca remota só começa depois que o usuário para de digitar e a interface fica ociosa, reduzindo chamadas repetidas e picos de CPU/rede.
- Resultados de buscas antigas são descartados por sequência, evitando que uma resposta lenta sobrescreva uma pesquisa mais nova.
- A consulta de conteúdo SQLite foi movida para `spawn_blocking`, impedindo que uma varredura de texto mais pesada dispute o runtime assíncrono usado pela interface.
- A tela de arquivos não entra em estado de carregamento total quando já existem resultados locais enquanto a busca remota continua.
- Reconciliações de pasta deixam de invalidar todas as queries `files`; somente a pasta ativa é atualizada.
- O polling do CFAPI agora é adaptativo: configuração não registrada é consultada raramente, conexão saudável em ritmo moderado e reconexão em ritmo mais rápido.
- O resumo da sincronização também é adaptativo: atualiza mais rápido quando há fila/problemas e desacelera quando está ocioso.
- A verificação automática de atualização é adiada para 30 s e preferencialmente roda em idle, evitando competir com a abertura e os primeiros cliques.

## Preservado

Telegram, sincronização em segundo plano, unidade T:, CFAPI, Cofre, upload/download, previews, miniaturas, cache de navegação, streaming, lixeira, versões, favoritos e integração com o Explorer permanecem preservados.
