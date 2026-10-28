# TCloud 12.9.0 — Performance & Fluidity

## Alterações principais

- progresso de upload/download agrupado para evitar dezenas de renders por segundo;
- persistência das filas desacoplada do progresso, reduzindo gravações repetidas no Store;
- ResizeObserver e roda do mouse do Explorer agrupados por `requestAnimationFrame`;
- thumbnails usam decode assíncrono/lazy e transição somente de opacidade;
- Query Client com política padrão menos agressiva de refetch/retry;
- sincronização silenciosa e reconciliação remota priorizadas para períodos ociosos da interface;
- pastas/grupos idênticos não provocam atualizações de estado ou gravações desnecessárias;
- Store de configurações reutilizado e gravações rápidas agrupadas;
- cabeçalho fixo e cabeçalhos sticky deixam de aplicar blur durante uso normal;
- isolamento de pintura dos cards para reduzir repaints durante scroll;
- perfil Performance limita thumbnails a 6 tarefas paralelas para evitar saturar CPU/decoder enquanto preserva velocidade;
- corrigida uma dupla redução do contador de downloads ao cancelar a janela de salvar.

A sincronização Telegram, CFAPI, unidade T:, Cofre e funções desktop permanecem habilitadas.
