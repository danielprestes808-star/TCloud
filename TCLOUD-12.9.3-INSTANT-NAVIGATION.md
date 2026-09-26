# TCloud 12.9.3 — Instant Navigation

Esta rodada reduz o tempo percebido ao trocar entre pastas sem aumentar indefinidamente o uso de RAM e sem criar tráfego especulativo no Telegram.

## Melhorias

- Cache RAM LRU dedicado às pastas mais usadas.
- Limite de 6 pastas quentes, 30 mil itens no total e 25 mil itens por pasta individual.
- Pastas acima do limite continuam usando o índice SQLite/React Query, sem manter uma cópia extra grande em RAM.
- Pastas já visitadas podem aparecer imediatamente por `initialData`, antes do refresh remoto.
- React Query libera entradas frias mais cedo; o LRU limitado preserva apenas as navegações mais úteis.
- Ao passar o mouse ou focar uma pasta na barra lateral, o TCloud aquece apenas o índice local.
- Cards de subpastas na visão de fórum também fazem prewarm local por intenção do usuário.
- Depois que uma pasta estabiliza, uma pasta adjacente pode ser aquecida pelo SQLite para acelerar a próxima navegação provável.
- Prewarm não consulta o Telegram; dados locais aquecidos permanecem marcados como antigos para que o refresh remoto ocorra em segundo plano quando necessário.
- Snapshots remotos autoritativos atualizam o LRU sem duplicar os objetos mantidos pelo React Query.
- A versão mantém o fast-resume da 12.9.2 e todas as otimizações anteriores.

## Preservado

Telegram, sincronização em segundo plano, unidade T:, CFAPI, Cofre, upload/download, lixeira, previews, miniaturas, favoritos, pesquisa e integração com o Explorer permanecem preservados.
