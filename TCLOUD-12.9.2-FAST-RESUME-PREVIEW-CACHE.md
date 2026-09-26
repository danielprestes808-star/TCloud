# TCloud 12.9.2 — Fast Resume & Preview Cache

Esta versão é uma rodada de desempenho focada em duas situações perceptíveis no desktop:
voltar ao TCloud depois de minimizar/ocultar a janela e navegar por pastas com muitas imagens.

## Melhorias

- restauração nativa da janela centralizada no Rust;
- a janela é mostrada antes de ser desminimizada/focada;
- abertura por bandeja e por segunda execução usa o mesmo caminho rápido;
- o estado `VISIBLE` deixou de ser persistido pelo plugin de window-state, evitando conflito entre
  o comportamento "fechar para a bandeja" e uma abertura normal;
- evento nativo `tcloud-window-resumed` informa ao frontend que a janela acabou de voltar;
- período curto de retomada visual com animações/blur caros temporariamente suspensos;
- miniaturas aguardam o primeiro frame antes de retomar;
- prefetch especulativo de miniaturas fica limitado a 1 tarefa simultânea;
- atualização remota da árvore de pastas é adiada até a interface estabilizar após a retomada;
- rotinas visíveis de housekeeping não competem com o primeiro frame;
- cards de imagem não baixam mais o preview completo como fallback de thumbnail;
- limpeza física dos caches de preview/thumbnail foi limitada a no máximo uma varredura a cada
  30 segundos por cache, evitando várias varreduras/sorts seguidas do diretório.

## O que foi preservado

- sincronização em segundo plano;
- Telegram;
- CFAPI e unidade T:;
- Cofre/criptografia;
- cache local;
- preview completo quando o usuário realmente abre um arquivo;
- atualização automática da árvore de pastas depois da retomada;
- comportamento de fechar para a bandeja.

## Validação recomendada no Windows

1. Abra o TCloud e deixe uma pasta com muitas imagens visível.
2. Minimize e espere alguns segundos.
3. Clique no ícone do TCloud na barra de tarefas.
4. Feche pelo X para a bandeja e abra novamente pelo ícone/pin.
5. Faça upload/sincronização e repita o teste.
6. Rode `npm run desktop:verify`.
7. Rode `npm run tauri:shell`.

A melhora esperada é que a janela reapareça primeiro e o trabalho visual/refresh volte em seguida,
em vez de tudo disputar CPU/GPU/IPC no mesmo instante.
