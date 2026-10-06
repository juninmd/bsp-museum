//! Lightmaps: empacota a luz pré-calculada de cada face num atlas único.
//!
//! O GoldSrc guarda, por face, um mapinha de luz de 1 texel a cada 16 unidades
//! de textura (`lightofs` no lump de iluminação, 3 bytes RGB por texel; o Quake
//! usa 1 byte). Aqui cada mapinha vira um retângulo de um atlas RGBA e cada
//! vértice ganha a coordenada UV dentro dele — o viewer multiplica textura ×
//! atlas e o mapa aparece iluminado como no jogo.

use super::Bsp;

/// Lado máximo do atlas. 4096 é aceito por qualquer GPU com WebGL2.
const ATLAS_WIDTH: usize = 2048;
const ATLAS_MAX_HEIGHT: usize = 4096;
/// Face com lightmap maior que isto é lixo (o compilador limita a ~18×18).
const MAX_LIGHTMAP_SIDE: usize = 64;
/// bit 0 do `flags` do texinfo: céu/água/etc. — não recebe lightmap
const TEX_SPECIAL: u32 = 1;

/// Posição de uma face dentro do atlas.
#[derive(Debug, Clone, Copy)]
pub struct FaceLight {
    /// canto do retângulo útil (sem a borda de 1 texel), em texels
    pub x: usize,
    pub y: usize,
    /// menores s/t da face, em unidades de textura (múltiplos de 16)
    pub min_s: f32,
    pub min_t: f32,
}

#[derive(Debug, Clone)]
pub struct Atlas {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
    /// um item por face do BSP; `None` = face sem lightmap (usa o texel branco)
    pub faces: Vec<Option<FaceLight>>,
}

impl Atlas {
    /// UV do texel branco, para o que não tem lightmap (props, céu, faces sem luz).
    pub fn white_uv(&self) -> [f32; 2] {
        [1.0 / self.width as f32, 1.0 / self.height as f32]
    }

    /// UV de um ponto (s, t em unidades de textura) de uma face dentro do atlas.
    pub fn uv(&self, face: usize, s: f32, t: f32) -> [f32; 2] {
        match self.faces.get(face).copied().flatten() {
            Some(f) => {
                // centro do texel: (s - min)/16 + 0.5, como no GL_BuildLightmaps do Quake
                let lx = (s - f.min_s) / 16.0 + 0.5;
                let ly = (t - f.min_t) / 16.0 + 0.5;
                [(f.x as f32 + lx) / self.width as f32, (f.y as f32 + ly) / self.height as f32]
            }
            None => self.white_uv(),
        }
    }
}

/// Coordenadas de textura (s, t) de um ponto do mundo segundo o texinfo.
pub fn st_of(vecs: &[[f32; 4]; 2], p: [f32; 3]) -> (f32, f32) {
    (
        vecs[0][0] * p[0] + vecs[0][1] * p[1] + vecs[0][2] * p[2] + vecs[0][3],
        vecs[1][0] * p[0] + vecs[1][1] * p[1] + vecs[1][2] * p[2] + vecs[1][3],
    )
}

struct Packer {
    width: usize,
    cursor_x: usize,
    shelf_y: usize,
    shelf_h: usize,
}

impl Packer {
    /// Reserva `w × h` e devolve o canto, ou `None` se o atlas estourou.
    fn place(&mut self, w: usize, h: usize) -> Option<(usize, usize)> {
        if w > self.width {
            return None;
        }
        if self.cursor_x + w > self.width {
            self.shelf_y += self.shelf_h;
            self.cursor_x = 0;
            self.shelf_h = 0;
        }
        if self.shelf_y + h > ATLAS_MAX_HEIGHT {
            return None;
        }
        let at = (self.cursor_x, self.shelf_y);
        self.cursor_x += w;
        self.shelf_h = self.shelf_h.max(h);
        Some(at)
    }
}

/// Monta o atlas. `None` quando o mapa não tem luz alguma (fullbright) — o viewer
/// cai no modo sem lightmap.
pub fn build_atlas(bsp: &Bsp) -> Option<Atlas> {
    if bsp.lighting.is_empty() {
        return None;
    }
    let bytes_per_texel = if bsp.is_quake() { 1 } else { 3 };
    let mut packer = Packer { width: ATLAS_WIDTH, cursor_x: 0, shelf_y: 0, shelf_h: 0 };
    let mut pixels: Vec<(usize, usize, Vec<u8>, usize, usize)> = Vec::new(); // x, y, rgba(w+2 × h+2), w, h
    let mut faces: Vec<Option<FaceLight>> = vec![None; bsp.faces.len()];

    // Bloco branco 2×2 no canto (0,0): é o "sem luz" — UV (1/W, 1/H) cai no centro dele.
    let (wx, wy) = packer.place(2, 2)?;
    pixels.push((wx, wy, vec![255u8; 2 * 2 * 4], 2, 2));

    for (index, face) in bsp.faces.iter().enumerate() {
        if face.lightofs < 0 || face.styles[0] == 255 {
            continue;
        }
        let flags = bsp.texinfo_flags.get(face.texinfo as usize).copied().unwrap_or(0);
        if flags & TEX_SPECIAL != 0 {
            continue;
        }
        let Some(vecs) = bsp.texinfo_vecs.get(face.texinfo as usize) else { continue };
        let Some(points) = bsp.face_polygon(face) else { continue };

        let (mut min_s, mut max_s, mut min_t, mut max_t) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for p in &points {
            let (s, t) = st_of(vecs, *p);
            min_s = min_s.min(s);
            max_s = max_s.max(s);
            min_t = min_t.min(t);
            max_t = max_t.max(t);
        }
        if !(min_s.is_finite() && max_s.is_finite() && min_t.is_finite() && max_t.is_finite()) {
            continue;
        }
        let bmin_s = (min_s / 16.0).floor();
        let bmax_s = (max_s / 16.0).ceil();
        let bmin_t = (min_t / 16.0).floor();
        let bmax_t = (max_t / 16.0).ceil();
        let w = (bmax_s - bmin_s) as usize + 1;
        let h = (bmax_t - bmin_t) as usize + 1;
        if w > MAX_LIGHTMAP_SIDE || h > MAX_LIGHTMAP_SIDE {
            continue;
        }
        let start = face.lightofs as usize;
        let Some(src) = start
            .checked_add(w * h * bytes_per_texel)
            .and_then(|end| bsp.lighting.get(start..end))
        else {
            continue;
        };

        // (w+2)×(h+2) com borda replicada: sem ela o filtro linear sangra a luz
        // do vizinho do atlas na beira da face.
        let (pw, ph) = (w + 2, h + 2);
        let Some((px, py)) = packer.place(pw, ph) else { break };
        let mut block = vec![255u8; pw * ph * 4];
        for y in 0..ph {
            let sy = y.saturating_sub(1).min(h - 1);
            for x in 0..pw {
                let sx = x.saturating_sub(1).min(w - 1);
                let i = (sy * w + sx) * bytes_per_texel;
                let rgb = if bytes_per_texel == 3 { [src[i], src[i + 1], src[i + 2]] } else { [src[i]; 3] };
                let o = (y * pw + x) * 4;
                block[o..o + 3].copy_from_slice(&rgb);
            }
        }
        pixels.push((px, py, block, pw, ph));
        faces[index] = Some(FaceLight { x: px + 1, y: py + 1, min_s: bmin_s * 16.0, min_t: bmin_t * 16.0 });
    }

    let height = (packer.shelf_y + packer.shelf_h).max(2).next_power_of_two().min(ATLAS_MAX_HEIGHT);
    let mut rgba = vec![255u8; ATLAS_WIDTH * height * 4];
    for (x0, y0, block, pw, ph) in &pixels {
        for y in 0..*ph {
            let dst = ((y0 + y) * ATLAS_WIDTH + x0) * 4;
            rgba[dst..dst + pw * 4].copy_from_slice(&block[y * pw * 4..(y + 1) * pw * 4]);
        }
    }
    Some(Atlas { width: ATLAS_WIDTH, height, rgba, faces })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bsp::Face;

    /// Chão 32×32 (2 texels por lado + 1) com eixos s=x, t=y e luz conhecida.
    fn lit_floor(lighting: Vec<u8>) -> Bsp {
        Bsp {
            version: 30,
            vertices: vec![[0.0, 0.0, 0.0], [32.0, 0.0, 0.0], [32.0, 32.0, 0.0], [0.0, 32.0, 0.0]],
            edges: vec![(0, 1), (1, 2), (2, 3), (3, 0)],
            surfedges: vec![0, 1, 2, 3],
            faces: vec![Face { first_edge: 0, num_edges: 4, texinfo: 0, lightofs: 0, styles: [0, 255, 255, 255] }],
            texinfo_miptex: vec![0],
            texinfo_vecs: vec![[[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0]]],
            texinfo_flags: vec![0],
            lighting,
            ..Default::default()
        }
    }

    #[test]
    fn mapa_sem_luz_nao_gera_atlas() {
        assert!(build_atlas(&lit_floor(Vec::new())).is_none());
    }

    #[test]
    fn face_ganha_retangulo_com_a_luz_dela() {
        // 3×3 texels (0..32 em passos de 16 = 3 amostras), vermelho puro
        let lighting: Vec<u8> = (0..9).flat_map(|_| [200u8, 0, 0]).collect();
        let atlas = build_atlas(&lit_floor(lighting)).expect("atlas");
        let f = atlas.faces[0].expect("face com lightmap");
        let at = |x: usize, y: usize| {
            let o = (y * atlas.width + x) * 4;
            [atlas.rgba[o], atlas.rgba[o + 1], atlas.rgba[o + 2]]
        };
        assert_eq!(at(f.x, f.y), [200, 0, 0]);
        assert_eq!(at(f.x + 2, f.y + 2), [200, 0, 0]);
        // borda replicada e bloco branco intactos
        assert_eq!(at(f.x - 1, f.y - 1), [200, 0, 0]);
        assert_eq!(at(0, 0), [255, 255, 255]);
    }

    #[test]
    fn uv_do_vertice_cai_no_centro_do_texel() {
        let lighting: Vec<u8> = vec![10; 27];
        let atlas = build_atlas(&lit_floor(lighting)).unwrap();
        let f = atlas.faces[0].unwrap();
        let [u, v] = atlas.uv(0, 0.0, 0.0);
        assert!((u - (f.x as f32 + 0.5) / atlas.width as f32).abs() < 1e-6);
        assert!((v - (f.y as f32 + 0.5) / atlas.height as f32).abs() < 1e-6);
        // última amostra (s=32 -> 2.5 texels)
        let [u2, _] = atlas.uv(0, 32.0, 0.0);
        assert!((u2 - (f.x as f32 + 2.5) / atlas.width as f32).abs() < 1e-6);
    }

    #[test]
    fn face_sem_lightofs_ou_special_usa_o_branco() {
        let mut bsp = lit_floor(vec![9; 27]);
        bsp.faces[0].lightofs = -1;
        let atlas = build_atlas(&bsp).unwrap();
        assert!(atlas.faces[0].is_none());
        assert_eq!(atlas.uv(0, 5.0, 5.0), atlas.white_uv());

        let mut sky = lit_floor(vec![9; 27]);
        sky.texinfo_flags[0] = TEX_SPECIAL;
        assert!(build_atlas(&sky).unwrap().faces[0].is_none());
    }

    #[test]
    fn lightmap_fora_do_lump_e_ignorado() {
        // pede 9 texels mas só há 3 bytes
        let atlas = build_atlas(&lit_floor(vec![1, 2, 3])).unwrap();
        assert!(atlas.faces[0].is_none());
    }

    #[test]
    fn quake_usa_um_byte_por_texel() {
        let mut bsp = lit_floor(vec![128; 9]);
        bsp.version = 29;
        let atlas = build_atlas(&bsp).unwrap();
        let f = atlas.faces[0].unwrap();
        let o = (f.y * atlas.width + f.x) * 4;
        assert_eq!(&atlas.rgba[o..o + 3], &[128, 128, 128]);
    }
}
