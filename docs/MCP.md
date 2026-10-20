# bsp-museum para agent harnesses (MCP)

O acervo GoldSrc (mapas, WADs, sprites, modelos, arquivos do mod) fica disponível para
*agent harnesses* — Claude Code, Codex, Claude Desktop e qualquer cliente MCP — de duas formas:

1. **Servidor MCP** (`bsp-museum-mcp serve`): consultas ao vivo, **somente leitura**, por JSON-RPC 2.0 em stdio.
2. **Exportação de contexto** (`bsp-museum-mcp export-context`): grava `REPORT.md`, `index.json`,
   `maps/<nome>.json`, `fastdl.txt` e `AGENTS.md` numa pasta que o agente lê sem servidor.

Os dois usam as mesmas funções do núcleo (`bsp_museum_core`) que o app usa — catálogo, auditoria,
recursos/FastDL, comparação, WAD, sprite, MDL, arquivos do mod, áudio, nav e demo. Nada é reimplementado.
O binário não depende de Tauri/GTK.

## Instalação

```bash
cd src-tauri
cargo build --release --no-default-features --bin bsp-museum-mcp
# binário: src-tauri/target/release/bsp-museum-mcp   (.exe no Windows)
```

`--no-default-features` desliga a interface (feature `app`), então não precisa de WebKit/GTK. Sem
nenhuma crate nova além das que o projeto já usa (`serde_json`, `base64`, `flate2`).

```bash
bsp-museum-mcp --help
bsp-museum-mcp --version
```

## Configuração nos clientes

Troque `/caminho/...` pelos caminhos reais. Sem `--root` o servidor só enxerga o diretório de
trabalho em que o cliente o iniciou — aponte sempre as pastas do acervo.

### Claude Code

```bash
claude mcp add bsp-museum -- /caminho/bsp-museum-mcp serve --root /caminho/cstrike
# várias raízes (cstrike + valve, para o cliente ver os arquivos do jogo base):
claude mcp add bsp-museum -- /caminho/bsp-museum-mcp serve --root /caminho/cstrike --root /caminho/valve
```

Ou, no projeto, um `.mcp.json` (há um modelo em [`.mcp.json.example`](../.mcp.json.example)):

```json
{
  "mcpServers": {
    "bsp-museum": {
      "command": "/caminho/bsp-museum-mcp",
      "args": ["serve", "--root", "/caminho/cstrike"]
    }
  }
}
```

### Codex (`~/.codex/config.toml`)

```toml
[mcp_servers.bsp-museum]
command = "/caminho/bsp-museum-mcp"
args = ["serve", "--root", "/caminho/cstrike"]
```

### Claude Desktop (`claude_desktop_config.json`)

Mesmo bloco `mcpServers` do `.mcp.json` acima.

### Variáveis de ambiente

| variável | efeito |
|---|---|
| `BSP_MUSEUM_ROOTS` | raízes extras, separadas por `:` ou `;` (soma-se aos `--root`) |
| `BSP_MUSEUM_ALLOW_WRITE=1` | mesmo que `--allow-write` |
| `BSP_MUSEUM_MAX_OUTPUT` | mesmo que `--max-output` (bytes) |

## Protocolo

JSON-RPC 2.0, uma mensagem por linha em stdin/stdout. **stdout carrega só o protocolo**; logs vão
para stderr. Protocolo MCP `2025-06-18`, com negociação para `2025-03-26` e `2024-11-05`
(`structuredContent` só nas versões que o definem). Métodos: `initialize`, `notifications/initialized`,
`ping`, `tools/list`, `tools/call`, `resources/list`, `resources/templates/list`, `resources/read`,
`prompts/list`, `prompts/get`.

Erros: JSON inválido `-32700`; mensagem malformada `-32600`; método desconhecido `-32601`;
argumentos fora do `inputSchema` (obrigatório ausente, tipo, faixa, chave desconhecida) ou
ferramenta/prompt desconhecido `-32602`; recurso inexistente `-32002`; requisição antes do
`initialize` `-32002`. Falhas de **execução** (arquivo corrompido, caminho recusado, extensão errada)
voltam como resultado normal com `isError: true` e a mensagem, para o modelo poder corrigir.

Cada resposta de ferramenta traz o JSON compacto em `content[type=text]` e o mesmo objeto em
`structuredContent`; imagens (radar, textura de WAD, quadro de sprite) vêm como `content[type=image]`
(PNG em base64).

## Ferramentas

Todas têm `inputSchema` completo e `annotations.readOnlyHint = true` (a exceção é `map_radar`
quando o servidor roda com `--allow-write`: aí o hint é `false`). Caminhos podem ser absolutos ou
relativos a uma raiz permitida.

| ferramenta | o que faz |
|---|---|
| `list_roots` | raízes permitidas, modo (leitura/gravação) e teto de saída |
| `list_maps(dir, query?, offset?, limit?)` | `.bsp` da pasta com resumo barato e problemas de entidades |
| `map_summary(path, slots?)` | `MapDetail` (sem a planta SVG): modo, spawns, céu, WADs, texturas, lumps, achados, recursos, contagens de hulls/zonas/cena |
| `map_diagnostics(path, slots?)` | achados do diagnóstico por gravidade (`critical`/`warn`/`info`) |
| `map_entities(path, classname?, query?, offset?, limit?)` | entidades com chaves e posição; `classname` aceita prefixo `func_*` |
| `map_resources(path)` | recursos de FastDL: o que baixar, tamanhos, o que falta |
| `map_textures(path, source?, offset?, limit?)` | nome, tamanho e origem (embutida / WAD / ausente) |
| `map_scene(path, type?, offset?, limit?)` | fog, céu, luzes, decals, sprites, beams, sons, movers, path_corners e rotas de trem |
| `map_volumes(path)` | zonas e gatilhos com AABB e superfícies invisíveis por contagem (sem triângulos) |
| `map_hulls_info(path, offset?, limit?)` | contagens de colisão: hulls 1-3, planos, nós, clipnodes, folhas, modelos |
| `audit_folder(dir, slots?, only_problems?, offset?, limit?)` | auditoria completa da pasta + duplicados |
| `compare_maps(a, b, slots?)` | diferença de lumps, entidades, texturas, WADs e achados |
| `map_radar(path, out_dir?, size?)` | radar PNG (imagem) + `overview.txt`; grava só com `--allow-write` |
| `server_checklist(dir, slots?)` | auditoria + ausentes + FastDL total (arquivos únicos) + ranking de peso + próximos passos |
| `wad_list(path, query?, kind?, offset?, limit?)` | diretório do WAD2/WAD3 |
| `wad_texture(path, name)` | textura do WAD como imagem PNG |
| `wad_usage(wad_path, maps_dir, only_used?, only_unused?, …)` | quais mapas usam cada textura do WAD |
| `sprite_info(path, frame?)` | cabeçalho e quadros do `.spr`; com `frame`, a imagem |
| `model_info(path, offset?, limit?)` | bones, sequências, bodyparts, controladores, hitboxes, attachments, texturas (sem malha) |
| `mod_files(mod_root, filter?, kind?, include_paks?, …)` | arquivos do mod, incluindo o conteúdo dos `.pak` |
| `mod_file_analyze(mod_root, rel, origin?)` | análise por formato (áudio, nav, demo, pak, liblist, mapcycle, texto…) |
| `audio_info(path)` | metadados de WAV/MP3 (sem o envelope da onda) |
| `nav_info(path, bsp_path?, max_areas?)` | cabeçalho/contagens do `.nav` e se bate com o `.bsp` |
| `demo_info(path)` | cabeçalho e diretório da demo `.dem` |

**Respostas grandes.** O teto padrão é 48 000 bytes de JSON por chamada (`--max-output`). Listas têm
`offset`/`limit` e devolvem `total`, `returned`, `next_offset` e `truncated`. Se mesmo assim a resposta
passar do teto, o servidor corta as maiores listas pela metade até caber e marca `truncated: true`
com `truncation.cuts` (caminho, total, quantos ficaram) — o agente sabe que deve paginar ou filtrar.
Strings com mais de 2000 caracteres são cortadas.

## Recursos

| URI | conteúdo |
|---|---|
| `bsp-museum://roots` | raízes e limites |
| `bsp-museum://maps` | mapas de todas as raízes |
| `bsp-museum://map/<nome>/summary` | `map_summary` |
| `bsp-museum://map/<nome>/diagnostics` | `map_diagnostics` |
| `bsp-museum://map/<nome>/resources` | `map_resources` |
| `bsp-museum://map/<nome>/textures` | `map_textures` |
| `bsp-museum://map/<nome>/entities` | `map_entities` |

`<nome>` é o nome do arquivo sem `.bsp` (com %-codificação). `resources/list` enumera os primeiros
100 mapas; `resources/templates/list` traz os modelos acima. Nome ambíguo (mesmo nome em duas pastas)
é recusado.

## Prompts

- `audit_map_pack(dir, slots?)` — revisar uma pasta de mapas para servidor CS 1.6.
- `fastdl_plan(dir)` — plano de FastDL: o que publicar, tamanhos, ausentes, o que cortar.
- `explain_map(path)` — explicar um mapa (modo, layout, objetivos, problemas, recursos).

## Modelo de segurança

- **Somente leitura.** Nenhuma ferramenta altera arquivos. A única que grava é `map_radar` com
  `out_dir`, e só se o servidor foi iniciado com `--allow-write`; mesmo assim escreve apenas
  `overviews/<mapa>.bmp|png|txt`, sob uma raiz permitida.
- **Raízes permitidas.** `--root` (repetível) e `BSP_MUSEUM_ROOTS`; sem nenhuma, só o diretório de
  trabalho atual. Todo caminho recebido é canonicalizado (symlinks resolvidos) e precisa estar sob uma
  raiz. `..` é recusado antes de tocar o disco; symlink que sai da raiz é recusado; caminho inexistente
  e caminho fora da raiz dão a mesma mensagem (não serve de oráculo sobre o disco). Nas varreduras de
  pasta, `.bsp` alcançado por symlink que foge da raiz é descartado (`skipped_outside_roots`).
- **A pasta `valve` irmã** do mod só entra em `mod_files`/`mod_file_analyze` se também estiver dentro
  de uma raiz.
- **Limites.** Tamanho máximo por tipo antes de abrir (BSP 256 MiB, MDL/nav 64 MiB, áudio 32 MiB,
  sprite 32 MiB, WAD 512 MiB), linha de entrada de no máximo 16 MiB, saída limitada (acima). Um
  parser que entre em pânico com arquivo hostil vira `isError`, não derruba o servidor.
- **Conteúdo hostil.** Título, valores de entidades, nomes de textura, mensagens e `.txt` do mapa são
  dados escritos por terceiros: as respostas levam `data_notice`, o servidor declara isso nas
  `instructions` do `initialize` e nos prompts, valores gigantes são cortados e o `REPORT.md` põe todo
  texto vindo de mapa em código inline, numa linha só. Nada disso vira instrução para o modelo.
- **Sem rede.** O servidor não abre conexões; nunca baixa nada nem segue URLs de mapas.
- **Limites conhecidos.** A conferência de raiz é feita ao resolver o caminho, não ao abrir (uma troca
  de symlink entre as duas etapas, por quem já escreve na raiz, não é impedida). `wad_usage` lê os
  nomes de textura de `.bsp` de `maps_dir` que o `find_bsp_files` encontra, inclusive por symlink de
  pasta. O diagnóstico confere a existência (e o tamanho) de recursos na pasta `valve` irmã do mod,
  mesmo fora das raízes, para dizer se o cliente já tem o arquivo; nenhum conteúdo dela é devolvido.

## Exportação de contexto

```bash
bsp-museum-mcp export-context /caminho/cstrike/maps --out ./contexto [--format json|md|jsonl] [--slots 32]
```

| arquivo | conteúdo |
|---|---|
| `REPORT.md` | visão geral para humano/LLM: próximos passos, problemas por gravidade (com os mapas), duplicados, FastDL total e maiores arquivos, recursos ausentes, ranking de peso |
| `index.json` | um resumo por mapa (modo, spawns, achados, download, ausentes, duplicado de quem) |
| `maps/<nome>.json` | detalhe completo de cada mapa (`--format json`, padrão); `maps/<nome>.md` com `md`; `maps.jsonl` com `jsonl` |
| `fastdl.txt` | arquivos que o jogador baixa, um caminho relativo ao mod por linha, sem repetição |
| `AGENTS.md` | instruções para o agente que abrir a pasta: o que cada arquivo é, aviso de segurança e como usar o servidor MCP |

No código: `bsp_museum_core::agentctx::export(dir, out, slots) -> Result<ExportSummary, String>`
(e `export_with(dir, out, slots, Format)`), para o app chamar sem passar pelo binário.

## Testes

```bash
cd src-tauri
cargo test --no-default-features
cargo clippy --no-default-features --all-targets -- -D warnings
```

Os testes do protocolo e das ferramentas usam os mapas sintéticos dos fixtures (nenhum mapa da Valve
no repositório); `tests/mcp_stdio.rs` roda o binário de verdade por stdin/stdout. Para gerar os mapas
sintéticos numa pasta e experimentar à mão:

```bash
BSP_MUSEUM_DUMP_FIXTURES=/tmp/acervo cargo test --no-default-features --lib gera_mapas_de_exemplo
bsp-museum-mcp serve --root /tmp/acervo
```

Teste rápido sem cliente:

```bash
printf '%s\n' \
 '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{}}}' \
 '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
 '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_maps","arguments":{"dir":"/tmp/acervo/cstrike/maps"}}}' \
 | bsp-museum-mcp serve --root /tmp/acervo
```
