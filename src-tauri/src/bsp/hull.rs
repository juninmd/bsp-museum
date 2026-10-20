//! Hulls de colisão: transforma a árvore de `clipnodes` em polígonos.
//!
//! O GoldSrc não colide o jogador contra a geometria visual: cada modelo tem 3 árvores
//! extras (`headnode[1..4]`) que já vêm **expandidas** pelo tamanho do corpo — testar o
//! *ponto* de origem do jogador contra elas equivale a testar a caixa inteira contra a
//! parede. Desenhar essas árvores mostra por onde o jogador realmente passa.
//!
//! Tamanhos (SDK do Half-Life, `pm_shared.c`: `VEC_HULL_*` e `VEC_DUCK_HULL_*`; hull 2 é o
//! "grande" de `pm_shared`), conferidos de memória do SDK — não contra um `.bsp` real:
//!
//! | hull | corpo                              | mins            | maxs          |
//! |------|------------------------------------|-----------------|---------------|
//! | 0    | ponto (árvore visual, `nodes`)     | 0 0 0           | 0 0 0         |
//! | 1    | em pé                              | -16 -16 -36     | 16 16 36      |
//! | 2    | grande (monstros grandes/barricada)| -32 -32 -32     | 32 32 32      |
//! | 3    | agachado                           | -16 -16 -18     | 16 16 18      |
//!
//! (O Quake usa outros valores: hull 1 = -16 -16 -24 / 16 16 32 e hull 2 = -32 -32 -24 /
//! 32 32 64. Aqui vale a tabela do GoldSrc, também para BSP v29.)
//!
//! Algoritmo ("hull to polygons"): para cada clipnode, um quadrado enorme no plano dele é
//! recortado pelos planos dos ancestrais (a "célula" do nó). O pedaço resultante separa o
//! subespaço da frente do de trás; descendo cada subárvore e partindo o polígono pelos
//! planos que encontra, cada fragmento fica com o `contents` dos dois lados. Só interessa
//! onde um lado é sólido e o outro não — essa é a superfície colidível.
//!
//! Tudo com orçamento: árvore torta (ciclo, DAG explosivo, índice fora da faixa) para por
//! limite de trabalho e devolve o que já montou, nunca panic nem laço infinito.

use super::Bsp;
use serde::Serialize;

/// `contents` sólido numa folha de clipnode.
pub const CONTENTS_SOLID: i32 = -2;

/// Caixa do corpo de um hull de colisão.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct HullSize {
    pub hull: u8,
    pub name: &'static str,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

pub const HULL_SIZES: [HullSize; 3] = [
    HullSize { hull: 1, name: "em pé", mins: [-16.0, -16.0, -36.0], maxs: [16.0, 16.0, 36.0] },
    HullSize { hull: 2, name: "grande", mins: [-32.0, -32.0, -32.0], maxs: [32.0, 32.0, 32.0] },
    HullSize { hull: 3, name: "agachado", mins: [-16.0, -16.0, -18.0], maxs: [16.0, 16.0, 18.0] },
];

/// Tamanho do hull `1..=3`.
pub fn hull_size(hull: usize) -> Option<HullSize> {
    HULL_SIZES.get(hull.checked_sub(1)?).copied()
}

/// Resultado da varredura: polígonos convexos (anti-horários vistos do lado vazio) e se o
/// orçamento de trabalho cortou o resultado.
#[derive(Debug, Clone, Default)]
pub struct HullPolys {
    pub polygons: Vec<Vec<[f32; 3]>>,
    pub truncated: bool,
}

type V3 = [f64; 3];
/// plano de um ancestral: normal, distância, e se a célula fica na frente dele
type CellPlane = (V3, f64, bool);

const EPS: f64 = 0.01;

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn lerp(a: V3, b: V3, t: f64) -> V3 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Parte o polígono pelo plano `n·p = d`: (frente, trás). Pontos sobre o plano contam para
/// os dois lados quando o polígono é cortado.
fn split(poly: &[V3], n: V3, d: f64) -> (Vec<V3>, Vec<V3>) {
    let dist: Vec<f64> = poly.iter().map(|p| dot(n, *p) - d).collect();
    let any_front = dist.iter().any(|&x| x > EPS);
    let any_back = dist.iter().any(|&x| x < -EPS);
    if !any_back {
        return (poly.to_vec(), Vec::new());
    }
    if !any_front {
        return (Vec::new(), poly.to_vec());
    }
    let mut front = Vec::with_capacity(poly.len() + 2);
    let mut back = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let j = (i + 1) % poly.len();
        let (a, b, da, db) = (poly[i], poly[j], dist[i], dist[j]);
        if da >= -EPS {
            front.push(a);
        }
        if da <= EPS {
            back.push(a);
        }
        if (da > EPS && db < -EPS) || (da < -EPS && db > EPS) {
            let p = lerp(a, b, da / (da - db));
            front.push(p);
            back.push(p);
        }
    }
    (front, back)
}

fn area(poly: &[V3]) -> f64 {
    let mut sum = [0.0f64; 3];
    for i in 1..poly.len().saturating_sub(1) {
        let a = [poly[i][0] - poly[0][0], poly[i][1] - poly[0][1], poly[i][2] - poly[0][2]];
        let b = [poly[i + 1][0] - poly[0][0], poly[i + 1][1] - poly[0][1], poly[i + 1][2] - poly[0][2]];
        let c = cross(a, b);
        for k in 0..3 {
            sum[k] += c[k];
        }
    }
    dot(sum, sum).sqrt() * 0.5
}

/// Quadrado de meia-aresta `size` sobre o plano, anti-horário visto de `+n`.
fn base_quad(n: V3, d: f64, center: V3, size: f64) -> Vec<V3> {
    // ponto do plano mais próximo do centro
    let off = dot(n, center) - d;
    let o = [center[0] - n[0] * off, center[1] - n[1] * off, center[2] - n[2] * off];
    // eixo auxiliar: o menos alinhado com a normal
    let aux = if n[0].abs() <= n[1].abs() && n[0].abs() <= n[2].abs() {
        [1.0, 0.0, 0.0]
    } else if n[1].abs() <= n[2].abs() {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let u = cross(aux, n);
    let ul = dot(u, u).sqrt().max(1e-12);
    let u = [u[0] / ul, u[1] / ul, u[2] / ul];
    let v = cross(n, u); // u × v = n
    let pt = |a: f64, b: f64| -> V3 {
        [
            o[0] + (u[0] * a + v[0] * b) * size,
            o[1] + (u[1] * a + v[1] * b) * size,
            o[2] + (u[2] * a + v[2] * b) * size,
        ]
    };
    vec![pt(-1.0, -1.0), pt(1.0, -1.0), pt(1.0, 1.0), pt(-1.0, 1.0)]
}

struct Tree<'a> {
    bsp: &'a Bsp,
}

impl Tree<'_> {
    /// Plano normalizado (normal unitária, distância ajustada) do clipnode.
    fn plane(&self, node: usize) -> Option<(V3, f64)> {
        let cn = self.bsp.clipnodes.get(node)?;
        let p = self.bsp.planes.get(usize::try_from(cn.plane).ok()?)?;
        let n = [f64::from(p.normal[0]), f64::from(p.normal[1]), f64::from(p.normal[2])];
        let len = dot(n, n).sqrt();
        if !len.is_finite() || len < 1e-6 || !p.dist.is_finite() {
            return None;
        }
        Some(([n[0] / len, n[1] / len, n[2] / len], f64::from(p.dist) / len))
    }

    /// Desce `poly` a partir de `start`, partindo-o pelos planos; cada fragmento sai com o
    /// `contents` da folha onde caiu. `dir` desempata o polígono coplanar com um plano:
    /// vai para o lado para onde `dir` aponta. `false` = orçamento esgotado.
    fn descend(&self, poly: Vec<V3>, start: i32, dir: V3, budget: &mut usize, out: &mut Vec<(Vec<V3>, i32)>) -> bool {
        let mut stack = vec![(poly, start)];
        while let Some((p, at)) = stack.pop() {
            if *budget == 0 {
                return false;
            }
            *budget -= 1;
            if at < 0 {
                out.push((p, at));
                continue;
            }
            let Some((n, d)) = self.plane(at as usize) else { continue };
            let cn = self.bsp.clipnodes[at as usize];
            let dists: Vec<f64> = p.iter().map(|q| dot(n, *q) - d).collect();
            let coplanar = dists.iter().all(|x| x.abs() <= EPS);
            if coplanar {
                let side = usize::from(dot(n, dir) < 0.0);
                stack.push((p, i32::from(cn.children[side])));
                continue;
            }
            let (front, back) = split(&p, n, d);
            if front.len() >= 3 && area(&front) > 1e-4 {
                stack.push((front, i32::from(cn.children[0])));
            }
            if back.len() >= 3 && area(&back) > 1e-4 {
                stack.push((back, i32::from(cn.children[1])));
            }
        }
        true
    }
}

impl Bsp {
    /// Superfície sólido/vazio do hull `1..=3` do modelo `model`, como polígonos convexos.
    /// `None` quando o modelo não existe, o hull é inválido ou a árvore está vazia.
    /// `max_polygons` limita a saída (`truncated` avisa quando cortou).
    pub fn hull_polygons(&self, model: usize, hull: usize, max_polygons: usize) -> Option<HullPolys> {
        hull_size(hull)?;
        let m = self.models.get(model)?;
        let head = m.hull_heads[hull - 1];
        if head < 0 || self.clipnodes.is_empty() || (head as usize) >= self.clipnodes.len() {
            return None;
        }
        let tree = Tree { bsp: self };

        // caixa de recorte: limites do modelo, com folga maior que o maior corpo
        let pad = 96.0f64;
        let lo = [m.mins[0], m.mins[1], m.mins[2]].map(|v| f64::from(v) - pad);
        let hi = [m.maxs[0], m.maxs[1], m.maxs[2]].map(|v| f64::from(v) + pad);
        if lo.iter().chain(hi.iter()).any(|v| !v.is_finite()) || (0..3).any(|k| hi[k] < lo[k]) {
            return None;
        }
        let center = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
        let size = (0..3).map(|k| hi[k] - lo[k]).fold(0.0f64, f64::max) * 2.0 + 64.0;
        // 6 planos da caixa: (normal, dist, manter a frente?)
        let mut cell_box: Vec<CellPlane> = Vec::with_capacity(6);
        for k in 0..3 {
            let mut n = [0.0; 3];
            n[k] = 1.0;
            cell_box.push((n, hi[k], false));
            cell_box.push((n, lo[k], true));
        }

        let max_visits = self.clipnodes.len().saturating_mul(4).saturating_add(1024);
        let mut budget = max_visits.saturating_mul(64).clamp(100_000, 40_000_000);
        let mut visits = 0usize;
        let mut result = HullPolys::default();
        // pilha de nós com a célula (planos dos ancestrais e lado escolhido)
        let mut stack: Vec<(i32, Vec<CellPlane>)> = vec![(head, Vec::new())];

        'walk: while let Some((node, cell)) = stack.pop() {
            if node < 0 {
                continue;
            }
            visits += 1;
            if visits > max_visits || cell.len() > 4096 {
                result.truncated = true;
                break;
            }
            let Some((n, d)) = tree.plane(node as usize) else { continue };
            let cn = self.clipnodes[node as usize];

            // polígono do nó: quadrado enorme recortado pela caixa e pelos ancestrais
            let mut poly = base_quad(n, d, center, size);
            for &(cn_n, cn_d, keep_front) in cell_box.iter().chain(cell.iter()) {
                let (f, b) = split(&poly, cn_n, cn_d);
                poly = if keep_front { f } else { b };
                if poly.len() < 3 {
                    break;
                }
            }

            if poly.len() >= 3 && area(&poly) > 1e-3 {
                let mut front_frags = Vec::new();
                if !tree.descend(poly, i32::from(cn.children[0]), n, &mut budget, &mut front_frags) {
                    result.truncated = true;
                    break;
                }
                let back_dir = [-n[0], -n[1], -n[2]];
                for (frag, cf) in front_frags {
                    let mut back_frags = Vec::new();
                    if !tree.descend(frag, i32::from(cn.children[1]), back_dir, &mut budget, &mut back_frags) {
                        result.truncated = true;
                        break 'walk;
                    }
                    for (g, cb) in back_frags {
                        let (solid_front, solid_back) = (cf == CONTENTS_SOLID, cb == CONTENTS_SOLID);
                        if solid_front == solid_back || g.len() < 3 || area(&g) < 0.05 {
                            continue;
                        }
                        let mut pts: Vec<[f32; 3]> = g.iter().map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect();
                        if solid_front {
                            // a superfície olha para o lado vazio (trás): inverte o enrolamento
                            pts.reverse();
                        }
                        if result.polygons.len() >= max_polygons {
                            result.truncated = true;
                            break 'walk;
                        }
                        result.polygons.push(pts);
                    }
                }
            }

            let mut front_cell = cell.clone();
            front_cell.push((n, d, true));
            let mut back_cell = cell;
            back_cell.push((n, d, false));
            stack.push((i32::from(cn.children[1]), back_cell));
            stack.push((i32::from(cn.children[0]), front_cell));
        }
        Some(result)
    }

    /// Contents do ponto no hull `1..=3` do modelo (`-1` vazio, `-2` sólido…). Útil para
    /// conferir um ponto contra a mesma árvore que o jogador usa.
    pub fn hull_contents_at(&self, model: usize, hull: usize, p: [f32; 3]) -> Option<i32> {
        hull_size(hull)?;
        let mut at = self.models.get(model)?.hull_heads[hull - 1];
        if at < 0 {
            return None; // modelo sem esse hull
        }
        for _ in 0..=self.clipnodes.len() {
            if at < 0 {
                return Some(at);
            }
            let cn = self.clipnodes.get(at as usize)?;
            let plane = self.planes.get(usize::try_from(cn.plane).ok()?)?;
            let d = plane.normal[0] * p[0] + plane.normal[1] * p[1] + plane.normal[2] * p[2] - plane.dist;
            at = i32::from(cn.children[usize::from(d < 0.0)]);
        }
        None
    }
}
