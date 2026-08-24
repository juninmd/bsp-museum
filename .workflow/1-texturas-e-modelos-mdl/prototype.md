# Prototype — decisão

Artefato: https://claude.ai/code/artifact/85cceb27-3921-4413-9f08-fa0939d7db3e

## Escolha

**Variante A** (props de `.mdl` do mapa aparecem automaticamente junto do toggle "texturizado"
existente, sem UI nova na vista 3D do mapa) — confirmada pelo usuário.

**+ escopo adicionado pelo usuário** (fora das 3 variantes originais, gerado como variante **D** no
mesmo round): uma rota/visualizador **avulso** de recursos `.mdl` — navega as pastas do mod
(`models/player`, `models/weapons`, `models/…`) e abre qualquer modelo solto (jogador, arma, prop)
para inspecionar fora do contexto de um `.bsp`, com troca de skin/sequência. Pedido textual do
usuário: "adicione feature para carregar os .mdl dos players, armas...etc.. não só no mapa, mas
para visualizar também os recursos".

## Rejeitadas

- **B (painel de props no mapa)** — a lista/toggle por classe de entidade dentro da vista do mapa
  não foi pedida; o usuário preferiu zero UI nova ali (variante A) e resolveu a necessidade de
  "ver os recursos soltos" com uma tela separada (D), não com um painel dentro do mapa.
- **C (fidelidade — bones + skin family + aviso de arquivo ausente)** — fica fora deste ciclo.
  O aviso de `.mdl` ausente ainda é útil (visto no research.md, risco de dado incompleto), mas
  entra como comportamento silencioso (prop não aparece, sem crash), não como UI dedicada.

## Consequência para o escopo

Duas entregas nesta issue, ambas reaproveitando o mesmo parser de `.mdl` (pose estática) e o mesmo
decodificador de textura (paleta 256 cores) já mapeados no `research.md`:

1. **No mapa**: `catalog::mesh` resolve `model`/`origin`/`angles` das entidades do BSP e anexa ao
   `MeshDetail`; `viewer3d.ts` instancia essas malhas junto da malha do brush, controladas pelo
   toggle "texturizado" que já existe — sem novo controle.
2. **Avulso**: um comando Tauri novo que recebe um caminho de `.mdl` (ou lista uma pasta do mod:
   `models/`, `models/player/`, `models/weapons/`) e devolve a malha decodificada pro frontend; uma
   tela/rota nova (fora da vista de um mapa específico) para escolher pasta → arquivo → ver o
   modelo isolado, com seletor de skin family e sequência (mesmo dado do MDL, sem precisar de
   bones animados — sequência aqui é só qual pose/skin mostrar, não animação em tempo real).

Isso amplia o "Non-goals" do research.md (que dizia não expor picker manual) — revisitar essa
seção no `phase-plan`: o picker deixou de ser opcional, é parte pedida do escopo.

Próximo: `phase-plan`.
