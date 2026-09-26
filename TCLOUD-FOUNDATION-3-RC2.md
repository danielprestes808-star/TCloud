# TCloud Foundation 3 RC2

RC2 de estrutura Telegram para o TCloud Core.

## Objetivo

Transformar a indexacao plana da RC anterior em uma arvore real:

- Mensagens Salvas
- forum TCloud / Meus Arquivos
  - topicos/pastas do forum
    - arquivos

## Seguranca de escopo

Por padrao, o Core le:

- Mensagens Salvas
- dialogos cujo nome comeca com TCloud
- nomes explicitamente incluidos em TCLOUD_INDEX_INCLUDED_DIALOGS

A RC2 inclui `Meus Arquivos` por padrao porque esse e o forum principal ja usado pelo
TCloud Desktop.

TCLOUD_INDEX_ALL_DIALOGS continua desligado.

## Reconciliacao

A primeira indexacao de um forum apos a RC2 usa structure_version=2 e relê os
metadados historicos para descobrir o topico correto de cada mensagem.

Nenhum arquivo e baixado novamente.

Depois da reconciliacao, o cursor incremental volta a ser usado normalmente.