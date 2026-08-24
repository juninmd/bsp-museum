# Research — texturas fora do jogo + suporte a `.mdl`

Issue: https://github.com/juninmd/bsp-museum/issues/1

## Goal

1. Corrigir texturas do BSP/WAD que não batem com o jogo real (relatado em `de_dust2`).
2. Adicionar suporte para carregar e desenhar modelos `.mdl` (GoldSrc studiomodel) na vista 3D —
   hoje props (`cycler`, `monster_*`, itens, armas no chão etc.) não aparecem, só a geometria de
   brush.

## Non-goals

- Não é para implementar animação de modelos (sequences/bones em movimento) — pose estática
  (bind pose / primeiro frame da sequência `idle`) já resolve "o prop aparece no lugar certo com a
  forma certa".
- Não é para renderizar sprites de partícula em runtime (`.spr` usado por `env_sprite`,
  muzzleflash etc.) neste ciclo — é um formato diferente do `.mdl` e não foi pedido explicitamente;
  fica como possível próximo passo, não nesta entrega.
- Não é para mudar a planta baixa (SVG) — o pedido é sobre a vista 3D WebGL.
- Não é para paralelizar/cachear modelos no `catalog::scan` (varredura rápida) — carregar `.mdl`
  só quando o mapa é aberto no detalhe/3D, igual ao WAD hoje.

## Acceptance criteria

- Textura de parede/chão que hoje aparece com buracos/transparência indevida em `de_dust2` (ou
  qualquer mapa) deixa de furar quando a textura **não** é do tipo transparente por convenção
  (nome sem `{`).
- Texturas `{`-prefixadas (grades, cercas, vidro) continuam furando onde o pixel de paleta é o
  índice 255 — isso é comportamento correto do GoldSrc e não pode regredir.
- Ao abrir um mapa com entidades que referenciam `.mdl` (`cycler`, `monster_generic`,
  `item_*`/`weapon_*` com override, etc.) e o arquivo existir na pasta do mod, o modelo aparece na
  vista 3D na posição/orientação (`origin`/`angles`) certa, com a textura do skin aplicada.
- Mapa sem os `.mdl` referenciados (arquivo ausente) continua abrindo normalmente — sem crash, sem
  erro bloqueante; a entidade some/vira um placeholder, igual ao comportamento atual de WAD
  ausente (cor neutra).
- `cargo test` continua verde; testes novos cobrem o parser de `.mdl` com arquivo montado
  byte-a-byte (mesmo padrão dos testes de BSP) e a regra de transparência (nome com/sem `{`).

## Codebase map

### Pipeline de textura hoje (a causa da divergência visual)

- `src-tauri/src/bsp/mod.rs:404` (`texture_image`, textura embutida no BSP) e
  `src-tauri/src/bsp/wad.rs:154` (`texture_image`, textura de WAD externo) — **ambos** aplicam,
  incondicionalmente, `let a = if idx == 255 { 0 } else { 255 };` para decidir alpha por pixel.
  - No GoldSrc, o índice 255 da paleta só é tratado como buraco (chroma-key) quando o **nome da
    textura começa com `{`** (convenção herdada do Quake: grades, cercas, vidro, folhagem). Numa
    textura comum (paredes, chão, etc.) o índice 255 é só mais uma cor da paleta — pode ser usado
    de propósito num tom escuro/detalhe. Furar esse pixel sempre faz partes de texturas normais
    (sem `{`) ficarem transparentes à toa, ou o inverso: uma textura `{` cuja paleta usa 255 para
    algo que devia ser opaco em outro contexto não é o caso aqui, o erro é sempre "buraco a mais".
  - Esse é exatamente o tipo de defeito que bate com "algumas sprites/texturas não ficam idênticas
    ao jogo" — o pixel errado vira furo em vez da cor da paleta.
  - Fix: o nome da textura já está disponível nas duas funções (`name` em `mod.rs:373` e o
    parâmetro implícito em `wad.rs`, onde falta repassar o nome para dentro de `texture_image` —
    hoje só é lido e descartado em `wad.rs:134`). Gate: `name.starts_with('{')`.
- `src/viewer3d.ts:44` — `minFilter: LinearMipmapLinearFilter` sem `magFilter` explícito (o padrão
  do Three.js já é `LinearFilter`) e sem `NearestFilter`: o GoldSrc original filtra em bilinear
  (não é o "pixelizado" do software renderer clássico), então isso não é a causa da divergência —
  só registrar como não-causa para não perseguir um fantasma.
- `src/viewer3d.ts:332` (`applyTransparent`) liga `transparent=true`/`alphaTest=0` na malha
  inteira quando o usuário marca "texturas transparentes" — isso é uma opção manual do usuário,
  ortogonal ao bug: mesmo com a opção desligada, o alpha=0 do pixel 255 já fez o buraco no PNG.

### Pipeline de textura, ponta a ponta (para saber onde plugar o fix e o `.mdl`)

- `src-tauri/src/bsp/mod.rs` — decodifica textura embutida no BSP (mip0 + paleta) → PNG base64.
- `src-tauri/src/bsp/wad.rs` — mesma decodificação para texturas vindas de `.wad` externo
  (`WadSet::for_map`/`resolve`).
- `src-tauri/src/catalog.rs:465` (`texture_slot`) — decide entre textura embutida e WAD, cacheia
  por índice cru, monta `MeshTexture { name, png }`.
- `src-tauri/src/catalog.rs:495` (`mesh`) — monta `MeshDetail` (posições, UVs, `texindex`,
  `textures`, `spawns`, `bounds`, `skybox`) que vai pro frontend via IPC do Tauri.
- `src/viewer3d.ts:37` (`buildTextures`) → `THREE.TextureLoader().load(t.png)` — consome os PNGs.
- `src/types.ts` — tipos espelhados do `MeshDetail`/`MeshTexture` (checar ao mudar o shape).
- `src/main.ts` — chama o comando Tauri e monta o viewer.

### Entidades (onde vive a referência a `.mdl`)

- `src-tauri/src/bsp/entities.rs:9` (`parse`) já extrai todo par chave/valor por entidade
  (`BTreeMap<String, String>`), incluindo `classname`, `origin` (via `origin_of`, `entities.rs:49`)
  — falta extrair `angles` (rotação, mesmo formato `"pitch yaw roll"`) e o campo `model` (nem toda
  entidade tem: monstros/`cycler_*` têm `model "models/algo.mdl"` explícito; itens/armas herdam o
  modelo do `.dll` do jogo — sem esse dado no BSP, não dá para saber o modelo sem uma tabela
  hardcoded do mod. Escopo realista: só desenhar entidades que **declaram `model` explicitamente**
  no BSP e cujo valor termina em `.mdl`).
- `src-tauri/src/catalog.rs:495` (`mesh`) é o lugar natural para, depois de montar a geometria do
  brush, iterar as entidades e anexar uma lista de "instâncias de modelo" (`path`, `origin`,
  `angles`) ao `MeshDetail`.
- `src-tauri/src/bsp/wad.rs:31` (`WadSet::for_map`) já resolve a pasta do mod a partir do caminho
  do `.bsp` (`map_path.parent().parent()`) — o mesmo caminho serve para achar `models/*.mdl`
  (padrão GoldSrc: `<mod>/models/<nome>.mdl`, referenciado no BSP já com esse prefixo relativo).

## Prior art

### Formato `.mdl` do GoldSrc (studiomodel v10)

- Valve Developer Community — página específica do MDL do GoldSrc:
  <https://developer.valvesoftware.com/wiki/MDL_(GoldSrc)> (bloqueou fetch direto por 403, mas o
  conteúdo é público e replicado em várias fontes secundárias abaixo).
- <https://developer.valvesoftware.com/wiki/StudioMDL_(GoldSrc)> — compilador oficial, formato de
  entrada (SMD) e limites (4080 tris por bodypart).
- Estrutura conhecida (`studiohdr_t`, de `studio.h` do SDK do Half-Life, é texto público e replicado
  em dezenas de loaders open-source — não há restrição de licença para *ler* o formato, só o SDK
  em si tem licença Valve para redistribuição do código-fonte deles):
  - Header: magic `"IDST"` (4 bytes) + `version` (i32, sempre `10` em MDL de retail) + `name[64]` +
    `length` (i32) + vetores (`eyeposition`, `min`, `max`, `bbmin`, `bbmax`) + `flags` (i32).
  - Depois do header vêm pares `(count, offset)` para cada seção: bones, bonecontrollers,
    hitboxes, sequences, sequencegroups, **textures** (`numtextures`/`textureindex` +
    `numskinref`/`numskinfamilies`/`skinindex`), **bodyparts** (`numbodyparts`/`bodypartindex`),
    attachments, sons, transitions.
  - Textura: `mstudiotexture_t` (`name[64]`, `flags`, `width`, `height`, `index` — pixels indexados
    numa paleta de 256 cores igual ao WAD/BSP, mesma decodificação que já existe em
    `mod.rs`/`wad.rs`; `flags & STUDIO_NF_MASKED` marca chroma-key igual ao `{` do BSP/WAD, mas
    aqui é um bit explícito, não convenção de nome).
  - Geometria: `bodypart` → `submodel` (`mstudiomodel_t`, tem várias variantes/LOD por bodypart) →
    `mesh` (`mstudiomesh_t`, aponta pra uma textura/skin) → stream de comandos de triângulo
    (`trivert_t`: índice de vértice, índice de normal, u, v) em fan/strip, terminado por `0`
    (mesmo padrão de trivial-strip do Quake).
  - Vértices/normais vivem por **bone** (`mstudiobone_t`) com uma matriz de transform; para pose
    estática (bind pose / frame 0 da sequência 0) dá pra aplicar só a transform base de cada bone
    sem precisar interpolar animação.
  - Fonte cruzada (não oficial, mas amplamente usada como referência de implementação real):
    parsers open-source de MDL v10 existem em C++ (HLMV/Jed's Half-Life Model Viewer, licença
    GPL — não copiar código, só usar como referência de layout) e em outras linguagens; o próprio
    Xash3D (engine GoldSrc-compatível open-source, licença GPL) tem `studio.h` com a struct
    completa.
- Convenção de transparência de textura de **mundo** (BSP/WAD), para contraste com o `.mdl`:
  documentada há décadas na comunidade Quake/Half-Life (mapping wikis, TWHL) — só texturas cujo
  nome começa com `{` usam paleta 255 como buraco; é convenção de **nome**, diferente do `.mdl`
  que usa um **flag** (`STUDIO_NF_MASKED`) na struct da textura.

### Como o próprio projeto já resolveu problema parecido

- `src-tauri/src/bsp/sky.rs` decodifica TGA/BMP do skybox sem depender de crate de imagem externo
  — mesmo padrão de "parser binário próprio, sem dependência pesada" que o `.mdl` deveria seguir
  (`REFERENCES.md:34-46` documenta a escolha).
- `src-tauri/src/bsp/reader.rs` já tem um `Cursor` little-endian com limites checados — reusar para
  o `.mdl` em vez de escrever outro parser de baixo nível.

## Constraints

- Rust edition/toolchain e dependências: `src-tauri/Cargo.toml` hoje só usa `serde`, `base64`,
  `flate2` (+ Tauri). Manter esse padrão — parser de `.mdl` deve ser código próprio (como
  `bsp/sky.rs`), não puxar crate de terceiros para não fugir do estilo do projeto.
- BSP versão suportada é só GoldSrc v30 (README) — `.mdl` correspondente é só **v10** (retail
  GoldSrc); rejeitar outra versão com erro legível, mesmo padrão de `BspError::BadVersion`.
- O parser precisa ser hostil-tolerante como o de BSP (`reader.rs`/`BspError`) — arquivo `.mdl`
  truncado ou corrompido não pode panicar o backend Tauri.
- Caminho do `.mdl` referenciado no BSP é relativo à pasta do mod (`models/...`) — resolver a
  partir da mesma raiz que `WadSet::for_map` já calcula (`map_path.parent().parent()`).
- Frontend: `MeshDetail`/`types.ts` precisam de um novo campo (ex.: `props: PropInstance[]`) e
  `viewer3d.ts` precisa instanciar uma malha por prop com sua própria textura — seguir o padrão de
  buckets por textura que já existe para o brush.
- Backward compatibility: mapas sem entidades `.mdl` (a maioria) não podem mudar de comportamento
  nem ficar mais lentos — carregar `.mdl` deve ser sob demanda (mesmo padrão preguiçoso do WAD),
  não na varredura (`catalog::scan`).

## Risks

- **Maior risco: formato `.mdl` não tem doc oficial fetchável diretamente** (403 na VDC) — a
  implementação vai se apoiar em `studiohdr_t` reconstruído de memória/fontes secundárias
  consistentes entre si. Mitigação: montar os testes do parser com arquivo `.mdl` sintético
  (mesmo padrão dos testes de BSP em `tests.rs`, campo a campo, com valores conhecidos) em vez de
  depender de um `.mdl` real do jogo — se o offset de algum campo estiver errado, o teste sintético
  aponta exatamente onde.
- Pose estática sem esqueleto (ignorar bone transforms) pode deformar modelos com múltiplos bones
  (ex.: personagens articulados) — aceitável para o objetivo (props parados: armas, itens,
  cyclers), mas precisa ficar documentado como limitação, não bug.
- Resolver `model` só quando declarado explicitamente na entidade deixa de fora armas/itens que o
  `.dll` do jogo injeta sem `model` no BSP — é uma limitação de dados (o BSP não carrega essa
  info), não algo corrigível só com o parser de `.mdl`.
- Textura de `.mdl` pode vir em paleta separada por skin (`numskinfamilies` > 1) — se o parser só
  ler a família 0, alguns modelos com skins alternativos (ex.: cor de time) mostram a cor errada;
  aceitável pro escopo (pose estática = skin default), mas registrar como não-goal explícito.
- Mudar o gate de transparência para `{` pode expor mapas que hoje "por acidente" pareciam certos
  porque o mapper nunca usou o índice 255 em textura normal — risco baixo (é o comportamento
  correto do motor), mas vale visualmente comparar um mapa com textura semi-transparente conhecida
  (ex.: `{`-grade em `de_dust2` ou outro mapa CS clássico) antes/depois do fix.

## Open questions (para o questionário do phase-plan)

1. Confirmar a hipótese: "sprites que não ficam idênticas" = texturas de parede/prop com buraco
   indevido (bug do índice 255 sem checar o prefixo `{`)? Ou o usuário quis dizer outra coisa por
   "sprite" (ex.: falta de brilho/lightmap, texture stretching/UV errado em alguma face
   específica)? Sem uma captura de tela do "errado" comparado ao jogo, o fix de transparência é a
   causa mais concreta e comprovável no código, mas vale confirmar antes de fechar o escopo.
2. Escopo do `.mdl`: só instanciar entidades que já têm campo `model` explícito no BSP (`cycler`,
   `monster_generic`, algumas `env_*`), ou também expor um jeito manual de o usuário escolher um
   `.mdl` avulso pra visualizar fora do contexto de um mapa? (Impacto: UI extra vs. zero UI nova.)
3. Pose: bind pose (sem aplicar nenhuma transform de bone) é aceitável, ou o modelo precisa
   respeitar ao menos a hierarquia de bones estática (sem animação, mas com a pose "default" correta
   de cada bone)? Aplicar bones estáticos é mais fiel, mas é trabalho a mais no parser.
   Recomendação: bind pose simples primeiro; hierarquia de bones fica como possível v2 se o
   resultado visual não for bom o suficiente.
4. `.spr` (sprites de partícula) ficou fora do non-goals — confirmar que não faz parte deste ciclo
   mesmo o usuário tendo dito "sprites" no pedido original (ambíguo com textura de parede).
