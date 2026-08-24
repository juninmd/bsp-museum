# Plan — texturas fora do jogo + suporte a `.mdl` (mapa + visualizador avulso)

Issue: https://github.com/juninmd/bsp-museum/issues/1 · Prototype: variante **A** + **D**
(`prototype.md`)

## Respostas do questionário

1. Causa confirmada: bug do índice de paleta 255 sem checar o prefixo `{`. `.spr` continua fora
   do escopo.
2. Pose: **bind pose** (sem aplicar animação de sequência). Ver nota técnica abaixo — isso ainda
   exige compor a hierarquia de bones na pose de repouso, só não decodifica dados de animação.
3. Visualizador avulso: aba/rota nova no app, ao lado da galeria de mapas.
4. Skin family + sequência já nesta entrega, com a ressalva técnica abaixo.

### Nota técnica: bind pose ainda precisa da hierarquia de bones

No MDL do GoldSrc cada vértice vive no espaço **local do seu bone** — diferente de um mesh já
"baked" em espaço de mundo. Não dá pra ignorar bones e ainda assim desenhar algo reconhecível:
mesmo um prop com um bone só precisa da transform (posição+rotação) daquele bone aplicada. O que a
resposta "bind pose simples" corta é a **animação** (blend de frames de uma sequência) — não a
composição da hierarquia de bones em si.

Consequência prática pro seletor de "sequência" do visualizador avulso: decodificar o array de
animação por bone (`mstudioanim_t`, valores comprimidos por *runs*) é um decoder à parte,
sensivelmente mais complexo que ler a pose de repouso (`mstudiobone_t.value`, 6 floats fixos por
bone). Ficaria dentro do orçamento de "bind pose simples" da resposta 2, mas contradiz decodificar
uma pose por sequência da resposta 4.

**Decisão de escopo (menor fatia que atende as duas respostas sem re-perguntar):** o seletor de
sequência mostra os **nomes** das sequências do `.mdl` (metadado real, lido do header — útil pra
saber que o modelo tem "idle", "deploy" etc.), mas trocar a sequência selecionada **não muda a
geometria** nesta entrega — o modelo sempre renderiza a pose de repouso (bind pose). Decodificar o
array de animação e aplicar por sequência vira item de backlog. Fica documentado na UI (tooltip
"pose de repouso — animação ainda não decodificada") pra não parecer bug.

## Non-goals confirmados/atualizados

- `.spr` (sprites de partícula) — fora do escopo.
- Animação de sequência (blend de frames) — backlog, ver nota acima.
- Modelos de arma/personagem injetados pelo `.dll` do jogo sem `model` explícito no BSP —
  continuam não aparecendo no mapa (limitação de dado, não de parser).

## Abordagem escolhida vs. alternativas rejeitadas

- **Props no mapa entram nos mesmos arrays de malha do brush** (`MeshDetail.positions/uvs/
  texindex/textures`), em vez de um campo `props: []` separado. Evidência: variante A foi escolhida
  justamente por "zero UI nova" — reusar o mesmo bucket-por-textura que `viewer3d.ts:57`
  (`makeBuckets`) já faz elimina qualquer código novo no frontend do mapa; o toggle "texturizado"
  que já existe (`viewer3d.ts:324`) já liga/desliga tudo que tem `map` no material.
  - Rejeitado: campo `MeshDetail.props` separado com pipeline próprio no frontend — mais código,
    sem ganho, já que não há UI por-prop nesta entrega (isso era a variante C, rejeitada).
- **Decodificação de pixel indexado (paleta 256 cores → RGBA) vira uma função compartilhada**,
  reusada por BSP embutido, WAD e agora MDL (skins). Evidência: hoje já está duplicada quase
  idêntica em `mod.rs:393-401` e `wad.rs:148-156`; adicionar uma terceira cópia pro MDL pioraria a
  duplicação exatamente no ponto que tem o bug. Consertar num lugar só reduz o risco de o fix da
  transparência ficar desalinhado entre os três caminhos.
- **Visualizador avulso é uma rota nova, não um modal dentro do mapa** — evidência: resposta 3.
  Reaproveita a mesma cena Three.js (câmera, luzes, orbit) do `viewer3d.ts`; extrair o setup comum
  pra não duplicar ~40 linhas de boilerplate de cena.
- **Reusar `mdl::` como módulo irmão de `bsp::`**, não dentro de `bsp/`: MDL não é parte do formato
  BSP (arquivo, versão e propósito diferentes), mas reusa o `Cursor`/estilo de erro de
  `bsp::reader` — import cruzado é aceitável, duplicar o cursor não.

## Passos

### 1. Extrair decodificação de paleta indexada compartilhada
- **Intenção:** um só lugar decide alpha por pixel, prep pro fix de transparência e pro MDL.
- **Arquivos:** novo `src-tauri/src/bsp/palette.rs` com `pub fn decode_indexed(pixels: &[u8], palette: &[u8; 768], transparent: bool) -> Vec<u8>` (RGBA); `mod.rs` e `wad.rs` passam a chamar essa função.
- **Verify:** `cd src-tauri && cargo build` (compila sem os dois blocos duplicados) e `cargo test` (suite atual continua verde — comportamento inalterado até o passo 2).
- **Resultado esperado:** zero mudança de output, só remoção de duplicação.

### 2. Corrigir o gate de transparência (o bug do relato)
- **Intenção:** só texturas `{`-prefixadas furam no índice 255.
- **Arquivos:** `src-tauri/src/bsp/mod.rs` (`texture_image`, repassa `name.starts_with('{')` como `transparent`), `src-tauri/src/bsp/wad.rs` (`texture_image`, hoje descarta o nome lido em `mip.fixed_str(16)` — passa a usá-lo).
- **Testes novos** em `src-tauri/src/tests.rs`: textura sintética `"{grade"` com pixel de paleta 255 → alpha 0; textura sintética `"wall01"` (sem `{`) com pixel de paleta 255 → alpha 255. Cobrir os dois caminhos (BSP embutido e WAD).
- **Verify:** `cd src-tauri && cargo test texture` (ou o nome dos testes novos) — verde.
- **Resultado esperado:** textura comum não fura mais; textura `{}` continua furando.

### 3. Entidades: extrair `angles` e instâncias de `.mdl`
- **Intenção:** saber onde e como posicionar cada prop.
- **Arquivos:** `src-tauri/src/bsp/entities.rs` — `angles_of(entity)` (mesmo padrão de `origin_of`, formato `"pitch yaw roll"`); struct `ModelInstance { classname: String, model: String, origin: [f32;3], angles: [f32;3] }`; `EntitySummary.model_instances: Vec<ModelInstance>`, populado em `summarize()` para toda entidade com chave `model` terminando em `.mdl` (case-insensitive).
- **Testes novos:** entidade sintética com `model`/`angles`/sem `angles` (default `[0,0,0]`) via `entities::parse` + `summarize`.
- **Verify:** `cargo test entities` — verde.
- **Resultado esperado:** `EntitySummary` de um mapa com `cycler`/`monster_generic` lista as instâncias corretas.

### 4. Parser de `.mdl` (GoldSrc studiomodel v10, pose de repouso)
- **Intenção:** decodificar header, bones (pose de repouso), bodyparts/meshes/triângulos e skins pra um formato pronto pro frontend.
- **Arquivos:** novo módulo `src-tauri/src/mdl/mod.rs` (+ `src-tauri/src/mdl/error.rs` se o erro não couber bem em `bsp::reader::BspError` — decidir na implementação; reusar `bsp::reader::Cursor`).
  - Header: magic `IDST`, `version == 10` (senão erro legível, mesmo padrão de `BadVersion`).
  - Bones: nome, parent, `value[6]` (pos xyz + rot xyz) — compor matriz de mundo por bone andando a cadeia de parents (sem aplicar sequência).
  - Bodyparts → submodels → vértices (posição local + índice de bone) → transformar pra espaço de mundo com a matriz do bone.
  - Meshes → stream de comandos de triângulo (fan/strip terminado em `0`) → triângulos com UV normalizado pela largura/altura da textura do skin.
  - Texturas: pixels indexados + paleta de 256 cores (reusa `bsp::palette::decode_indexed`, `transparent` = bit `STUDIO_NF_MASKED` da flag da textura, **não** o nome — convenção diferente do BSP/WAD).
  - Sequências: só nome e frame count (metadado), sem decodificar `mstudioanim_t`.
- **Testes novos:** `.mdl` sintético montado byte-a-byte (mesmo estilo de `tests.rs` pros BSPs) — 1 bone raiz, 1 bodypart/submodel/mesh, 1 textura pequena — cobrindo: geometria/UV esperada, versão errada rejeitada, arquivo truncado/corrompido não panica (`Result::Err`), lump apontando pra fora do arquivo tratado como erro.
- **Verify:** `cargo test mdl` — verde; `cargo clippy` sem warning novo.
- **Resultado esperado:** dado um `.mdl` válido pequeno, o parser devolve posições/UVs/texturas coerentes; dado lixo, devolve erro, nunca panic.

### 5. Wire de props no mesh do mapa
- **Intenção:** entidades com `.mdl` aparecem na vista 3D do mapa, sem controle novo (variante A).
- **Arquivos:** `src-tauri/src/catalog.rs` (`mesh`) — após montar a malha do brush, itera `ents.model_instances`; resolve o caminho do `.mdl` a partir da pasta do mod (generalizar `wad::find_wad` pra `find_asset(mod_dir, rel_path)` reutilizável); no sucesso, aplica `origin`/`angles` da entidade sobre os vértices do `mdl::Model` e concatena em `positions/uvs/texindex/textures` (mesmos arrays do brush — textures com índice deslocado pelo tamanho atual de `textures`); no erro (arquivo ausente/corrompido), pula a entidade silenciosamente (sem crashar o mapa).
- **Testes novos:** função de resolução+transform isolada e testável (`transform_instance(origin, angles, vertices) -> Vec<[f32;3]>`) com um caso conhecido (rotação 90° em yaw, por exemplo) checado por igualdade aproximada.
- **Verify:** `cargo test catalog` — verde; smoke manual: `bun run app`, abrir um `.bsp`+`.mdl` de teste numa pasta de mod local (fixture criada à mão pro smoke, não commitada) e conferir visualmente que o prop aparece na posição certa.
- **Resultado esperado:** mapa com prop `.mdl` existente mostra o prop; mapa sem `.mdl` ou com `.mdl` ausente abre normalmente, sem prop e sem erro bloqueante.

### 6. Comandos Tauri pro visualizador avulso
- **Intenção:** listar pastas/arquivos `.mdl` do mod e decodificar um modelo isolado.
- **Arquivos:** `src-tauri/src/main.rs` — `list_model_dirs(mod_dir) -> Vec<{name, count}>` (varre `models/`, `models/player/`, `models/weapons/` e outras subpastas de primeiro nível com `.mdl`), `list_models(dir) -> Vec<String>`, `load_model(path) -> MdlSummary` (posições/UVs/texindex/texturas + `skin_families: Vec<String>` + `sequences: Vec<String>`, reaproveitando o parser do passo 4).
- **Testes novos:** helper de varredura de pasta testado com `std::env::temp_dir()` (mesmo padrão sem dependência externa que o resto do projeto usa) — pasta com `.mdl` e lixo misturado, garante que só `.mdl` entra na lista.
- **Verify:** `cargo test list_model` — verde.
- **Resultado esperado:** dado um diretório de mod válido, retorna a árvore de pastas/arquivos `.mdl` esperada.

### 7. UI do visualizador avulso (frontend)
- **Intenção:** nova aba "Recursos" — navega pasta → arquivo → modelo isolado, com seletor de skin family e (metadado) sequência.
- **Arquivos:** `src/main.ts` (item de navegação novo, roteamento simples entre "mapas" e "recursos"), novo `src/resources.ts` (cena Three.js do modelo isolado — extrair setup de câmera/luzes/orbit compartilhado com `viewer3d.ts` pra não duplicar), `src/types.ts` (tipos espelhando `MdlSummary`), `src/style.css` (layout da aba nova, mesmo padrão visual dos painéis existentes).
- **Verify:** `bun run app` — smoke manual: abrir "Recursos", navegar até uma pasta com `.mdl`, abrir um modelo, trocar skin family (textura muda), conferir que trocar "sequência" não altera a geometria e mostra o aviso de pose de repouso.
- **Resultado esperado:** fluxo completo clicável, igual ao protótipo D, com o modelo real (não mock).

### 8. Documentação
- **Intenção:** README/REFERENCES não podem seguir dizendo que o app "não renderiza modelos".
- **Arquivos:** `README.md` (linha da seção "Limitações" sobre `.mdl`; nova seção curta sobre a aba Recursos), `REFERENCES.md` (fontes do formato MDL v10 já levantadas em `research.md`).
- **Verify:** leitura manual, sem comando.
- **Resultado esperado:** docs batendo com o comportamento real do app.

## Testes (resumo)

- `cargo test` cobrindo: gate de transparência (`{` vs sem `{`, BSP e WAD), `angles_of`/
  `model_instances` de entidades, parser `.mdl` sintético (feliz + versão errada + truncado +
  lump fora dos limites), transform de instância (origin+angles), varredura de pastas de modelo.
- Sem teste automatizado de ponta a ponta da UI (o projeto não tem harness de frontend) — os
  passos 5 e 7 fecham com smoke manual (`bun run app`), igual ao padrão já usado no projeto pra
  vista 3D.

## Rollback

Cada passo é um commit isolado. Passo 1–2 (fix de transparência) tem valor e risco isolados do
resto — pode ficar em produção mesmo que os passos de `.mdl` sejam revertidos. Passos 3–8 dependem
uns dos outros na ordem listada; reverter qualquer um deles não afeta a leitura de mapas sem
`.mdl` (todo o código novo é aditivo e falha de forma silenciosa/isolada por design).

## Confirmação necessária

Nenhum passo é destrutivo (sem deploy, sem escrita em banco, sem push automático). Commits e push
seguem a política padrão: peço confirmação antes de cada um, salvo indicação em contrário.

## Backlog (fora desta entrega)

- Animação de sequência real (decodificar `mstudioanim_t`, aplicar por frame).
- `.spr` (sprites de partícula).
- Aviso visual de `.mdl` referenciado e ausente (variante C) — hoje fica silencioso.
- Modelos de arma/jogador sem `model` explícito no BSP (dependeria de tabela hardcoded por mod).
