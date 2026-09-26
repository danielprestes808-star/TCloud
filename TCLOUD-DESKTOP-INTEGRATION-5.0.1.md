# TCloud Desktop Integration 5.0

Desktop: 13.0.17
Foundation: 5.0
Data: 2026-08-22 19:33:38

## Modelo
O Desktop permanece hibrido:
- Core Foundation 5.0: contrato/capabilities e funcoes compartilhadas.
- Tauri/Rust local: CFAPI, cache, thumbnails, preview, offline e integracao Windows.

## Integracao aplicada
- Bridge Foundation 5.0 global.
- Revalidacao ao ficar online, focar a janela ou voltar da suspensao.
- Refresh apos alteracoes no gerenciador de forums.
- Cache de capabilities em sessionStorage.
- Dataset HTML tcloudFoundation/tcloudCore para estados visuais futuros.
- Correcao de mojibake nos fontes ativos.
- Contrato local TCLOUD-DESKTOP-CONTRACT-5.0.json.

## Validacao
- npm run build
- cargo check --manifest-path src-tauri/Cargo.toml
- capabilities Foundation 5.0 quando Core estiver online

## Backup
D:\TCloud\desktop-backups\integration-5.0.1-20260822-193319