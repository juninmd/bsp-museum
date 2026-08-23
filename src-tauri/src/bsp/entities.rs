use serde::Serialize;
use std::collections::BTreeMap;

/// Uma entidade do lump: pares chave/valor, na ordem em que aparecem.
pub type Entity = BTreeMap<String, String>;

/// Lê o texto do lump de entidades. Formato: blocos `{ "chave" "valor" ... }`.
/// Tolerante de propósito: mapa velho tem lixo entre blocos e aspas soltas.
pub fn parse(text: &str) -> Vec<Entity> {
    let mut out = Vec::new();
    let mut current: Option<Entity> = None;
    let mut pending_key: Option<String> = None;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '{' => current = Some(Entity::new()),
            '}' => {
                if let Some(entity) = current.take() {
                    if !entity.is_empty() {
                        out.push(entity);
                    }
                }
                pending_key = None;
            }
            '"' => {
                let mut token = String::new();
                for c in chars.by_ref() {
                    if c == '"' {
                        break;
                    }
                    token.push(c);
                }
                match (&mut current, pending_key.take()) {
                    (Some(entity), Some(key)) => {
                        entity.insert(key, token);
                    }
                    (Some(_), None) => pending_key = Some(token),
                    // Aspas fora de bloco: ignora em vez de abortar o mapa inteiro.
                    (None, _) => {}
                }
            }
            _ => {}
        }
    }
    out
}

pub fn origin_of(entity: &Entity) -> Option<[f32; 3]> {
    let raw = entity.get("origin")?;
    let mut parts = raw.split_whitespace().map(|p| p.parse::<f32>().ok());
    Some([parts.next()??, parts.next()??, parts.next()??])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GameMode {
    /// de_ — plantar a bomba
    Bomb,
    /// cs_ — resgate de reféns
    Hostage,
    /// as_ — escolta do VIP
    Assassination,
    /// es_ — fuga
    Escape,
    /// mapa de Zombie Plague / deathmatch: só spawns
    Zombie,
    Deathmatch,
    Unknown,
}

impl GameMode {
    pub fn label(self) -> &'static str {
        match self {
            GameMode::Bomb => "de_ · bomba",
            GameMode::Hostage => "cs_ · reféns",
            GameMode::Assassination => "as_ · VIP",
            GameMode::Escape => "es_ · fuga",
            GameMode::Zombie => "zombie plague",
            GameMode::Deathmatch => "deathmatch",
            GameMode::Unknown => "indefinido",
        }
    }
}

/// Prefixo do nome do arquivo — é ele que o CS usa para decidir o modo.
pub fn mode_from_prefix(map_name: &str) -> GameMode {
    let lower = map_name.to_ascii_lowercase();
    if lower.starts_with("de_") {
        GameMode::Bomb
    } else if lower.starts_with("cs_") {
        GameMode::Hostage
    } else if lower.starts_with("as_") {
        GameMode::Assassination
    } else if lower.starts_with("es_") {
        GameMode::Escape
    } else if lower.starts_with("zm_") || lower.starts_with("zp_") || lower.starts_with("bio_") {
        GameMode::Zombie
    } else {
        GameMode::Unknown
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SpawnPoint {
    pub team: &'static str,
    pub position: [f32; 3],
}

#[derive(Debug, Clone, Serialize)]
pub struct EntitySummary {
    /// "message" do worldspawn — o título que o mapper deu
    pub title: Option<String>,
    pub sky: Option<String>,
    pub wads: Vec<String>,
    pub total: usize,
    /// classname -> quantidade, do mais comum para o menos
    pub histogram: Vec<(String, usize)>,
    pub ct_spawns: usize,
    pub t_spawns: usize,
    pub spawns: Vec<SpawnPoint>,
    pub buy_zones: usize,
    pub bomb_targets: usize,
    pub hostages: usize,
    pub rescue_zones: usize,
    pub vip_starts: usize,
    pub vip_safety: usize,
    pub escape_zones: usize,
    pub mode_by_entities: GameMode,
}

fn count_of(hist: &BTreeMap<String, usize>, class: &str) -> usize {
    hist.get(class).copied().unwrap_or(0)
}

/// Nome do WAD sem o caminho do disco de quem compilou (`\half-life\valve\x.wad`).
fn wad_basename(path: &str) -> Option<String> {
    let cleaned = path.trim().replace('\\', "/");
    if cleaned.is_empty() {
        return None;
    }
    let name = cleaned.rsplit('/').next().unwrap_or(&cleaned);
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

pub fn summarize(entities: &[Entity]) -> EntitySummary {
    let mut hist: BTreeMap<String, usize> = BTreeMap::new();
    let mut spawns = Vec::new();
    let mut title = None;
    let mut sky = None;
    let mut wads = Vec::new();

    for entity in entities {
        let class = entity.get("classname").cloned().unwrap_or_default();
        if class.is_empty() {
            continue;
        }
        *hist.entry(class.clone()).or_insert(0) += 1;

        if class == "worldspawn" {
            title = entity.get("message").filter(|m| !m.trim().is_empty()).cloned();
            sky = entity.get("skyname").filter(|s| !s.trim().is_empty()).cloned();
            if let Some(raw) = entity.get("wad") {
                wads = raw.split(';').filter_map(wad_basename).collect();
                wads.sort();
                wads.dedup();
            }
        }

        let team = match class.as_str() {
            "info_player_start" => Some("CT"),
            "info_player_deathmatch" => Some("T"),
            "info_vip_start" => Some("VIP"),
            _ => None,
        };
        if let (Some(team), Some(position)) = (team, origin_of(entity)) {
            spawns.push(SpawnPoint { team, position });
        }
    }

    let ct_spawns = count_of(&hist, "info_player_start");
    let t_spawns = count_of(&hist, "info_player_deathmatch");
    let bomb_targets = count_of(&hist, "func_bomb_target") + count_of(&hist, "info_bomb_target");
    let hostages = count_of(&hist, "hostage_entity");
    let rescue_zones =
        count_of(&hist, "func_hostage_rescue") + count_of(&hist, "info_hostage_rescue");
    let vip_starts = count_of(&hist, "info_vip_start");
    let vip_safety = count_of(&hist, "func_vip_safetyzone");
    let escape_zones = count_of(&hist, "func_escapezone") + count_of(&hist, "info_map_parameters");

    // O que as entidades dizem, independente do nome do arquivo. A divergência
    // entre isto e o prefixo é exatamente o bug que faz "o round nunca acabar".
    let mode_by_entities = if bomb_targets > 0 {
        GameMode::Bomb
    } else if hostages > 0 && rescue_zones > 0 {
        GameMode::Hostage
    } else if vip_starts > 0 && vip_safety > 0 {
        GameMode::Assassination
    } else if count_of(&hist, "func_escapezone") > 0 {
        GameMode::Escape
    } else if ct_spawns + t_spawns > 0 {
        GameMode::Deathmatch
    } else {
        GameMode::Unknown
    };

    let mut histogram: Vec<(String, usize)> = hist.into_iter().collect();
    histogram.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    EntitySummary {
        title,
        sky,
        wads,
        total: entities.len(),
        histogram,
        ct_spawns,
        t_spawns,
        spawns,
        buy_zones: count_of_class(entities, "func_buyzone"),
        bomb_targets,
        hostages,
        rescue_zones,
        vip_starts,
        vip_safety,
        escape_zones,
        mode_by_entities,
    }
}

fn count_of_class(entities: &[Entity], class: &str) -> usize {
    entities
        .iter()
        .filter(|e| e.get("classname").map(String::as_str) == Some(class))
        .count()
}
