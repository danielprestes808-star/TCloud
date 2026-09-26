# TCloud 12.9.1 — Progressive Cache & Fluidity

Esta rodada aprofunda o desempenho do Explorer sem alterar a experiência visual ou as funções do TCloud.

## Melhorias

- O scanner Telegram agora separa o modo de uso do Explorer do reconciliador em segundo plano.
- No Explorer, os arquivos continuam chegando em chunks, mas o Rust deixa de montar e serializar uma segunda lista completa de até 50 mil itens pelo IPC.
- No reconciliador, a varredura deixa de emitir eventos de interface que ninguém consome.
- Chunks podem ser movidos diretamente entre buffers, reduzindo clones de `FileMetadata`.
- Listas remotas idênticas reutilizam a referência já presente no React Query, evitando re-render completo quando nada mudou.
- A árvore de pastas também deixa de provocar atualização visual quando o Telegram devolve o mesmo conteúdo.
- A grade virtualizada calcula apenas as linhas visíveis e não cria uma matriz completa de linhas para toda a pasta.
- Ordenação por nome reutiliza `Intl.Collator`, evitando reconstrução do comparador para cada comparação.
- Seleções usam `Set` para checagem O(1), melhorando seleções grandes.
- Durante rolagem rápida, animações/transições dos itens são temporariamente suspensas sem alterar estado React.
- `FileListItem` passa a usar `memo`.

## Preservado

Telegram, unidade T:, CFAPI, sincronização em segundo plano, cache, previews, Cofre, upload/download, lixeira, versões e integração com Explorer continuam preservados.
