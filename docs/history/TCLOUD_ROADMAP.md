# TCloud — Roadmap

## Objetivo
Transformar a base Telegram Drive em um armazenamento em nuvem pessoal usando Telegram como backend.

## Fases
1. Identidade e organização do projeto
2. Pastas, subpastas e grupos
3. Índice local SQLite
4. Sync Engine
5. Cache e arquivos offline
6. Integração com Windows Explorer / unidade virtual T:
7. Web Gateway / API externa (cliente mobile, se voltar, será projeto separado)


## Marco concluído — 12.9.7

- Checkpoint remoto crash-safe para upload.
- Integridade SHA-256 no stream.
- Proteção contra atomic-save/delete + recreate.
- Recuperação de fila e finalização local após encerramento inesperado.
- Revalidação de callbacks CFAPI concorrentes.

## Próximo marco sugerido — 13.0

Consolidação de produto: simplificação da interface, linguagem amigável, onboarding, configurações essenciais em primeiro plano e acabamento visual inspirado no Explorer/OneDrive.
