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
