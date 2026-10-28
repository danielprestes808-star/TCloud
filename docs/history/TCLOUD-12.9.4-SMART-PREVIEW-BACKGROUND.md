# TCloud 12.9.4 — Smart Preview & Background Efficiency

Esta versão prioriza a interface ativa e reduz trabalho invisível quando o TCloud fica minimizado ou em segundo plano.

## Melhorias principais

- Previews explícitos e previews especulativos agora usam filas separadas.
- Ações do usuário têm prioridade sobre prefetch; um preview especulativo enfileirado é promovido imediatamente se o usuário abrir aquele arquivo.
- Prefetch de preview fica limitado a uma tarefa de baixa prioridade e aguarda a interface ficar ociosa.
- Ao ocultar/minimizar o TCloud, novas miniaturas/previews especulativos deixam de iniciar.
- Após 60 segundos oculto, caches de imagem em RAM são reduzidos para um conjunto quente pequeno; o cache persistente em disco é preservado.
- Após longos períodos oculto, o LRU de navegação é reduzido mantendo as pastas mais recentes, evitando crescimento de RAM.
- Queries inativas de arquivos e metadados de vídeo são liberadas depois de 90 segundos em segundo plano; a pasta ativa continua preservada.
- O polling de conectividade pausa quando a WebView está oculta e faz uma checagem imediata ao voltar.
- A reconciliação remota silenciosa deixa de iniciar logo após clique, roda do mouse ou teclado, dando prioridade à interface.
- O preview de imagem inicia após o primeiro frame do modal em vez de aguardar um atraso fixo de 60 ms.
- PDF longos mantêm uma vizinhança menor de páginas renderizadas, reduzindo RAM/GPU.
- Miniaturas persistentes continuam em `app_data_dir`, portanto reiniciar o TCloud não exige recriá-las quando ainda estão válidas.

## Preservado

Telegram, sincronização em segundo plano, unidade T:, CFAPI, Cofre, uploads/downloads, streaming, lixeira, versões, pesquisa, favoritos e integração com o Explorer permanecem preservados.
