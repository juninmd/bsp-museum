# Referências

Documentação e fontes usadas para implementar a vista 3D e a decodificação de texturas.

## Formato BSP GoldSrc (Half-Life / CS 1.6)

- **Valve Developer Community — BSP (Source/GoldSrc)** — hierarquia de lumps, `dheader_t`,
  `miptex_lump_t`, `miptex_t`, `texinfo_t` e o modelo de coordenadas (Z-up):
  - <https://developer.valvesoftware.com/wiki/BSP> (estrutura geral)
  - <https://developer.valvesoftware.com/wiki/MIPTEX> (name, width, height, offsets[4] e mips)
  - <https://developer.valvesoftware.com/wiki/TEXINFO> (vecs[2][4], miptex, flags → mapeiam
    vértice do mundo em UV)

- **Quake/Half-Life `dmiptexlump_t`** — a tabela de offsets abre o lump de texturas; índice `-1`
  marca WAD externo. O mip0 é `offset + offsets[0]` e indexa uma **paleta de 256 cores** que fecha
  o lump (`3 * 256` bytes finais). Vale lembrar que o GoldSrc herda essa estrutura do Quake 1.

- **`texinfo.vecs` → UV**: `s = vecs[0][0]*x + vecs[0][1]*y + vecs[0][2]*z + vecs[0][3]`,
  `t = vecs[1][0]*x + vecs[1][1]*y + vecs[1][2]*z + vecs[1][3]`, normalizado pelo `width/height`
  da textura. É a mesma transformação que o motor usa para amostrar o conjunto de mip.

- **WAD (Quake/Half-Life `WAD2`/`WAD3`)** — cabeçalho de 12 bytes (`magic`, `numlumps`,
  `infotableofs`); cada lump de tipo `'C'` é uma textura `miptex` (name + mip chain + paleta de
  256 cores que fecha o lump). O BSP guarda o **nome** da textura de WAD (offset de mip `0`), o que
  permite resolver o pixel no `.wad` externo que o mapa declara no worldspawn:
  - <https://qtools.github.io/wadtool/format.html> (formato de WAD Quake/HL)

- **Skybox do GoldSrc/CS 1.6** — o worldspawn declara `skyname`; o motor monta uma caixa com
  `<skyname>_{up,dn,lf,rt,ft,bk}.tga` (ou `.bmp`) em `gfx/env`. A convenção varia entre
  `<skyname>up.tga` e `<skyname>_up.tga`. TGA: cabeçalho de 18 bytes, truecolor/RLE (tipos 2/3/10/11),
  24/32-bit, com origem controlada pelo bit `0x20` do dezcriptor.
  - <https://en.wikipedia.org/wiki/Truevision_TGA> (formato TGA)

## Formato MDL do GoldSrc (studiomodel v10)

- **Valve Developer Community — MDL/StudioMDL (GoldSrc)** — não foi possível buscar a página
  diretamente neste ciclo (a VDC bloqueou o fetch automatizado com HTTP 403); o layout de
  `studiohdr_t`/`mstudiobone_t`/`mstudiotexture_t`/`mstudiomodel_t`/`mstudiomesh_t`/
  `mstudioseqdesc_t` usado no parser (`src-tauri/src/mdl/mod.rs`) vem de `studio.h` do SDK do
  Half-Life — layout público, replicado em várias engines/loaders GoldSrc-compatíveis (ex.:
  Xash3D, licença GPL) e em ferramentas de terceiros:
  - <https://developer.valvesoftware.com/wiki/MDL_(GoldSrc)>
  - <https://developer.valvesoftware.com/wiki/StudioMDL_(GoldSrc)>

- **Textura do `.mdl`**: mesma paleta indexada de 256 cores do BSP/WAD (`mstudiotexture_t.width/
  height/index`, pixels no offset `index`, paleta logo depois). A transparência é decidida por uma
  *flag* na própria textura (`STUDIO_NF_MASKED`, `0x0040`), diferente da convenção de **nome**
  (`{`) do BSP/WAD.

- **Malha**: `bodypart` → `submodel` (`mstudiomodel_t`) → `mesh` (`mstudiomesh_t`, aponta pra uma
  skin) → stream de comandos de triângulo (`i16 count`; positivo = fan, negativo = strip, `0`
  termina o stream), cada vértice com `vertindex, normindex, s, t` — `s`/`t` em espaço de pixel da
  textura, normalizados dividindo por `width`/`height`.

- **Pose**: cada vértice pertence a um bone (`mstudiobone_t`); a posição final é a transform do
  bone (composta pela cadeia de pais, usando `value[0..6]` = posição + rotação fixas do arquivo)
  aplicada à posição local do vértice. Esta entrega só compõe essa pose de **repouso** — não
  decodifica `mstudioanim_t` (dados de animação por sequência), então trocar a sequência
  selecionada no visualizador não muda a geometria.

- **Risco residual**: a ordem de composição dos ângulos de rotação por bone
  (`rotation_matrix` em `mdl/mod.rs`) não pôde ser confirmada contra um `.mdl` real neste
  ambiente — só foi validada com um modelo sintético (montado byte a byte no teste). Se um modelo
  com mais de um bone sair com a pose torta, é o primeiro lugar a revisar.

## Renderização 3D no frontend

- **Three.js** (`three` + `@types/three`): cena, `PerspectiveCamera`, `WebGLRenderer`,
  `MeshStandardMaterial` (cor por altura via `vertexColors`) e `MeshLambert/Standard` com `map`
  para as texturas reais.
  - <https://threejs.org/docs/>
  - `OrbitControls`: <https://threejs.org/docs/#examples/en/controls/OrbitControls>
  - `DataTexture`/`TextureLoader` para subir as imagens.

- **PNG mínimo**: o gerador sobe a imagem como stream zlib (RFC 1950) via `flate2` — arquivo
  PNG válido e comprimido, sem depender de um crate de imagem completo.
  - <https://www.rfc-editor.org/rfc/rfc1950> (zlib)
  - Estrutura do container PNG: <https://www.w3.org/TR/PNG/>
