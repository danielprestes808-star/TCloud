# TCloud 12.8.3 — Desktop Clean

Esta revisão conclui a remoção do cliente mobile da árvore de produção do TCloud Desktop.

## Removido

- inicialização Android/JNI e `ndk-context`;
- tratamento de `content://`, cache Android e MediaStore;
- share sheet e cache de arquivos recebidos do Android;
- caminhos condicionais Android/iOS no ciclo de vida do Tauri;
- branches mobile de upload, download, streaming e API;
- condicionais mobile para RAR;
- linker `c++_shared` exclusivo do Android;
- chave de tradução `socks5_desc_mobile`, que não era usada pelo desktop.

## Preservado

- Telegram e autenticação;
- Explorer/CFAPI e unidade T:;
- sincronização em segundo plano e bandeja;
- upload/download desktop;
- cache, streaming local, compartilhamento e Web Gateway/API;
- criptografia/cofre;
- suporte desktop do Tauri.

O mobile, caso volte futuramente, deve permanecer em projeto separado e consumir uma API/Gateway do TCloud.
