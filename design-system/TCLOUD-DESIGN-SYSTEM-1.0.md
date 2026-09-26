# TCloud Design System 1.0

## Principio central
Uma identidade, tres interfaces.

Desktop, Web e Mobile devem usar os mesmos nomes, cores, icones, estados e regras de negocio. A disposicao pode mudar para respeitar cada plataforma.

## Navegacao oficial
1. Arquivos
2. Recentes
3. Favoritos
4. Disponivel offline
5. Lixeira
6. Foruns
7. Configuracoes

## Acoes principais
- Nova pasta
- Enviar
- Novo forum

Essas tres acoes devem aparecer juntas quando houver espaco suficiente.

## Estados de sincronizacao
- Somente online
- Disponivel neste dispositivo
- Sincronizando
- Enviando
- Baixando
- Precisa de atencao
- Pausado

## Regras de consistencia
- Um mesmo icone nunca pode significar duas coisas diferentes.
- Um mesmo recurso deve ter o mesmo nome nas tres plataformas.
- Desktop e Web usam sidebar quando houver largura suficiente.
- Mobile usa navegacao inferior e telas adaptadas a toque.
- O Desktop preserva recursos exclusivos do Windows: CFAPI, unidade T:, Explorer, tray e arquivos sob demanda.
- O Web preserva recursos naturais do navegador: drag-and-drop, links e PWA.
- O Mobile preserva recursos naturais do Android/iOS: camera, compartilhamento e gestos.
- Recursos exclusivos nao devem alterar o significado das funcoes compartilhadas.

## Hierarquia
A interface deve priorizar arquivos e pastas. Ferramentas tecnicas ficam em Configuracoes ou centros avancados e nao competem com as tarefas principais.