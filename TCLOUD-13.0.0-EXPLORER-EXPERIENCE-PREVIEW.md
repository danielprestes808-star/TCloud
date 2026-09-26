# TCloud 13.0.0 — Explorer Experience Preview

Esta entrega reúne em uma única versão de teste todas as fases planejadas para a experiência 13.0. O objetivo é permitir avaliar o produto inteiro antes de dividir correções e refinamentos em versões menores.

## 1. Explorer Experience

- Interface principal mais compacta e com menos ruído visual.
- Upload é a ação principal; ações menos usadas ficam em menus secundários.
- Navegação Voltar/Avançar integrada à barra de localização.
- Atalhos Alt+Esquerda e Alt+Direita para histórico de navegação.
- Estados vazios e comandos usam linguagem consistente com a nova interface.

## 2. Navigation & Sidebar

- Início, Recentes, Favoritos, Sincronização, Lixeira e itens que precisam de atenção.
- Seção Computador com acesso ao TCloud no Explorador do Windows.
- Seção Meu TCloud para pastas e bibliotecas.
- Cofre acessível diretamente pela barra lateral.
- Informações técnicas e diagnóstico ficam em Opções avançadas.

## 3. File Experience

Os estados foram simplificados para:

- Na nuvem.
- Disponível neste computador.
- Sempre disponível.
- Sincronizando.
- Precisa de atenção.

Menus de contexto usam Lixeira, Sempre disponível e Liberar espaço, evitando termos internos do CFAPI.

## 4. Settings Simplification

As configurações principais agora são organizadas em:

- Geral.
- Sincronização.
- Armazenamento.
- Telegram.
- Segurança.
- Aparência.
- Compartilhamento.

Proxy, rede e controles técnicos permanecem disponíveis quando Mostrar opções avançadas é ativado. A página de armazenamento evita expor chats/grupos internos como conceitos obrigatórios ao usuário.

## 5. Transfer Center

- Resumo único de sincronização.
- Progresso geral visível sem abrir uma tela técnica.
- Erros aparecem como Precisa de atenção.
- Uploads e downloads detalhados ficam recolhidos até o usuário pedir.
- A Central de Sincronização completa continua disponível.

## 6. Onboarding & First Run

Novo fluxo de quatro passos:

1. Conta.
2. Pastas iniciais.
3. Integração com o Windows.
4. Pronto.

A integração com o Windows é recomendada, mas não bloqueia a conclusão caso falhe e pode ser ativada depois.

Em instalações já configuradas, o fluxo pode ser reaberto em **Configurações > Assistente inicial** para teste.

## 7. Polish

- Menos cards e indicadores repetidos na tela inicial.
- Barra lateral e barra de comandos mais compactas.
- Configurações e onboarding responsivos.
- Mensagens mais orientadas à ação.
- Funções avançadas continuam disponíveis, mas deixam de competir visualmente com tarefas comuns.

## Compatibilidade

Nenhuma dependência foi adicionada. Esta versão mantém o motor de sincronização e confiabilidade da 12.9.7, incluindo Telegram, unidade T:, CFAPI, placeholders do Windows, cache, Cofre, Lixeira, filas persistentes e recuperação após falhas.

## Instalação do PATCH

Extraia o PATCH diretamente em `D:\TCloud\app` e substitua os arquivos. Depois execute:

```powershell
cd D:\TCloud\app
npm run desktop:verify
npm run tauri:shell
```
