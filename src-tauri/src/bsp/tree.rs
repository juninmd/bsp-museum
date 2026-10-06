//! Árvore BSP: classificar um ponto numa folha e ler o PVS (visibilidade).
//!
//! Serve a dois consumidores: o diagnóstico (spawn dentro de sólido) e o
//! viewer 3D (só desenhar o que a folha da câmera enxerga). Tudo checado:
//! índice fora da faixa ou ciclo na árvore vira `None`, nunca panic ou loop.

use super::{Bsp, Leaf};

pub const CONTENTS_EMPTY: i32 = -1;
pub const CONTENTS_SOLID: i32 = -2;

impl Bsp {
    /// Folha do hull visual (`headnode[0]`) que contém o ponto.
    pub fn leaf_at(&self, headnode: i32, p: [f32; 3]) -> Option<usize> {
        let mut at = headnode;
        // Uma árvore válida desce no máximo `nodes.len()` níveis; passar disso é ciclo.
        for _ in 0..=self.nodes.len() {
            if at < 0 {
                let leaf = usize::try_from(-1 - at as i64).ok()?;
                return (leaf < self.leaves.len()).then_some(leaf);
            }
            let node = self.nodes.get(at as usize)?;
            let plane = self.planes.get(usize::try_from(node.plane).ok()?)?;
            let d = plane.normal[0] * p[0] + plane.normal[1] * p[1] + plane.normal[2] * p[2] - plane.dist;
            at = i32::from(node.children[usize::from(d < 0.0)]);
        }
        None
    }

    /// `contents` da folha do ponto (`-1` vazio, `-2` sólido…). `None` se não há árvore.
    pub fn contents_at(&self, p: [f32; 3]) -> Option<i32> {
        let model = self.models.first()?;
        let leaf = self.leaf_at(model.headnode, p)?;
        Some(self.leaves.get(leaf)?.contents)
    }

    /// Folhas visíveis a partir de `leaf` (índice na lista de folhas, onde a 0 é
    /// o "fora do mundo"). `None` quando o mapa não tem vis para essa folha —
    /// quem chama deve tratar como "tudo visível".
    pub fn visible_leaves(&self, leaf: usize) -> Option<Vec<bool>> {
        let info: &Leaf = self.leaves.get(leaf)?;
        if info.visofs < 0 || self.visibility.is_empty() {
            return None;
        }
        let model = self.models.first()?;
        let count = usize::try_from(model.visleafs).ok()?;
        if count == 0 || count + 1 > self.leaves.len() {
            return None;
        }
        let row = decompress_vis(&self.visibility, info.visofs as usize, count.div_ceil(8))?;
        let mut out = vec![false; self.leaves.len()];
        // bit i da linha = folha i + 1 (a folha 0 não entra no PVS)
        for i in 0..count {
            if row[i >> 3] & (1 << (i & 7)) != 0 {
                out[i + 1] = true;
            }
        }
        out[leaf] = true;
        Some(out)
    }
}

/// Descomprime uma linha de PVS: byte ≠ 0 passa direto; 0 é seguido do número de
/// bytes zero a pular (RLE do `vis`).
pub fn decompress_vis(vis: &[u8], offset: usize, row_bytes: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(row_bytes);
    let mut at = offset;
    while out.len() < row_bytes {
        let b = *vis.get(at)?;
        at += 1;
        if b != 0 {
            out.push(b);
            continue;
        }
        let run = *vis.get(at)? as usize;
        at += 1;
        if run == 0 {
            return None; // corrido: zero seguido de zero nunca termina
        }
        out.extend(std::iter::repeat(0u8).take(run.min(row_bytes - out.len())));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bsp::{Model, Node, Plane};

    /// Duas salas separadas por um plano x=0: folha 1 (x<0) e folha 2 (x>0),
    /// mais a folha 0 (fora do mundo, sólida).
    fn two_rooms() -> Bsp {
        Bsp {
            planes: vec![Plane { normal: [1.0, 0.0, 0.0], dist: 0.0 }],
            nodes: vec![Node { plane: 0, children: [-2, -3] }], // frente (x>=0) -> folha 1... ver abaixo
            leaves: vec![
                Leaf { contents: CONTENTS_SOLID, visofs: -1, ..Default::default() },
                Leaf { contents: CONTENTS_EMPTY, visofs: 0, ..Default::default() },
                Leaf { contents: CONTENTS_EMPTY, visofs: 1, ..Default::default() },
            ],
            models: vec![Model { headnode: 0, visleafs: 2, ..Default::default() }],
            // linha da folha 1 vê só a si mesma (bit 0); a da folha 2 vê as duas (bits 0,1)
            visibility: vec![0b01, 0b11],
            ..Default::default()
        }
    }

    #[test]
    fn classifica_ponto_nas_duas_folhas() {
        let bsp = two_rooms();
        // children[0] = lado da frente (d >= 0) -> -2 => folha 1; children[1] = trás -> -3 => folha 2
        assert_eq!(bsp.leaf_at(0, [10.0, 0.0, 0.0]), Some(1));
        assert_eq!(bsp.leaf_at(0, [-10.0, 0.0, 0.0]), Some(2));
        assert_eq!(bsp.contents_at([10.0, 0.0, 0.0]), Some(CONTENTS_EMPTY));
    }

    #[test]
    fn arvore_com_ciclo_nao_trava() {
        let mut bsp = two_rooms();
        bsp.nodes[0].children = [0, 0]; // aponta para si mesmo
        assert_eq!(bsp.leaf_at(0, [1.0, 0.0, 0.0]), None);
    }

    #[test]
    fn indices_invalidos_viram_none() {
        let mut bsp = two_rooms();
        bsp.nodes[0].plane = 99;
        assert_eq!(bsp.leaf_at(0, [1.0, 0.0, 0.0]), None);
        assert_eq!(bsp.leaf_at(7, [1.0, 0.0, 0.0]), None);
        assert_eq!(Bsp::default().contents_at([0.0; 3]), None);
    }

    #[test]
    fn pvs_le_a_linha_da_folha() {
        let bsp = two_rooms();
        let from1 = bsp.visible_leaves(1).unwrap();
        assert_eq!(from1, vec![false, true, false]);
        let from2 = bsp.visible_leaves(2).unwrap();
        assert_eq!(from2, vec![false, true, true]);
        assert!(bsp.visible_leaves(0).is_none(), "folha 0 não tem vis");
    }

    #[test]
    fn rle_do_vis_expande_zeros() {
        // 0xFF, depois 3 bytes zero, depois 0x01
        let row = decompress_vis(&[0xFF, 0, 3, 0x01], 0, 5).unwrap();
        assert_eq!(row, vec![0xFF, 0, 0, 0, 0x01]);
        // run que estoura o tamanho da linha é cortada, não panica
        assert_eq!(decompress_vis(&[0, 200], 0, 4).unwrap(), vec![0, 0, 0, 0]);
        // truncado e zero-zero viram None
        assert!(decompress_vis(&[0xFF], 0, 4).is_none());
        assert!(decompress_vis(&[0, 0], 0, 4).is_none());
    }
}
