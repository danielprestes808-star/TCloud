# TCloud 13.0.3 — Explorer Minimal UI

Esta rodada reduz o peso visual que ainda aparecia na tela Início e em Configurações.

## Mudanças

- Início passa a se comportar mais como Explorador e menos como dashboard.
- O aviso de problemas aparece uma vez no conteúdo; a sidebar mantém o acesso dedicado.
- O atalho “Precisa de atenção” foi removido da faixa de acesso rápido para evitar duplicação.
- O atalho “Recentes” foi removido da faixa porque a seção Recentes já aparece logo abaixo.
- “Meus arquivos” no Início usa ícones pequenos, evitando cards enormes sem miniatura.
- Sincronização não mostra badge numérico na sidebar; o contador fica em “Precisa de atenção”.
- A barra inferior exibe apenas “Precisa de atenção”, sem repetir o total.
- Configurações Gerais foram compactadas.
- Integração com Windows mostra só as ações principais; detalhes técnicos ficam recolhidos.
- A seção educativa “Proteção e recuperação” saiu da página Geral, sem remover nenhum recurso do backend.
- Sombras, bordas e estados continuam seguindo o perfil leve da 13.0.2.

## Compatibilidade

Sem alteração de dependências. O motor de sincronização, Telegram, CFAPI, unidade T:, Cofre e recuperação da série 12.9/13.0 permanecem preservados.
