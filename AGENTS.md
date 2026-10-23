# AGENTS.md

App desktop Tauri (Rust + Vite/TypeScript puro + Three.js) que cataloga mapas GoldSrc `.bsp`. O núcleo Rust
(`bsp_museum_core`) é usado pelo app e pelo servidor MCP `bsp-museum-mcp`. Detalhes em `README.md` e `docs/MCP.md`.

## Stack
- Frontend: Bun, Vite 6, TypeScript 5, Three.js (`src/`). Testes com `bun test`.
- Backend: Rust edição 2021, MSRV 1.77, Tauri 2 (`src-tauri/`). Feature `app` = interface; sem ela só núcleo + MCP.

## Comandos
```bash
bun install --frozen-lockfile
bun run typecheck            # tsc --noEmit
bun test                     # frontend (src/**/*.test.ts)
bun run build                # typecheck + vite build
bun run app                  # tauri dev
cd src-tauri
cargo clippy --all-targets -- -D warnings && cargo test                         # com GTK/WebKit
cargo clippy --no-default-features --all-targets -- -D warnings && cargo test --no-default-features   # núcleo + MCP
```

## Gates de CI (todos precisam passar)
- `ci.yml`: jobs `web` (typecheck, test, build), `rust` e `core` (clippy com `-D warnings` + cargo test). Roda em PR.
- `validate.yml` e `security.yml`: reusable workflows do `juninmd/base-actions` (validação, auditoria, varredura de segredos).
- `release.yml`: só em tag `v*`.

## Convenções
- Testes usam arquivos BSP/MDL/WAD sintéticos montados byte a byte; não commite mapas, WADs ou modelos reais da Valve.
- Parsers tratam o arquivo como entrada hostil: acesso checado, erro legível, nunca `panic`.
- Textos de interface passam por `src/i18n.ts` (pt e en devem ter as mesmas chaves; há teste de paridade).
- Mudou comando Tauri ou o contrato de tipos (`src/types/`)? Atualize os dois lados.

## Não fazer
- Não commitar `.env*`, chaves ou caminhos locais; `src-tauri/target/`, `dist/` e `node_modules/` ficam fora do git.
- Não editar `bun.lock` nem `src-tauri/Cargo.lock` à mão; regenere com `bun install` / `cargo`.
- Não suprimir warning do clippy nem pular teste para passar o gate.
- Não ligar o updater do Tauri sem o par de chaves de assinatura (ver `release.yml`).
