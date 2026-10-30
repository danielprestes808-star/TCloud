# TCloud Mobile 1.1.42+123 — Pessoas e Navegação Simplificada

- Agrupamento facial da biblioteca inteira e por álbum, com fotos e amostras de vídeos analisadas localmente.
- Correção manual de grupos, indicação de grupos novos para revisão e remoção local dos dados faciais.
- Progresso da análise pode ser pausado e retomado; itens corrigidos manualmente não são reclassificados automaticamente.
- Navegação principal e exibição de pastas simplificadas, mantendo o estilo inspirado no TeraBox.

# TCloud Mobile 1.1.41+122 — Galeria na Nuvem e Painel Atualizado

- Agrupamento facial pode incluir imagens que ainda estão na nuvem, após confirmação da quantidade e estimativa de dados a baixar.
- Reconhecimento permanece local; downloads temporários são limpos após cada análise, com falhas de limpeza informadas.
- Progresso da análise e cancelamento após o item em andamento.
- Painel da galeria com biblioteca catalogada e tamanho offline, sem presumir a cota total da conta.

# TCloud Mobile 1.1.38+119 — Gerenciador de Armazenamento e Limpeza de Cache

- **Central de Armazenamento e Cache (`StorageManagementScreen`)**:
  - Nova interface dedicada acessível em *Configurações > Armazenamento e cache*.
  - Gráfico visual segmentado de uso de disco: distinção entre arquivos offline e cache de miniaturas.
  - Ação com um toque para limpar cache de prévias e liberar espaço sem deslogar ou perder arquivos.
  - Ferramenta de otimização de banco de dados e temporários do TDLib.
  - Gerenciamento detalhado de arquivos offline, com remoção individual ou em lote das cópias locais.
- **Higiene e Desempenho do Repositório**:
  - Limpeza e liberação de mais de 1,4 GB em APKs compilados antigos e arquivos de diagnóstico soltos.
  - Documentação do `mobile/README.md` sincronizada para a versão ativa `1.1.38+119`.
  - Suíte de testes estendida com validação de renderização e fluxos da central de armazenamento.
- **Sincronização de Contratos**:
  - Atualização do contrato oficial em `contracts/tcloud-ecosystem-contract.json` com a nova feature `storage-cache-manager`.

---

# TCloud 13.0.61 — Ecosystem Alignment & Hygiene Consolidation

- **Sincronização de Versões do Ecossistema**: Alinhamento formal de versões dos projetos ativos: Desktop (`13.0.61`), Core API (`0.9.0`), Mobile (`1.1.37+118`) e Web (`0.3.9`).
- **Contratos Unificados**: Publicado o contrato [`contracts/tcloud-ecosystem-contract.json`](file:///d:/TCloud/contracts/tcloud-ecosystem-contract.json) com a matriz de compatibilidade e recursos ativos de cada cliente.
- **Higiene do Repositório**:
  - Remoção completa de mais de 200 arquivos residuais `.bak` dispersos no código-fonte.
  - Reorganização da raiz: documentos históricos, snapshots e logs de auditoria migrados para `docs/history/`, `docs/contracts/` e `docs/audits/`.
  - Remoção de arquivos legados duplicados (`Package.appxmanifest` desatualizado e `index.html` vazio).
- **Validação e Confiabilidade (CI/DevOps)**:
  - Correção no script `scripts/verify-all.ps1`: validação explícita de `$LASTEXITCODE` para evitar falsos positivos quando builds ou análises estáticas falham.
- **Mobile Onboarding**:
  - Adicionado `mobile/lib/telegram/telegram_credentials.example.dart` como modelo seguro para compilação local sem depender de credenciais versionadas.

---

# TCloud 13.0.3 — Explorer Minimal UI

- Início simplificado: um único alerta de saúde, menos atalhos duplicados e arquivos principais em ícones pequenos.
- Sincronização na sidebar não exibe contador redundante; problemas continuam concentrados em “Precisa de atenção”.
- Barra de status inferior reduzida e sem repetir a quantidade de problemas.
- Configurações Gerais deixaram de funcionar como dashboard: integração Windows compacta, detalhes recolhíveis e cartões menores.
- Removida da página Geral a seção educativa “Proteção e recuperação”; os recursos continuam nas áreas próprias.
- Paleta e superfícies mantêm a linha leve da 13.0.2, com alertas por faixa lateral em vez de grandes bordas coloridas.

# TCloud 13.0.2 — Lightweight UI & Polish

Esta versão reduz o peso visual e o custo de renderização da interface sem remover recursos.

## Visual
- Shell plano, sem gradiente decorativo e sem efeito de janela dentro da janela.
- Sidebar um pouco mais estreita e com seleção mais discreta.
- Menos sombras, brilhos, blur e transparências.
- Header e barra de comandos mais compactos.
- Cards de arquivos sem elevação/zoom ao passar o mouse.
- Galeria sem zoom de imagem no hover.
- Coleções, Cofre, Sincronização e Configurações usam superfícies mais simples.
- Paleta escura passa de azul-noite forte para graphite neutro com periwinkle menos saturado.
- Movimento reduzido é respeitado globalmente.

## Uso de recursos
- Status geral de sincronização: 4 s durante atividade e 20 s em repouso.
- Banda: atualização a cada 15 s em vez de 5 s.
- Árvore de fóruns: 120 s em vez de 60 s.
- Estado CFAPI da sidebar: 30 s em vez de 15 s.
- Tela de Sincronização: 3 s quando ativa e 15 s em repouso.
- Home de Configurações pausa polling quando a janela está oculta e usa 15 s quando visível.
- API/VPN/Proxy avançados usam cadência mais baixa.
- Verificação de rede usa 30 s e continua pausada quando a janela está oculta.

## Compatibilidade
Nenhuma dependência nova. O backend, CFAPI, unidade T:, Telegram, Cofre, Lixeira e mecanismos de recuperação da 13.0.1 permanecem preservados.

---

# TCloud 13.0.1 — Navigation Unification & Visual Refresh

## Navegação unificada

- Lixeira, Sincronização, Galeria, Música, Neste computador e Cofre abrem dentro da área central do Explorer do TCloud.
- A antiga Central/Ferramentas avançadas deixa de ser o destino dos itens comuns da barra lateral; ela permanece apenas para diagnóstico e opções técnicas.
- Voltar/Avançar e breadcrumbs registram também as novas coleções.
- Busca funciona dentro das coleções; filtros de arquivos permanecem em coleções de arquivos e são escondidos em Lixeira/Sincronização.
- Ações Upload/Nova pasta não aparecem em coleções onde não fazem sentido.

## Cofre

- Cofre ganhou entrada própria no canvas principal.
- Criar, desbloquear, bloquear e desbloquear com Windows Hello (quando configurado) podem ser feitos sem abrir a tela técnica.
- Arquivos protegidos já conhecidos pelo cache de navegação aparecem na coleção Cofre e usam o fluxo normal de preview/download.
- O backend TDENC2/vault permanece habilitado; compartilhamento criptográfico remoto e migração legada continuam separados/fail-closed.

## Visual 13.0.1

- Nova identidade Midnight Graphite + Periwinkle.
- Fundo e superfícies mais neutros, menos azulados; acento violeta-azulado consistente.
- Sidebar reduzida para 232 px, seleção mais discreta, badges menores e status menos chamativo.
- Topbar, breadcrumb, pesquisa, command bar, cards, Lixeira, Sincronização e Cofre usam a mesma linguagem visual.
- Paleta clara correspondente também foi atualizada.

## Limpeza

- Removido botão duplicado de limpar seleção na barra de comandos.
- Rótulo antigo “TCLOUD 12.7” da área avançada atualizado para 13.0.1.

# 13.0.0 — Explorer Experience Preview

- Entrega integrada das fases de experiência da versão 13 para avaliação.
- Tela principal e barra de comandos mais próximas do Explorador do Windows.
- Navegação Voltar/Avançar com Alt+Esquerda e Alt+Direita.
- Barra lateral simplificada: Início, Recentes, Favoritos, Sincronização, Computador, Meu TCloud, Cofre e Lixeira.
- Linguagem de disponibilidade simplificada: Na nuvem, Disponível neste computador, Sempre disponível, Sincronizando e Precisa de atenção.
- Central de transferências reduzida a resumo + detalhes sob demanda.
- Configurações reorganizadas em Geral, Sincronização, Armazenamento, Telegram, Segurança, Aparência e Compartilhamento, mantendo opções técnicas em Avançado.
- Novo assistente inicial de quatro passos: Conta, Pastas, Windows e Pronto.
- O Assistente inicial pode ser reaberto em Configurações para testar o fluxo em instalações já configuradas.
- Pastas/armazenamento deixam termos internos do Telegram em segundo plano.
- Tela vazia, menus de contexto e ações de arquivo com textos mais diretos.
- Motor da série 12.9.x, CFAPI, unidade T:, Telegram, Cofre, cache e recuperação de sincronização preservados.

# TCloud 12.9.7 — Sync Reliability & Explorer Safety

- Checkpoint remoto de upload com `message_id`, fingerprint e SHA-256 para recuperação sem reupload após crash.
- SHA-256 calculado durante o próprio stream de upload, sem segunda leitura do arquivo.
- Finalização local falha volta para a fila preservando o commit remoto.
- Startup consolida jobs duplicados, cancela uploads órfãos de itens removidos e prioriza commits remotos incompletos.
- Janela de estabilidade pré-upload reforçada para impedir cópias parciais.
- Delete + recreate do Explorer/Office passa por grace period e é tratado como atualização quando o arquivo reaparece.
- CFAPI revalida identidade/caminho para descartar callback de delete obsoleto após rename/move.
- Backup SQLite pré-migração atualizado para a revisão 12.9.7.

# TCloud 12.9.5 — Startup & Search Efficiency

- A conexão do provider CFAPI em abertura normal foi retirada da thread crítica de setup e passa a iniciar 350 ms depois em worker de fundo; `--background` continua imediato.
- Busca global agora mostra o índice SQLite primeiro e só consulta o Telegram após pausa de digitação + janela ociosa.
- Respostas antigas de busca são descartadas para evitar resultados pulando de consultas anteriores.
- Busca de conteúdo SQLite roda via `spawn_blocking`.
- Pesquisa não bloqueia toda a grade quando já existem resultados locais.
- Reconciliação periódica invalida somente a pasta ativa, não toda a família de queries de arquivos.
- Polling de CFAPI e sincronização passa a ser adaptativo ao estado.
- Verificação automática de atualização é deslocada para 30 s e `requestIdleCallback`.
- Mantidas todas as melhorias de fluidez, fast-resume, cache LRU e eficiência em segundo plano das versões 12.9.0–12.9.4.

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

# TCloud 12.9.2 — Fast Resume & Preview Cache

- Reabertura/restauração da janela centralizada em um caminho nativo rápido.
- `window-state` deixa de persistir `VISIBLE`, evitando que o estado "oculto na bandeja" concorra com a próxima abertura normal.
- Frontend recebe um curto período de retomada leve ao voltar de minimizado/oculto.
- Blur, animações e trabalho visual não essencial são temporariamente suspensos durante o primeiro frame de retorno.
- Miniaturas visíveis aguardam a retomada; prefetch especulativo fica limitado a uma tarefa por vez.
- Atualização remota da árvore de pastas é reagendada para depois da retomada da interface.
- Cards de imagem deixam de baixar o preview completo como fallback de thumbnail.
- Varreduras de limpeza dos caches de preview/thumbnail passam a ser limitadas a uma por cache a cada 30 segundos.
- Sincronização em segundo plano, CFAPI, unidade T:, Telegram, Cofre e previews explícitos permanecem preservados.

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

# 12.9.0 Performance & Fluidity

- Progresso de upload/download passa a ser agrupado antes de atualizar o React, reduzindo renders repetidos.
- Persistência das filas deixa de gravar o Store a cada atualização de bytes.
- Sincronização silenciosa deixa de alternar o estado visual de sincronização e passa a preferir períodos ociosos da interface.
- Pastas/grupos idênticos não são regravados nem reenviados ao React sem necessidade.
- Rolagem do Explorer e ResizeObserver passam a trabalhar por frame para reduzir layout thrashing.
- Miniaturas usam carregamento lazy e transição somente de opacidade.
- Configurações reutilizam a mesma Store e agrupam gravações rápidas.
- Cabeçalhos fixos removem blur durante o uso normal para reduzir repaint no WebView2.
- Corrigido contador de downloads que podia ser decrementado duas vezes ao cancelar a janela de destino.
- Mantidos Telegram, CFAPI, unidade T:, sincronização em segundo plano, Cofre, cache e recursos desktop.

# 12.8.3 Desktop Clean

- Concluída a remoção do cliente mobile da árvore de produção.
- Removidos JNI/ndk-context, content://, MediaStore, share sheet e cache de arquivos Android.
- Upload, download, abertura externa, streaming, API, bandeja e ciclo de vida agora seguem apenas o caminho desktop.
- RAR e single-instance deixam de depender de condicionais Android/iOS.
- Removido o linker Android `c++_shared` e a chave de tradução mobile não utilizada.
- Telegram, Explorer/CFAPI, unidade T:, sincronização em segundo plano, cache, streaming local e compartilhamento permanecem preservados.

# 12.8.2 Desktop Complete RC

- Fecha a janela mantendo sincronização na bandeja por padrão, com opção de encerrar completamente.
- Aviso nativo apenas na primeira vez em que o TCloud continua em segundo plano.
- Uploads ativos são persistidos como pendentes para recuperação após reinício/crash.
- Contador de problemas agora conta itens realmente acionáveis e remove histórico resolvido.
- Perfis Economia/Equilibrado/Desempenho expostos nas configurações simples.
- Verificação de saúde, correção automática e exportação segura de diagnóstico.
- Economia automática de espaço preserva itens fixados offline.
- Bandeja mostra pausa e percentual agregado de transferências.
- Mantidas as otimizações 12.8.1 de carregamento local imediato e barra estilo Explorer.

# 12.8.1 RC — Explorer consolidado

- Consolida Explorer Logic Etapas 1–5 em uma única base.
- Incorpora o hotfix Rust E0505 validado no Windows.
- Incorpora a criação rápida de pastas validada no Windows.
- Versões de package.json, Tauri, Cargo e Package.appxmanifest alinhadas em 12.8.1.
- Remove notas de hotfix/etapas obsoletas do pacote operacional; o histórico permanece neste changelog.
- Mantém a migração de banco da 12.8.0 sem forçar backup/migração desnecessária.

## 12.8.0 — Explorer Logic Etapa 5
- Árvore lateral simplificada em Meus arquivos.
- Renomear/excluir pastas principais pelo app.
- Nomes duplicados no padrão do Windows.
- Remoção dos últimos termos de fórum/tópico da navegação normal.
- Menus e disponibilidade alinhados ao Explorer.

# 12.8.0 — Explorer Logic Stage 4

- sincronização estrutural Explorer ↔ TCloud com fila persistente;
- criação/renomeação/exclusão de pastas gerenciadas pelo Explorer;
- atualização do Windows Shell sem F5;
- eventos frontend em tempo real;
- proteção contra exclusão implícita de pasta principal não vazia;
- rebind de caminhos CFAPI em renomeações de namespace.

# 12.8.0 — Explorer Logic Etapa 3

- breadcrumb completo TCloud > pasta principal > subpasta;
- árvore de pastas compartilhada entre Dashboard, Sidebar e gerenciador;
- nomes duplicados no padrão Windows (2), (3)...;
- mover/copiar subpastas com destino simplificado;
- Propriedades sem IDs/termos técnicos do Telegram;
- seleção/expansão da árvore sincronizada com o local aberto.

# TCloud 12.8.0 — Explorer Logic Stage 2

- Nova pasta e renomear com editor rápido, sem prompt na área principal.
- Novo menu no estilo Explorer.
- Ctrl+C/X/V para arquivos do TCloud.
- Delete reconhece arquivos e pastas.
- Duplo clique abre itens.
- Menu de contexto no espaço vazio.

## 12.8.0 — Explorer Logic Stage 1

- Descoberta de todos os fóruns Telegram acessíveis como pastas principais.
- Sincronização de tópicos como subpastas e reconciliação com Explorer/CFAPI.
- Remoção visual do conceito de fórum/pasta padrão.
- Nova tela simplificada de Pastas principais.
- Nova pasta na raiz passa a criar uma pasta principal, no estilo Explorer.
- Assistente inicial e barra lateral passam a usar apenas linguagem de pastas.

# 12.8.0 — Frontend sob demanda / abertura mais leve

- PDF, arquivos compactados, previews, mídia e centros avançados passam a carregar somente quando abertos.
- Player adaptativo (`hls.js` + `mp4box`) só entra na memória ao reproduzir MP4.
- `pdfjs-dist` fica isolado do bundle principal e só é solicitado ao abrir PDF/preview PDF.
- Configurações, compartilhamento, renomeação, setup e painéis secundários deixam de inflar o DesktopDashboard inicial.
- AuthWizard passa a ser carregado sob demanda quando não há sessão reutilizável.
- Diálogos nativos de arquivo são importados somente no momento em que o usuário abre um seletor.
- CSS exclusivo das Configurações sai do carregamento global.
- Vite separa engines PDF/HLS/MP4 e vendors principais em chunks estáveis para cache entre atualizações.
- Grafo estático do DesktopDashboard cai de ~1,06 MB de fonte alcançável para ~470 KB antes da minificação.

# 12.8.0 — Hotfix de Responsividade / “Não está respondendo”

- Move consultas bloqueantes do Explorer/WinRT/registro para workers de fundo.
- Evita manter o lock do SQLite durante consultas ao Shell do Windows.
- Move leitura e atualização pesada do índice local para workers bloqueantes.
- Adiciona resumo leve da fila para polling do dashboard.
- Compartilha o polling CFAPI entre dashboard e sidebar.
- Evita sobreposição de polls nos centros de operações/sincronização.
- Reduz repinturas durante refresh remoto quando o cache local já está disponível.
- Adiciona índices SQLite para consultas de status/atividade.

# TCloud 12.8.0 — Stability Release (Fase 3)

- Snapshot SQLite verificado antes de migrações em banco existente.
- Identidade persistente de instalação replicada em `TCloudRecovery` para testar continuidade após reinstalação.
- Watcher do Explorer desacoplado do callback do Windows para aguentar rajadas de milhares de arquivos.
- Rechecagens suprimidas usam um único worker coalescente em vez de uma thread por arquivo.
- Novo `npm run stability:stress`.
- Novo `npm run stability:upgrade` para capturar/verificar atualização e reinstalação.
- Novo `npm run release:windows` para gerar NSIS, SHA-256 e metadados de release.
- Verificação do bundle confirma `tcloud.exe` e VC++ Runtime.
- Interface explica proteção de upgrade, backup pré-migração e recuperação.

---

# TCloud 12.8.0 — Stability Release (Fase 2)

- Workers entram em backoff progressivo quando a fila fica vazia, reduzindo CPU/SQLite em longas sessões.
- Upload com limite de banda usa espera assíncrona real e deixa de fazer busy-loop.
- Heartbeat detecta retorno de suspensão/hibernação e libera cooldown antigo de rede.
- Manutenção ociosa remove histórico concluído com mais de 7 dias e metadados órfãos.
- Checkpoint profundo do WAL a cada 6 horas ociosas.
- Aplicação do limite de cache evita carregar a tabela inteira duas vezes.
- Bandeja reduz polling para 10 s quando tudo está estável.
- Novo `npm run stability:soak -- -Minutes 60` para monitorar RAM, CPU, handles, threads, WAL e instância única.
- Configurações mostram Uso prolongado e Suspensão/hibernação.

---

# TCloud 12.8.0 — Stability Release (Fase 1)

- Recuperação de sessão após crash/encerramento forçado com validação SQLite/WAL.
- Manutenção automática do banco enquanto o cliente está ocioso.
- Quedas temporárias de rede deixam de consumir tentativas de sincronização.
- Backoff global evita que workers concorrentes sobrecarreguem Telegram/rede durante desconexões.
- Bandeja informa quando o TCloud está aguardando a internet.
- Shell de desenvolvimento deixa de encerrar processos TCloud fora do staging atual.
- Novo `npm run stability:check`.
- Configurações mostram Proteção e recuperação em linguagem simples.

---

## 12.7.2 — Explorer Integration Final

- Remove menus compatíveis duplicados quando o Windows oferece pinning/offline nativo.
- Mantém fallback de disponibilidade apenas quando necessário.
- Adiciona verificação e reparo de integridade do namespace CFAPI.
- Adiciona autorreparo periódico de placeholders após crash/reinício/logout.
- Exibe contagens simples de disponibilidade na interface.
- Adiciona ação `Verificar arquivos`.
- Prepara a transição para a 12.8 Stability Release.

# TCloud 12.7.1 — Performance / Large Files — Parte 2

- Progresso com velocidade suavizada e ETA mais estável.
- Menos gravações SQLite no hot path de progresso.
- Cancelamento mais rápido e seguro antes da criação da mensagem remota.
- Download `.part` preservado em falhas/reinício e removido em cancelamento explícito.
- Miniaturas cedem prioridade às transferências do Explorer; prefetch é suspenso em background.
- Thumbnail reduzida para 512 px e downloads ocultos de originais muito grandes foram evitados.
- Central de sincronização mostra `Cancelando…`, progresso pausado e corrige contagem duplicada de pausados.

# TCloud 12.7.1 — Performance / Startup Priority

- Explorer abre pastas indexadas pelo cache SQLite antes da consulta completa ao Telegram.
- Atualização remota passa a ocorrer em segundo plano após a primeira pintura.
- Inventário remoto registra último scan e evita repetir a mesma pasta por 60 s.
- Worker global espera 45 s no startup e cede por 10 s após ações recentes do usuário.
- Prioridades: download 150, upload do Explorer 130, delete 125 e refresh 115.
- Novos índices SQLite para fila, listagem por chat e identidade remota; WAL/NORMAL para reduzir contenção.
- Corrigidas declarações duplicadas antigas na Lixeira e no índice local.

# TCloud 12.7.0 — Interface integrada ao Explorer

- Tela inicial passa a resumir conexão, sincronização, recuperação automática e problemas.
- Badges simples para Problemas e Atividade na barra lateral.
- Botão e barra de status mostram Sincronizando N / N problemas / Tudo atualizado.
- Central de sincronização redesenhada sem tabela técnica ou prioridade exposta.
- Tela Problemas ganhou **Tentar novamente** e acesso direto à sincronização.
- Configurações explicam reconexão automática, retomada após reinício e arquivos sob demanda.

# TCloud 12.6.2 — Explorer Recovery & Crash Safety — Parte 7

- Recuperação automática de jobs interrompidos por desligamento/update/crash.
- Checkpoint remoto de movimentos entre destinos para reduzir duplicação após reinício.
- Operações CFAPI esgotadas aparecem em Problemas e podem ser reenfileiradas por **Tentar novamente**.
- `failed_operations` separado das operações apenas pendentes no diagnóstico.

# TCloud 11.7.0 — CFAPI Phase 3

- Estados `PINNED`/`UNPINNED` para disponibilidade OneDrive-like.
- Placeholders novos começam online-only.
- Controles globais **Manter neste dispositivo** e **Liberar espaço**.
- WebDAV T: e upload Telegram/fórum preservados.

# TCloud 11.6.0

- CFAPI Fase 2: provider conectado com `CfConnectSyncRoot` e callback `FETCH_DATA`.
- Hidratação de placeholders sob demanda usando Telegram + cache local e `CfExecute/TRANSFER_DATA`.
- Reconexão automática do provider no startup e desligamento seguro antes do cliente Telegram.
- Progresso de hidratação com `CfReportProviderProgress` e transferência em blocos alinhados.
- Central avançada mostra provider conectado, hidratação pronta e placeholders.
- WebDAV T: continua como fallback e o fluxo atual de upload Telegram/fórum permanece inalterado.
- Downloads CFAPI de arquivos criptografados são bloqueados nesta fase para nunca entregar ciphertext ao Explorer.

# TCloud 11.0.0

- Explorer Redesign Base: nova sidebar, TCloud Drive (T:), barra de comandos, status bar e controle de tamanho dos itens.

## 10.0.1
- Corrigido import incompatível de `grammers_tl_types` em `share_routes.rs`.

## [2.2.7] - 2026-08-04

### Windows Build Experience

- Fixed `npm run tauri dev` failing when `resources/vc_redist.x64.exe` had not been downloaded.
- Separated development-safe Windows settings from release-only NSIS resources and installer hooks.
- Made the standard `npm run tauri build` command automatically prepare and validate Microsoft's signed Visual C++ Redistributable on Windows.
- Kept the Visual C++ runtime and embedded WebView2 bootstrapper in the finished Windows installer for reliable startup on clean Windows systems.

### Verification

- Production frontend build passed.
- Desktop Rust tests passed.
- The tagged release workflow prepares and verifies the Windows runtime on a native Windows runner before publishing the NSIS installer.

## [2.2.6] - 2026-08-04

### Windows Startup Reliability

- Added the signed Microsoft Visual C++ 2015–2022 x64 Redistributable to the Windows NSIS installer so fresh Windows installations have `MSVCP140.dll` and the matching runtime libraries before Telegram Drive starts.
- Added silent prerequisite detection and installation with explicit failure handling and reboot-required support.
- Embedded the WebView2 bootstrapper for Windows systems where the Evergreen WebView2 Runtime is unavailable.
- Added release-time checks for the redistributable's Microsoft Authenticode signature, its presence inside the finished NSIS installer, and the built executable's PE dependencies.
- Restricted Windows release artifacts to the verified NSIS installer so an installer without the prerequisite cannot be published accidentally.

### Desktop Advertising

- Fixed blank desktop advertisement panels by relaying and validating the configured advertisement loader through the local desktop server.
- Added resilient DNS recovery, short-term loader caching, and a clear sponsored-content fallback when the service is blocked or unavailable.

### Windows Folder Sidebar

- Fixed folder option and right-click menus opening without allowing actions to be selected in Windows WebView2.
- Isolated sidebar menu pointer events from drag-and-drop handling and rendered the menu outside the draggable folder element.

### Verification

- Production frontend build passed.
- Desktop Rust tests passed.
- The tagged release workflow verifies and compiles the Windows installer on a native Windows runner before publishing.

## [Unreleased] - 2026-08-02

### Vietnamese Language Support

- Added complete Vietnamese (`vi`) translations across the application, settings, dialogs, notifications, authentication, encryption, sharing, and WebDAV surfaces.
- Added Vietnamese to desktop and mobile language selectors with automatic `vi` and `vi-VN` system-language detection and locale-aware number and date formatting.
- Added Vietnamese localization for password-protected public share pages.

### File Size Metadata

- Fixed Telegram photos being reported as `0 KB` in folder listings, REST responses, storage statistics, streaming headers, and Windows clients by consistently using the largest available photo representation size.
- Aligned file-size metadata across the application, downloads, previews, public sharing, and WebDAV while safely handling invalid negative values from remote metadata.

### Security

- Hardened ad-link fallback windows with `noopener,noreferrer` to prevent opened ad pages from accessing the application window through `window.opener`.

## [2.2.5] - 2026-08-03

### Highlights

- Added complete Vietnamese language support across desktop, mobile, settings, sharing, authentication, encryption, and WebDAV.
- Added automatic Vietnamese system-locale detection and localized number/date formatting.
- Added Vietnamese localization for password-protected share pages.

### Fixes

- Fixed photos being displayed as `0 KB` in folder listings, REST responses, storage statistics, streaming headers, downloads, previews, and WebDAV clients.
- Standardized file-size metadata using Telegram’s largest available photo representation.
- Improved handling of invalid remote size metadata.

### Security

- Added `noopener,noreferrer` to ad-link `window.open` fallbacks to prevent tabnabbing through `window.opener`.

### Verification

- Production frontend build passed.
- Full Rust test suite passed.
- Localization validation passed.

## [2.2.0] - 2026-07-29

### WebDAV Access

- Added a local WebDAV server for browsing Telegram Drive in macOS Finder, Windows File Explorer, and Linux WebDAV clients.
- Added a WebDAV tab in Settings with server status, custom port selection, read-only-by-default access, and optional write access.
- Added private connection links with secure token regeneration.
- Supports Saved Messages and Telegram Drive folders, directory browsing, range downloads, uploads, rename, move, copy, and delete for unencrypted files when write access is enabled.
- Added port conflict checks, staged upload cleanup, client-lock support, and macOS Finder compatibility fixes.
- Added WebDAV setup and troubleshooting documentation.

### Desktop File Handling & Interface

- Added external file drag-and-drop uploads from the desktop or file manager into the currently open folder.
- Moved WebDAV in Settings to sit between VPN and Encryption.
- Improved spacing and hit areas in folder action menus and the Grid/List layout picker.

### Italian Language Support & Build Fixes

- Added complete Italian (`it`) language support across all application surfaces, settings, dialogs, and error messages.
- Resolved build warnings and runtime layout/formatting issues.

### Verification

- Verified production frontend build, localization validation, Rust compilation, and WebDAV regression tests.

## [2.0.0] - 2026-07-26

### Major Release — Internationalization & Quiet Utility Redesign

- **Internationalization & Localization Engine**
  - Added multi-language architecture with 13 supported locales (English, Spanish, Russian, Simplified Chinese, French, Arabic, Brazilian Portuguese, German, Hindi, Indonesian, Turkish, Japanese, Korean).
  - Added system language preference resolution, locale formatters for dates/sizes/rates, Bidi text isolation primitives, and Rust share route localization.
  - Implemented automated localization validation scripts and CI workflow.

### Quiet Utility Redesign

- **New application-wide visual language**
  - Implemented the Path B “Quiet Utility” redesign across the desktop shell, mobile shell, authentication, sponsored surfaces, file workspace, settings, transfers, dialogs, menus, viewers, and empty/loading states.
  - Added semantic canvas, surface, border, text, accent, status, radius, elevation, typography, density, and motion tokens instead of relying on scattered component-specific styling.
  - Rebalanced the interface toward neutral graphite and warm-light surfaces, restrained depth, thinner borders, reduced decorative blur, and content-first hierarchy.
  - Reduced desktop typography and control dimensions to a denser modern scale inspired by current productivity applications while retaining 44–48px touch targets on mobile.
  - Replaced excessive card-lift and button-scale animations with faster, quieter state transitions and reduced-motion-safe behavior.
- **Shared UI primitives and development gallery**
  - Added reusable buttons, icon buttons, fields, selects, switches, segmented controls, surfaces, menus, badges, status dots, progress indicators, dividers, and skeleton states.
  - Added centralized Quiet Utility layout contracts for toolbar, navigation, row, sidebar, card, dialog, typography, and motion sizing.
  - Added a development-only design gallery for light, dark, custom theme, compact-density, LTR, RTL, loading, authentication, mobile-sheet, and viewer-state review.
- **Desktop shell and toolbar refinements**
  - Reduced and aligned the sidebar identity bar and primary file toolbar to the same 48px height so their bottom borders remain pixel-aligned.
  - Corrected the collapsed-sidebar layout so the logo and expand/collapse control remain within the application bounds.
  - Consolidated search, sort, grid/list selection, thumbnail sizing, folder creation, settings, and upload actions into a more compact toolbar hierarchy.
  - Added a unified transfer center for upload and download activity while preserving cancellation, retry, clear-finished, progress, speed, and concurrency behavior.
- **Settings, authentication, and viewer polish**
  - Rebuilt Settings as a larger, calmer category-based surface with denser controls, clearer section hierarchy, and updated tab/category switching motion.
  - Restyled API setup, phone login, QR login, Telegram code, 2FA, help, donation, and session-restoration surfaces without changing their underlying authentication commands.
  - Modernized image, PDF, audio/video, adaptive-streaming, and archive viewer chrome, navigation controls, loading states, and error presentation while retaining existing shortcuts and media behavior.
- **Mobile experience**
  - Updated the mobile toolbar, file rows, folder drawer, bottom navigation, action sheets, selection controls, transfer views, settings, and safe-area handling.
  - Replaced overly card-heavy mobile presentation with quieter edge-to-edge rows and compact semantic surfaces.

### Theme System

- Preserved Default, System, Light, Dark, built-in preset, and user-created custom themes through the redesign.
- Added semantic theme adapters so legacy presets and stored custom themes continue to drive the new design tokens.
- Fixed theme switching so users can return to the redesigned Default theme after selecting Light, Dark, System, a preset, or a custom theme.
- Improved custom-theme contrast mappings, theme persistence, scrollbar styling, and light/dark surface behavior.

### File Workspace, Layout & Preview Performance

- **Responsive file cards**
  - Reworked file-card containment so long filenames, thumbnails, metadata, encryption state, media badges, cached-quality badges, selection controls, and hover actions cannot overlap adjacent cards.
  - Added protected minimum card dimensions and virtualizer remeasurement while retaining responsive 4:3 sizing.
  - Restored thumbnail scaling below 100%; users can now resize file cards from 50% through 200% in 25% increments.
  - Replaced oversized file icons and controls with compact, consistently aligned variants.
- **Faster image previews and thumbnails**
  - Added bounded in-memory preview and thumbnail caches with one-hour expiry, least-recently-used refresh behavior, and duplicate in-flight request coalescing.
  - Added backend preview/thumbnail caching, partial-download guards, file integrity checks, progress events, cache quotas, stale-part cleanup, and cache pruning.
  - Added lazy image loading, asynchronous decoding, skeleton placeholders, fade-in presentation, cache invalidation on decode failure, and shared preview reuse between cards and the full viewer.
  - Avoided clearing usable preview caches during ordinary component unmounts, making repeat opens substantially faster.
- **Folder and query stability**
  - Fixed duplicated files on the initially restored folder by deduplicating streamed folder chunks by message ID.
  - Delayed file queries until the persisted startup folder is restored, preventing overlapping Saved Messages and startup-folder requests.
  - Coalesced concurrent initial folder sync requests and cleared image memory caches on account/logout transitions.
- Added skeleton layouts for grid/list loading states and refined empty, error, search, selection, and drag-target presentation.

### Buttons, Actions & Sponsored Content

- Added restrained color-matched gradients to Sync and Log Out actions.
- Updated Upload buttons to use the same compact gradient treatment and Quiet Utility sizing.
- Preserved the desktop 300×250 advertisement’s original in-flow placement so it no longer cuts into or displaces the top application banner/toolbar.
- Retained desktop ad launch timing, recurrence, countdown, hover pause, auto-dismiss, sandboxing, and external click-through behavior.
- Restyled the post-authentication sponsor gateway and Android sponsored banner with clearer disclosure, calmer presentation, safe-area handling, and preserved dismissal/click-through behavior.

### Encryption — Opt-In Alpha

- **TDENC2 encrypted Telegram transfers**
  - Added optional Standard, Vault, File Passphrase, and Vault + File Passphrase upload modes; Standard remains the default.
  - Added bounded-memory streaming encryption before Telegram upload and authenticated streaming decryption before final file publication.
  - Added authenticated protected metadata for original filenames and MIME types.
  - Applied explicit protection intent to manual uploads, drag-and-drop, folder ZIPs, retries, Android cached shares, and remote URL uploads.
- **Vault and recovery**
  - Added a persistent passphrase-protected vault, create/unlock/lock controls, inactivity auto-lock, background/sleep lock, logout lock, and exit lock.
  - Added authenticated recovery-bundle export/import and safe vault replacement only after bundle verification and explicit confirmation.
  - Added vault passphrase changes without rotating the vault key or re-encrypting every stored file.
  - Added short-lived opaque single-use prompt tokens so raw per-file passphrases are not persisted in transfer queues.
  - Added queue pause/retry states for credentials, decryption, and verification rather than silently falling back to plaintext.
- **Format and security hardening**
  - Replaced the quarantined TDENC1 prototype with the versioned TDENC2 envelope using XChaCha20-Poly1305, full 24-byte wrap nonces, keyed header authentication, authenticated metadata/chunks/final records, and exact ciphertext-length accounting.
  - Added strict parser bounds for versions, algorithms, KDF parameters, chunk sizes, slot counts, lengths, truncation, trailing data, integer overflow, and Telegram’s post-encryption size limit.
  - Added owner-only permissions for decrypted partial files and Unix remote-upload temporary files.
  - Added transactional encryption registry migrations, RAII bandwidth reservations, registry reconstruction from strict remote TDENC2 header probes, and reconciliation handling for move/copy/delete operations.
  - Added fail-closed detection for unindexed `.tdenc` and `TDENC2` objects so ciphertext is never treated as a normal preview, thumbnail, stream, REST download, or shared plaintext file.
- **Encryption UI and mobile parity**
  - Added encryption capability diagnostics, accurate loading/ready/blocked/error states, build identifiers, vault status, protected-file badges, and locked-name placeholders.
  - Added the encryption settings surface to both desktop and mobile.
  - Added a localized key-loss disclaimer explaining that forgotten passphrases, keys, and recovery material cannot be recovered by Telegram Drive, with acknowledgement required before vault creation.
  - Preserved verified Android download publication through MediaStore.
- **Intentionally unavailable for encrypted files**
  - Encrypted in-app previews, thumbnails, PDF/archive/media playback, transcoding, HLS/range streaming, plaintext share links, REST plaintext access, remote rename, migration, and rekey workflows remain explicitly disabled until credential-scoped logical-media implementations are complete.
  - Existing plaintext behavior for these features is unchanged.

### Language & Internationalisation Infrastructure

- Added System language preference and locale alias resolution while preserving all 13 supported language choices.
- Added centralized locale metadata for text direction, number/date locale, aliases, and future font selection.
- Added locale-aware number, byte-size, transfer-rate, date/time, relative-time, duration, percent, and list formatting helpers.
- Added bidi-safe primitives for user filenames and technical strings such as URLs, paths, hashes, API keys, IP addresses, and ports.
- Added generated typed translation keys, locale structure/type/interpolation validation, copied-English reporting, Arabic plural awareness, UI-literal scanning, pseudo-locales, and an i18n CI workflow.
- Added translated encryption settings, recovery actions, status copy, passphrase warnings, and key-loss disclaimer across English, Spanish, Russian, Simplified Chinese, French, Arabic, Brazilian Portuguese, German, Hindi, Indonesian, Turkish, Japanese, and Korean.
- Added a comprehensive language-support implementation plan covering remaining English extraction, native review, RTL, text expansion, accessibility, and release gates.

### Reliability, Compatibility & Documentation

- Added encryption-aware REST, local streaming, sharing, preview, and file-operation safety checks while preserving existing plaintext API behavior.
- Added stable encryption registry records, protected metadata state, plaintext/ciphertext sizes, header hashes, protection modes, and reconciliation state to SQLite.
- Preserved advertisements, Telegram authentication, folder/group navigation, sync, uploads/downloads, sharing, proxy/VPN settings, REST API, archives, viewers, updates, light/dark themes, and custom theme editing throughout the redesign.
- Added the Quiet Utility design handoff and implementation plan, comprehensive language plan, encryption architecture/remediation plans, TDENC2 format ADR, and encryption execution report.
- Verified the current work with a production frontend build, Rust compilation, 23 passing Rust library tests, localization validation/scanning, and whitespace validation.

### Known Pre-Release Work

- Complete the broader language plan’s remaining copied-English cleanup, hardcoded UI-string extraction, native linguistic review, RTL/long-string/CJK review, and accessibility testing.
- Complete credential-scoped logical plaintext sources before enabling encrypted previews, media, archives, sharing, REST access, migration, or rekey workflows.
- Run isolated Telegram integration tests, cross-platform recovery drills, fuzz/property testing, dependency/license review, and an independent cryptographic audit before describing encryption as generally available.

---

## [1.9.9] - 2026-07-13

### Bug Fixes & UI Enhancements

- **Video Playback Stability**
  - Resolved MediaSource buffer corruption error ("Invalid data found while parsing box") triggered during directory sorting.
  - Implemented state isolation using unique file IDs as React keys on the media player, preventing React from recycling HTMLMediaElement nodes and mixing video data streams.
  - Added explicit MediaSource stream termination and video element source reset during player unmount.
- **File Card UI Layout Fixes**
  - Corrected overlap issues between file names, metadata badges, and file type icons.
  - Restricted the icon container boundaries to avoid collision with the bottom info overlay on smaller or resized cards.
  - Added flex wrap and shrink constraints to file card metadata rows to handle narrow viewport dimensions.
- **Shift-Click Range Selection**
  - Fixed range selection index calculations to align with the visually sorted file list instead of the raw backend array.

---

## [1.9.8] - 2026-07-10

### Features, Security & Performance

- **Optimized Large Folder Handling**
  - Fixed application freezing and infinite loading loops when browsing folders with large datasets (8,000+ files).
  - Replaced bulk JSON payloads with incremental, event-driven chunk streaming from the Tauri backend.
  - Configured safe boundaries in directory reading commands to stop redundant API polling cycles.
- **Secure RAR Archive Extraction**
  - Replaced the deprecated `rar` dependency with the maintained `unrar` crate.
  - Added path normalization checks via the `path-clean` library to prevent directory traversal and arbitrary file write vulnerabilities during extraction.
  - Switched from disk-bound extraction to direct memory stream processing for listing contents safely.

---

## [1.9.7] - 2026-07-01

- Hotfixes

## [1.9.6] - 2026-06-30

### Features & Ad System Refinement

- **Ad system link updates**
  - Updated the desktop ad banner to load the hosted HTML page directly in the sandboxed iframe.
  - Set the clickable outer wrapper button to route to the actual Adsterra destination URL, ensuring correct navigation flow.
  - Reverted to using inline srcDoc for the desktop ad banner to ensure ads render reliably across all platforms, while preserving the user's custom click destination.
  - Updated the Mobile ad banner to route clicks to the custom ad landing page.

---

## [1.9.5] - 2026-06-24

### Features & Grouping Polish

- **Local-First Folder Grouping & Management**
  - Integrated a local SQLite database to track custom folder groups and user-defined sort order for Telegram channels.
  - Added horizontal scrollable group tabs at the top of the sidebar with dnd-kit drag-and-drop sorting support.
  - Added inline name editing and a custom color picker for folder groups.
  - Added a "Move to Group" submenu within the sidebar item context menu to quickly assign folders to groups.
  - Added a "Hide Groups" setting to toggle the visibility of the group tabs.

- **Theme Engine & Custom Styling**
  - Built a comprehensive React Context theme engine with support for custom themes and theme persistence.
  - Included a preset Cyber Teal theme.
  - Implemented a dedicated "Themes" tab in Settings to manage, configure, and delete custom user themes.

- **Proxy Connection Status Indicator**
  - Added real-time proxy connection checking and status indicator display in the sidebar.

- **Localization & Theme System Cleanups**
  - Added translated i18n keys for group management controls across all 13 supported languages.
  - Cleaned up custom theme contrast, scrollbar visibility, and light mode class toggles.

---

## [1.9.1] - 2026-06-20

### Localization & Internationalisation

- **Comprehensive Multi-Language Support**
  - Extended full translation coverage to all modal titles, tab headers, button labels, and input placeholders across both Desktop and Mobile interfaces.
  - Localized the Settings modal title and all five tab headers (General, Proxy, VPN, Sharing, About) across all thirteen supported languages: English, Spanish, Russian, Simplified Chinese, French, Arabic, Brazilian Portuguese, German, Hindi, Indonesian, Turkish, Japanese, and Korean.
  - Translated modal titles and interactive elements for the Rename Folder, Rename File, Move to Folder, and Remote Upload dialogs on Desktop.
  - Translated the mobile Rename Folder bottom sheet title, description prompt, input placeholder, and action buttons.
  - Translated the mobile bottom navigation bar tab labels (Files, Transfers, Settings) dynamically via the active language setting.
  - All translated strings correctly respond to language selection changes at runtime without requiring an application restart.

---

## [1.9.0] - 2026-06-16

### Features & Bug Fixes

- **Expanded REST API Interface**
  - Upgraded the REST API server to a fully functional programmatic Cloud Drive interface.
  - Implemented secure API key authentication and endpoints for file upload (via multipart forms), renaming, moving, copying, and deletion.
  - Added folder CRUD endpoints to manage Telegram channels programmatically.
  - Added endpoints for storage statistics, duplicate detection, empty folder discovery, and media metadata inspection.
  - Implemented a streaming ZIP archive generation endpoint for downloading bulk files.

- **Built-in Archive Viewer & Extractor**
  - Added support for viewing the contents of ZIP, RAR, and 7Z archives directly inside the application.
  - Implemented archive extraction capabilities to extract files to local storage or re-upload them.

- **Drag and Drop Interface**
  - Integrated drag-and-drop mechanics inside the file explorer to move files between folder views dynamically.

- **Context Menu Alignment**
  - Refined context menu positioning, boundaries, and quick-action menu options.

- **Thumbnail Resolution & Scoping**
  - Fixed duplicate thumbnail displays by scoping cache lookups and storage directories under folder-specific keys.

---

## [1.8.8] - 2026-06-12

### Features & Caching Polish

- **Enterprise-Grade Remote Upload**
  - Integrated dual-phase remote file upload from direct HTTP/HTTPS URLs.
  - Added frontend dialog `RemoteUploadModal` for URL input and destination folder selection.
  - Implemented backend download cache manager (`cmd_upload_from_url`) with disk space pre-flight checks, resumable downloads using HTTP `Range` headers, SOCKS5/MTProto proxy routing config, oneshot cancellation tracking, and bandwidth throttling.
  - Integrated dual-phase progress states and progress bars in `UploadQueue` UI.

---

## [1.8.7] - 2026-06-09

### Bug Fixes & Windows Enhancements

- **Windows Filename Sanitization**
  - Added filename sanitization to strip colons (`:`) and other invalid OS characters from target filenames, preventing Windows OS from refusing to save downloads or writing streams to NTFS Alternate Data Streams (ADS).
- **Cache Download Integrity**
  - Added strict file size verification in backend caching logic to detect interrupted downloads or server connection drop-offs. Prevents truncated files from being cached and causing FFmpeg remux parsing errors (like `Invalid data found while parsing box`).

---

## [1.8.4] - 2026-06-03

### Bug Fixes & UI Enhancements

- **Grid Virtualizer Cache Invalidation**
  - Resolved layout overlapping issues where "Upload File" and "Upload Folder" buttons overlapped with files in the Grid view by forcing virtualizer remeasurement on layout changes.

---

## [1.8.3] - 2026-06-03

### Bug Fixes & UI Enhancements

- **Cross-Platform Video Streaming Resolution**
  - Resolved dynamic CORS blocking behavior across client webviews, rectifying media loading and playback errors on Windows, macOS, and Linux.
- **File Grid UI & Layout Alignment**
  - Corrected card shifting and overlapping behaviors within the file explorer's grid layout.
  - Configured automated scroll reset and virtualizer cache purging during directory navigation.

---

## [1.8.2] - 2026-06-03

### Streaming Hotfix: Cross-Platform Video Playback Correction

- **CORS Configuration Update**
  - Updated CORS configuration on local streaming and API servers to use dynamic origin matching.
  - Resolved video playback failures (`TypeError: Failed to fetch` / `TypeError: Load failed`) on Windows, macOS, and Linux clients by explicitly supporting platform-specific custom schemes and WebKit's `null` origin.

---

## [1.8.1] - 2026-06-03

### Features, Security & Architecture (MimoPro Cleanups Part 2)

- **User Interface Enhancements**
  - Optimized the media player controls by standardizing close and fullscreen overlay buttons.
  - Refined layout geometries for player circle buttons and balanced spacing gaps.
  - Integrated a new periodic interstitial ad flow, featuring timed countdowns and automated clicks/focus dismissal logic.

- **Storage & Backend Stability Fixes**
  - Recreated missing mobile capability rules to support compilation of Tauri configuration context.
  - Corrected progress field mappings (`downloadedBytes`) in UI progress bars and download queues.
  - Resolved dynamic import resolution behavior for local builds.
  - Expanded Content Security Policy (CSP) rules to permit ad network script invocations.

- **Improved Accessibility Support**
  - Auto-enabled performance and reduced-motion modes using platform media queries.
  - Wrapped major component views in granular error boundaries to prevent app-wide crashes.

---

## [1.8.0] - 2026-06-02

### Features, Security & Architecture (MimoPro Analysis Cleanups)

- **Critical Fixes**
  - Synchronized versions across `package.json`, `tauri.conf.json`, and `Cargo.toml`.
  - Replaced hardcoded `localhost` references in `sharing.rs` with `127.0.0.1` to ensure local loopback resolution works reliably.
  - Removed misleading mock rate limit headers (`x-ratelimit-limit`, etc.) from API endpoint responses.

- **Dependency Upgrades**
  - Upgraded deprecated dependencies (`base64` to `0.22`, `rand` to `0.9`).
  - Standardized Tauri plugin version strings (`tauri-plugin-dialog`, `tauri-plugin-updater`, `tauri-plugin-process`) to consistently use version `"2"`.

- **Dead Code Cleanup**
  - Completely removed empty `DropZoneContext.tsx` and removed the `<DropZoneProvider>` wrapper from `App.tsx`.
  - Cleaned up unused state variables (`_internalDragFileId`) and unused hook exports (`isNetworkError`, `forceLogout`, `handleDownload`).
  - Removed dead settings fields (`proxySecret`, `ProxyConfig.secret`).

- **Security Enhancements**
  - Implemented secure `bcrypt` hashing (work factor `12`) for folder share passwords.
  - Swapped direct comparisons in API key verification for constant-time checks using `constant_time_eq` to prevent timing attacks.

- **Architecture & Code Deduplication**
  - Extracted shared byte-range parsing and chunk calculation logic for downloads and media streaming.
  - Consolidated duplicate MP4 container box header navigation and box validations into a single module.
  - Unified duplicate password hashing and verification code.
  - Corrected field naming inconsistency (`uploadedBytes` to `downloadedBytes` inside `DownloadItem`).

---

## [1.7.9] - 2026-06-02

### Features & Fixes

- **Folder Rename Bug Fix** — Resolved an issue where renaming a folder in the sidebar failed because the capture-phase click listener unmounted the context menu before the button's action could execute.
- **Media Player Styling** — Equalized the close and fullscreen circle buttons to standard sizes with a small gap between them.

---

## [1.7.8] - 2026-06-01

### Features & Enhancements

- **Major UI improvements**
- **Complete media player revamp**
- **Multiple bug fixes**

---

## [1.7.0] - 2026-05-29

### Features & Documentation

- **Pre-built Android APK Update** — Updated the sidelined APK build release reference to `v2.1.0-beta` in the documentation.
- **Android App Gallery** — Added an extensive screenshots section highlighting the beautiful mobile interface layout, theme views, and active queue transfers.
- **Desktop Bump to v1.7.0** — Bumped desktop versions to v1.7.0 for the synchronized release.

---

## [1.6.9] - 2026-05-29

### Features & Enhancements

- **Android Compilation Fixes** — Cleaned build configuration files to prevent deepLinkProtocols syntax conflicts.
- **Mobile Shell & macOS Entitlements** — Configured custom shell integration capabilities and set up macOS entitlements plists.

---

## [1.6.8] - 2026-05-25

### Features & Fixes

- **In-App Update Permission Fix** — Granted the `"process:allow-restart"` capability permission in `src-tauri/capabilities/default.json` to allow the frontend updater to safely relaunch the app after installing an update.

---

## [1.6.7] - 2026-05-23

### Features & Fixes

- **Windows Build & Git Checkout Fix** — Untracked and ignored `app/.npm-cache` files from Git to fix "Filename too long" checkouts and build errors on Windows platforms.
- **Tauri signing key security** — Replaced updater signing key with a password-protected keypair and restored the secret password integration in the CI pipeline.

---

## [1.6.6] - 2026-05-22

### Features & Fixes

- **Tauri Updater Integration & Dedicated UI** — Fully integrated and resolved production updater configurations.
  - **Updater Build Artifacts**: Set `createUpdaterArtifacts` to `true` in `tauri.conf.json` to generate signing signatures (`.sig`) and the `latest.json` manifest dynamically during production builds.
  - **In-App Update Interface**: Added a native "Check for Updates" control panel within the General Settings tab, complete with a visual download progress bar, status toasts, and automatic "Update & Restart" integration.
  - **Promise Safety**: Handled fire-and-forget background update-check Promises by appending explicit `.catch` error logging to prevent unhandled rejection behaviors.
  - **Automated Workflow Releases**: Enhanced the GitHub Release CI workflow to automatically parse and extract only the latest release notes from `CHANGELOG.md` dynamically using `awk`.

---

## [1.6.5] - 2026-05-21

### Features & Enhancements

- **REST API Enhancements (Actix-web & Rust)** — Fully implemented the comprehensive REST API extension in Rust/Actix-web with backwards-compatible response structures.
  - **Refined Folder Navigation**: Resolved `folder_id` query handling into three deterministic query states: all files when omitted, root-only when `?folder_id=`, and subfolder files when filtering specifically by a folder ID.
  - **Standardized Pagination Envelope**: Wrapped collections in a clean payload format featuring a `data` array, `pagination` metrics (`page`, `limit`, `total_items`, `total_pages`), and a `filters` echo block.
  - **Advanced Query Parameters**: Introduced server-side sorting (`sort_by`, `sort_order`) and robust filters for MIME type, file size bounds, and creation date ranges.
  - **Sparse Fieldsets**: Added a `?fields=` selector enabling clients to request specific metadata subsets to reduce bandwidth overhead.
  - **Bulk Operations & Global Search**: Added `POST /api/v1/files/bulk` for batch moves and deletes, and `GET /api/v1/files/search` supporting the full pagination envelope.
  - **Rate Limiting Integration**: Injected simulated API rate-limit headers (`X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Reset`) to standard responses.

---

## [1.6.0] - 2026-05-21

### Features & Fixes

- **"Copy Telegram Link" Feature** — Added a right-click context menu option to copy raw `t.me` message links for files in public channels (`https://t.me/{username}/{message_id}`). If the channel is private, the item displays in a disabled state with a descriptive tooltip.
- **Tauri 2 Tokio Runtime Panic Fix** — Fixed the `there is no reactor running` panic caused by `tokio::task::spawn_blocking` executing outside of a Tokio runtime context within the Bandwidth Manager. Replaced the asynchronous task with a lightweight, synchronous write, resolving the panic completely.

---

## [1.5.0] - 2026-05-19

### Feature

- **VPN Optimizer & Proxy Configuration** — Added robust support for toggling VPN mode to optimize network connection timeouts, retry limits, backoff delays, adaptive polling, flood wait handling, and peer caches. Fully integrated proxy configuration (SOCKS5 and MTProto) to allow custom routing and bypass geo-blocks.

---

## [1.4.2] - 2026-05-18

### Feature

- **Folder Upload with Automatic Zipping** — Support uploading entire folders directly, automatically compressing them into highly-optimized zip archives before transfer.

---

## [1.1.7] - 2026-05-01

### Feature

- Added a donation button and popup modal to the main login screen to support the project via PayPal, Litecoin, and Bitcoin.

---

## [1.1.6] - 2026-04-28

### Fix

- Fixed process not terminating on Ctrl+C (SIGINT) when launched from a terminal.
  The Actix-web streaming server and grammers network runner were running on
  non-daemon threads with no shutdown signal wired to process exit, causing the
  application to hang indefinitely after the main window closed. The app now
  registers a RunEvent::Exit handler that gracefully stops both background
  services before the process exits.

---

## [1.1.5] - 2026-04-27

### Hotfix

- **CI fix: AppImage patch step now runs cleanly** — Replaced the fragile `grep -oP` Perl lookahead (which exited with code 2 under `set -euo pipefail`) with a safe `awk`-based `.desktop` file lookup. Added `APPIMAGE_EXTRACT_AND_RUN=1` so `appimagetool` doesn't require the FUSE kernel module on GitHub Actions runners.

---

## [1.1.4] - 2026-04-27

### Hotfix

- **Deeper AppImage EGL fix for Arch/rolling-release Linux** — Added a CI post-build patching step that strips the Ubuntu-bundled `libEGL`, `libGL`, `libGLdispatch`, `libGLX`, and `libGLESv2` from the AppImage squashfs and replaces the `AppRun` wrapper with one that: normalises the locale to `C.UTF-8`, sets `NO_AT_BRIDGE=1` to silence ATK warnings, auto-detects `EGL_PLATFORM` from `$WAYLAND_DISPLAY`/`$DISPLAY`, points GLVND at the system ICD vendor dirs, preloads the system `libEGL.so.1`, and orders `LD_LIBRARY_PATH` so host GPU drivers are always resolved before bundled stubs.

---

## [1.1.3] - 2026-04-27

### Hotfix

- **Fixed Arch Linux AppImage crash** — Resolved `EGL_BAD_ALLOC` error on Arch Linux (and other rolling-release distros) caused by bundled Mesa/EGL libraries conflicting with the host GPU driver stack. The app now automatically disables WebKitGTK's DMA-BUF renderer on Linux before the WebView initializes, with no impact to Windows or macOS builds.

---

## [1.0.4] - 2026-02-13

### Fixes

- Finally squashed the grid overlap bug for real. Cards were using CSS `aspect-[4/3]` to size themselves, but the virtualizer was computing row heights separately — at certain window widths these disagreed and rows would bleed into each other. Now both use the same explicit pixel height, so no more overlap regardless of how you resize the window.

### Cleanup

- Went through the whole codebase and ripped out every `console.log` / `console.error` we'd left in from debugging (16 of them). The one in `ErrorBoundary` stays since that's the whole point of an error boundary.
- Got rid of all `as any` casts on the frontend — everything's properly typed now.
- Ran Clippy and fixed all 7 warnings, including a couple of `collapsible_match` ones in `fs.rs` that needed manual refactoring.
- Dropped `clsx`, `tailwind-merge`, and `@tauri-apps/plugin-opener` from `package.json` — none of them were actually imported anywhere.
- General comment cleanup throughout.

---

## [1.0.3] - 2026-02-09

### Bug Fixes

- **Grid Spacing Fix** - Fixed cards overlapping in grid view
- **Dynamic Row Height** - Grid now properly calculates row height based on window size
- **Virtualizer Re-measurement** - Grid correctly updates when resizing window

---

## [1.0.2] - 2026-02-07

### Automated Release Pipeline

- **GitHub Actions Workflow** - Automatic builds triggered on version tags
- **Cross-Platform Builds** - Windows, Linux, macOS (Intel + ARM) built in parallel
- **Signed Updates** - All builds signed with Ed25519 for secure auto-updates
- **Automatic Publishing** - Releases published to GitHub automatically

---

## [1.0.1] - 2026-02-07

### Auto-Update System

- **Automatic Update Checks** - App checks for updates 5 seconds after startup
- **Update Banner** - Beautiful animated banner when new version available
- **One-Click Updates** - Download and install updates with progress indicator
- **Cross-Platform** - Windows, Mac, and Linux users get platform-specific updates

### 🔧 Technical

- Added Tauri updater plugin with Ed25519 signing
- Created `useUpdateCheck` hook for update lifecycle management
- Added `UpdateBanner` component with download progress

---

## [1.0.0] - 2026-02-06 🎉

### First Stable Release

Telegram Drive is now production-ready! This release focuses on performance, reliability, and user experience polish.

### ✨ New Features

- **Virtual Scrolling** - Smooth performance with folders containing 1000+ files
- **Inline Thumbnails** - Image files now display thumbnails directly in the file grid
- **Thumbnail Caching** - Thumbnails are cached locally for instant loading on revisit
- **API Setup Help Guide** - Step-by-step modal explaining how to get Telegram API credentials

### 🚀 Performance Improvements

- Grid and list views now only render visible items (virtualized)
- Responsive column layout adapts to window width
- Lazy loading of thumbnails to reduce initial load time

### 🎨 UI/UX Improvements

- Refined grid spacing (6px gaps between cards)
- Gradient overlay on thumbnail cards for text readability
- Improved light mode support across all components

### 🔧 Technical

- Added `@tanstack/react-virtual` for virtualization
- Separate thumbnail cache directory (`app_data_dir/thumbnails/`)
- FileTypeIcon now supports multiple sizes

---

## [0.6.0] - 2026-02-05

### Reliability Update

- Session persistence (window state, UI state, active folder)
- Network resilience with connection status indicator
- Queue persistence for uploads/downloads
- Light mode UI fixes

---

## [0.5.0] - 2026-02-04

### Drag & Drop Update

- Stable hybrid drag-drop system
- External drop blocker
- GitHub Actions workflow fixes

---

## [0.4.0] - 2026-02-01

### Media & Performance

- Audio/Video streaming player
- Global search filter
- Internal drag & drop between folders

## 12.6.2 — Explorer Auto-Recovery Part 3

- CFAPI provider watchdog with silent retry/backoff after Windows logon.
- Explorer availability state is reconciled into the local TCloud index after restart.
- Repair flows no longer re-enable Windows startup after the user disables it.
- Explorer fallback wording aligned to “Disponível offline” / “Liberar espaço”.

## 12.6.2 Parte 4 — Bandeja + Explorer

- Corrigido ícone vazio da bandeja com ícone TCloud explícito.
- Clique esquerdo abre o TCloud; clique direito abre o menu.
- Menu simplificado com Explorer, Upload, Sincronização e Configurações.
- Acesso rápido "Abrir no Explorer" na barra superior.

## 12.6.2 Parte 5 — Explorer Daily Use

- Rename/move valida colisão de caminho antes do commit do Explorer.
- Operações rápidas consecutivas substituem transações CFAPI antigas ainda pendentes.
- Worker revalida operações supersedidas antes de acessar a nuvem.
- Movimentos em andamento rebaseiam ações posteriores para a nova identidade remota.
- Linguagem de conflitos simplificada para Computador × Nuvem.

## 12.7.1 — Parte 4

- Instância única do desktop.
- Bandeja com status dinâmico de sincronização/Explorer.
- Varredura CFAPI adaptativa em idle.
- Retry de autorreparo de estado dos placeholders.
- Interface distingue configuração inicial de reconexão automática.
## 12.9.6 — Queue & Explorer Efficiency

- Filas de sync orientadas a evento, reduzindo polling SQLite em idle.
- Explorer/CFAPI acordam workers imediatamente ao criar trabalho.
- Coalescimento de bursts de eventos Modify do watcher.
- Recheck worker sem wakeups periódicos quando vazio.
- Limpeza de amostras de progresso após falhas/retries.
- Centro de Sincronização com polling adaptativo e supressão de renders idênticos.
- Transferência CFAPI de cache local em blocos maiores.
# TCloud 13.0.50

- Corrige arquivos presos permanentemente em `applying_remote_rename`.
- Finaliza corretamente o estado transitório após criar, atualizar ou mover placeholders.
- Recupera automaticamente estados antigos sem reenviar nem apagar arquivos remotos.

# TCloud 13.0.49

- Corrige transferências pendentes criadas sem tamanho total.
- Repara automaticamente filas antigas sem reiniciar uploads concluídos.
- Garante o preenchimento de progresso em todos os fluxos de criação de jobs.
- Impede que atalhos `.lnk` e `.url` sejam enviados pelo backup do computador.
