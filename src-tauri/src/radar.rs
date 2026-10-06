//! Radar estilo Counter-Strike: vista de cima em tons de verde, pronta para
//! virar `overviews/<mapa>.bmp` (+ `.txt` com origem e zoom).

use crate::bsp::render::is_invisible;
use crate::bsp::Bsp;

pub struct Radar {
    pub size: usize,
    /// RGB, `size × size × 3`, linha 0 = norte
    pub rgb: Vec<u8>,
    /// centro do mapa no mundo (x, y)
    pub origin: [f32; 2],
    /// unidades de mundo por pixel
    pub units_per_pixel: f32,
}

fn height_green(t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, (70.0 + 170.0 * t) as u8, (20.0 + 40.0 * t) as u8]
}

/// Preenche um polígono (regra par-ímpar por scanline) no buffer.
fn fill_polygon(buf: &mut [u8], size: usize, pts: &[(f32, f32)], color: [u8; 3]) {
    if pts.len() < 3 {
        return;
    }
    let min_y = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
    let max_y = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil().min(size as f32 - 1.0).max(0.0) as usize;
    let mut xs: Vec<f32> = Vec::with_capacity(8);
    for y in min_y..=max_y {
        let yc = y as f32 + 0.5;
        xs.clear();
        for i in 0..pts.len() {
            let (x1, y1) = pts[i];
            let (x2, y2) = pts[(i + 1) % pts.len()];
            if (y1 <= yc && y2 > yc) || (y2 <= yc && y1 > yc) {
                xs.push(x1 + (yc - y1) / (y2 - y1) * (x2 - x1));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in xs.chunks_exact(2) {
            let x0 = pair[0].round().max(0.0) as usize;
            let x1 = (pair[1].round() as i64).clamp(0, size as i64) as usize;
            for x in x0..x1.min(size) {
                let o = (y * size + x) * 3;
                buf[o..o + 3].copy_from_slice(&color);
            }
        }
    }
}

fn line(buf: &mut [u8], size: usize, a: (f32, f32), b: (f32, f32), color: [u8; 3]) {
    let steps = ((b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil() as usize).max(1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        if x >= 0.0 && y >= 0.0 && (x as usize) < size && (y as usize) < size {
            let o = (y as usize * size + x as usize) * 3;
            buf[o..o + 3].copy_from_slice(&color);
        }
    }
}

/// Renderiza o radar. `None` se o mapa não tem modelo 0.
pub fn render(bsp: &Bsp, size: usize) -> Option<Radar> {
    let b = bsp.bounds()?;
    let span_x = (b.maxs[0] - b.mins[0]).max(1.0);
    let span_y = (b.maxs[1] - b.mins[1]).max(1.0);
    let margin = size as f32 * 0.04;
    let units_per_pixel = span_x.max(span_y) / (size as f32 - 2.0 * margin);
    let origin = [(b.mins[0] + b.maxs[0]) / 2.0, (b.mins[1] + b.maxs[1]) / 2.0];
    let z_range = (b.maxs[2] - b.mins[2]).max(1.0);
    let half = size as f32 / 2.0;
    let to_px = |p: &[f32; 3]| ((p[0] - origin[0]) / units_per_pixel + half, half - (p[1] - origin[1]) / units_per_pixel);

    let mut floors: Vec<(f32, Vec<(f32, f32)>)> = Vec::new();
    let mut walls: Vec<Vec<(f32, f32)>> = Vec::new();
    for face in &bsp.faces {
        if bsp.texture_of(face).is_some_and(|t| is_invisible(&t.name)) {
            continue;
        }
        let Some(points) = bsp.face_polygon(face) else { continue };
        let n = newell_z(&points);
        let px: Vec<(f32, f32)> = points.iter().map(to_px).collect();
        if n > 0.7 {
            let z = points.iter().map(|p| p[2]).sum::<f32>() / points.len() as f32;
            floors.push((z, px));
        } else if n.abs() <= 0.7 {
            walls.push(px);
        }
    }
    floors.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut rgb = vec![0u8; size * size * 3];
    for (z, poly) in &floors {
        fill_polygon(&mut rgb, size, poly, height_green((z - b.mins[2]) / z_range));
    }
    for poly in &walls {
        for i in 0..poly.len() {
            line(&mut rgb, size, poly[i], poly[(i + 1) % poly.len()], [150, 255, 150]);
        }
    }
    Some(Radar { size, rgb, origin, units_per_pixel })
}

/// Componente Z da normal (Newell), normalizada pelo comprimento.
fn newell_z(points: &[[f32; 3]]) -> f32 {
    let mut n = [0.0f32; 3];
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > f32::EPSILON { n[2] / len } else { 0.0 }
}

impl Radar {
    pub fn png(&self) -> Option<Vec<u8>> {
        let rgba: Vec<u8> = self.rgb.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        let data_url = crate::bsp::rgba_png(self.size, self.size, &rgba)?;
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.decode(data_url.strip_prefix("data:image/png;base64,")?).ok()
    }

    /// BMP de 24 bits (o CS lê `overviews/<mapa>.bmp`): linhas de baixo para cima, BGR.
    pub fn bmp(&self) -> Vec<u8> {
        let row = (self.size * 3).div_ceil(4) * 4;
        let data_len = row * self.size;
        let mut out = Vec::with_capacity(54 + data_len);
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&((54 + data_len) as u32).to_le_bytes());
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&54u32.to_le_bytes());
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&(self.size as i32).to_le_bytes());
        out.extend_from_slice(&(self.size as i32).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&24u16.to_le_bytes());
        out.extend_from_slice(&[0u8; 4]); // sem compressão
        out.extend_from_slice(&(data_len as u32).to_le_bytes());
        out.extend_from_slice(&[0u8; 16]); // resolução e paleta
        for y in (0..self.size).rev() {
            for x in 0..self.size {
                let o = (y * self.size + x) * 3;
                out.extend_from_slice(&[self.rgb[o + 2], self.rgb[o + 1], self.rgb[o]]);
            }
            out.extend(std::iter::repeat(0u8).take(row - self.size * 3));
        }
        out
    }

    /// `overviews/<mapa>.txt`. ORIGIN/ZOOM são a melhor estimativa para o enquadramento
    /// gerado aqui (a imagem cobre `size × units_per_pixel` unidades); o calibre exato do
    /// ZOOM no cliente não foi conferido contra o jogo — ajuste à mão se o radar desalinhar.
    pub fn overview_txt(&self, map: &str) -> String {
        let span = self.size as f32 * self.units_per_pixel;
        format!(
            "// overview gerado pelo bsp-museum para {map}\nglobal\n{{\n\tZOOM\t\t{:.3}\n\tORIGIN\t\t{:.0} {:.0} 0\n\tROTATED\t\t0\n}}\n",
            4096.0 / span.max(1.0),
            self.origin[0],
            self.origin[1],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{museum_bsp, Options};

    #[test]
    fn radar_da_fixture_pinta_o_chao_e_deixa_o_resto_preto() {
        let bsp = Bsp::parse(&museum_bsp(Options::default())).unwrap();
        let radar = render(&bsp, 256).expect("radar");
        let painted = radar.rgb.chunks_exact(3).filter(|p| p[1] > 0).count();
        let total = 256 * 256;
        assert!(painted > total / 5, "chão deveria cobrir boa parte: {painted}/{total}");
        // o mapa é 3:1 (1536×512): sobra preto em cima e embaixo
        assert!(radar.rgb[..256 * 3].iter().all(|&b| b == 0), "primeira linha é margem");
    }

    #[test]
    fn bmp_tem_cabecalho_e_tamanho_certos() {
        let bsp = Bsp::parse(&museum_bsp(Options::default())).unwrap();
        let radar = render(&bsp, 64).unwrap();
        let bmp = radar.bmp();
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(bmp.len(), 54 + 64 * 3 * 64); // 64*3 = 192, múltiplo de 4
        assert_eq!(u32::from_le_bytes(bmp[2..6].try_into().unwrap()) as usize, bmp.len());
    }

    #[test]
    fn png_e_overview_saem() {
        let bsp = Bsp::parse(&museum_bsp(Options::default())).unwrap();
        let radar = render(&bsp, 64).unwrap();
        assert_eq!(&radar.png().unwrap()[..4], &[0x89, b'P', b'N', b'G']);
        let txt = radar.overview_txt("de_museu");
        assert!(txt.contains("ZOOM") && txt.contains("ORIGIN") && txt.contains("ROTATED"));
    }

    #[test]
    fn mapa_sem_modelo_nao_gera_radar() {
        assert!(render(&Bsp::default(), 64).is_none());
    }
}
