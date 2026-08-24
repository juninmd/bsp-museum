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

**Pendências que precisam de teste manual com jogo real instalado** (não dá pra automatizar sem um
`.mdl`/mapa de verdade no ambiente):
1. Abrir um mapa real com prop `.mdl` (`cycler`, `monster_generic`) e conferir visualmente se a
   posição/pose bate com o jogo — é onde a composição de rotação por bone não validada pode
   aparecer torta.
2. Comparar visualmente uma textura antes/depois do fix de transparência num mapa real (ex.
   `de_dust2`) — o teste sintético prova a lógica, não prova que era *essa* a causa do relato
   original.
3. Abrir a aba Recursos apontando pra uma pasta `cstrike/` de verdade e navegar
   `models/player`/`models/weapons`.
