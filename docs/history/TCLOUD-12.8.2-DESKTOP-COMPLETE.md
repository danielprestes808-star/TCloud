# TCloud 12.8.2 — Desktop Complete RC

Esta versão consolida a experiência Explorer e as melhorias de desempenho/estabilidade da série 12.8.

## Destaques

- Fechar a janela pode manter a sincronização na bandeja ou encerrar o TCloud, conforme a preferência do usuário.
- Aviso nativo apenas na primeira vez em que a janela é fechada mantendo o TCloud em segundo plano.
- Uploads ativos são persistidos como trabalho pendente para retomada segura após reinício/crash; o arquivo é reenviado integralmente, sem criar objeto parcial no Telegram.
- Contador de problemas mostra arquivos realmente pendentes de ação, sem somar histórico antigo ou duplicar operações do Explorer.
- Histórico resolvido de transferências é limpo automaticamente no início.
- Perfil de desempenho simples: Economia / Equilibrado / Desempenho.
- Verificação de saúde e correção automática do banco/Explorer pela tela de configurações.
- Exportação de diagnóstico em ZIP sem credenciais, sessão do Telegram ou conteúdo dos arquivos.
- Economia automática de espaço por idade do cache, preservando itens marcados como “Sempre manter neste dispositivo”.
- O limite de cache também preserva arquivos fixados offline.
- Bandeja mostra sincronização pausada e percentual agregado quando disponível.
- Mantidas as otimizações de carregamento local imediato, thumbnails em cache, upload sem revarredura bloqueante e estrutura TCloud/Explorer 1:1.

## Recursos já consolidados nesta base

Fila persistente, progresso/velocidade/ETA, pausa/cancelamento/retry, prioridades, conflitos, histórico de versões, lixeira, pesquisa local, índice de conteúdo, duplicados por hash, cache LRU, disponibilidade offline, CFAPI, recuperação após crash/hibernação, startup com Windows, single-instance, diagnóstico e scripts de estabilidade/instalador.

## Limite conhecido

A retomada de um upload interrompido reinicia o arquivo inteiro. A API atual de upload usada pelo cliente Telegram não expõe um checkpoint persistente confiável para continuar do mesmo byte após encerrar o processo. A fila, porém, não perde o arquivo pendente.

## O que fica para uma fase separada

- Atualização automática real permanece desativada até existir endpoint HTTPS de atualização e chave pública de assinatura configurados.
- Retomada por byte/chunk depois de encerrar o processo não é prometida: um upload interrompido é recolocado na fila e reinicia o envio integralmente.
- A camada mobile foi removida desta base desktop. Web Gateway, acesso por navegador e compartilhamento público por link serão tratados como serviços separados.
- O limite de tamanho do arquivo continua sujeito ao limite aceito pela conta/API Telegram usada pelo TCloud; arquivos acima desse limite precisam ser divididos antes do envio.
