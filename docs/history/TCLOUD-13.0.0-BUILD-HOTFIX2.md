# TCloud 13.0.0 — Build Hotfix 2

Correção localizada para o backend Rust em `src-tauri/src/commands/fs.rs`.

- Remove a normalização de `emit_chunks` / `collect_results` que havia sido inserida por engano no caminho de upload criptografado.
- Normaliza os dois `Option<bool>` dentro de `cmd_get_files`, antes dos `match` que trabalham com `bool`.
- Corrige os erros Rust E0425 e E0308 reportados no build da 13.0.0.
- Não altera dependências, banco, Telegram, CFAPI, unidade T:, interface ou configurações.
