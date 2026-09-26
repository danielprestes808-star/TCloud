# TCloud 12.9.6 — Queue & Explorer Efficiency

## Objetivo

Reduzir consumo de CPU/RAM em sessões longas e tornar as ações originadas no Windows Explorer/unidade T: mais imediatas, sem aumentar tráfego do Telegram.

## Alterações principais

- Sync workers passaram de polling frequente de SQLite para espera orientada a eventos com timeout de segurança.
- Criação/alteração de arquivo pelo Explorer acorda a fila imediatamente.
- Downloads sob demanda, previews, retomada de jobs e operações manuais também sinalizam a fila.
- A decisão de pausa global + limite de concorrência + claim do próximo job ocorre em um único caminho protegido pelo mutex do banco, reduzindo consultas duplicadas.
- Operações CFAPI pendentes (rename/move/delete/cleanup) acordam o worker CFAPI imediatamente em vez de esperar o próximo ciclo periódico.
- Eventos Modify repetidos do watcher são coalescidos por caminho dentro do mesmo burst, reduzindo fingerprints e consultas SQLite duplicadas.
- A thread de rechecagem do watcher deixa de acordar 10 vezes por segundo quando não existe trabalho; fica bloqueada até surgir um evento ou chegar o prazo real de uma rechecagem.
- Amostras de progresso são removidas da RAM em falhas/retries de rede, evitando retenção acumulada em sessões longas.
- Centro de Sincronização usa polling adaptativo: rápido enquanto há transferência, lento quando está parado e ainda mais lento quando a janela está oculta.
- Snapshots idênticos do Centro de Sincronização não provocam novo render React.
- Transferência de cache local para CFAPI passou de blocos de 512 KiB para 1 MiB, reduzindo overhead de chamadas no caminho já hidratado.
- Varredura completa de atributos/pinning do Sync Root ganha backoff adicional após vários ciclos sem mudanças.

## Compatibilidade

- Sem novas dependências npm ou crates.
- Mantidos Telegram, CFAPI, unidade T:, Cofre, previews, cache e sincronização existentes.
- `package.json`, `Cargo.toml`, `tauri.conf.json` e `Package.appxmanifest` atualizados para 12.9.6.
