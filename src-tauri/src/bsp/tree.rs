//! Árvore BSP: classificar um ponto numa folha (diagnóstico de spawn em sólido).
//!
//! O PVS (visibilidade) é lido no frontend (`src/lib/pvs.ts`), que é quem desenha.
//! Tudo checado: índice fora da faixa ou ciclo na árvore vira `None`, nunca panic ou loop.

use super::Bsp;

#[cfg(test)]
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bsp::{Leaf, Model, Node, Plane};

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
}
