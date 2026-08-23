use super::entities::SpawnPoint;
use super::Bsp;
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

/// Texturas que marcam volume invisível: entram no BSP mas não são o mapa.
/// Sem filtrar isto, a planta vira um borrão de caixas de clip.
const INVISIBLE: [&str; 9] = [
    "aaatrigger",
    "clip",
    "clipbevel",
    "null",
    "origin",
    "hint",
    "skip",
    "sky",
    "trigger",
];

fn is_invisible(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    INVISIBLE.contains(&lower.as_str())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RenderOptions {
    /// lado maior do SVG, em unidades de viewBox
    pub size: f32,
    /// teto de polígonos desenhados (miniatura pede menos)
    pub max_polygons: usize,
    /// descarta polígono menor que esta fração da área do mapa
    pub min_area_ratio: f32,
    pub show_walls: bool,
    pub show_spawns: bool,
}

impl RenderOptions {
    pub fn thumbnail() -> Self {
        Self {
            size: 480.0,
            max_polygons: 900,
            min_area_ratio: 0.00004,
            show_walls: false,
            show_spawns: false,
        }
    }

    pub fn detail() -> Self {
        Self {
            size: 1400.0,
            max_polygons: 12_000,
            min_area_ratio: 0.0000015,
            show_walls: true,
            show_spawns: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderResult {
    pub svg: String,
    pub width: f32,
    pub height: f32,
    pub polygons: usize,
    pub skipped: usize,
}

struct Poly {
    points: Vec<(f32, f32)>,
    z: f32,
    area: f32,
    floor: bool,
}

/// Normal do polígono pelo método de Newell — funciona para polígono não plano
/// e dispensa ler o lump de planos.
fn newell_normal(points: &[[f32; 3]]) -> [f32; 3] {
    let mut n = [0.0f32; 3];
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > f32::EPSILON {
        [n[0] / len, n[1] / len, n[2] / len]
    } else {
        [0.0, 0.0, 0.0]
    }
}

/// Área da projeção no plano XY (fórmula do shoelace).
fn projected_area(points: &[(f32, f32)]) -> f32 {
    let mut sum = 0.0;
    for i in 0..points.len() {
        let (x1, y1) = points[i];
        let (x2, y2) = points[(i + 1) % points.len()];
        sum += x1 * y2 - x2 * y1;
    }
    (sum * 0.5).abs()
}

fn lerp(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t) as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t) as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t) as u8,
    ]
}

/// Altura vira cor: o andar de baixo é frio, o de cima é quente.
/// É o que transforma uma silhueta chapada em algo legível.
fn height_color(t: f32) -> String {
    let low = [26, 42, 71];
    let mid = [31, 122, 140];
    let high = [242, 193, 78];
    let rgb = if t < 0.5 { lerp(low, mid, t * 2.0) } else { lerp(mid, high, (t - 0.5) * 2.0) };
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Planta baixa em SVG, vista de cima.
///
/// Desenha as faces viradas para cima (o chão de cada andar) do mais baixo para
/// o mais alto, então uma passarela aparece sobre a rua. Paredes entram como
/// traço fino só na vista detalhada.
pub fn top_down(bsp: &Bsp, spawns: &[SpawnPoint], opts: RenderOptions) -> RenderResult {
    let Some(bounds) = bsp.bounds() else {
        return RenderResult {
            svg: empty_svg("mapa sem modelo 0"),
            width: opts.size,
            height: opts.size,
            polygons: 0,
            skipped: 0,
        };
    };

    let (min_x, min_y) = (bounds.mins[0], bounds.mins[1]);
    let (max_x, max_y) = (bounds.maxs[0], bounds.maxs[1]);
    let (min_z, max_z) = (bounds.mins[2], bounds.maxs[2]);
    let world_w = (max_x - min_x).max(1.0);
    let world_h = (max_y - min_y).max(1.0);
    let z_range = (max_z - min_z).max(1.0);
    let map_area = world_w * world_h;
    let min_area = map_area * opts.min_area_ratio;

    let mut polys: Vec<Poly> = Vec::new();
    let mut skipped = 0usize;

    for face in &bsp.faces {
        if let Some(tex) = bsp.texture_of(face) {
            if is_invisible(&tex.name) {
                skipped += 1;
                continue;
            }
        }
        let Some(points3) = bsp.face_polygon(face) else {
            skipped += 1;
            continue;
        };
        let normal = newell_normal(&points3);
        let floor = normal[2] > 0.7;
        let ceiling = normal[2] < -0.7;
        if ceiling {
            // Teto esconderia tudo que está embaixo: nunca entra na planta.
            skipped += 1;
            continue;
        }
        if !floor && !opts.show_walls {
            skipped += 1;
            continue;
        }

        let points: Vec<(f32, f32)> = points3.iter().map(|p| (p[0], p[1])).collect();
        let area = projected_area(&points);
        if floor && area < min_area {
            skipped += 1;
            continue;
        }

        let z = points3.iter().map(|p| p[2]).sum::<f32>() / points3.len() as f32;
        polys.push(Poly { points, z, area, floor });
    }

    // Mais alto por cima; entre iguais, o maior primeiro para não sumir sob detalhe.
    polys.sort_by(|a, b| {
        a.z.partial_cmp(&b.z).unwrap_or(std::cmp::Ordering::Equal).then(
            b.area.partial_cmp(&a.area).unwrap_or(std::cmp::Ordering::Equal),
        )
    });

    if polys.len() > opts.max_polygons {
        // Corta os menores, não os últimos: preserva a silhueta do mapa.
        let mut by_area: Vec<usize> = (0..polys.len()).collect();
        by_area.sort_by(|&a, &b| {
            polys[b].area.partial_cmp(&polys[a].area).unwrap_or(std::cmp::Ordering::Equal)
        });
        let keep: std::collections::HashSet<usize> =
            by_area.into_iter().take(opts.max_polygons).collect();
        let mut kept = Vec::with_capacity(opts.max_polygons);
        for (i, poly) in polys.into_iter().enumerate() {
            if keep.contains(&i) {
                kept.push(poly);
            } else {
                skipped += 1;
            }
        }
        polys = kept;
    }

    let scale = (opts.size / world_w).min(opts.size / world_h);
    let width = world_w * scale;
    let height = world_h * scale;
    // Y do mundo cresce para o norte; no SVG cresce para baixo — daí a inversão.
    let to_svg = |x: f32, y: f32| ((x - min_x) * scale, (max_y - y) * scale);

    let mut svg = String::with_capacity(polys.len() * 90 + 512);
    let _ = write!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width:.1} {height:.1}" width="{width:.0}" height="{height:.0}" role="img"><rect width="{width:.1}" height="{height:.1}" fill="#0d1117"/>"##
    );

    let mut drawn = 0usize;
    for poly in &polys {
        let mut d = String::with_capacity(poly.points.len() * 12);
        for &(x, y) in &poly.points {
            let (sx, sy) = to_svg(x, y);
            let _ = write!(d, "{sx:.1},{sy:.1} ");
        }
        let t = ((poly.z - min_z) / z_range).clamp(0.0, 1.0);
        if poly.floor {
            let _ = write!(
                svg,
                r#"<polygon points="{}" fill="{}" fill-opacity="0.92"/>"#,
                d.trim_end(),
                height_color(t)
            );
        } else {
            let _ = write!(
                svg,
                r##"<polyline points="{}" fill="none" stroke="#8b98a5" stroke-opacity="0.16" stroke-width="0.7"/>"##,
                d.trim_end()
            );
        }
        drawn += 1;
    }

    if opts.show_spawns {
        for spawn in spawns {
            let (sx, sy) = to_svg(spawn.position[0], spawn.position[1]);
            let color = match spawn.team {
                "CT" => "#4c7cf3",
                "T" => "#f0883e",
                _ => "#e6edf3",
            };
            let _ = write!(
                svg,
                r##"<circle cx="{sx:.1}" cy="{sy:.1}" r="4.5" fill="{color}" fill-opacity="0.9" stroke="#0d1117" stroke-width="1.2"><title>spawn {}</title></circle>"##,
                escape_xml(spawn.team)
            );
        }
    }

    svg.push_str("</svg>");

    RenderResult { svg, width, height, polygons: drawn, skipped }
}

fn empty_svg(message: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 480 300" width="480" height="300"><rect width="480" height="300" fill="#0d1117"/><text x="240" y="150" fill="#7d8590" font-family="sans-serif" font-size="16" text-anchor="middle">{}</text></svg>"##,
        escape_xml(message)
    )
}
