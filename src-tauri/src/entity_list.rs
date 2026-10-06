//! Lista de entidades do mapa, com posição — o que o painel clicável usa para
//! levar a câmera 3D até a entidade.

use crate::bsp::entities::{origin_of, Entity};
use crate::bsp::Bsp;
use serde::Serialize;

const MAX_ROWS: usize = 6000;

#[derive(Debug, Clone, Serialize)]
pub struct EntityRow {
    pub index: usize,
    pub classname: String,
    pub targetname: Option<String>,
    /// `origin` da entidade, ou o centro do modelo quando é brush entity (`model "*N"`)
    pub origin: Option<[f32; 3]>,
    pub model: Option<String>,
    pub keys: Vec<(String, String)>,
}

pub fn list(bsp: &Bsp, parsed: &[Entity]) -> Vec<EntityRow> {
    parsed
        .iter()
        .enumerate()
        .take(MAX_ROWS)
        .map(|(index, e)| {
            let model = e.get("model").cloned();
            let origin = origin_of(e).or_else(|| {
                let n: usize = model.as_deref()?.strip_prefix('*')?.parse().ok()?;
                let m = bsp.models.get(n)?;
                Some([
                    (m.mins[0] + m.maxs[0]) / 2.0,
                    (m.mins[1] + m.maxs[1]) / 2.0,
                    (m.mins[2] + m.maxs[2]) / 2.0,
                ])
            });
            EntityRow {
                index,
                classname: e.get("classname").cloned().unwrap_or_default(),
                targetname: e.get("targetname").filter(|t| !t.is_empty()).cloned(),
                origin,
                model,
                keys: e.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            }
        })
        .collect()
}
