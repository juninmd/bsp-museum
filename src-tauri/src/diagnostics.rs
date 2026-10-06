//! Regras de diagnóstico: o que faz o round não terminar, o jogador não comprar,
//! o servidor travar ou o mapa carregar como deathmatch.
//!
//! Três camadas, da mais barata à mais cara:
//! - `entity_findings`: só entidades e resumo (roda na varredura da pasta inteira);
//! - `bsp_findings`: geometria/árvore/limites do motor (precisa do BSP aberto);
//! - `disk_findings`: confere WADs, modelos e sons na pasta do mod.

use crate::bsp::entities::{EntitySummary, GameMode};
use crate::bsp::tree::CONTENTS_SOLID;
use crate::bsp::{wad, Bsp, LUMP_COUNT};
use crate::catalog::MapSummary;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    Warn,
    Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub id: &'static str,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    pub hint: String,
    /// valores dinâmicos do achado — o frontend monta o texto no idioma escolhido
    pub args: Vec<String>,
}

impl Finding {
    fn new(id: &'static str, severity: Severity, title: String, detail: String, hint: &str, args: Vec<String>) -> Self {
        Self { id, severity, title, detail, hint: hint.to_string(), args }
    }
}

/// Regras que dependem só das entidades e do resumo do arquivo. `slots == 0`
/// desliga `poucos-spawns` (a varredura da pasta não sabe o número de slots).
pub fn entity_findings(summary: &MapSummary, ents: &EntitySummary, slots: usize) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut push = |id: &'static str, severity: Severity, title: String, detail: String, hint: &str, args: Vec<String>| {
        out.push(Finding::new(id, severity, title, detail, hint, args));
    };

    match summary.mode {
        GameMode::Bomb if ents.bomb_targets == 0 => push(
            "de-sem-bomb-target",
            Severity::Critical,
            "Prefixo de_ sem func_bomb_target".into(),
            "O CS entra em modo bomba pelo prefixo, mas não existe alvo para plantar.".into(),
            "O round nunca termina por objetivo — só por tempo ou eliminação. Adicione func_bomb_target (ou info_bomb_target) e um info_map_parameters se quiser afinar o tempo.",
            vec![],
        ),
        GameMode::Hostage if ents.hostages == 0 => push(
            "cs-sem-refem",
            Severity::Critical,
            "Prefixo cs_ sem hostage_entity".into(),
            "Mapa de resgate sem refém nenhum.".into(),
            "Sem hostage_entity o objetivo não existe. Adicione os reféns e a func_hostage_rescue correspondente.",
            vec![],
        ),
        GameMode::Hostage if ents.rescue_zones == 0 && ents.ct_spawns == 0 => push(
            "cs-sem-resgate",
            Severity::Warn,
            "Reféns sem zona de resgate e sem spawn CT".into(),
            format!("{} refém(ns), nenhuma zona de resgate e nenhum spawn de CT.", ents.hostages),
            "O GoldSrc resgata refém perto de qualquer info_player_start — sem spawn de CT o resgate não tem para onde convergir.",
            vec![ents.hostages.to_string(), "warn".into()],
        ),
        GameMode::Hostage if ents.rescue_zones == 0 => push(
            "cs-sem-resgate",
            Severity::Info,
            "Reféns sem zona de resgate explícita".into(),
            format!("{} refém(ns), nenhuma func_/info_hostage_rescue.", ents.hostages),
            "Não é erro: sem zona, o motor resgata o refém quando ele fica a menos de 256u de qualquer spawn de CT (fallback oficial do GoldSrc).",
            vec![ents.hostages.to_string(), "info".into()],
        ),
        GameMode::Assassination if ents.vip_starts == 0 || ents.vip_safety == 0 => push(
            "as-incompleto",
            Severity::Critical,
            "Prefixo as_ incompleto".into(),
            format!("info_vip_start={}, func_vip_safetyzone={}", ents.vip_starts, ents.vip_safety),
            "Modo VIP precisa dos dois: um spawn de VIP e ao menos uma zona segura.",
            vec![ents.vip_starts.to_string(), ents.vip_safety.to_string()],
        ),
        _ => {}
    }

    // O caminho inverso: as entidades prometem um modo que o nome não entrega.
    if ents.mode_by_entities != GameMode::Unknown
        && ents.mode_by_entities != GameMode::Deathmatch
        && summary.mode != ents.mode_by_entities
    {
        push(
            "prefixo-divergente",
            Severity::Warn,
            format!(
                "Entidades de {} num arquivo {}",
                ents.mode_by_entities.label(),
                summary.mode.label()
            ),
            "O CS decide o modo pelo prefixo do arquivo, não pelo conteúdo.".into(),
            "Renomeie o BSP para o prefixo certo, ou o objetivo montado no mapa nunca será usado.",
            vec![mode_id(ents.mode_by_entities).into(), mode_id(summary.mode).into()],
        );
    }

    let total_spawns = ents.ct_spawns + ents.t_spawns;
    if total_spawns == 0 {
        push(
            "sem-spawn",
            Severity::Critical,
            "Nenhum ponto de spawn".into(),
            "Nem info_player_start nem info_player_deathmatch.".into(),
            "Ninguém entra. Este BSP não sobe em servidor.",
            vec![],
        );
    } else {
        if ents.ct_spawns == 0 || ents.t_spawns == 0 {
            push(
                "spawn-de-um-time-so",
                Severity::Critical,
                "Spawn de um time só".into(),
                format!("CT={} · T={}", ents.ct_spawns, ents.t_spawns),
                "O time sem spawn não consegue entrar em jogo. Em mapa de Zombie Plague isso às vezes é proposital, mas confira.",
                vec![ents.ct_spawns.to_string(), ents.t_spawns.to_string()],
            );
        }
        if total_spawns < slots {
            push(
                "poucos-spawns",
                Severity::Warn,
                format!("{total_spawns} spawns para {slots} slots"),
                format!("CT={} · T={}", ents.ct_spawns, ents.t_spawns),
                "Com mais jogadores que spawns, o servidor empilha gente no mesmo ponto (telefrag e queda de FPS no início do round).",
                vec![total_spawns.to_string(), slots.to_string(), ents.ct_spawns.to_string(), ents.t_spawns.to_string()],
            );
        }
    }

    if summary.fullbright {
        push(
            "fullbright",
            Severity::Warn,
            "Mapa sem lightmap (fullbright)".into(),
            "O lump de iluminação está vazio.".into(),
            "Ou faltou rodar o RAD na compilação, ou houve leak. Visualmente é aquele mapa chapado, sem sombra.",
            vec![],
        );
    }

    if matches!(summary.mode, GameMode::Bomb | GameMode::Hostage | GameMode::Assassination)
        && ents.buy_zones == 0
    {
        push(
            "sem-buyzone",
            Severity::Warn,
            "Mapa competitivo sem func_buyzone".into(),
            "Nenhuma zona de compra encontrada.".into(),
            "Sem buyzone o jogador só tem a pistola inicial. Em mapa de zumbi isso costuma ser intencional.",
            vec![],
        );
    }

    if summary.file_size > 8 * 1024 * 1024 {
        let mb = format!("{:.1}", summary.file_size as f64 / 1024.0 / 1024.0);
        push(
            "mapa-pesado",
            Severity::Info,
            format!("{mb} MB"),
            "Mapa grande para download de jogador em servidor público.".into(),
            "Acima de ~8 MB muita gente desiste no meio do download. Vale checar o que domina os lumps.",
            vec![mb],
        );
    }

    out
}

pub fn mode_id(mode: GameMode) -> &'static str {
    match mode {
        GameMode::Bomb => "bomb",
        GameMode::Hostage => "hostage",
        GameMode::Assassination => "assassination",
        GameMode::Escape => "escape",
        GameMode::Zombie => "zombie",
        GameMode::Deathmatch => "deathmatch",
        GameMode::Unknown => "unknown",
    }
}

/// Limites do compilador/motor do GoldSrc (valores de referência do HLCSG/HLBSP e
/// do `MAX_EDICTS` do CS 1.6): passar do teto quebra o mapa, chegar perto avisa.
/// `(nome, lump, tamanho do registro, limite)`.
const LUMP_LIMITS: [(&str, usize, usize, usize); 10] = [
    ("models", 14, 64, 400),
    ("planes", 1, 20, 32767),
    ("vertexes", 3, 12, 65535),
    ("nodes", 5, 24, 32767),
    ("leaves", 10, 28, 8192),
    ("clipnodes", 9, 8, 32767),
    ("faces", 7, 20, 65535),
    ("edges", 12, 4, 256000),
    ("marksurfaces", 11, 2, 65535),
    ("texinfo", 6, 40, 8192),
];
const ENTITY_LIMIT: usize = 1800;
const ENTSTRING_LIMIT: usize = 128 * 1024;
/// a partir de que fração do teto o aviso aparece
const LIMIT_WARN_RATIO: f64 = 0.9;

/// `(nome, usado, limite)` de tudo que o mapa consome do motor.
pub fn engine_usage(bsp: &Bsp, ents: &EntitySummary) -> Vec<(&'static str, usize, usize)> {
    let mut usage: Vec<(&'static str, usize, usize)> = LUMP_LIMITS
        .iter()
        .filter(|(_, lump, _, _)| *lump < LUMP_COUNT)
        .map(|(name, lump, stride, limit)| (*name, bsp.lumps[*lump].length / stride, *limit))
        .collect();
    usage.push(("entities", ents.total, ENTITY_LIMIT));
    usage.push(("entstring", bsp.lumps[0].length, ENTSTRING_LIMIT));
    usage
}

/// Regras que precisam do BSP aberto: WAD não declarado, vis ausente, spawn em
/// sólido e limites do motor.
pub fn bsp_findings(ents: &EntitySummary, bsp: &Bsp) -> Vec<Finding> {
    let mut out = Vec::new();

    let from_wad = bsp.textures.iter().filter(|t| !t.embedded).count();
    if from_wad > 0 && ents.wads.is_empty() {
        out.push(Finding::new(
            "wad-nao-declarado",
            Severity::Warn,
            format!("{from_wad} textura(s) de WAD, mas o worldspawn não lista nenhum"),
            "A chave \"wad\" do worldspawn está vazia.".into(),
            "Quem não tiver o WAD certo vê tudo rosa e preto. Ou embuta as texturas, ou declare o WAD.",
            vec![from_wad.to_string()],
        ));
    }

    // Spawn dentro de sólido (ou fora do mundo, que também é sólido): o jogador
    // nasce preso ou morre no primeiro frame.
    let stuck: Vec<&crate::bsp::entities::SpawnPoint> = ents
        .spawns
        .iter()
        .filter(|s| bsp.contents_at(s.position) == Some(CONTENTS_SOLID))
        .collect();
    if let Some(first) = stuck.first() {
        let at = format!("{:.0} {:.0} {:.0}", first.position[0], first.position[1], first.position[2]);
        out.push(Finding::new(
            "spawn-em-solido",
            Severity::Critical,
            format!("{} spawn(s) dentro de parede ou fora do mapa", stuck.len()),
            format!("Primeiro em ({at}) — {}.", first.team),
            "O jogador nasce preso. Mova o info_player_* para dentro do espaço jogável (e confira que o mapa não vazou).",
            vec![stuck.len().to_string(), at, first.team.to_string()],
        ));
    }

    // Leak: o VIS se recusa a rodar num mapa vazado e deixa o lump vazio. É o
    // sinal mais confiável que dá para ler do arquivo compilado.
    if bsp.leaves.len() > 1 && bsp.visibility.is_empty() {
        out.push(Finding::new(
            "sem-vis",
            Severity::Warn,
            "Sem dados de visibilidade (VIS)".into(),
            "O lump de visibilidade está vazio num mapa com folhas.".into(),
            "Ou o mapa vazou (leak) e o VIS não rodou, ou foi compilado sem VIS. Sem ele o motor desenha tudo a toda hora: FPS cai e o mapa pesa.",
            vec![],
        ));
    }

    let mut over = Vec::new();
    let mut near = Vec::new();
    for (name, used, limit) in engine_usage(bsp, ents) {
        let text = format!("{name} {used}/{limit}");
        if used > limit {
            over.push(text);
        } else if used as f64 >= limit as f64 * LIMIT_WARN_RATIO {
            near.push(text);
        }
    }
    if !over.is_empty() || !near.is_empty() {
        let severity = if over.is_empty() { Severity::Warn } else { Severity::Critical };
        let all: Vec<String> = over.iter().chain(near.iter()).cloned().collect();
        out.push(Finding::new(
            "limite-motor",
            severity,
            if over.is_empty() {
                format!("Perto do limite do motor ({})", all.len())
            } else {
                format!("Estoura limite do motor ({})", over.len())
            },
            all.join(" · "),
            "Passar do teto faz o mapa não carregar ou o servidor cair; perto do teto, qualquer edição pode quebrá-lo. Simplifique brushes, junte entidades ou divida o mapa.",
            vec![over.len().to_string(), all.join(" · ")],
        ));
    }

    out
}

/// Regras que olham a pasta do mod: WAD ausente, textura que nenhum WAD tem e
/// recurso referenciado (modelo/som/sprite) que não está no disco.
pub fn disk_findings(
    path: &Path,
    ents: &EntitySummary,
    bsp: &Bsp,
    report: &crate::resources::ResourceReport,
) -> Vec<Finding> {
    let mut out = Vec::new();
    let wads = wad::WadSet::for_map(path, &ents.wads);

    if wads.loaded() == 0 {
        if !ents.wads.is_empty() && bsp.textures.iter().any(|t| !t.embedded) {
            out.push(Finding::new(
                "wad-nao-encontrado",
                Severity::Info,
                "Nenhum WAD achado ao lado do mapa".into(),
                format!("Declarados: {}", ents.wads.join(", ")),
                "Os WADs são procurados na pasta do mod (a que contém maps/). Fora dela não dá para conferir as texturas.",
                vec![ents.wads.join(", ")],
            ));
        }
    } else {
        let mut missing: Vec<String> = bsp
            .textures
            .iter()
            .filter(|t| !t.embedded && !crate::bsp::render::is_invisible(&t.name) && !t.name.is_empty())
            .filter(|t| !wads.contains(&t.name))
            .map(|t| t.name.clone())
            .collect();
        missing.sort();
        missing.dedup();
        if !missing.is_empty() {
            let sample: Vec<&str> = missing.iter().take(6).map(String::as_str).collect();
            out.push(Finding::new(
                "textura-ausente",
                Severity::Warn,
                format!("{} textura(s) que nenhum WAD achado possui", missing.len()),
                sample.join(", "),
                "Quem joga vê rosa e preto nessas faces. Inclua o WAD que as contém, ou embuta as texturas no BSP.",
                vec![missing.len().to_string(), sample.join(", ")],
            ));
        }
    }

    let gone: Vec<&str> = report
        .items
        .iter()
        .filter(|i| !i.found && matches!(i.kind, "model" | "sprite" | "sound"))
        .map(|i| i.path.as_str())
        .collect();
    if !gone.is_empty() {
        let sample: Vec<&str> = gone.iter().take(6).copied().collect();
        out.push(Finding::new(
            "recurso-ausente",
            Severity::Info,
            format!("{} modelo(s)/som(ns)/sprite(s) referenciado(s) e ausente(s)", gone.len()),
            sample.join(", "),
            "O mapa pede esses arquivos e eles não estão na pasta do mod. Se forem do jogo base, ignore; se forem customizados, faltam no pacote do servidor.",
            vec![gone.len().to_string(), sample.join(", ")],
        ));
    }

    out
}
