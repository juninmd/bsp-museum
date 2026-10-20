//! Volumes invisíveis e zonas: o que o jogo usa mas ninguém vê.
//!
//! Duas coisas separadas da malha visual (`catalog::mesh` continua sem elas):
//!
//! - **zonas**: brush entities (`model "*N"`) de gatilhos e objetivos (`trigger_*`,
//!   `func_buyzone`, `func_bomb_target`, `func_hostage_rescue`, `func_vip_safetyzone`,
//!   `func_escapezone`, `func_ladder`, `func_water`), cada uma com AABB, malha, cor sugerida
//!   e rótulo (`message` › `targetname` › `target`);
//! - **superfícies invisíveis**: faces das texturas `aaatrigger`, `clip`, `null`, `origin`,
//!   `hint`, `skip`, `sky`… que pertencem ao mundo (ou a brush entities que não são zona),
//!   agrupadas por textura. O compilador costuma descartar `clip`/`null`/`hint`/`skip` na
//!   hora de gerar as faces (só sobra o hull de colisão), então só o que o mapa de fato
//!   guardou aparece aqui — para ver o clip de verdade, use `hulls`.

use crate::bsp::entities::{self, Entity};
use crate::bsp::{render, Bsp, Face};
use crate::scene::brush_model_of;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// Triângulos por zona e no total (mapa hostil com milhares de triggers enormes).
const MAX_TRIS_PER_VOLUME: usize = 4000;
const MAX_TRIS_TOTAL: usize = 250_000;
const MAX_VOLUMES: usize = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneKind {
    /// `trigger_*` (a cor varia por classe)
    Trigger,
    BuyZone,
    BombTarget,
    HostageRescue,
    VipSafety,
    EscapeZone,
    Ladder,
    Water,
}

impl ZoneKind {
    pub fn label(self) -> &'static str {
        match self {
            ZoneKind::Trigger => "gatilho",
            ZoneKind::BuyZone => "zona de compra",
            ZoneKind::BombTarget => "alvo da bomba",
            ZoneKind::HostageRescue => "resgate de reféns",
            ZoneKind::VipSafety => "zona segura do VIP",
            ZoneKind::EscapeZone => "zona de fuga",
            ZoneKind::Ladder => "escada",
            ZoneKind::Water => "água",
        }
    }

    /// Cor sugerida (`#rrggbb`) para o viewer.
    pub fn color(self) -> &'static str {
        match self {
            ZoneKind::Trigger => "#b388ff",
            ZoneKind::BuyZone => "#ffd54a",
            ZoneKind::BombTarget => "#ff5252",
            ZoneKind::HostageRescue => "#4fc3f7",
            ZoneKind::VipSafety => "#ba68c8",
            ZoneKind::EscapeZone => "#81c784",
            ZoneKind::Ladder => "#a1887f",
            ZoneKind::Water => "#29b6f6",
        }
    }
}

/// Classifica a classe de uma brush entity.
pub fn classify(class: &str) -> Option<ZoneKind> {
    Some(match class {
        "func_buyzone" => ZoneKind::BuyZone,
        "func_bomb_target" => ZoneKind::BombTarget,
        "func_hostage_rescue" => ZoneKind::HostageRescue,
        "func_vip_safetyzone" => ZoneKind::VipSafety,
        "func_escapezone" => ZoneKind::EscapeZone,
        "func_ladder" => ZoneKind::Ladder,
        "func_water" => ZoneKind::Water,
        c if c.starts_with("trigger_") => ZoneKind::Trigger,
        _ => return None,
    })
}

/// Cor mais específica para alguns gatilhos comuns (o resto herda a do tipo).
fn color_of(class: &str, kind: ZoneKind) -> &'static str {
    match class {
        "trigger_hurt" => "#e53935",
        "trigger_teleport" => "#7e57c2",
        "trigger_push" => "#26c6da",
        "trigger_multiple" | "trigger_once" => "#b388ff",
        "trigger_changelevel" => "#ff7043",
        _ => kind.color(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Volume {
    /// posição na lista de entidades do mapa
    pub entity: usize,
    pub model: u32,
    pub classname: String,
    pub kind: ZoneKind,
    pub kind_label: &'static str,
    /// `#rrggbb`
    pub color: String,
    /// `message` › `targetname` › `target` › classname
    pub label: String,
    pub target: Option<String>,
    pub targetname: Option<String>,
    pub message: Option<String>,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// triângulos soltos (9 floats cada) das faces do modelo; se o modelo não tem face
    /// (só clip), é a caixa do AABB (12 triângulos)
    pub positions: Vec<f32>,
    pub triangles: usize,
    /// `positions` veio do AABB, não de faces reais
    pub from_box: bool,
}

/// Faces de uma textura invisível, agrupadas.
#[derive(Debug, Clone, Serialize)]
pub struct InvisibleSurface {
    pub texture: String,
    pub color: String,
    pub faces: usize,
    pub positions: Vec<f32>,
    pub triangles: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZoneLegend {
    pub kind: ZoneKind,
    pub label: &'static str,
    pub color: &'static str,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct VolumeReport {
    pub volumes: Vec<Volume>,
    pub invisible: Vec<InvisibleSurface>,
    pub legend: Vec<ZoneLegend>,
    /// zonas por tipo
    pub counts: BTreeMap<String, usize>,
    /// algum limite de triângulos cortou a malha
    pub truncated: bool,
}

fn invisible_color(texture: &str) -> &'static str {
    match texture {
        "clip" | "clipbevel" => "#ef5350",
        "null" => "#9e9e9e",
        "aaatrigger" | "trigger" => "#b388ff",
        "origin" => "#ffee58",
        "hint" | "skip" => "#66bb6a",
        "sky" => "#4dd0e1",
        _ => "#bdbdbd",
    }
}

fn fan(points: &[[f32; 3]], out: &mut Vec<f32>) -> usize {
    let mut n = 0;
    for i in 1..points.len().saturating_sub(1) {
        for p in [points[0], points[i], points[i + 1]] {
            out.extend_from_slice(&p);
        }
        n += 1;
    }
    n
}

fn box_triangles(mins: [f32; 3], maxs: [f32; 3]) -> Vec<f32> {
    let c = |x: usize, y: usize, z: usize| -> [f32; 3] {
        [if x == 0 { mins[0] } else { maxs[0] }, if y == 0 { mins[1] } else { maxs[1] }, if z == 0 { mins[2] } else { maxs[2] }]
    };
    // 6 faces, 2 triângulos cada
    let quads: [[[f32; 3]; 4]; 6] = [
        [c(0, 0, 0), c(1, 0, 0), c(1, 1, 0), c(0, 1, 0)],
        [c(0, 0, 1), c(0, 1, 1), c(1, 1, 1), c(1, 0, 1)],
        [c(0, 0, 0), c(0, 0, 1), c(1, 0, 1), c(1, 0, 0)],
        [c(0, 1, 0), c(1, 1, 0), c(1, 1, 1), c(0, 1, 1)],
        [c(0, 0, 0), c(0, 1, 0), c(0, 1, 1), c(0, 0, 1)],
        [c(1, 0, 0), c(1, 0, 1), c(1, 1, 1), c(1, 1, 0)],
    ];
    let mut out = Vec::with_capacity(6 * 18);
    for q in quads {
        fan(&q, &mut out);
    }
    out
}

fn faces_of(bsp: &Bsp, first: i32, count: i32) -> &[Face] {
    let (Ok(first), Ok(count)) = (usize::try_from(first), usize::try_from(count)) else { return &[] };
    let end = first.saturating_add(count).min(bsp.faces.len());
    bsp.faces.get(first..end).unwrap_or(&[])
}

pub fn legend() -> Vec<ZoneLegend> {
    [
        ZoneKind::Trigger,
        ZoneKind::BuyZone,
        ZoneKind::BombTarget,
        ZoneKind::HostageRescue,
        ZoneKind::VipSafety,
        ZoneKind::EscapeZone,
        ZoneKind::Ladder,
        ZoneKind::Water,
    ]
    .into_iter()
    .map(|kind| ZoneLegend { kind, label: kind.label(), color: kind.color() })
    .collect()
}

/// Contagem barata (sem malha): zonas por classname e faces de textura invisível.
pub fn counts(bsp: &Bsp, parsed: &[Entity]) -> (BTreeMap<String, usize>, usize) {
    let mut zones: BTreeMap<String, usize> = BTreeMap::new();
    for e in parsed {
        let class = e.get("classname").map(String::as_str).unwrap_or("");
        if classify(class).is_some() && brush_model_of(e).is_some_and(|n| n > 0 && n < bsp.models.len()) {
            *zones.entry(class.to_string()).or_insert(0) += 1;
        }
    }
    let invisible = bsp.faces.iter().filter(|f| bsp.texture_name_of(f).is_some_and(render::is_invisible)).count();
    (zones, invisible)
}

pub fn collect(bsp: &Bsp, parsed: &[Entity]) -> VolumeReport {
    let mut report = VolumeReport { legend: legend(), ..Default::default() };
    let mut total = 0usize;
    // modelos já tratados como zona: suas faces não entram no grupo "invisível"
    let mut claimed: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for (entity, e) in parsed.iter().enumerate() {
        let class = e.get("classname").map(String::as_str).unwrap_or("");
        let (Some(kind), Some(n)) = (classify(class), brush_model_of(e)) else { continue };
        let Some(m) = bsp.models.get(n) else { continue };
        if n == 0 || report.volumes.len() >= MAX_VOLUMES {
            continue;
        }
        claimed.insert(n);

        let mut positions = Vec::new();
        let mut triangles = 0usize;
        for face in faces_of(bsp, m.first_face, m.num_faces) {
            if triangles >= MAX_TRIS_PER_VOLUME || total + triangles >= MAX_TRIS_TOTAL {
                report.truncated = true;
                break;
            }
            if let Some(points) = bsp.face_polygon(face) {
                triangles += fan(&points, &mut positions);
            }
        }
        let from_box = triangles == 0;
        if from_box {
            positions = box_triangles(m.mins, m.maxs);
            triangles = positions.len() / 9;
        }
        total += triangles;

        let get = |k: &str| e.get(k).map(|s| s.trim()).filter(|s| !s.is_empty()).map(str::to_string);
        let (target, targetname, message) = (get("target"), get("targetname"), get("message"));
        let label = message.clone().or_else(|| targetname.clone()).or_else(|| target.clone()).unwrap_or_else(|| class.to_string());
        *report.counts.entry(class.to_string()).or_insert(0) += 1;
        report.volumes.push(Volume {
            entity,
            model: n as u32,
            classname: class.to_string(),
            kind,
            kind_label: kind.label(),
            color: color_of(class, kind).to_string(),
            label,
            target,
            targetname,
            message,
            mins: m.mins,
            maxs: m.maxs,
            positions,
            triangles,
            from_box,
        });
    }

    // superfícies invisíveis: faces de textura marcada, em modelos que não são zona
    let mut models_of_face: Vec<Option<usize>> = vec![None; bsp.faces.len()];
    for (mi, m) in bsp.models.iter().enumerate() {
        // num_faces vem do arquivo: o laço percorre só o que existe de fato
        let first = usize::try_from(m.first_face).unwrap_or(usize::MAX);
        let end = first.saturating_add(usize::try_from(m.num_faces).unwrap_or(0)).min(bsp.faces.len());
        for slot in models_of_face.get_mut(first..end).unwrap_or(&mut []) {
            slot.get_or_insert(mi);
        }
    }
    let mut groups: BTreeMap<String, InvisibleSurface> = BTreeMap::new();
    for (fi, face) in bsp.faces.iter().enumerate() {
        let Some(name) = bsp.texture_name_of(face) else { continue };
        if !render::is_invisible(name) {
            continue;
        }
        if models_of_face[fi].is_some_and(|m| claimed.contains(&m)) {
            continue;
        }
        let Some(points) = bsp.face_polygon(face) else { continue };
        let key = name.to_ascii_lowercase();
        let g = groups.entry(key.clone()).or_insert_with(|| InvisibleSurface {
            color: invisible_color(&key).to_string(),
            texture: key.clone(),
            faces: 0,
            positions: Vec::new(),
            triangles: 0,
        });
        g.faces += 1;
        if total >= MAX_TRIS_TOTAL {
            report.truncated = true;
            continue;
        }
        let t = fan(&points, &mut g.positions);
        g.triangles += t;
        total += t;
    }
    report.invisible = groups.into_values().collect();
    report
}

/// Lê o mapa e devolve zonas + superfícies invisíveis (comando `map_volumes`).
pub fn volumes(path: &Path) -> Result<VolumeReport, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("não leu o arquivo: {e}"))?;
    let bsp = Bsp::parse(&bytes).map_err(|e| e.to_string())?;
    drop(bytes);
    Ok(collect(&bsp, &entities::parse(&bsp.entities_raw)))
}
