# Progress — implementação

Branch: `feat/texturas-e-modelos-mdl` · Issue: https://github.com/juninmd/bsp-museum/issues/1

## Passo 1+2 — decodificação de paleta compartilhada + gate de transparência

**Feito.** Commit `b5d0209`.

- `src-tauri/src/bsp/palette.rs` (novo): `decode_indexed(pixels, palette, transparent)`.
- `mod.rs`/`wad.rs`: `texture_image` agora passa `name.starts_with('{')` como `transparent`.

**Drift do plano:** ao escrever o teste de regressão pro gate de transparência (multi-textura, pra
provar que o índice certo da tabela é lido), achei um **segundo bug, mais grave**, não previsto no
`research.md`: `bsp::texture_image` fazia `cur.skip(4 + texindex * 4)` **depois** de já ter
consumido os 4 bytes do campo `count` — um offset a mais, então `texindex=0` devolvia os pixels da
textura **1**, e a última textura da tabela saía vazia/com lixo. Isso desalinha toda textura
embutida no BSP num mapa com mais de uma textura — plausivelmente uma causa tão ou mais relevante
pro relato original ("sprites não ficam idênticas ao jogo") quanto o gate de transparência. Corrigi
junto (mesmo commit) e cobri com teste (`texture_image_pega_a_entrada_certa_da_tabela_nao_a_seguinte`).

**Verify:**
```
cd src-tauri && cargo test
```
```
test result: ok. 55 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 0.01s
```
(61 testes no total contando os de `bsp::palette::tests` e `bsp::wad::tests`, que rodam junto.)

## Passo 3 — entidades: `angles` e instâncias de `.mdl`

**Feito.** Commit `26aa533`. `entities::angles_of` + `EntitySummary.model_instances`, testes
cobrindo ausência/malformação de ângulos e o filtro `.mdl` vs. `*N` (modelo de brush).

## Passo 4 — parser de `.mdl` (GoldSrc studiomodel v10, pose de repouso)

**Feito.** Commit `1ed9554`. Novo módulo `src-tauri/src/mdl/`, header/bones/texturas/bodyparts/
meshes/stream de triângulo decodificados; sequência só como metadado (nome), sem animação.

**Verify:**
```
cd src-tauri && cargo test
```
```
test result: ok. 67 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

**Risco residual documentado (não é drift do plano, é o risco que o `plan.md` já previa):** a
composição de rotação por bone (`rotation_matrix`) não pôde ser validada contra um `.mdl` real —
a Valve Developer Community bloqueou o fetch automatizado da doc oficial (HTTP 403) neste
ambiente. Coberto só por teste sintético (bone com translação pura, sem rotação, pra não depender
da ordem dos eixos estar certa). Documentado em `mdl/mod.rs`, `README.md` e `REFERENCES.md`.
**Precisa de smoke manual com um `.mdl` real antes do merge** pra confirmar visualmente.

## Passo 5 — props no mesh do mapa

**Feito.** Commit `c654331`. `wad::mod_dir_of`/`find_asset` generalizados; `catalog::mesh` resolve
`ModelInstance` → `.mdl` → concatena nos arrays existentes (`positions/uvs/texindex/textures`).
`.mdl` ausente/corrompido pulado em silêncio.

**Verify:**
```
cd src-tauri && cargo test
```
```
test result: ok. 69 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 0.02s
```
Testes de integração: `mesh_inclui_prop_de_entidade_com_model_mdl` (prop sintético entra na malha),
`mesh_ignora_mdl_ausente_sem_quebrar_o_mapa`.

## Passo 6 — comandos Tauri pro visualizador avulso

**Feito.** Commit `47e3c1f`. `catalog::list_model_dirs/list_models/load_model` + 3 comandos Tauri
finos em `main.rs`.

**Verify:**
```
cd src-tauri && cargo test
```
```
test result: ok. 71 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

## Passo 7 — UI da aba Recursos

**Feito.** Commit `b0682e0`. Nav Mapas/Recursos, fluxo pasta → subpastas com `.mdl` → arquivos →
modelo isolado. `resources.ts` reusa a cena orbitável de `viewer3d.ts` (`mount3D`) montando um
`MeshDetail` sintético — sem duplicar setup de Three.js.

**Verify:**
```
bun run typecheck   # tsc --noEmit — sem erro
bun run build        # tsc --noEmit && vite build — build ok (556 KB, aviso de chunk grande
                      # já existia antes desta mudança, é o three.js)
```

**Drift do plano:** o `plan.md` previa expor seletor de **skin family** (texturas alternativas,
ex. cor de time) no visualizador avulso. O parser do passo 4 só decodifica o **conjunto base** de
texturas (`mstudiotexture_t`), não a tabela `skinindex` de famílias alternativas — decodificar
isso é trabalho extra não orçado nesta entrega. Cortado do escopo (mesmo princípio já usado pra
sequência: expor metadado é uma coisa, decodificar de verdade é outra). Documentado como backlog
em `README.md`, `plan.md` e no commit. A maioria dos props estáticos (armas, barris, itens) só tem
mesmo uma família — quem perde é um eventual modelo de personagem com skin de time, que não troca
de cor no visualizador avulso.

## Passo 8 — documentação

**Feito.** `README.md` (arquitetura, seção nova sobre `.mdl`/Recursos, limitações atualizadas) e
`REFERENCES.md` (fontes do formato MDL v10, risco residual da rotação por bone).

## Estado final

Branch `feat/texturas-e-modelos-mdl`, commits `b5d0209..b0682e0` + este de docs. `cargo test`
verde (71 testes + 6 ignorados que precisam de uma pasta de mapas real — mesmo padrão que já
existia). `bun run build` verde. Nenhum push feito ainda — aguardando autorização explícita antes
de empurrar a branch e abrir o PR (política do projeto: nada de push sem confirmação).

## Smoke manual com jogo real (feito depois do PR aberto)

Rodei `bun run app` de verdade contra a instalação local de Half-Life/CS 1.6 (Steam) e dirigi a UI
(`de_dust2`, `c1a0e` — "Anomalous Materials" — e a aba Recursos com `cstrike/models`):

1. **`de_dust2` texturizado**: abre sem buraco de transparência indevido — os dois bugs do passo
   1+2 (gate `{` e o off-by-one) resolvem o relato original.
2. **Props no mapa (`c1a0e`)**: as entidades `fungus`/`hair` (decorativas do Xen) aparecem como
   malhas próprias, separadas do brush, na posição certa perto do spawn CT — confirma o passo 5.
3. **Aba Recursos**: abriu `models/player/terror/terror.mdl` isolado, com textura real (colete,
   máscara, luvas, botas) e a pose de repouso **coerente** — braços e pernas no lugar certo, sem
   deformação visível. Isso valida na prática a composição de rotação por bone
   (`rotation_matrix`) que o `plan.md`/`README.md` marcavam como risco não confirmado: funcionou
   num modelo real de múltiplos bones, não só no sintético. Ainda vale manter a ressalva nos docs
   (não testei todos os modelos, só este), mas o risco baixou de "não validado" pra "validado num
   caso real".
4. **Bug real achado nesse smoke** (fora do que os testes automatizados cobrem — UI pura):
   `switchView()` setava `gallery.hidden = true`/`resources.hidden = true` mas `.gallery {
   display: grid }` tem a mesma especificidade do `[hidden]{display:none}` do user-agent, e regra
   de autor sempre vence empate com UA — a galeria de mapas continuava desenhada por baixo da aba
   Recursos. Corrigido (`src/style.css`, `.gallery[hidden]{display:none}`) e empurrado como commit
   `5e38afe`.

Prints enviados ao usuário no chat (não commitados no repo — ficam em
`.workflow/1-texturas-e-modelos-mdl/shot-*.png`, local, fora do PR).

## Rodada de iteração 1 — "a visualização da skin precisa ser melhorada"

Pedido do usuário depois de ver os prints do `terror.mdl` na aba Recursos. Diagnóstico a partir do
próprio print: o modelo saía quase todo escuro (a iluminação do visualizador é a mesma "sol +
hemisférica" ajustada pra mapa inteiro — pra um personagem pequeno, metade dele fica na sombra) e
pequeno demais no quadro (margem de câmera de mapa, 1.4x, sobra tela de céu à toa num objeto só).

**Feito** (commit a seguir):
- `viewer3d.ts`: novo `Mount3DOptions.inspect` — quando ligado, troca a luz de "sol único de cena
  externa" por um rig mais uniforme (ambiente alta + 3 luzes de preenchimento), aperta a margem de
  câmera (1.4x → 1.05x) e tenta soldar vértices coincidentes (`mergeVertices` do three.js) antes de
  calcular a normal, pra suavizar o sombreamento. Não mexe no modo mapa (chamada default do
  `main.ts` não passa `opts`, comportamento inalterado — confirmado nos prints de `de_dust2`/`c1a0e`
  depois da mudança, sem diferença visual ali).
- `resources.ts`: passa `{ inspect: true }` pro `mount3D`.

**Verify:** `bun run typecheck` + `bun run build` — ok, sem erro. Sem mudança no backend Rust
(`cargo test` continua 71/71 do estado anterior, não precisou rodar de novo).

**Smoke manual** (mesmo `terror.mdl`, mesma pasta): comparei o antes/depois lado a lado —
- Câmera mais próxima e luz mais uniforme deixam a skin claramente mais visível e legível à
  distância normal de visualização (print `shot-19-terror-final.png` vs. o antigo
  `shot-14-terror-zoom.png`) — isso resolve a parte prática do pedido.
- **Limitação que ficou**: de perto (zoom forçado bem além do normal,
  `shot-20-terror-close.png`), o modelo ainda mostra facetas — `mergeVertices` só suaviza onde o
  `.mdl` reusa exatamente o mesmo vértice com o mesmo UV entre triângulos vizinhos; boa parte da
  malha declara vértice de novo a cada fan/strip mesmo sem costura real. Documentado no
  `plan.md` como backlog (decodificar `mstudiomodel_t.normindex` de verdade, em vez de recalcular).
