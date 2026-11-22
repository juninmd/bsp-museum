# AGENTS.md

BSP Museum: a Tauri desktop app (Rust + Vite/TypeScript + Three.js) that catalogs GoldSrc `.bsp` maps. The Rust core (`bsp_museum_core`) serves both the app and the `bsp-museum-mcp` MCP server. Details: `README.md` and `docs/MCP.md`.

## Stack
- Frontend: Bun, Vite 6, TypeScript 5, Three.js (`src/`), tests with `bun test`
- Backend: Rust 2021, MSRV 1.77, Tauri 2 (`src-tauri/`). Feature `app` is the UI; without it you get only the core and MCP

## Commands
```bash
bun install --frozen-lockfile
bun run typecheck            # tsc --noEmit
bun test                     # frontend (src/**/*.test.ts)
bun run build                # typecheck + vite build
bun run app                  # tauri dev
cd src-tauri
cargo clippy --all-targets -- -D warnings && cargo test                                              # with GTK/WebKit
cargo clippy --no-default-features --all-targets -- -D warnings && cargo test --no-default-features  # core + MCP
```

## Structure
- `src/`: frontend. `src/i18n.ts` holds UI strings; `src/types/` holds the shared type contract
- `src-tauri/src/bsp/`: BSP, WAD and render code. `src-tauri/src/mcp/`: MCP server. `src-tauri/src/bin/mcp.rs`: MCP binary
- `src-tauri/tauri.conf.json` and `tauri.updater.conf.json` (updater public key)
- `scripts/`: release scripts (`release.ts`) and screenshot generators
- `docs/MCP.md`: MCP reference; `prints/`: README screenshots

## Conventions
- Release version comes from the `vX.Y.Z` tag that `release.yml` creates from commits. Do not edit `version` in `package.json` or `tauri.conf.json` to "release"
- Tests use synthetic BSP, MDL and WAD files built byte by byte. Do not commit real Valve maps, WADs or models
- Parsers treat input as hostile: checked access, readable errors, never `panic`
- UI strings go through `src/i18n.ts`. pt and en must have the same keys (a parity test checks this)
- If you change a Tauri command or the type contract (`src/types/`), update both sides

## Gotchas
- **CI gates (all must pass):** `ci.yml` runs jobs `web` (typecheck, test, build), `rust` and `core` (clippy with `-D warnings`, plus `cargo test`). It runs on PRs and on push to `master`. `validate.yml` and `security.yml` are reusable workflows from `juninmd/base-actions` (validation, audit, secret scan)
- **Release:** `release.yml` runs on push to `master`, every 6 hours, and on demand. `scripts/release.ts plan` computes the version from semantic commits and calls `release-commit.yml`, which checks the commit, creates or resumes the draft, uploads installers and MCP, and publishes the `vX.Y.Z` tag
- Do not commit `.env*`, keys or local paths. `src-tauri/target/`, `dist/` and `node_modules/` stay out of git
- Do not hand-edit `bun.lock` or `src-tauri/Cargo.lock`. Regenerate them with `bun install` or `cargo`
- Do not suppress clippy warnings or skip tests to pass a gate
- Do not enable the Tauri updater without its key pair. The private key lives only in the `TAURI_SIGNING_PRIVATE_KEY` secret; the public key goes in `src-tauri/tauri.updater.conf.json` (see `release-commit.yml`)

<!-- base-actions-main-policy -->
## GitHub Actions — padrão obrigatório (@main)

- Referencie as actions internas sempre pelo branch, nunca por SHA:
  - `uses: juninmd/base-actions/<action>@main`
  - `uses: juninmd/base-actions/.github/workflows/<arquivo>.yml@main`
- Não fixe `@<sha>` em `juninmd/base-actions`: a atualização é centralizada no
  `main` e passa a valer no próximo run.
- PRs cujos checks não iniciam por billing/spending limit são mesclados pelo bot central
  `juninmd/base-actions/.github/workflows/auto-merge-billing-blocked.yml`.
  Para impedir o merge automático de um PR, aplique a label `do-not-merge`.