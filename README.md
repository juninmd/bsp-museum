# 🗺️ bsp-museum

Galeria navegável do acervo de mapas GoldSrc (Half-Life / CS 1.6). App desktop em **Tauri**:
aponta para a pasta `cstrike/maps`, e cada `.bsp` vira um card com **planta baixa renderizada**,
metadados e diagnóstico.

Uma pasta com 200 mapas é hoje 200 arquivos binários de nome sugestivo. Isto transforma o acervo
em algo que dá para olhar, filtrar e auditar.

```
┌──────────────────────────────────────────────────────────────┐
│ bsp-museum      [filtrar…] [modo ▾] [ordenar ▾] [pasta…]     │
├──────────────────────────────────────────────────────────────┤
│ ┌────────────┐  ┌────────────┐  ┌────────────┐               │
│ │ planta     │  │ planta     │  │ planta     │               │
│ │ baixa      │  │ baixa      │  │ baixa      │               │
│ ├────────────┤  ├────────────┤  ├────────────┤               │
│ │ bio_beach  │  │ de_dust2   │  │ zm_castle  │               │
│ │ zombie     │  │ de_ · bomba│  │ zombie     │               │
│ │ 32CT · 32T │  │ 16CT · 16T │  │ 20CT · 0T  │               │
│ │            │  │            │  │ ⚠ spawn de │               │
│ │            │  │            │  │  um time só│               │
│ └────────────┘  └────────────┘  └────────────┘               │
└──────────────────────────────────────────────────────────────┘
```

## Rodando

```bash
bun install
bun run app          # tauri dev
bun run app:build    # instaladores em src-tauri/target/release/bundle (NSIS/MSI, deb/AppImage, dmg)
```

Precisa de Rust (MSVC no Windows) e do WebView2 — que já vem no Windows 11. No Linux, as libs do Tauri
(`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `librsvg2-dev`).

Na primeira execução, clique em **escolher pasta…** e aponte para a pasta com os `.bsp`
(subpastas incluídas). A escolha fica salva para a próxima abertura.

Para só ver o app no navegador (sem Tauri; os comandos do backend não respondem): `bun run dev` serve em
`http://localhost:1420` (porta fixa, `strictPort`). `bun run build` faz typecheck e gera `dist/`.

## Requisitos

- [Bun](https://bun.sh) (gerenciador de pacotes e runner de testes; o CI usa a versão estável mais recente — não há versão fixada).
- Rust estável, MSRV `1.77` (`src-tauri/Cargo.toml`); no Windows, toolchain MSVC e WebView2.
- Linux: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `libjavascriptcoregtk-4.1-dev`, `librsvg2-dev`.
- Só para refazer as capturas de tela: Node e `playwright` (ver [Testes](#testes)).

O servidor MCP (`bsp-museum-mcp`) compila sem GTK/WebKit com `--no-default-features`.

## Configuração

O app não lê variáveis de ambiente; a pasta de mapas e as anotações ficam no `settings.json` do app. As
variáveis abaixo valem para o servidor MCP e para o script de capturas (detalhes em [`docs/MCP.md`](docs/MCP.md)):

| variável | uso |
|---|---|
| `BSP_MUSEUM_ROOTS` | raízes extras do servidor MCP, separadas por `:` ou `;` (soma-se aos `--root`) |
| `BSP_MUSEUM_ALLOW_WRITE` | `1` = mesmo que `--allow-write` (deixa `map_radar` gravar `overviews/`) |
| `BSP_MUSEUM_MAX_OUTPUT` | teto em bytes do JSON de cada resposta MCP (padrão 48000) |
| `FIXTURE_DIR` | pasta de saída dos mapas sintéticos dos testes `gerar_dados_*` |
| `PLAYWRIGHT_MODULE` | caminho alternativo do módulo `playwright` em `scripts/screenshots.mjs` |

Nenhuma variável guarda segredo. `.env*` está no `.gitignore`.

## Terminal (servidor MCP)

`--help`, `--version` e um `export-context` real (pasta sem mapas, sem credencial) estão em
[`docs/terminal.txt`](docs/terminal.txt).

## Novidades da v0.7

> Tudo abaixo foi escrito a partir do SDK/Xash3D e testado com arquivos **sintéticos**; nenhum arquivo da
> Valve passou pelo código. Os pontos de formato não conferidos contra o jogo estão em
> [Limitações](#limitações) e em `REFERENCES.md`.

- **Vista 3D**: render modes (aditivo, transparência, cor), texturas `scroll*` e animadas (`+0nome`),
  lightstyles animados, hulls de colisão, volumes/zonas (triggers, buyzone, bomb target…), luzes, sprites
  (`env_sprite`), sons ambientes, decals, névoa, sol, portas/plataformas/trens animados — ver o painel
  [Camadas](#vista-3d-painel-camadas-v07).
- **Skins** (nova seção): o app agora tem duas features — **Mapas** e **Skins**. A seção Skins tem subabas
  por tipo de modelo (`p_` jogadores, `v_` 1ª pessoa, `w_` no mundo), por time (**TRs**, **CTs**, **Zumbis**,
  reconhecidos pelo nome/pasta: `terror`, `leet`, `arctic`, `guerilla` / `urban`, `gign`, `gsg9`, `sas`, `vip` /
  `zombie*`, `zumbi`, `zp_`), **Recursos** (todos os `.mdl`), **Texturas (WAD)**, **Sprites**, **Áudio** e
  **Arquivos** do mod (mod/valve/`.pak`) com visualizador por tipo; navegador de
  **WAD** (grade de texturas, mips, PNG, quais mapas usam); player de **sprites** `.spr` (2D | 3D: billboard por tipo, blend, `renderamt`/`rendercolor`, esfera do raio e câmera do mundo girando sozinha; ver `prints/v8-sprite-3d.png`); **áudio** com
  forma de onda e loop; `.nav`, `.dem`, `liblist.gam`, `hud.txt`, `_detail.txt`, `mapcycle.txt`.
- **`.mdl` completo**: bodygroups, bone controllers, blends, cores de time, chrome/aditivo/masked,
  hitboxes, attachments, eventos.
- **Servidor MCP + exportação para agentes** (`bsp-museum-mcp`): 24 ferramentas somente leitura sobre mapas,
  WADs, sprites, modelos e arquivos do mod, mais `export-context` (`index.json`, `REPORT.md`, `fastdl.txt`,
  `AGENTS.md`). Também há o botão **exportar contexto p/ agentes** na auditoria. Detalhes em
  [`docs/MCP.md`](docs/MCP.md).

```bash
cd src-tauri
cargo build --release --no-default-features --bin bsp-museum-mcp   # sem GTK/WebKit
claude mcp add bsp-museum -- ./target/release/bsp-museum-mcp serve --root /caminho/cstrike
```

### Skins, sprites e sons em 3D

| skins (galeria 3D, comparar, equipes) | sprites (billboard por tipo) | sons espaciais no mapa |
|---|---|---|
| ![skins em 3D](prints/v8-skins-3d.png) | ![sprite em 3D](prints/v8-sprite-3d-aditivo.png) | ![sons 3D](prints/v8-sons-3d.png) |

- **Skins** (seção **Skins** → subaba do modelo ou **Recursos**): uma miniatura 3D girando por família de skin (um só contexto WebGL compartilhado),
  comparação lado a lado ou com divisor arrastável, prévia das equipes (top/bottom) e **mapa UV** com a malha
  sobre a textura, destaque do triângulo no modelo (e o caminho inverso) e exportação PNG.
- **Sprites** (Skins → Sprites, alternador 2D | 3D): os cinco tipos de `.spr` (`parallel`, `parallel_upright`,
  `facing_upright`, `oriented`, `parallel_oriented`) comportam-se como billboards de verdade numa cena orbitável, com
  blend por `tex_format`, origem do frame, esfera do raio, `renderamt`/`rendercolor` e câmera do jogo girando sozinha.
- **Sons 3D** (camada **Sons 3D** na vista do mapa): cada `ambient_generic` vira uma fonte Web Audio com panner HRTF e
  atenuação linear pelo raio; o ouvinte acompanha a câmera (órbita e no-clip). O painel "Sons do mapa" lista distância,
  volume efetivo e medidor de nível, com mudo/solo/"ouvir" por som; arquivo ausente ou ADPCM aparecem marcados como
  indisponíveis. O áudio só começa após o botão "Ativar áudio" (política de autoplay). Os WAVs sem laço repetem por
  padrão (opção desligável); no jogo tocariam uma vez.

> Ainda não ouvido em máquina real: o Chromium de teste não tem saída de som, então a espacialização foi conferida
> pelo grafo Web Audio e pelos medidores, não de ouvido. As regras de orientação dos sprites vêm do Xash3D de memória.

## Novidades da v0.6

> Capturas geradas pelo próprio app a partir de um mapa **sintético** (montado byte a byte nos
> testes — nenhum mapa da Valve no repositório). Para refazê-las: ver [Testes](#testes).

**Luz de verdade na vista 3D.** Os lightmaps do BSP viram um atlas e multiplicam a textura, como no
jogo. À esquerda com luz, à direita sem:

| com lightmaps | sem lightmaps |
|---|---|
| ![sala com lightmaps](prints/v6-3d-lightmap.png) | ![sala sem lightmaps](prints/v6-3d-sem-lightmap.png) |

**Só desenha o que a câmera enxerga (PVS + frustum).** A árvore BSP localiza a folha da câmera, o
`vis` diz quais folhas ela vê, e a malha é dividida em pedaços (textura × célula do mapa) que o
frustum culling descarta. O contador no canto mostra triângulos desenhados / total
(`50 / 102` com PVS contra `88 / 102` sem). A montagem dos pedaços roda num Web Worker.

![sem PVS](prints/v6-3d-sem-pvs.png)

**Galeria com filtros, favoritos, tags e notas.** Filtra por gravidade, por regra do diagnóstico, por
tag e por favoritos; anotações ficam no `settings.json` (chave = nome do arquivo, então sobrevivem a
mover o acervo). Tema claro/escuro e interface em português/inglês (botões no topo).

| pt · escuro | en · claro · filtrando "crítico" |
|---|---|
| ![galeria](prints/v6-galeria.png) | ![galeria em inglês, tema claro](prints/v6-galeria-en-claro.png) |

**Auditoria da pasta.** Roda o diagnóstico completo em todos os mapas (em paralelo), aponta arquivos
duplicados (mesmo conteúdo, nomes diferentes) e exporta Markdown, CSV ou HTML.

![auditoria](prints/v6-auditoria.png)

**Comparar dois mapas** (marque dois cards): diferença de lumps, entidades, texturas, WADs e quais
problemas sumiram ou surgiram.

![comparação](prints/v6-comparar.png)

**Mais regras de diagnóstico** — `spawn-em-solido` (desce a árvore BSP com o ponto do spawn),
`sem-vis` (o VIS não roda em mapa vazado: é o sinal de *leak* que dá para ler do arquivo),
`limite-motor` (planos, folhas, vértices, faces, entidades… contra os tetos do HLCSG/engine, aviso
a 90 %), `textura-ausente`, `wad-nao-encontrado` e `recurso-ausente`.

![diagnóstico](prints/v6-diagnostico.png)

**Entidades clicáveis**: lista todas (com chaves/valores), busca por classe, nome ou valor e **leva a
câmera 3D até a entidade** (brush entities usam o centro do modelo).

![entidades](prints/v6-entidades.png)

**Recursos necessários / FastDL**: lista mapa, `.res`, overview, WADs, céu, modelos, sprites e sons,
marca o que já vem no jogo base, soma o que o jogador baixa e exporta a lista (`.txt`).

![recursos](prints/v6-recursos-fastdl.png)

**Radar estilo CS**: vista de cima em verde; exporta `overviews/<mapa>.bmp`, `.png` e `.txt`.

![radar](prints/v6-radar.png)

**Também**: animação de `.mdl` e troca de skin na seção Skins (ver [Modelos](#modelos-mdl-props-e-o-visualizador-avulso)),
BSP do **Quake (v29)** abre (texturas em tons de cinza, lightmap monocromático), índice persistente
da varredura (reabrir uma pasta só relê o que mudou), varredura/auditoria/malha fora da thread da UI,
CI no GitHub Actions e instaladores para Windows, Linux e macOS.

## O que cada mapa mostra

**Planta baixa.** As faces viradas para cima são desenhadas do andar mais baixo para o mais alto,
com a **cor vindo da altura** (frio embaixo, quente em cima) — então uma passarela aparece por
cima da rua em vez de se perder na silhueta. Na vista detalhada, as paredes entram como traço
fino e os spawns como bolinhas (azul CT, laranja T).

**Vista 3D.** Cada mapa tem um viewport **WebGL** orbitável (arraste para girar, scroll para zoom).
Dois modos: *3D* mostra a geometria colorida por altura — você enxerga um *biowall* por cima, uma
passarela no ar, um teto que a planta escondia; *3D + texturas* aplica os **pixels reais** nas
faces: primeiro os embutidos no BSP, depois os dos **WADs** que o mapa declara (ele abre os `.wad`
da pasta do mod e resolve pelo nome). O **céu** também vem de verdade: os 6 lados de `gfx/env`
pelo `skyname` do worldspawn. Quando nada disso existe, entra uma cor neutra no lugar da textura.

Na vista 3D dá para: **tela cheia**, marcar **texturas transparentes** (respeita o alpha do mip), e ir
para **primeira pessoa (no-clip)** — você navega o mapa como no jogo: WASD anda, mouse olha,
Space/Ctrl sobe/desce, Shift corre, Esc sai. A vista é orbitável (arraste gire, scroll zoom).
Nos mapas `cs_` sem zona de resgate explícita (`func_/info_hostage_rescue`) o aviso é só informativo:
o GoldSrc resgata o refém perto de qualquer spawn de CT (fallback oficial).

### Vista 3D: painel "Camadas" (v0.7)

Botão **Camadas** no canto do viewer. Cada grupo só aparece se o mapa tem o que mostrar:

- **Superfície** — modos de render das brush entities (`rendermode`/`renderamt`/`rendercolor`/`renderfx`:
  aditivo, translúcido, cor, alpha-test, pulso e strobe), texturas `scroll*` (esteira/água) com a velocidade do
  backend, texturas animadas `+0…+9` (10 quadros/s; o botão alterna para a sequência `+A…+J`) e **luz animada**:
  os lightstyles (`a`..`z` a 10 Hz) somam as camadas do lightmap num shader, e o estilo 0 sai igual ao de sempre.
- **Colisão (hulls)** — botões 1/2/3. O hull vem em janelas (`map_hulls` paginado por `next_offset`), com barra de
  progresso e tetos de segurança (400 mil triângulos, 12 janelas), desenhado em aramado + translúcido por cima.
- **Volumes e zonas** — gatilhos, buyzone, alvo da bomba, resgate, VIP, fuga, escada, água e superfícies
  invisíveis (`clip`, `aaatrigger`…); a legenda liga/desliga cada categoria e o mouse mostra `message`/`target`.
- **Cena** — luzes (cor e estilo; as ligáveis alternam ao clicar), `env_sprite`/`env_glow` (o `.spr` é lido com
  `load_sprite`, com o `tex_format` e o modo de render), sons ambientes (clique: arquivo e raio), decalques, rotas de
  trem, **animar portas, plataformas e trens** (vai-e-vem entre a posição inicial e a final), névoa (`env_fog`),
  sol (`light_environment`, no sombreado do modo sem textura) e céu.

| camadas (hull 1, volumes, luzes, sprite, som, decalque) | luz animada | luz estática |
|---|---|---|
| ![painel de camadas](prints/v7-3d-camadas.png) | ![luz animada](prints/v7-3d-luz-animada.png) | ![luz estática](prints/v7-3d-luz-estatica.png) |

> Capturas do mapa sintético dos recursos extras (`fixture_extra.rs`), com as texturas de 8×8 do fixture: o chão tem
> o bloco de luz vermelho do estilo 0 e um bloco verde no estilo 1 — à direita só o estático, ao meio a soma.

### `de_dust2` no bsp-museum

Planta baixa (chão + paredes + spawns) e vista 3D (cor por altura) do mapa clássico,
geradas pelo próprio app a partir do `.bsp`:

| | |
|---|---|
| ![planta baixa do de_dust2](prints/dust2-planta.png) | ![vista 3D do de_dust2](prints/dust2-3d.png) |

**Metadados.** Título do worldspawn, céu, WADs declarados, dimensões do mapa, contagem de faces,
vértices e brush entities, texturas usadas e quantas estão embutidas no BSP.

**O que pesa no arquivo.** Ranking dos lumps por tamanho. É como se descobre que o mapa tem 7 MB
por causa do lightmap, não da geometria.

**Diagnóstico.** As regras que decidem se o mapa realmente joga:

| id | severidade | pega |
|---|---|---|
| `de-sem-bomb-target` | crítico | prefixo `de_` sem alvo de bomba — o round nunca termina por objetivo |
| `cs-sem-refem` / `cs-sem-resgate` | crítico / info | `cs_` sem refém (crítico); refém sem `func_hostage_rescue` é só info — o GoldSrc resgata perto de any spawn CT |
| `as-incompleto` | crítico | `as_` sem `info_vip_start` ou sem `func_vip_safetyzone` |
| `prefixo-divergente` | aviso | as entidades montam um modo que o nome do arquivo não ativa |
| `sem-spawn` | crítico | nenhum `info_player_start`/`info_player_deathmatch` |
| `spawn-de-um-time-so` | crítico | só um dos times consegue entrar |
| `poucos-spawns` | aviso | menos spawns que slots do servidor → telefrag no início do round |
| `fullbright` | aviso | lump de iluminação vazio: faltou RAD ou houve leak |
| `sem-buyzone` | aviso | mapa competitivo sem `func_buyzone` |
| `wad-nao-declarado` | aviso | usa textura de WAD e o worldspawn não lista nenhum |
| `mapa-pesado` | info | acima de 8 MB, jogador desiste no download |

O número de slots usado em `poucos-spawns` é configurável (padrão 32).

Dá para **exportar a planta em SVG** — serve para post, para o site do servidor ou para anexar
num pedido de correção ao mapper.

## Arquitetura

```
src-tauri/src/
  bsp/reader.rs     cursor little-endian com limites checados (nada de panic em arquivo torto)
  bsp/mod.rs        header + lumps: vértices, arestas, surfedges, faces, texinfo (vecs), texturas; e a decodificação de pixels (mip0 + paleta -> PNG)
  bsp/entities.rs   parser do lump de texto + resumo (spawns, WADs, skyname, modo pelas entidades, instâncias de .mdl)
  bsp/palette.rs    paleta indexada (8bpp + 256 cores) -> RGBA, compartilhada por BSP/WAD/.mdl
  bsp/wad.rs        abre os `.wad`/`.mdl` declarados pelo mapa e resolve o pixel/arquivo por nome
  bsp/sky.rs        decodifica os 6 lados do céu (TGA/BMP de gfx/env) e monta a caixa do skybox
  bsp/render.rs     planta baixa em SVG                    ← puro, sem I/O
  bsp/light.rs      atlas de lightmaps + uv por vértice    ← puro
  bsp/tree.rs       ponto -> folha da árvore BSP (spawn em sólido)
  diagnostics.rs    regras: entidades (varredura), BSP/limites, disco (WAD/modelos/sons)
  resources.rs      o que o mapa precisa (FastDL), `.res`, sky, modelos, sons
  audit.rs          diagnóstico da pasta inteira + duplicados por hash
  compare.rs        comparação lado a lado de dois mapas
  entity_list.rs    entidades com posição para o painel clicável
  radar.rs          radar verde (PNG/BMP + overview .txt)
  mdl/mod.rs        parser de modelo `.mdl` (GoldSrc studiomodel v10) — pose, animação, blends, controladores, bodygroups
  mdl/extra.rs      metadados: controladores, bodyparts, flags de material, remap de cor, hitboxes, attachments, eventos
  mdl/files.rs      arquivos irmãos na pasta do modelo: `nomeT.mdl` (texturas) e `nome01.mdl` (sequências)
  catalog.rs        varredura, diagnóstico, montagem do detalhe, da malha 3D e do visualizador avulso de .mdl
  main.rs           comandos Tauri, cache e settings (só com a feature `app`)
  lib.rs            núcleo sem interface (usado pelo app e pelo servidor MCP)
  mcp/, bin/mcp.rs  servidor MCP stdio; agentctx.rs exporta o contexto para agentes
  spr.rs wadlist.rs image.rs audio.rs pak.rs modfiles.rs nav.rs demo.rs textfiles.rs  formatos de Skins (modelos, WAD, sprites, áudio, arquivos)
  hulls.rs volumes.rs scene.rs bsp/lightstyle.rs bsp/texanim.rs  recursos extras do mapa
src/                frontend (Vite + TS puro)
  viewer3d.ts       cena Three.js: chunks, lightmap, PVS, frustum, foco em entidade
  lib/geometry.ts   malha -> chunks (puro; roda no Web Worker e nos testes)
  lib/pvs.ts        folha da câmera + faces visíveis (puro)
  lib/filters.ts    filtros/ordenação/anotações da galeria (puro)
  lib/report.ts     relatório de auditoria (MD/CSV/HTML) e lista de FastDL (puro)
  i18n.ts           pt/en (`lib/dict.ts`, `lib/findings-en.ts`)
  ui/               detalhe, comparação, auditoria, modal
  lib/mdl-controls.ts  slider -> byte, cache de quadros, plano de material, caixas por quadro do .mdl (puro)
  lib/dict-model.ts    textos do painel do .mdl (entra em `DICT`)
  lib/skins.ts      família de skin -> textura, projeção UV <-> pixel, hit-test de triângulo UV (puro)
  lib/skins-catalog.ts  subabas de modelo: prefixo v_/w_/p_, pasta models/player e time TR/CT/zumbi (puro)
  lib/dict-skins.ts textos da visualização de skins (entra em `DICT`)
  resources.ts      seção Skins: barra de animação e painel de controles do .mdl
  ui/skins/         seção Skins: shell das subabas + catálogo de modelos (pastas, busca, visualizador)
  model-viewer.ts   cena própria do .mdl (materiais, overlays, comparação de skins, seleção por clique)
  mdl-rig.ts        geometria do .mdl montada uma vez e reaproveitada por todas as skins (um "look" = jogo de materiais)
  skin-studio.ts    um único WebGLRenderer compartilhado que desenha as miniaturas 3D das skins
  ui/skins.ts       faixa de skins: galeria 3D, comparar, equipes; ui/uvmap.ts: mapa UV
```

Duas decisões que valem explicação:

**A varredura não lê o mapa inteiro.** Para montar o catálogo bastam o cabeçalho, o lump de
entidades e o modelo 0 — o resto é lido por `seek`. Sem isso, abrir uma pasta com 200 mapas
significaria ler ~1 GB do disco só para preencher os cards. A varredura ainda é paralelizada com
`std::thread::scope`, sem dependência externa.

**As miniaturas são preguiçosas e cacheadas.** O SVG só é gerado quando o card entra na tela
(`IntersectionObserver`) e é guardado em disco com chave `caminho + tamanho + mtime` — mapa
recompilado invalida sozinho.

## Sobre o formato

BSP versão 30 (GoldSrc). O parser trata o arquivo como binário hostil: todo acesso é checado e
vira `BspError` legível em vez de panic — mapa velho baixado de fórum tem lump torto com
frequência maior do que se imagina.

Textura com offset de pixel `0` vem de WAD externo; diferente de `0` está embutida no BSP. É essa
diferença que alimenta o aviso de WAD não declarado.

Faces com textura `aaatrigger`, `clip`, `null`, `origin`, `hint`, `skip` e `sky` são volumes
invisíveis e **não entram na planta** nem na malha visual (o módulo `volumes` as devolve à parte) — sem esse filtro, o desenho vira um borrão de caixas de
clip.

O índice `255` da paleta de 256 cores só é buraco (transparência) em textura cujo nome começa com
`{` — grade, cerca, vidro (convenção herdada do Quake). Numa textura comum o `255` é só mais uma
cor: tratá-lo sempre como buraco furava pixels que não deveriam sumir. Modelo `.mdl` usa a mesma
paleta, mas decide a transparência por uma *flag* da textura (`STUDIO_NF_MASKED`), não pelo nome.

## Modelos `.mdl` (props e o visualizador avulso)

Entidade do BSP com `model` terminando em `.mdl` (`cycler`, `monster_generic`, itens/armas
posicionados à mão) entra na vista 3D do mapa junto com o brush — sem controle novo, o toggle
"texturizado" já existente liga tudo junto. Modelo referenciado que não existe no disco é ignorado
em silêncio; o mapa continua abrindo normalmente.

A seção **Skins** (ao lado da galeria de mapas) navega o mod inteiro: as subabas `p_`, `v_`, `w_` filtram os
modelos pelo prefixo, **TRs**/**CTs**/**Zumbis** filtram pelo time reconhecido no nome ou na pasta
(`models/player/urban`), e **Recursos** mostra todos os `.mdl` agrupados por pasta. Cada subaba abre qualquer
`.mdl` isolado — jogador, arma, prop — fora do contexto de um mapa específico.

**Malha.** Cada `mesh` traz um stream de comandos de triângulo: `count > 0` é uma **tira**
(triângulos consecutivos `(i-1, i, i+1)`) e `count < 0` é um **leque** (pivô no primeiro vértice) —
o inverso do que o nome sugere. Ler o sinal trocado mantém a contagem de triângulos (`n-2` nos dois
casos) mas rasga a superfície: nos `.mdl` da Valve isso deixava 3 a 5 vezes mais arestas de fronteira
e buracos visíveis no modelo. Há teste com tira e leque de 4 vértices fixando a convenção.

**Câmera.** O visualizador enquadra a **pose animada** (no bind pose a origem fica nos pés, na
animação no quadril — enquadrar o bind deixava o modelo baixo e pequeno na tela) e tem atalhos de
vista: `3/4` (padrão), frente, lado, costas, topo e *enquadrar*.

**Animação.** O backend decodifica `mstudioseqdesc`/`mstudioanim_t` (RLE por canal, como no SDK) e entrega
a pose de mundo de cada bone por quadro (`load_sequence`, sob demanda e com cache); o frontend interpola
entre quadros e faz o skinning na CPU. A subaba de modelo (ou **Recursos**) tem seletor de sequência, tocar/pausar, quadro e
velocidade; sequências em `nome01.mdl` (grupo externo) são lidas da mesma pasta. **Skins**: as famílias
(`skinref`) aparecem num seletor quando há mais de uma. A composição de rotação segue `AngleQuaternion`
do SDK e é coberta por teste contra uma matriz de Euler independente; o `.mdl` de teste é sintético.

**Extras (contrato em `src/types/mdl-extra.ts`).** Tudo entra como campo novo e opcional no `MdlSummary`/`SeqFrames`;
as chamadas antigas continuam iguais. O painel ao lado da cena (subaba Recursos) usa todos eles — ver *Painel de controles* abaixo.

- *Bone controllers* (`controllers`): olhar/mira (slots 0..3) e boca (slot 4), com tipo, faixa e byte de
  repouso para montar sliders; `load_sequence(.., controllers)` aplica `StudioCalcBoneAdj` do SDK.
- *Blends* (`blend`): 2 e 4 blends como no SDK (`StudioSetupBones`); outros N (as sequências de mira/marcha do
  jogador, 9) usam uma extensão **não conferida contra o motor** (grade 3x3 ou eixo único).
  `seq_details[i].blend_*` traz o eixo e a faixa em graus de cada blender.
- *Bodygroups* (`bodyparts`, `load_model(.., body)`): um submodel por bodypart, `(body / base) % nummodels`.
- *Skin e cor de time* (`load_model(.., skin, top, bottom)`): texturas `DM_Base*` e `Remap?_TTT_UUU_BBB` têm a
  faixa de cima (160..191 em `DM_Base`) e a de baixo (192..223) refeitas com o matiz 0..255, regra do Xash3D.
- *Material* (`texture_info`): flatshade, chrome (UV por reflexão), fullbright, aditivo e máscara por textura.
- *Hitboxes, attachments e eventos*: em repouso no `MdlSummary` e por quadro em `load_sequence`
  (`attachments`, `hitboxes` com a flag); eventos com o nome dos IDs conhecidos (5001/5011/5021/5031 flash,
  5002 faísca, 5004 som, 1000..1010 script).
- *Texturas externas* (`nomeT.mdl`) e grupos de sequência (`nome01.mdl`) são achados na pasta do modelo, sem
  diferenciar maiúsculas; o mesmo vale para props do mapa.

**Painel de controles (seção Skins).** Só aparecem as seções que o modelo tem:

- *Bodygroups*: um seletor por bodypart (`bodyValue`); trocar recarrega o modelo (`load_model(path, body, top, bottom)`) sem mexer na câmera.
- *Cores de time*: matiz 0..255 de cima e de baixo (amostra do matiz; clicar nela volta à cor do modelo), só em modelo com
  textura de remap; a textura recolorida vem do backend.
- *Controladores*: um slider por bone controller (olhar/mira e boca). O slider anda em **bytes** (0..`input_max`) e o rótulo
  mostra o valor real em graus/unidades (`controllerValue`, espelho de `ControllerInfo::value_for`). Com todos no repouso
  o `load_sequence` vai sem `controllers` (mesma pose e mesma chave de cache de antes).
- *Blends*: 1 ou 2 sliders conforme `seq_details[i].blend_*` (o 9-blend do jogador usa 2 eixos); começam no zero da faixa
  quando ela o cruza (mira reta).
- *Sobreposições*: hitboxes (12 arestas por caixa, cor por grupo, interpoladas por quadro) e attachments (cruz + nome),
  ambas desenhadas por cima da malha. As caixas por quadro só são pedidas (`hitboxes: true`) com o overlay ligado.
- *Eventos* da sequência atual: lista com o quadro (clicar vai ao quadro) e um marcador na barra do tempo; o evento do
  quadro atual fica destacado. *Avisos* do modelo (`warnings`) e o aviso de textura externa ficam numa seção discreta.

Mexer num slider de controlador/blend recarrega só os quadros da sequência (`load_sequence`, com *debounce* de 120 ms no
arrasto, mantendo o quadro e o tocar/pausar); os quadros ficam em cache por (sequência, blend, controladores, hitboxes),
no máximo 48 entradas por modelo aberto. Materiais por textura (`texture_info`): *chrome* = matcap com a própria textura
(aproximação da reflexão do motor, sem o UV por pixel), *fullbright* = sem iluminação, *aditivo* = mistura aditiva sem
escrita de profundidade, *máscara* = `alphaTest`, *flatshade* = normal da face (o resto usa normais suavizadas por
vértice com corte de aresta a 60°, já que o `.mdl` não é lido com as normais próprias). Partes puras (slider -> byte,
chave de cache, plano de material, interpolação das caixas) em `src/lib/mdl-controls.ts`, com testes.

**Skins em 3D (faixa abaixo do visualizador).** *Galeria*: uma miniatura 3D girando por família de skin; um único
`WebGLRenderer` compartilhado (`skin-studio.ts`) desenha cada uma e copia o quadro para um canvas 2D, então o número de
famílias não cria contextos WebGL (são dois no total: visualizador + miniaturas). A geometria é montada uma vez por modelo
(`mdl-rig.ts`) e cada skin é só outro jogo de materiais sobre os mesmos buffers: como o `skin_families` do `MdlSummary`
já diz qual textura substitui qual, trocar de skin não chama `load_model(.., skin = i)` de novo (daria o mesmo `texindex`
trocado, mas reenviando todos os PNGs). Clicar aplica a skin; Shift+clique define o lado B da comparação.
*Comparar*: duas skins na mesma cena com a mesma câmera — lado a lado (dois quadros do mesmo canvas, camadas do Three.js) ou
antes/depois com divisor arrastável. *Equipes* (só se há textura de remap): o mesmo modelo com as cores de time azul
(140/140) e vermelha (0/0) lado a lado, via `load_model(.., top, bottom)` com cache; são cores de prévia do matiz do
Half-Life, não uma regra do CS (os jogadores do CS trocam de modelo por time em vez de remapear a cor). *Mapa UV*: a
textura escolhida com a malha UV por cima (zoom na roda, pan arrastando, xadrez, canal alpha, exportar PNG), lista com
tamanho e flags de cada textura; passar o mouse num triângulo o realça no mapa e no modelo, e clicar no modelo seleciona
a textura dele. Partes puras (tabela de skin, projeção UV, acerto de triângulo) em `src/lib/skins.ts`, com testes.

## Gates de CI

| workflow | quando | o que checa |
|---|---|---|
| `ci.yml` (CI) | pull requests | `web`: `bun install --frozen-lockfile`, `bun run typecheck`, `bun test`, `bun run build`. `rust`: `cargo clippy --all-targets -- -D warnings` e `cargo test` em `src-tauri` (com GTK/WebKit). `core`: o mesmo com `--no-default-features` (núcleo + MCP). |
| `validate.yml` (Validate) | pull requests e push em `master` | validação padrão do `juninmd/base-actions` (`reusable-validate.yml`): instalação, lint, testes e auditoria de dependências. |
| `security.yml` (Security Scan) | push em `master`, semanal (domingo) e manual | varredura de segurança do `juninmd/base-actions` (`reusable-security-scan.yml`). |
| `release.yml` (Release) | tag `v*` e manual | instaladores Tauri (Windows, Linux, macOS arm64/Intel) como release em rascunho e binário `bsp-museum-mcp` por plataforma. |

O gatilho de push do `ci.yml` aponta para `main`, mas a branch padrão é `master`: hoje o CI roda só em pull request.

## Testes

```bash
cd src-tauri && cargo test && cargo clippy --all-targets -- -D warnings
cargo test --no-default-features       # núcleo + MCP, sem GTK/WebKit
bun run typecheck && bun test        # filtros, PVS, geometria, i18n (paridade pt/en), relatórios
```

Os BSPs de teste são **montados byte a byte no próprio teste**: dá para descrever exatamente um
chão quadrado, uma parede, um teto, um surfedge invertido ou um lump corrompido sem carregar
nenhum arquivo de 3 MB no repositório. Cobrem versão de outro engine, arquivo truncado, lump
apontando para fora, lump com tamanho não múltiplo do registro, lixo aleatório (não pode dar
panic), e cada regra do diagnóstico.

### Refazendo as capturas de tela

```bash
export FIXTURE_DIR=/tmp/museu
(cd src-tauri && cargo test gerar_dados_das_capturas -- --ignored)   # mapas sintéticos + JSON do backend
(cd src-tauri && cargo test gerar_dados_extras_das_capturas -- --ignored)   # mapa dos recursos extras (v7-*)
bun run build
node scripts/screenshots.mjs $FIXTURE_DIR prints                      # precisa de `playwright`
```

O frontend real roda no Chromium; só o `invoke` do Tauri é trocado por um simulador que devolve os
JSON gerados pelo backend em Rust (WebGL via SwiftShader).

## Limitações

- GoldSrc v30 e Quake v29. BSP2 e Source (`de_dust2` do CS:S) não abrem — e o app diz isso em vez de fingir.
  No v29 as texturas saem em tons de cinza (a paleta global do Quake não acompanha o mapa) e o lightmap é
  monocromático.
- A vista 3D mostra as texturas embutidas no BSP **e** as dos WADs, mas precisa que os `.wad`
  estejam na pasta do mod (mesma de onde o mapa foi aberto) e que o `skyname` exista em
  `gfx/env`. Sem isso, entra cor neutra / domo de reserva.
- `.mdl`: o `motiontype` (`STUDIO_X/Y/Z` zeram a translação do bone de movimento) foi conferido no
  `StudioCalcRotations` do SDK, mas nenhum `.mdl` real passou pelo parser (só modelos sintéticos); `STUDIO_LX/LY/LZ`
  não fazem nada porque o próprio SDK multiplica o deslocamento por 0. Blends além de 2 e 4 (os 9 do jogador),
  o remap de cor (regra do Xash3D, não do `hw.dll`) e o nome de eventos fora do SDK não foram conferidos contra
  o jogo. Pivôs, `automove` e grafo de transição de sequências não são lidos. Controladores, bodygroups,
  remap e hitboxes já aparecem na vista do modelo (`src/lib/mdl-controls.ts`, `src/model-viewer.ts`), mas
  só foram conferidos com modelos sintéticos. Sprites (`.spr`) têm player 2D/3D próprio (ver
  `src/ui/explorer/sprite-player.ts`).
- Props `.mdl` dentro do mapa continuam em pose de repouso.
- Só entram na vista 3D do mapa entidades com `model` **explícito** no BSP — armas/itens que o
  jogo injeta por conta própria (sem essa chave) não têm como ser resolvidos só com o que está no
  `.bsp`.
- A conferência de WAD olha só o índice de nomes do arquivo (`textura-ausente`), não valida os pixels.
- Radar: o `ZOOM` do `overviews/<mapa>.txt` é uma estimativa para o enquadramento gerado; o calibre exato
  no cliente do CS não foi conferido contra o jogo — ajuste à mão se o radar desalinhar.
- Estilos de luz: o backend entrega os 4 slots de estilo de cada face (atlas principal + camadas) e os padrões
  (`m`, flicker, pulso…, os de fábrica de memória do `world.cpp` do SDK, sem conferir contra o motor); somar as
  camadas com os multiplicadores animados é feito por um shader do viewer (`MeshBasicMaterial` + `onBeforeCompile`).
  Luz ligável (estilo 32+) começa no estado do `spawnflags`; quem a liga é o gatilho, que o app não simula — no
  viewer, clicar na esfera da luz alterna o estilo.
- Vista 3D (camadas): a animação dos movers é uma demonstração em vai-e-vem (o jogo depende de gatilhos); sprites
  são sempre voltados para a câmera (os tipos `oriented` não são respeitados); o sol só afeta o sombreado do modo
  sem textura (o modo texturizado usa o lightmap); `renderamt` ausente vale 0, como informa o backend, e uma brush
  entity translúcida com 0 some; `renderfx` além de pulso, strobe e tremer não anima. Nada disso foi conferido
  contra o jogo, só contra o código do HLSDK de memória.
- Hulls de colisão (`headnode[1..3]`) saem como malha por varredura da árvore de clipnodes. Os tamanhos dos hulls
  (em pé ±16×±16×±36, grande ±32, agachado ±16×±16×±18) e a regra "só `contents == -2` é sólido" vêm de memória
  do SDK; nenhum mapa real foi usado para conferir. Hull de brush entity só sai pedindo o modelo.
- Zonas/volumes: só aparecem brush entities (`model "*N"`) e faces que o compilador guardou — `clip`/`null`/`hint`/
  `skip` costumam ser descartados na compilação, então o clip de verdade só se vê pelo hull.
- Texturas animadas: 10 quadros/s e a alternativa `+A…J` ligada por botão seguem o código do Quake; no GoldSrc não
  foi conferido. Rolagem `scroll*` (regra do `rendercolor`/`func_conveyor`) e fórmulas de porta/botão/plataforma
  (`lip`, eixo de rotação, deslocamento) foram escritas de memória do HLSDK — a VDC bloqueou a consulta automática.
- O brilho do lightmap usa fator fixo 2× (convenção *overbright* do Quake); não há gama configurável.
- Limites do motor (`limite-motor`) são valores de referência do compilador/engine, não lidos do jogo.
- Atualização automática (`tauri-plugin-updater`) não está ligada: exige par de chaves de assinatura e um
  endpoint de publicação do mantenedor. Os instaladores saem pelo workflow `release.yml`.
