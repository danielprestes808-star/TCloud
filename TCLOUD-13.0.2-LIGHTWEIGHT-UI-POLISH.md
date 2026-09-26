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
