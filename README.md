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
bun run app:build    # instalador NSIS em src-tauri/target/release/bundle
```

Precisa de Rust (MSVC no Windows) e do WebView2 — que já vem no Windows 11.

Na primeira execução, clique em **escolher pasta…** e aponte para a pasta com os `.bsp`
(subpastas incluídas). A escolha fica salva para a próxima abertura.

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
  mdl/mod.rs        parser de modelo `.mdl` (GoldSrc studiomodel v10) — pose de repouso
  catalog.rs        varredura, diagnóstico, montagem do detalhe, da malha 3D e do visualizador avulso de .mdl
  main.rs           comandos Tauri, cache e settings
src/                frontend (Vite + TS puro)
  viewer3d.ts       cena Three.js: orbit, cor por altura ou textura real, skybox
  resources.ts      visualizador avulso de .mdl — reusa a mesma cena de viewer3d.ts
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
invisíveis e **não entram na planta** — sem esse filtro, o desenho vira um borrão de caixas de
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

A aba **Recursos** (ao lado da galeria de mapas) navega as pastas do mod (`models/player`,
`models/weapons`, `models/`) e abre qualquer `.mdl` isolado — jogador, arma, prop — fora do
contexto de um mapa específico.

O parser decodifica só a **pose de repouso** (bind pose): a hierarquia de bones é composta a
partir dos valores fixos do arquivo, sem aplicar nenhuma sequência de animação. O nome de cada
sequência aparece como metadado no visualizador avulso, mas trocar a sequência selecionada não
muda a pose desenhada nesta versão — decodificar a animação de verdade fica pro backlog.

## Testes

```bash
cd src-tauri && cargo test
```

Os BSPs de teste são **montados byte a byte no próprio teste**: dá para descrever exatamente um
chão quadrado, uma parede, um teto, um surfedge invertido ou um lump corrompido sem carregar
nenhum arquivo de 3 MB no repositório. Cobrem versão de outro engine, arquivo truncado, lump
apontando para fora, lump com tamanho não múltiplo do registro, lixo aleatório (não pode dar
panic), e cada regra do diagnóstico.

## Limitações

- Só GoldSrc v30. BSP2/Source (`de_dust2` do CS:S) não abre — e diz isso em vez de fingir.
- A vista 3D mostra as texturas embutidas no BSP **e** as dos WADs, mas precisa que os `.wad`
  estejam na pasta do mod (mesma de onde o mapa foi aberto) e que o `skyname` exista em
  `gfx/env`. Sem isso, entra cor neutra / domo de reserva.
- `.mdl` renderiza só a pose de repouso (sem animação/sequência) e só a família base de skins —
  modelo de jogador com skin alternativa de time não troca de cor. Sprites de partícula (`.spr`)
  continuam fora do escopo.
- A composição da rotação por bone (hierarquia do `.mdl`) não foi validada contra um arquivo real
  do jogo neste ciclo — se um modelo com mais de um bone sair torto, é o primeiro lugar a revisar
  (`src-tauri/src/mdl/mod.rs`, função `rotation_matrix`).
- Só entram na vista 3D do mapa entidades com `model` **explícito** no BSP — armas/itens que o
  jogo injeta por conta própria (sem essa chave) não têm como ser resolvidos só com o que está no
  `.bsp`.
- Não valida WAD de verdade (não abre o arquivo `.wad` quando não declarado).
