# TCloud 12.9.7 — Sync Reliability & Explorer Safety

## Objetivo

Aumentar a segurança da sincronização bidirecional depois das otimizações de desempenho das versões 12.9.x, reduzindo risco de duplicação, upload parcial, exclusão acidental e jobs presos após crash/encerramento forçado.

## Alterações principais

- **Checkpoint remoto crash-safe para uploads:** assim que o Telegram confirma a nova mensagem, o job persiste `message_id`, fingerprint, SHA-256 e horário antes de qualquer limpeza/finalização local.
- **Recuperação sem reupload:** ao reiniciar após crash/update, um job com checkpoint verifica a mensagem remota e conclui a sincronização sem reenviar os bytes.
- **SHA-256 sem segunda leitura do arquivo:** o hash é calculado nos mesmos bytes lidos pelo stream de upload e salvo em `tcloud_files.sha256` quando o commit é concluído.
- **Finalização local autorrecuperável:** se Telegram já confirmou a operação mas SQLite/placeholder falhar na etapa final, o job volta para `pending` e retoma em segundos, preservando o checkpoint.
- **Lixeira não é ressuscitada por upload tardio:** se o arquivo for removido enquanto um upload já está finalizando, o novo `message_id` pode ser associado com segurança, mas os estados `trashed/deleted` são preservados.
- **Checkpoint é descartado só após commit local:** depois que SQLite e o placeholder terminam com sucesso, os campos temporários do checkpoint são limpos do job concluído.
- **Recuperação de fila na inicialização:** jobs de arquivos já removidos/lixeira são cancelados; uploads pendentes duplicados são consolidados; jobs com commit remoto recebem prioridade de recuperação.
- **Janela de estabilidade mais forte:** antes de abrir um arquivo criado/modificado pelo Explorer, o TCloud exige um período contínuo de tamanho/mtime estáveis, proporcional ao tamanho do arquivo.
- **Proteção de atomic-save no watcher:** eventos `delete + recreate` usados por Office, editores e compiladores esperam 1,6 s antes de virar exclusão; se o arquivo reaparece, a ação vira atualização de conteúdo.
- **Proteção de atomic-save no CFAPI:** um callback de delete aguarda brevemente a recriação do mesmo caminho materializado antes de mover a identidade para a Lixeira.
- **Proteção contra callback de delete atrasado:** após a janela de segurança, o CFAPI revalida o `relative_path`; se a mesma identidade já foi renomeada/movida, o callback antigo é ignorado.
- **Proteção contra caminho físico obsoleto:** o watcher não envia um arquivo à Lixeira se sua identidade CFAPI já estiver vinculada a outro caminho.
- **Backup pré-migração 12.9.7:** bancos existentes recebem um snapshot verificado antes das novas colunas de checkpoint serem aplicadas.

## Modelo de consistência do upload

1. Explorer produz/edita os bytes locais.
2. O watcher cria ou reutiliza um job durável.
3. O worker espera o arquivo ficar realmente estável.
4. O stream envia os bytes ao Telegram e calcula SHA-256 no mesmo passe.
5. Telegram cria a nova mensagem.
6. **O checkpoint remoto é gravado imediatamente.**
7. Versão anterior é preservada/limpa conforme a política.
8. Alterações locais ocorridas durante o upload geram um follow-up.
9. SQLite e placeholder CFAPI são finalizados.
10. O job é marcado como concluído.

Se o processo parar entre 6 e 10, a próxima execução retoma do checkpoint em vez de voltar ao passo 4.

## Compatibilidade

- Sem novas dependências npm.
- Usa `sha2`, crate que já fazia parte do backend.
- Preserva Telegram, unidade T:, CFAPI, Cofre, previews, cache, versões, Lixeira e todas as melhorias 12.9.0–12.9.6.
- Banco continua compatível por migrações incrementais `ALTER TABLE`.
