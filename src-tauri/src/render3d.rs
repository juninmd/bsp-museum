//! Rasterizador por software (CPU, sem GPU nem dependência nova): desenha a malha de um mapa
//! ou de um modelo `.mdl` com textura, lightmap e sombreado simples, e devolve pixels RGBA.
//! Existe para o servidor MCP poder *mostrar* um mapa/modelo a um agente (imagem PNG) sem
//! abrir o app. Perspectiva correta, z-buffer, recorte no plano próximo, textura com wrap.
//! Coordenadas GoldSrc: Z para cima, yaw 0 olha para +X.


/// Lado máximo da imagem (o rasterizador é por CPU).
pub const MAX_SIDE: usize = 1600;

#[derive(Debug, Clone)]
pub struct Tex {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
    /// textura de buraco (`{nome`): pixels com alfa < 128 não são desenhados
    pub masked: bool,
}

/// Decodifica o PNG que `bsp::rgba_png` gera (RGBA 8 bits, filtro "None").
pub fn decode_png_url(data_url: &str) -> Option<Tex> {
    use base64::Engine as _;
    let b64 = data_url.strip_prefix("data:image/png;base64,")?;
    let png = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    if png.get(0..8)? != [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return None;
    }
    let (mut w, mut h) = (0usize, 0usize);
    let mut pos = 8usize;
    let mut idat = Vec::new();
    while pos + 12 <= png.len() {
        let len = u32::from_be_bytes(png[pos..pos + 4].try_into().ok()?) as usize;
        let kind = png.get(pos + 4..pos + 8)?;
        let body = png.get(pos + 8..pos + 8 + len)?;
        if kind == b"IHDR" {
            w = u32::from_be_bytes(body.get(0..4)?.try_into().ok()?) as usize;
            h = u32::from_be_bytes(body.get(4..8)?.try_into().ok()?) as usize;
            if body.get(8) != Some(&8) || body.get(9) != Some(&6) {
                return None;
            }
        } else if kind == b"IDAT" {
            idat.extend_from_slice(body);
        }
        pos += 12 + len;
    }
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return None;
    }
    let mut scan = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::ZlibDecoder::new(&idat[..]), &mut scan).ok()?;
    let stride = w * 4;
    let mut rgba = Vec::with_capacity(w * h * 4);
    for row in scan.chunks(stride + 1).take(h) {
        if row.len() != stride + 1 || row[0] != 0 {
            return None;
        }
        rgba.extend_from_slice(&row[1..]);
    }
    (rgba.len() == w * h * 4).then_some(Tex { w, h, rgba, masked: false })
}

pub struct MeshInput<'a> {
    pub positions: &'a [f32],
    pub uvs: &'a [f32],
    /// textura por triângulo (`u32::MAX` ou índice inexistente = cor lisa)
    pub texindex: &'a [u32],
    pub textures: &'a [Option<Tex>],
    /// uv no atlas de lightmap (6 floats por triângulo); vazio = sem luz de mapa
    pub lm_uvs: &'a [f32],
    pub lightmap: Option<&'a Tex>,
}

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    pub pos: [f32; 3],
    /// graus; 0 olha para +X, 90 para +Y
    pub yaw: f32,
    /// graus; positivo olha para cima
    pub pitch: f32,
    /// campo de visão horizontal, em graus
    pub fov: f32,
}

#[derive(Debug, Clone)]
pub struct Marker {
    pub pos: [f32; 3],
    pub rgb: [u8; 3],
}

#[derive(Debug, Clone)]
pub struct Options {
    pub width: usize,
    pub height: usize,
    pub bg: [u8; 3],
    /// descarta triângulos cujo ponto mais baixo passa desta altura (corte para ver dentro do mapa)
    pub clip_z: Option<f32>,
    pub markers: Vec<Marker>,
    /// ilumina pela normal quando não há lightmap
    pub shade: bool,
}

impl Options {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width: width.clamp(16, MAX_SIDE), height: height.clamp(16, MAX_SIDE), bg: [18, 20, 26], clip_z: None, markers: Vec::new(), shade: true }
    }
}

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 1e-9 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// Câmera que enquadra a caixa `[mins, maxs]` olhando para o centro, de `yaw`/`pitch` dados.
pub fn frame_box(mins: V3, maxs: V3, yaw: f32, pitch: f32, fov: f32) -> Camera {
    let c = [(mins[0] + maxs[0]) / 2.0, (mins[1] + maxs[1]) / 2.0, (mins[2] + maxs[2]) / 2.0];
    let radius = (dot(sub(maxs, mins), sub(maxs, mins)).sqrt() / 2.0).max(1.0);
    let dist = radius / (fov.to_radians() / 2.0).sin().max(0.2) * 0.95;
    let (y, p) = (yaw.to_radians(), pitch.to_radians());
    let fwd = [p.cos() * y.cos(), p.cos() * y.sin(), p.sin()];
    Camera { pos: [c[0] - fwd[0] * dist, c[1] - fwd[1] * dist, c[2] - fwd[2] * dist], yaw, pitch, fov }
}

#[derive(Clone, Copy)]
struct Vtx {
    c: V3, // espaço da câmera (x direita, y cima, z à frente)
    uv: [f32; 2],
    lm: [f32; 2],
}

fn lerp_v(a: &Vtx, b: &Vtx, t: f32) -> Vtx {
    let l = |x: f32, y: f32| x + (y - x) * t;
    Vtx {
        c: [l(a.c[0], b.c[0]), l(a.c[1], b.c[1]), l(a.c[2], b.c[2])],
        uv: [l(a.uv[0], b.uv[0]), l(a.uv[1], b.uv[1])],
        lm: [l(a.lm[0], b.lm[0]), l(a.lm[1], b.lm[1])],
    }
}

fn sample(t: &Tex, u: f32, v: f32) -> [u8; 4] {
    let fx = u - u.floor();
    let fy = v - v.floor();
    let x = ((fx * t.w as f32) as usize).min(t.w - 1);
    let y = ((fy * t.h as f32) as usize).min(t.h - 1);
    let i = (y * t.w + x) * 4;
    [t.rgba[i], t.rgba[i + 1], t.rgba[i + 2], t.rgba[i + 3]]
}

fn sample_clamped(t: &Tex, u: f32, v: f32) -> [u8; 4] {
    let x = (u.clamp(0.0, 1.0) * t.w as f32) as usize;
    let y = (v.clamp(0.0, 1.0) * t.h as f32) as usize;
    let i = (y.min(t.h - 1) * t.w + x.min(t.w - 1)) * 4;
    [t.rgba[i], t.rgba[i + 1], t.rgba[i + 2], t.rgba[i + 3]]
}

/// Cor lisa estável por índice de textura (para o que não tem imagem).
fn flat_color(ix: u32) -> [u8; 3] {
    let h = ix.wrapping_mul(2654435761);
    [96 + (h >> 24) as u8 / 3, 96 + (h >> 16) as u8 / 3, 96 + (h >> 8) as u8 / 3]
}

/// Desenha a malha; devolve `width*height*4` bytes RGBA.
pub fn render(mesh: &MeshInput, cam: &Camera, opt: &Options) -> Vec<u8> {
    let (w, h) = (opt.width, opt.height);
    let mut color = vec![0u8; w * h * 4];
    for px in color.chunks_exact_mut(4) {
        px.copy_from_slice(&[opt.bg[0], opt.bg[1], opt.bg[2], 255]);
    }
    let mut depth = vec![0f32; w * h]; // 1/z; 0 = vazio

    let (y, p) = (cam.yaw.to_radians(), cam.pitch.clamp(-89.0, 89.0).to_radians());
    let fwd = [p.cos() * y.cos(), p.cos() * y.sin(), p.sin()];
    let right = norm(cross(fwd, [0.0, 0.0, 1.0]));
    let up = cross(right, fwd);
    let focal = (w as f32 / 2.0) / (cam.fov.clamp(20.0, 150.0).to_radians() / 2.0).tan();
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let near = 1.0f32;
    let to_cam = |p: V3| {
        let d = sub(p, cam.pos);
        [dot(d, right), dot(d, up), dot(d, fwd)]
    };
    // luz fixa vinda do alto e um pouco do lado da câmera
    let light = norm([fwd[0] * -0.4 + 0.2, fwd[1] * -0.4 + 0.3, 0.85]);

    let tris = mesh.positions.len() / 9;
    for t in 0..tris {
        let pos = |k: usize| -> V3 { [mesh.positions[t * 9 + k * 3], mesh.positions[t * 9 + k * 3 + 1], mesh.positions[t * 9 + k * 3 + 2]] };
        let (p0, p1, p2) = (pos(0), pos(1), pos(2));
        if let Some(cz) = opt.clip_z {
            if p0[2].min(p1[2]).min(p2[2]) > cz {
                continue;
            }
        }
        let n = norm(cross(sub(p1, p0), sub(p2, p0)));
        let uv = |k: usize| -> [f32; 2] { [*mesh.uvs.get(t * 6 + k * 2).unwrap_or(&0.0), *mesh.uvs.get(t * 6 + k * 2 + 1).unwrap_or(&0.0)] };
        let lmuv = |k: usize| -> [f32; 2] {
            if mesh.lm_uvs.len() >= (t + 1) * 6 {
                [mesh.lm_uvs[t * 6 + k * 2], mesh.lm_uvs[t * 6 + k * 2 + 1]]
            } else {
                [0.0, 0.0]
            }
        };
        let vs = [
            Vtx { c: to_cam(p0), uv: uv(0), lm: lmuv(0) },
            Vtx { c: to_cam(p1), uv: uv(1), lm: lmuv(1) },
            Vtx { c: to_cam(p2), uv: uv(2), lm: lmuv(2) },
        ];
        // recorte no plano próximo
        let mut poly: Vec<Vtx> = Vec::with_capacity(4);
        for i in 0..3 {
            let (a, b) = (&vs[i], &vs[(i + 1) % 3]);
            let (ain, bin) = (a.c[2] >= near, b.c[2] >= near);
            if ain {
                poly.push(*a);
            }
            if ain != bin {
                let tt = (near - a.c[2]) / (b.c[2] - a.c[2]);
                poly.push(lerp_v(a, b, tt));
            }
        }
        if poly.len() < 3 {
            continue;
        }
        let tex_ix = mesh.texindex.get(t).copied().unwrap_or(u32::MAX);
        let tex = mesh.textures.get(tex_ix as usize).and_then(Option::as_ref);
        let has_lm = mesh.lightmap.is_some() && mesh.lm_uvs.len() >= (t + 1) * 6;
        let shade = if opt.shade && !has_lm { 0.55 + 0.45 * dot(n, light).abs() } else { 1.0 };
        let flat = flat_color(tex_ix);
        for i in 1..poly.len() - 1 {
            raster(&mut color, &mut depth, w, h, [poly[0], poly[i], poly[i + 1]], focal, cx, cy, |u, v, lu, lv| {
                let mut rgb = match tex {
                    Some(tx) => {
                        let s = sample(tx, u, v);
                        if tx.masked && s[3] < 128 {
                            return None;
                        }
                        [s[0] as f32, s[1] as f32, s[2] as f32]
                    }
                    None => [flat[0] as f32, flat[1] as f32, flat[2] as f32],
                };
                if has_lm {
                    if let Some(lm) = mesh.lightmap {
                        let l = sample_clamped(lm, lu, lv);
                        for (c, lc) in rgb.iter_mut().zip(&l) {
                            *c = (*c * *lc as f32 / 255.0 * 2.0).min(255.0);
                        }
                    }
                }
                Some([(rgb[0] * shade) as u8, (rgb[1] * shade) as u8, (rgb[2] * shade) as u8])
            });
        }
    }

    for m in &opt.markers {
        let c = to_cam(m.pos);
        if c[2] < near {
            continue;
        }
        let (sx, sy) = (cx + c[0] / c[2] * focal, cy - c[1] / c[2] * focal);
        let r = 5i32;
        for dy in -r..=r {
            for dx in -r..=r {
                let d2 = dx * dx + dy * dy;
                if d2 > r * r {
                    continue;
                }
                let (px, py) = (sx as i32 + dx, sy as i32 + dy);
                if px < 0 || py < 0 || px >= w as i32 || py >= h as i32 {
                    continue;
                }
                let di = py as usize * w + px as usize;
                if 1.0 / c[2] < depth[di] * 0.97 {
                    continue; // atrás de uma parede
                }
                let i = di * 4;
                let edge = d2 > (r - 1) * (r - 1);
                let rgb = if edge { [255, 255, 255] } else { m.rgb };
                color[i..i + 3].copy_from_slice(&rgb);
            }
        }
    }
    color
}

#[allow(clippy::too_many_arguments)]
fn raster(
    color: &mut [u8],
    depth: &mut [f32],
    w: usize,
    h: usize,
    t: [Vtx; 3],
    focal: f32,
    cx: f32,
    cy: f32,
    shade: impl Fn(f32, f32, f32, f32) -> Option<[u8; 3]>,
) {
    // projeta: (x, y, 1/z, u/z, v/z, lu/z, lv/z)
    let mut s = [[0f32; 7]; 3];
    for (k, v) in t.iter().enumerate() {
        let iz = 1.0 / v.c[2];
        s[k] = [cx + v.c[0] * iz * focal, cy - v.c[1] * iz * focal, iz, v.uv[0] * iz, v.uv[1] * iz, v.lm[0] * iz, v.lm[1] * iz];
    }
    let area = (s[1][0] - s[0][0]) * (s[2][1] - s[0][1]) - (s[2][0] - s[0][0]) * (s[1][1] - s[0][1]);
    if area.abs() < 1e-6 {
        return;
    }
    let minx = s.iter().map(|v| v[0]).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
    let maxx = s.iter().map(|v| v[0]).fold(f32::MIN, f32::max).ceil().min(w as f32 - 1.0);
    let miny = s.iter().map(|v| v[1]).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
    let maxy = s.iter().map(|v| v[1]).fold(f32::MIN, f32::max).ceil().min(h as f32 - 1.0);
    if maxx < 0.0 || maxy < 0.0 {
        return;
    }
    let (maxx, maxy) = (maxx as usize, maxy as usize);
    let inv = 1.0 / area;
    for py in miny..=maxy {
        for px in minx..=maxx {
            let (x, y) = (px as f32 + 0.5, py as f32 + 0.5);
            let w0 = ((s[1][0] - x) * (s[2][1] - y) - (s[2][0] - x) * (s[1][1] - y)) * inv;
            let w1 = ((s[2][0] - x) * (s[0][1] - y) - (s[0][0] - x) * (s[2][1] - y)) * inv;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let iz = w0 * s[0][2] + w1 * s[1][2] + w2 * s[2][2];
            let di = py * w + px;
            if iz <= depth[di] {
                continue;
            }
            let z = 1.0 / iz;
            let at = |k: usize| (w0 * s[0][k] + w1 * s[1][k] + w2 * s[2][k]) * z;
            if let Some(rgb) = shade(at(3), at(4), at(5), at(6)) {
                depth[di] = iz;
                color[di * 4..di * 4 + 3].copy_from_slice(&rgb);
            }
        }
    }
}

/// `value` em graus normalizado para `[-180, 180)`.
pub fn wrap_deg(value: f32) -> f32 {
    (value + 180.0).rem_euclid(360.0) - 180.0
}


#[cfg(test)]
mod tests {
    use super::*;

    fn quad_tex() -> Tex {
        Tex { w: 2, h: 2, rgba: vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255], masked: false }
    }

    #[test]
    fn desenha_triangulo_na_frente_e_ignora_o_de_tras() {
        // triângulo grande em x=100 (à frente de uma câmera em 0 olhando +X)
        let positions = [100.0, -50.0, -50.0, 100.0, 50.0, -50.0, 100.0, 0.0, 50.0];
        let uvs = [0.0; 6];
        let tex = [Some(quad_tex())];
        let mesh = MeshInput { positions: &positions, uvs: &uvs, texindex: &[0], textures: &tex, lm_uvs: &[], lightmap: None };
        let cam = Camera { pos: [0.0, 0.0, 0.0], yaw: 0.0, pitch: 0.0, fov: 90.0 };
        let img = render(&mesh, &cam, &Options::new(64, 64));
        let center = (32 * 64 + 32) * 4;
        assert_ne!(&img[center..center + 3], &[18, 20, 26], "o centro deveria estar coberto");
        // câmera olhando para trás não vê nada
        let back = Camera { yaw: 180.0, ..cam };
        let img = render(&mesh, &back, &Options::new(64, 64));
        assert_eq!(&img[center..center + 3], &[18, 20, 26]);
    }

    #[test]
    fn mais_perto_cobre_mais_longe() {
        let tri = |x: f32| [x, -80.0, -80.0, x, 80.0, -80.0, x, 0.0, 80.0];
        let mut positions = Vec::new();
        positions.extend_from_slice(&tri(300.0)); // longe, textura 0 (vermelho/verde…)
        positions.extend_from_slice(&tri(100.0)); // perto, sem textura (cor lisa)
        let uvs = [0.0; 12];
        let tex = [Some(Tex { w: 1, h: 1, rgba: vec![255, 0, 0, 255], masked: false })];
        let mesh = MeshInput { positions: &positions, uvs: &uvs, texindex: &[0, u32::MAX], textures: &tex, lm_uvs: &[], lightmap: None };
        let cam = Camera { pos: [0.0; 3], yaw: 0.0, pitch: 0.0, fov: 90.0 };
        let mut o = Options::new(64, 64);
        o.shade = false;
        let img = render(&mesh, &cam, &o);
        let c = (32 * 64 + 32) * 4;
        assert_ne!(&img[c..c + 3], &[255, 0, 0], "a textura vermelha está atrás e não pode aparecer");
    }

    #[test]
    fn corte_por_altura_e_marcador() {
        let positions = [100.0, -50.0, 200.0, 100.0, 50.0, 200.0, 100.0, 0.0, 300.0];
        let uvs = [0.0; 6];
        let mesh = MeshInput { positions: &positions, uvs: &uvs, texindex: &[u32::MAX], textures: &[], lm_uvs: &[], lightmap: None };
        let cam = Camera { pos: [0.0; 3], yaw: 0.0, pitch: 0.0, fov: 90.0 };
        let mut o = Options::new(32, 32);
        o.clip_z = Some(100.0);
        o.markers.push(Marker { pos: [100.0, 0.0, 0.0], rgb: [0, 0, 255] });
        let img = render(&mesh, &cam, &o);
        let c = (16 * 32 + 16) * 4;
        assert_eq!(&img[c..c + 3], &[0, 0, 255]);
        assert_eq!(&img[0..3], &[18, 20, 26], "triângulo acima do corte some");
    }

    #[test]
    fn png_url_ida_e_volta() {
        let rgba = quad_tex().rgba;
        let url = crate::bsp::rgba_png(2, 2, &rgba).unwrap();
        let t = decode_png_url(&url).unwrap();
        assert_eq!((t.w, t.h, t.rgba), (2, 2, rgba));
        assert!(decode_png_url("data:image/png;base64,AAAA").is_none());
    }

    #[test]
    fn enquadra_a_caixa() {
        let cam = frame_box([-100.0; 3], [100.0; 3], 45.0, -30.0, 70.0);
        let d = dot(cam.pos, cam.pos).sqrt();
        assert!(d > 100.0 && d < 1000.0);
        assert_eq!(wrap_deg(190.0), -170.0);
    }
}
