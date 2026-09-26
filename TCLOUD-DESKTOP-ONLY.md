# TCloud 12.8.3 — Desktop Only

Esta árvore de produção contém somente o cliente desktop do TCloud.

Removidos desta base:
- interface React mobile;
- dashboard e componentes touch mobile;
- pareamento de celulares;
- servidor LAN dedicado ao cliente mobile;
- descoberta mDNS do cliente mobile;
- regras de firewall exclusivas do servidor mobile;
- configuração/capability Android e ícones iOS;
- dependências Rust usadas apenas por essa camada mobile.
- JNI/ndk-context, content://, MediaStore e share sheet do Android;
- branches condicionais Android/iOS no backend Tauri e no fluxo de upload/download.

O desktop, Telegram, Explorer/CFAPI, sincronização em segundo plano, cache, streaming local, compartilhamento e a futura base de atualização/Web Gateway permanecem preservados. Se um cliente mobile voltar no futuro, ele deve ser desenvolvido como projeto separado, consumindo a API/Gateway do TCloud, sem voltar a inflar o cliente desktop.
