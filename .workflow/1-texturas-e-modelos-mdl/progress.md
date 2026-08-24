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

Em andamento.
