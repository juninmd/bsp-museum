//! Testes do parser e do renderizador.
//!
//! Os BSPs de teste são montados byte a byte aqui: dá para descrever exatamente
//! o mapa (um chão quadrado, uma rampa, um lump corrompido) sem depender de
//! nenhum arquivo de 3 MB no repositório.

use crate::bsp::entities::{self, GameMode};
use crate::bsp::reader::BspError;
use crate::bsp::render::{self, RenderOptions};
use crate::bsp::{Bsp, Lump, GOLDSRC_VERSION, LUMP_COUNT};
use crate::catalog;

const LUMP_ENTITIES: usize = 0;
const LUMP_TEXTURES: usize = 2;
const LUMP_VERTEXES: usize = 3;
const LUMP_TEXINFO: usize = 6;
const LUMP_FACES: usize = 7;
const LUMP_LIGHTING: usize = 8;
const LUMP_EDGES: usize = 12;
const LUMP_SURFEDGES: usize = 13;
const LUMP_MODELS: usize = 14;

#[derive(Default)]
struct BspBuilder {
    entities: String,
    vertices: Vec<[f32; 3]>,
    edges: Vec<(u16, u16)>,
    surfedges: Vec<i32>,
    /// (first_edge, num_edges, texinfo)
    faces: Vec<(i32, u16, u16)>,
    texinfo: Vec<u32>,
    /// (nome, embutida)
    textures: Vec<(&'static str, bool)>,
    models: Vec<([f32; 3], [f32; 3])>,
    lighting: usize,
    version: i32,
}

impl BspBuilder {
    fn new() -> Self {
        Self { version: GOLDSRC_VERSION, ..Default::default() }
    }

    /// Quadrado no plano XY, com normal para cima (chão).
    fn floor(mut self, x0: f32, y0: f32, x1: f32, y1: f32, z: f32, texinfo: u16) -> Self {
        let base = self.vertices.len() as u16;
        // Ordem anti-horária vista de cima => normal +Z pelo método de Newell.
        self.vertices.push([x0, y0, z]);
        self.vertices.push([x1, y0, z]);
        self.vertices.push([x1, y1, z]);
        self.vertices.push([x0, y1, z]);

        let edge_base = self.edges.len() as i32;
        self.edges.push((base, base + 1));
        self.edges.push((base + 1, base + 2));
        self.edges.push((base + 2, base + 3));
        self.edges.push((base + 3, base));

        let first_surfedge = self.surfedges.len() as i32;
        for i in 0..4 {
            self.surfedges.push(edge_base + i);
        }
        self.faces.push((first_surfedge, 4, texinfo));
        self
    }

    /// Parede vertical: normal horizontal, então não é chão nem teto.
    fn wall(mut self, x: f32, y0: f32, y1: f32, z0: f32, z1: f32, texinfo: u16) -> Self {
        let base = self.vertices.len() as u16;
        self.vertices.push([x, y0, z0]);
        self.vertices.push([x, y1, z0]);
        self.vertices.push([x, y1, z1]);
        self.vertices.push([x, y0, z1]);

        let edge_base = self.edges.len() as i32;
        self.edges.push((base, base + 1));
        self.edges.push((base + 1, base + 2));
        self.edges.push((base + 2, base + 3));
        self.edges.push((base + 3, base));

        let first_surfedge = self.surfedges.len() as i32;
        for i in 0..4 {
            self.surfedges.push(edge_base + i);
        }
        self.faces.push((first_surfedge, 4, texinfo));
        self
    }

    /// Chão desenhado com surfedges negativos (aresta percorrida ao contrário).
    fn floor_reversed_edges(mut self, z: f32) -> Self {
        let base = self.vertices.len() as u16;
        self.vertices.push([0.0, 0.0, z]);
        self.vertices.push([64.0, 0.0, z]);
        self.vertices.push([64.0, 64.0, z]);
        self.vertices.push([0.0, 64.0, z]);

        // Aresta 0 reservada: `-(0)` em Rust é `0`, positivo — colidiria com
        // "aresta 0 percorrida para a frente" e o teste não validaria a inversão.
        self.edges.push((base, base));

        let edge_base = self.edges.len() as i32;
        // Arestas gravadas invertidas de propósito.
        self.edges.push((base + 1, base));
        self.edges.push((base + 2, base + 1));
        self.edges.push((base + 3, base + 2));
        self.edges.push((base, base + 3));

        let first_surfedge = self.surfedges.len() as i32;
        for i in 0..4 {
            self.surfedges.push(-(edge_base + i));
        }
        self.faces.push((first_surfedge, 4, 0));
        self
    }

    fn texture(mut self, name: &'static str, embedded: bool) -> Self {
        self.textures.push((name, embedded));
        self.texinfo.push((self.textures.len() - 1) as u32);
        self
    }

    fn entities(mut self, text: &str) -> Self {
        self.entities = text.to_string();
        self
    }

    fn model(mut self, mins: [f32; 3], maxs: [f32; 3]) -> Self {
        self.models.push((mins, maxs));
        self
    }

    fn lighting(mut self, bytes: usize) -> Self {
        self.lighting = bytes;
        self
    }

    fn version(mut self, v: i32) -> Self {
        self.version = v;
        self
    }

    fn build(self) -> Vec<u8> {
        let mut lumps: Vec<Vec<u8>> = vec![Vec::new(); LUMP_COUNT];

        let mut ents = self.entities.into_bytes();
        ents.push(0);
        lumps[LUMP_ENTITIES] = ents;

        for v in &self.vertices {
            for c in v {
                lumps[LUMP_VERTEXES].extend_from_slice(&c.to_le_bytes());
            }
        }
        for (a, b) in &self.edges {
            lumps[LUMP_EDGES].extend_from_slice(&a.to_le_bytes());
            lumps[LUMP_EDGES].extend_from_slice(&b.to_le_bytes());
        }
        for se in &self.surfedges {
            lumps[LUMP_SURFEDGES].extend_from_slice(&se.to_le_bytes());
        }
        for (first_edge, num_edges, texinfo) in &self.faces {
            let f = &mut lumps[LUMP_FACES];
            f.extend_from_slice(&0u16.to_le_bytes()); // planenum
            f.extend_from_slice(&0u16.to_le_bytes()); // side
            f.extend_from_slice(&first_edge.to_le_bytes());
            f.extend_from_slice(&num_edges.to_le_bytes());
            f.extend_from_slice(&texinfo.to_le_bytes());
            f.extend_from_slice(&[0u8; 4]); // styles
            f.extend_from_slice(&(-1i32).to_le_bytes()); // lightofs
        }
        for miptex in &self.texinfo {
            let t = &mut lumps[LUMP_TEXINFO];
            t.extend_from_slice(&[0u8; 32]); // vecs
            t.extend_from_slice(&miptex.to_le_bytes());
            t.extend_from_slice(&0u32.to_le_bytes()); // flags
        }

        if !self.textures.is_empty() {
            let count = self.textures.len();
            let table = 4 + count * 4;
            let mut body = Vec::new();
            let mut offsets = Vec::new();
            for (name, embedded) in &self.textures {
                offsets.push((table + body.len()) as i32);
                let mut raw = [0u8; 16];
                let bytes = name.as_bytes();
                raw[..bytes.len().min(15)].copy_from_slice(&bytes[..bytes.len().min(15)]);
                body.extend_from_slice(&raw);
                body.extend_from_slice(&64u32.to_le_bytes()); // width
                body.extend_from_slice(&64u32.to_le_bytes()); // height
                // offset do primeiro mip: 0 = vem de WAD externo
                body.extend_from_slice(&(if *embedded { 40u32 } else { 0u32 }).to_le_bytes());
                body.extend_from_slice(&[0u8; 12]); // offsets[1..4]
            }
            let tex = &mut lumps[LUMP_TEXTURES];
            tex.extend_from_slice(&(count as u32).to_le_bytes());
            for o in offsets {
                tex.extend_from_slice(&o.to_le_bytes());
            }
            tex.extend_from_slice(&body);
        }

        for (mins, maxs) in &self.models {
            let m = &mut lumps[LUMP_MODELS];
            for c in mins {
                m.extend_from_slice(&c.to_le_bytes());
            }
            for c in maxs {
                m.extend_from_slice(&c.to_le_bytes());
            }
            m.extend_from_slice(&[0u8; 12]); // origin
            m.extend_from_slice(&[0u8; 16]); // headnode
            m.extend_from_slice(&0i32.to_le_bytes()); // visleafs
            m.extend_from_slice(&0i32.to_le_bytes()); // firstface
            m.extend_from_slice(&(self.faces.len() as i32).to_le_bytes());
        }

        lumps[LUMP_LIGHTING] = vec![7u8; self.lighting];

        let header_size = 4 + LUMP_COUNT * 8;
        let mut out = Vec::new();
        out.extend_from_slice(&self.version.to_le_bytes());
        let mut offset = header_size;
        for lump in &lumps {
            out.extend_from_slice(&(offset as i32).to_le_bytes());
            out.extend_from_slice(&(lump.len() as i32).to_le_bytes());
            offset += lump.len();
        }
        for lump in &lumps {
            out.extend_from_slice(lump);
        }
        out
    }
}

const SIMPLE_ENTITIES: &str = r#"{
"classname" "worldspawn"
"message" "Mapa de Teste"
"skyname" "desert"
"wad" "\half-life\valve\halflife.wad;\half-life\cstrike\cstrike.wad"
}
{
"classname" "info_player_start"
"origin" "32 32 8"
}
{
"classname" "info_player_deathmatch"
"origin" "96 96 8"
}
"#;

fn simple_map() -> Vec<u8> {
    BspBuilder::new()
        .texture("concrete", true)
        .floor(0.0, 0.0, 128.0, 128.0, 0.0, 0)
        .model([0.0, 0.0, 0.0], [128.0, 128.0, 64.0])
        .entities(SIMPLE_ENTITIES)
        .lighting(1024)
        .build()
}

// ---------------------------------------------------------------- cabeçalho

#[test]
fn le_cabecalho_valido() {
    let (version, lumps) = Bsp::header(&simple_map()).expect("cabeçalho válido");
    assert_eq!(version, GOLDSRC_VERSION);
    assert!(lumps[LUMP_VERTEXES].length > 0);
}

#[test]
fn recusa_versao_de_outro_engine() {
    // 38 é Quake 2; 46 é Quake 3. Nenhum dos dois é GoldSrc.
    let data = BspBuilder::new().version(38).model([0.0; 3], [1.0; 3]).build();
    assert_eq!(Bsp::header(&data), Err(BspError::BadVersion(38)));
}

#[test]
fn recusa_arquivo_truncado() {
    let data = simple_map();
    let err = Bsp::header(&data[..40]).unwrap_err();
    assert!(matches!(err, BspError::TooSmall { .. }));
}

#[test]
fn recusa_arquivo_vazio() {
    assert!(Bsp::header(&[]).is_err());
}

#[test]
fn lump_apontando_fora_do_arquivo_vira_erro() {
    let mut data = simple_map();
    // Aponta o lump de vértices para muito além do fim do arquivo.
    let vertex_lump_offset = 4 + LUMP_VERTEXES * 8;
    data[vertex_lump_offset..vertex_lump_offset + 4]
        .copy_from_slice(&(900_000i32).to_le_bytes());
    let err = Bsp::parse(&data).unwrap_err();
    assert!(matches!(err, BspError::LumpOutOfBounds { .. }), "esperava fora de faixa, veio {err:?}");
}

#[test]
fn lump_com_tamanho_quebrado_vira_erro() {
    let mut data = simple_map();
    // 13 bytes não é múltiplo de 12 (tamanho de um vértice).
    let len_field = 4 + LUMP_VERTEXES * 8 + 4;
    data[len_field..len_field + 4].copy_from_slice(&13i32.to_le_bytes());
    let err = Bsp::parse(&data).unwrap_err();
    assert!(matches!(err, BspError::LumpMisaligned { stride: 12, .. }), "veio {err:?}");
}

#[test]
fn arquivo_de_lixo_nao_causa_panic() {
    for len in [1usize, 7, 64, 200, 1000] {
        let junk: Vec<u8> = (0..len).map(|i| (i * 37 % 251) as u8).collect();
        let _ = Bsp::header(&junk);
        let _ = Bsp::parse(&junk);
    }
}

// ---------------------------------------------------------------- geometria

#[test]
fn le_geometria_e_texturas() {
    let bsp = Bsp::parse(&simple_map()).expect("mapa válido");
    assert_eq!(bsp.vertices.len(), 4);
    assert_eq!(bsp.faces.len(), 1);
    assert_eq!(bsp.textures.len(), 1);
    assert_eq!(bsp.textures[0].name, "concrete");
    assert!(bsp.textures[0].embedded);
}

#[test]
fn textura_sem_pixel_embutido_e_marcada_como_wad() {
    let data = BspBuilder::new()
        .texture("de_dust_wall", false)
        .floor(0.0, 0.0, 64.0, 64.0, 0.0, 0)
        .model([0.0; 3], [64.0, 64.0, 32.0])
        .build();
    let bsp = Bsp::parse(&data).unwrap();
    assert!(!bsp.textures[0].embedded);
}

#[test]
fn monta_poligono_da_face() {
    let bsp = Bsp::parse(&simple_map()).unwrap();
    let poly = bsp.face_polygon(&bsp.faces[0]).expect("polígono");
    assert_eq!(poly.len(), 4);
    assert_eq!(poly[0], [0.0, 0.0, 0.0]);
    assert_eq!(poly[2], [128.0, 128.0, 0.0]);
}

#[test]
fn surfedge_negativo_inverte_a_aresta() {
    let data = BspBuilder::new()
        .texture("concrete", true)
        .floor_reversed_edges(0.0)
        .model([0.0; 3], [64.0, 64.0, 32.0])
        .build();
    let bsp = Bsp::parse(&data).unwrap();
    let poly = bsp.face_polygon(&bsp.faces[0]).expect("polígono");
    // Com a inversão aplicada, o percurso volta a ser o do quadrado original.
    assert_eq!(poly[0], [0.0, 0.0, 0.0]);
    assert_eq!(poly[1], [64.0, 0.0, 0.0]);
}

#[test]
fn face_com_indice_invalido_devolve_none_em_vez_de_estourar() {
    let mut builder = BspBuilder::new().texture("concrete", true);
    builder.faces.push((999, 4, 0)); // aponta para surfedge inexistente
    let data = builder.model([0.0; 3], [64.0; 3]).build();
    let bsp = Bsp::parse(&data).unwrap();
    assert!(bsp.face_polygon(&bsp.faces[0]).is_none());
}

// ---------------------------------------------------------------- entidades

#[test]
fn le_pares_chave_valor() {
    let ents = entities::parse(SIMPLE_ENTITIES);
    assert_eq!(ents.len(), 3);
    assert_eq!(ents[0].get("classname").map(String::as_str), Some("worldspawn"));
    assert_eq!(ents[1].get("origin").map(String::as_str), Some("32 32 8"));
}

#[test]
fn ignora_lixo_entre_blocos() {
    let ents = entities::parse("\n\0 lixo \n{\n\"classname\" \"info_player_start\"\n}\nsobra");
    assert_eq!(ents.len(), 1);
}

#[test]
fn ignora_aspas_soltas_fora_de_bloco() {
    let ents = entities::parse("\"orfa\" {\"classname\" \"light\"}");
    assert_eq!(ents.len(), 1);
    assert_eq!(ents[0].get("classname").map(String::as_str), Some("light"));
}

#[test]
fn bloco_vazio_nao_vira_entidade() {
    assert!(entities::parse("{}{}").is_empty());
}

#[test]
fn le_origem_como_vetor() {
    let ents = entities::parse("{\"origin\" \"-32 64.5 8\"}");
    assert_eq!(entities::origin_of(&ents[0]), Some([-32.0, 64.5, 8.0]));
}

#[test]
fn origem_malformada_devolve_none() {
    let ents = entities::parse("{\"origin\" \"32 abc\"}");
    assert_eq!(entities::origin_of(&ents[0]), None);
}

#[test]
fn resume_worldspawn_e_spawns() {
    let summary = entities::summarize(&entities::parse(SIMPLE_ENTITIES));
    assert_eq!(summary.title.as_deref(), Some("Mapa de Teste"));
    assert_eq!(summary.sky.as_deref(), Some("desert"));
    assert_eq!(summary.ct_spawns, 1);
    assert_eq!(summary.t_spawns, 1);
    assert_eq!(summary.spawns.len(), 2);
}

#[test]
fn wad_perde_o_caminho_de_quem_compilou() {
    let summary = entities::summarize(&entities::parse(SIMPLE_ENTITIES));
    assert_eq!(summary.wads, vec!["cstrike.wad", "halflife.wad"]);
}

#[test]
fn histograma_vem_do_mais_comum_para_o_menos() {
    let text = "{\"classname\" \"light\"}{\"classname\" \"light\"}{\"classname\" \"func_door\"}";
    let summary = entities::summarize(&entities::parse(text));
    assert_eq!(summary.histogram[0], ("light".to_string(), 2));
}

#[test]
fn prefixo_define_o_modo() {
    assert_eq!(entities::mode_from_prefix("de_dust2"), GameMode::Bomb);
    assert_eq!(entities::mode_from_prefix("cs_assault"), GameMode::Hostage);
    assert_eq!(entities::mode_from_prefix("as_oilrig"), GameMode::Assassination);
    assert_eq!(entities::mode_from_prefix("es_frantic"), GameMode::Escape);
    assert_eq!(entities::mode_from_prefix("zm_dust"), GameMode::Zombie);
    assert_eq!(entities::mode_from_prefix("bio_beach"), GameMode::Zombie);
    assert_eq!(entities::mode_from_prefix("fy_pool_day"), GameMode::Unknown);
}

#[test]
fn entidades_definem_o_modo_independente_do_nome() {
    let bomba = entities::summarize(&entities::parse("{\"classname\" \"func_bomb_target\"}"));
    assert_eq!(bomba.mode_by_entities, GameMode::Bomb);

    let refem = entities::summarize(&entities::parse(
        "{\"classname\" \"hostage_entity\"}{\"classname\" \"func_hostage_rescue\"}",
    ));
    assert_eq!(refem.mode_by_entities, GameMode::Hostage);

    let vip = entities::summarize(&entities::parse(
        "{\"classname\" \"info_vip_start\"}{\"classname\" \"func_vip_safetyzone\"}",
    ));
    assert_eq!(vip.mode_by_entities, GameMode::Assassination);
}

#[test]
fn refem_so_com_hostage_ja_e_modo_de_resgate() {
    // Sem zona explícita o GoldSrc resgata perto de spawn CT: logo basta ter
    // hostage_entity para o modo ser Hostage (não deixa virar deathmatch).
    let summary = entities::summarize(&entities::parse("{\"classname\" \"hostage_entity\"}"));
    assert_eq!(summary.mode_by_entities, GameMode::Hostage);
}

// ---------------------------------------------------------------- render

#[test]
fn planta_desenha_o_chao() {
    let bsp = Bsp::parse(&simple_map()).unwrap();
    let ents = entities::summarize(&entities::parse(&bsp.entities_raw));
    let out = render::top_down(&bsp, &ents.spawns, RenderOptions::detail());
    assert_eq!(out.polygons, 1);
    assert!(out.svg.contains("<polygon"));
    assert!(out.svg.starts_with("<svg"));
    assert!(out.svg.ends_with("</svg>"));
}

#[test]
fn miniatura_nao_desenha_spawn_nem_parede() {
    let data = BspBuilder::new()
        .texture("concrete", true)
        .floor(0.0, 0.0, 128.0, 128.0, 0.0, 0)
        .wall(0.0, 0.0, 128.0, 0.0, 64.0, 0)
        .model([0.0, 0.0, 0.0], [128.0, 128.0, 64.0])
        .entities(SIMPLE_ENTITIES)
        .build();
    let bsp = Bsp::parse(&data).unwrap();
    let ents = entities::summarize(&entities::parse(&bsp.entities_raw));
    let thumb = render::top_down(&bsp, &ents.spawns, RenderOptions::thumbnail());
    assert_eq!(thumb.polygons, 1, "só o chão entra na miniatura");
    assert!(!thumb.svg.contains("<circle"));

    let detail = render::top_down(&bsp, &ents.spawns, RenderOptions::detail());
    assert_eq!(detail.polygons, 2, "a vista detalhada inclui a parede");
    assert!(detail.svg.contains("<circle"), "e os spawns");
}

#[test]
fn textura_invisivel_nao_entra_na_planta() {
    let data = BspBuilder::new()
        .texture("concrete", true)
        .texture("aaatrigger", true)
        .texture("clip", true)
        .floor(0.0, 0.0, 128.0, 128.0, 0.0, 0)
        .floor(0.0, 0.0, 128.0, 128.0, 16.0, 1)
        .floor(0.0, 0.0, 128.0, 128.0, 32.0, 2)
        .model([0.0, 0.0, 0.0], [128.0, 128.0, 64.0])
        .build();
    let bsp = Bsp::parse(&data).unwrap();
    let out = render::top_down(&bsp, &[], RenderOptions::detail());
    assert_eq!(out.polygons, 1, "clip e aaatrigger são volume invisível");
    assert_eq!(out.skipped, 2);
}

#[test]
fn andar_mais_alto_desenha_por_cima() {
    let data = BspBuilder::new()
        .texture("concrete", true)
        .floor(0.0, 0.0, 256.0, 256.0, 0.0, 0) // rua
        .floor(64.0, 64.0, 128.0, 128.0, 128.0, 0) // passarela
        // O teto do mundo é a própria passarela, então ela fica no topo do gradiente.
        .model([0.0, 0.0, 0.0], [256.0, 256.0, 128.0])
        .build();
    let bsp = Bsp::parse(&data).unwrap();
    let svg = render::top_down(&bsp, &[], RenderOptions::detail()).svg;
    let first = svg.find("<polygon").unwrap();
    let second = svg[first + 1..].find("<polygon").unwrap() + first + 1;
    // A cor do segundo polígono é a do topo do gradiente (altura máxima).
    assert!(svg[second..].contains("#f2c14e"), "a passarela precisa vir depois da rua");
}

#[test]
fn mapa_sem_modelo_devolve_svg_de_aviso_em_vez_de_erro() {
    let data = BspBuilder::new().texture("concrete", true).build();
    let bsp = Bsp::parse(&data).unwrap();
    let out = render::top_down(&bsp, &[], RenderOptions::detail());
    assert_eq!(out.polygons, 0);
    assert!(out.svg.contains("modelo 0"));
}

#[test]
fn teto_nunca_entra_na_planta() {
    // Mesmo quadrado, ordem invertida => normal para baixo.
    let mut builder = BspBuilder::new().texture("concrete", true);
    let base = builder.vertices.len() as u16;
    builder.vertices.push([0.0, 0.0, 64.0]);
    builder.vertices.push([0.0, 64.0, 64.0]);
    builder.vertices.push([64.0, 64.0, 64.0]);
    builder.vertices.push([64.0, 0.0, 64.0]);
    for i in 0..4u16 {
        builder.edges.push((base + i, base + (i + 1) % 4));
    }
    for i in 0..4i32 {
        builder.surfedges.push(i);
    }
    builder.faces.push((0, 4, 0));
    let data = builder.model([0.0; 3], [64.0, 64.0, 64.0]).build();
    let bsp = Bsp::parse(&data).unwrap();
    assert_eq!(render::top_down(&bsp, &[], RenderOptions::detail()).polygons, 0);
}

// ---------------------------------------------------------------- pixels de textura

/// Monta o lump de texturas cru (tabela + 1 miptex com pixels reais + paleta):
/// o suficiente pra chamar `bsp::texture_image` sem depender do `BspBuilder`
/// (que não grava pixel nenhum, só o cabeçalho da tabela).
fn texture_lump_with_pixels(name: &str, w: u32, h: u32, pixel_idx: u8, idx255_rgb: [u8; 3]) -> Vec<u8> {
    let mut raw_name = [0u8; 16];
    let bytes = name.as_bytes();
    raw_name[..bytes.len().min(15)].copy_from_slice(&bytes[..bytes.len().min(15)]);

    let mut body = Vec::new();
    body.extend_from_slice(&raw_name);
    body.extend_from_slice(&w.to_le_bytes());
    body.extend_from_slice(&h.to_le_bytes());
    body.extend_from_slice(&40u32.to_le_bytes()); // mip0 logo após o miptex_t (16+4*4+4*4)
    body.extend_from_slice(&[0u8; 12]); // offsets[1..4], não usados
    body.extend_from_slice(&vec![pixel_idx; (w * h) as usize]);

    let mut out = Vec::new();
    out.extend_from_slice(&1u32.to_le_bytes()); // count
    out.extend_from_slice(&8i32.to_le_bytes()); // offset da única entrada (4 + 1*4)
    out.extend_from_slice(&body);

    let mut palette = vec![0u8; 768];
    palette[765..768].copy_from_slice(&idx255_rgb);
    out.extend_from_slice(&palette);
    out
}

/// Descompacta o `data:image/png;base64,...` de volta em pixels RGBA planos.
/// Só entende o que `png_encode` gera (8-bit RGBA, filtro "None" em toda linha)
/// — não é um decoder de PNG geral, só o bastante pra provar o alpha do pixel.
fn decode_test_png(data_url: &str) -> Vec<u8> {
    let b64 = data_url.strip_prefix("data:image/png;base64,").expect("data URL de PNG");
    let png = base64_decode(b64);
    let mut pos = 8usize; // assinatura PNG
    let mut width = 0u32;
    let mut height = 0u32;
    let mut idat = Vec::new();
    while pos + 8 <= png.len() {
        let len = u32::from_be_bytes(png[pos..pos + 4].try_into().unwrap()) as usize;
        let kind = &png[pos + 4..pos + 8];
        let data = &png[pos + 8..pos + 8 + len];
        match kind {
            b"IHDR" => {
                width = u32::from_be_bytes(data[0..4].try_into().unwrap());
                height = u32::from_be_bytes(data[4..8].try_into().unwrap());
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }
        pos += 8 + len + 4; // + CRC
    }
    let mut scan = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::ZlibDecoder::new(&idat[..]), &mut scan).unwrap();
    let stride = width as usize * 4;
    let mut rgba = Vec::with_capacity(stride * height as usize);
    for row in scan.chunks(stride + 1) {
        rgba.extend_from_slice(&row[1..]); // pula o byte de filtro (sempre 0)
    }
    rgba
}

#[test]
fn textura_comum_nao_fura_no_indice_255() {
    let raw = texture_lump_with_pixels("wall01", 1, 1, 255, [200, 10, 10]);
    let lump = Lump { offset: 0, length: raw.len() };
    let img = crate::bsp::texture_image(&raw, lump, 0).expect("decodifica");
    let rgba = decode_test_png(&img.png);
    assert_eq!(rgba, vec![200, 10, 10, 255], "sem `{{`, o índice 255 é só mais uma cor");
}

/// Mesmo layout de `texture_lump_with_pixels`, mas com N texturas na tabela —
/// pra provar que `texture_image(texindex)` pega a entrada certa e não a
/// seguinte (bug real corrigido junto do gate de transparência).
fn texture_lump_multi(names_and_pixels: &[(&str, u8)]) -> Vec<u8> {
    let count = names_and_pixels.len();
    let table_size = 4 + count * 4;
    let mut bodies = Vec::new();
    let mut offsets = Vec::new();
    for (name, pixel_idx) in names_and_pixels {
        offsets.push((table_size + bodies.len()) as i32);
        let mut raw_name = [0u8; 16];
        let bytes = name.as_bytes();
        raw_name[..bytes.len().min(15)].copy_from_slice(&bytes[..bytes.len().min(15)]);
        bodies.extend_from_slice(&raw_name);
        bodies.extend_from_slice(&1u32.to_le_bytes()); // width
        bodies.extend_from_slice(&1u32.to_le_bytes()); // height
        bodies.extend_from_slice(&40u32.to_le_bytes()); // mip0
        bodies.extend_from_slice(&[0u8; 12]);
        bodies.push(*pixel_idx);
    }
    let mut out = Vec::new();
    out.extend_from_slice(&(count as u32).to_le_bytes());
    for o in offsets {
        out.extend_from_slice(&o.to_le_bytes());
    }
    out.extend_from_slice(&bodies);
    out.extend_from_slice(&[0u8; 768]); // paleta neutra (não é o que este teste checa)
    out
}

#[test]
fn texture_image_pega_a_entrada_certa_da_tabela_nao_a_seguinte() {
    let raw = texture_lump_multi(&[("wall01", 10), ("wall02", 20), ("wall03", 30)]);
    let lump = Lump { offset: 0, length: raw.len() };
    for (i, expected_name) in ["wall01", "wall02", "wall03"].into_iter().enumerate() {
        let img = crate::bsp::texture_image(&raw, lump, i).unwrap_or_else(|| panic!("textura {i}"));
        assert_eq!(img.name, expected_name, "texindex {i} devolveu a textura errada");
    }
}

#[test]
fn textura_chave_fura_no_indice_255() {
    let raw = texture_lump_with_pixels("{grade", 1, 1, 255, [200, 10, 10]);
    let lump = Lump { offset: 0, length: raw.len() };
    let img = crate::bsp::texture_image(&raw, lump, 0).expect("decodifica");
    let rgba = decode_test_png(&img.png);
    assert_eq!(rgba, vec![200, 10, 10, 0], "com `{{`, o índice 255 vira buraco");
}

// ---------------------------------------------------------------- catálogo

fn write_temp(name: &str, data: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("bsp-museum-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, data).unwrap();
    path
}

#[test]
fn resumo_le_so_o_necessario() {
    let path = write_temp("de_teste.bsp", &simple_map());
    let summary = catalog::summarize_file(&path);
    assert_eq!(summary.name, "de_teste");
    assert_eq!(summary.mode, GameMode::Bomb);
    assert_eq!(summary.title.as_deref(), Some("Mapa de Teste"));
    assert_eq!(summary.ct_spawns, 1);
    assert!(!summary.fullbright);
    assert!(summary.error.is_none());
    let bounds = summary.bounds.expect("bounds");
    assert_eq!(bounds.size, [128.0, 128.0, 64.0]);
}

#[test]
fn arquivo_invalido_vira_erro_no_card_em_vez_de_derrubar_a_varredura() {
    let path = write_temp("quebrado.bsp", b"nao sou um bsp");
    let summary = catalog::summarize_file(&path);
    assert!(summary.error.is_some());
    assert_eq!(summary.name, "quebrado");
}

#[test]
fn lump_de_iluminacao_vazio_marca_fullbright() {
    let data = BspBuilder::new()
        .texture("concrete", true)
        .floor(0.0, 0.0, 64.0, 64.0, 0.0, 0)
        .model([0.0; 3], [64.0, 64.0, 32.0])
        .entities(SIMPLE_ENTITIES)
        .build();
    let path = write_temp("fullbright.bsp", &data);
    assert!(catalog::summarize_file(&path).fullbright);
}

#[test]
fn varredura_acha_bsp_em_subpasta() {
    let root = std::env::temp_dir().join("bsp-museum-scan");
    let sub = root.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(root.join("a.bsp"), simple_map()).unwrap();
    std::fs::write(sub.join("b.BSP"), simple_map()).unwrap();
    std::fs::write(root.join("leia-me.txt"), b"nao e mapa").unwrap();

    let found = catalog::find_bsp_files(&root);
    assert_eq!(found.len(), 2, "extensão maiúscula também conta");
    let scanned = catalog::scan(&root);
    assert_eq!(scanned.len(), 2);
    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------- diagnóstico

fn findings_for(name: &str, entity_text: &str, lighting: usize) -> Vec<catalog::Finding> {
    let data = BspBuilder::new()
        .texture("concrete", true)
        .floor(0.0, 0.0, 128.0, 128.0, 0.0, 0)
        .model([0.0, 0.0, 0.0], [128.0, 128.0, 64.0])
        .entities(entity_text)
        .lighting(lighting)
        .build();
    let path = write_temp(name, &data);
    let bsp = Bsp::parse(&data).unwrap();
    let ents = entities::summarize(&entities::parse(entity_text));
    let summary = catalog::summarize_file(&path);
    catalog::findings(&summary, &ents, &bsp, 32)
}

fn spawns_text(ct: usize, t: usize) -> String {
    let mut text = String::from("{\"classname\" \"worldspawn\" \"wad\" \"x.wad\"}");
    for i in 0..ct {
        text.push_str(&format!(
            "{{\"classname\" \"info_player_start\" \"origin\" \"{i} 0 0\"}}"
        ));
    }
    for i in 0..t {
        text.push_str(&format!(
            "{{\"classname\" \"info_player_deathmatch\" \"origin\" \"{i} 64 0\"}}"
        ));
    }
    text
}

fn has(findings: &[catalog::Finding], id: &str) -> bool {
    findings.iter().any(|f| f.id == id)
}

#[test]
fn de_sem_alvo_de_bomba_e_critico() {
    let findings = findings_for("de_sem_alvo.bsp", &spawns_text(16, 16), 512);
    let finding = findings.iter().find(|f| f.id == "de-sem-bomb-target").expect("achado");
    assert_eq!(finding.severity, catalog::Severity::Critical);
    assert!(finding.hint.contains("nunca termina"));
}

#[test]
fn de_com_alvo_de_bomba_passa() {
    let mut text = spawns_text(16, 16);
    text.push_str("{\"classname\" \"func_bomb_target\"}{\"classname\" \"func_buyzone\"}");
    let findings = findings_for("de_ok.bsp", &text, 512);
    assert!(!has(&findings, "de-sem-bomb-target"));
    assert!(!has(&findings, "sem-buyzone"));
}

#[test]
fn cs_sem_refem_e_critico() {
    let findings = findings_for("cs_vazio.bsp", &spawns_text(16, 16), 512);
    assert!(has(&findings, "cs-sem-refem"));
}

#[test]
fn refem_sem_zona_de_resgate_e_info_com_fallback() {
    let mut text = spawns_text(16, 16);
    text.push_str("{\"classname\" \"hostage_entity\"}");
    let findings = findings_for("cs_semzona.bsp", &text, 512);
    let finding = findings.iter().find(|f| f.id == "cs-sem-resgate").expect("achado");
    // Sem zona explícita o motor resgata perto de spawn CT: é info, não erro.
    assert_eq!(finding.severity, catalog::Severity::Info);
    assert!(finding.hint.contains("fallback"));
}

#[test]
fn entidade_de_bomba_em_arquivo_zm_avisa_divergencia() {
    let mut text = spawns_text(16, 16);
    text.push_str("{\"classname\" \"func_bomb_target\"}");
    let findings = findings_for("zm_confuso.bsp", &text, 512);
    let finding = findings.iter().find(|f| f.id == "prefixo-divergente").expect("achado");
    assert!(finding.hint.contains("Renomeie"));
}

#[test]
fn mapa_sem_spawn_nenhum_e_critico() {
    let findings = findings_for("zm_vazio.bsp", "{\"classname\" \"worldspawn\"}", 512);
    let finding = findings.iter().find(|f| f.id == "sem-spawn").expect("achado");
    assert_eq!(finding.severity, catalog::Severity::Critical);
}

#[test]
fn spawn_de_um_time_so_e_critico() {
    let findings = findings_for("zm_um_time.bsp", &spawns_text(32, 0), 512);
    assert!(has(&findings, "spawn-de-um-time-so"));
}

#[test]
fn menos_spawns_que_slots_vira_aviso() {
    let findings = findings_for("zm_pequeno.bsp", &spawns_text(6, 6), 512);
    let finding = findings.iter().find(|f| f.id == "poucos-spawns").expect("achado");
    assert!(finding.title.contains("12 spawns para 32 slots"));
}

#[test]
fn spawns_suficientes_nao_geram_aviso() {
    let findings = findings_for("zm_grande.bsp", &spawns_text(20, 20), 512);
    assert!(!has(&findings, "poucos-spawns"));
}

#[test]
fn lightmap_vazio_vira_aviso_de_fullbright() {
    let findings = findings_for("zm_escuro.bsp", &spawns_text(16, 16), 0);
    let finding = findings.iter().find(|f| f.id == "fullbright").expect("achado");
    assert!(finding.hint.contains("RAD"));
}

#[test]
fn textura_de_wad_sem_wad_declarado_avisa() {
    let data = BspBuilder::new()
        .texture("de_dust_wall", false)
        .floor(0.0, 0.0, 128.0, 128.0, 0.0, 0)
        .model([0.0, 0.0, 0.0], [128.0, 128.0, 64.0])
        .entities("{\"classname\" \"worldspawn\"}{\"classname\" \"info_player_start\" \"origin\" \"0 0 0\"}{\"classname\" \"info_player_deathmatch\" \"origin\" \"1 0 0\"}")
        .lighting(64)
        .build();
    let path = write_temp("zm_semwad.bsp", &data);
    let bsp = Bsp::parse(&data).unwrap();
    let ents = entities::summarize(&entities::parse(&bsp.entities_raw));
    let findings = catalog::findings(&catalog::summarize_file(&path), &ents, &bsp, 2);
    assert!(has(&findings, "wad-nao-declarado"));
}

#[test]
fn mapa_de_zumbi_sem_buyzone_nao_e_cobrado() {
    let findings = findings_for("zm_semloja.bsp", &spawns_text(20, 20), 512);
    assert!(!has(&findings, "sem-buyzone"), "buyzone só é cobrado em de_/cs_/as_");
}

#[test]
fn detalhe_traz_planta_diagnostico_e_lumps() {
    let path = write_temp("de_detalhe.bsp", &simple_map());
    let detail = catalog::detail(&path, RenderOptions::detail(), 32).expect("detalhe");
    assert!(detail.svg.contains("<svg"));
    assert_eq!(detail.face_count, 1);
    assert_eq!(detail.textures, vec!["concrete"]);
    assert!(detail.lumps.iter().any(|l| l.name == "vertexes" && l.length > 0));
    assert!(has(&detail.findings, "de-sem-bomb-target"));
}

/// Passada sobre um acervo real, para além dos BSPs sintéticos.
///
/// ```text
/// $env:BSP_MUSEUM_MAPS="D:\...\cstrike\maps"
/// cargo test acervo_real -- --ignored --nocapture
/// ```
/// Grava um SVG de amostra em `BSP_MUSEUM_OUT` quando a variável existe.
#[test]
#[ignore = "precisa de uma pasta de mapas de verdade"]
fn acervo_real() {
    let Ok(dir) = std::env::var("BSP_MUSEUM_MAPS") else {
        eprintln!("defina BSP_MUSEUM_MAPS");
        return;
    };
    let root = std::path::PathBuf::from(dir);
    let files = catalog::find_bsp_files(&root);
    assert!(!files.is_empty(), "nenhum .bsp em {}", root.display());

    let started = std::time::Instant::now();
    let summaries = catalog::scan(&root);
    let scan_ms = started.elapsed().as_millis();

    let com_erro: Vec<_> = summaries.iter().filter(|s| s.error.is_some()).collect();
    println!("varredura: {} mapas em {scan_ms} ms", summaries.len());
    for s in &com_erro {
        println!("  ! {}: {}", s.name, s.error.as_deref().unwrap_or(""));
    }

    let mut renderizados = 0usize;
    let mut vazios = Vec::new();
    let mut falhas = Vec::new();
    let started = std::time::Instant::now();
    for summary in summaries.iter().filter(|s| s.error.is_none()) {
        let path = std::path::PathBuf::from(&summary.path);
        match catalog::detail(&path, RenderOptions::thumbnail(), 32) {
            Ok(detail) => {
                if detail.polygons == 0 {
                    vazios.push(summary.name.clone());
                } else {
                    renderizados += 1;
                }
            }
            Err(err) => falhas.push(format!("{}: {err}", summary.name)),
        }
    }
    let render_ms = started.elapsed().as_millis();
    println!("plantas: {renderizados} desenhadas em {render_ms} ms");
    if !vazios.is_empty() {
        println!("  sem polígono: {}", vazios.join(", "));
    }
    for f in &falhas {
        println!("  ! {f}");
    }

    if let Ok(out) = std::env::var("BSP_MUSEUM_OUT") {
        let _ = std::fs::create_dir_all(&out);
        for summary in summaries.iter().filter(|s| s.error.is_none()).take(3) {
            if let Ok(detail) =
                catalog::detail(&std::path::PathBuf::from(&summary.path), RenderOptions::detail(), 32)
            {
                let file = std::path::Path::new(&out).join(format!("{}.svg", summary.name));
                let _ = std::fs::write(&file, &detail.svg);
                println!("amostra: {}", file.display());
            }
        }
    }

    // O acervo pode ter mapa de outra engine; o que não pode é o parser quebrar
    // em massa nem devolver planta vazia para a maioria.
    let validos = summaries.len() - com_erro.len();
    assert!(
        renderizados * 10 >= validos * 9,
        "menos de 90% dos mapas válidos renderizaram ({renderizados} de {validos})"
    );
}

#[test]
#[ignore = "precisa de uma pasta de mapas de verdade"]
fn malha_acervo() {
    let Ok(dir) = std::env::var("BSP_MUSEUM_MAPS") else {
        eprintln!("defina BSP_MUSEUM_MAPS");
        return;
    };
    let root = std::path::PathBuf::from(dir);
    let files = catalog::find_bsp_files(&root);
    assert!(!files.is_empty(), "nenhum .bsp em {}", root.display());

    let mut com_pixels = 0usize;
    let mut malhas = 0usize;
    let mut vazias = 0usize;
    let mut falhas = Vec::new();
    let mut total_tris = 0usize;
    let mut pior_payload = 0usize;
    let mut pior_nome = String::new();
    let mut wad_total = 0usize;
    let mut skyboxes = 0usize;
    let started = std::time::Instant::now();

    for file in &files {
        match catalog::mesh(file) {
            Ok(mesh) => {
                if mesh.triangles == 0 {
                    vazias += 1;
                    continue;
                }
                malhas += 1;
                total_tris += mesh.triangles;
                wad_total += mesh.wad_textures;
                if mesh.skybox.is_some() {
                    skyboxes += 1;
                }
                assert_eq!(
                    mesh.positions.len() % 9,
                    0,
                    "{}: posições não são triângulos",
                    file.display()
                );
                assert_eq!(
                    mesh.uvs.len() % 6,
                    0,
                    "{}: uvs não são triângulos",
                    file.display()
                );
                assert_eq!(mesh.texindex.len(), mesh.triangles);
                let payload: usize = mesh.textures.iter().map(|t| t.png.as_ref().map(|p| p.len()).unwrap_or(0)).sum();
                if payload > pior_payload {
                    pior_payload = payload;
                    pior_nome = file.file_stem().unwrap_or_default().to_string_lossy().to_string();
                }
                com_pixels += mesh.textures.iter().filter(|t| t.png.is_some()).count();
            }
            Err(err) => falhas.push(format!("{}: {err}", file.display())),
        }
    }

    let ms = started.elapsed().as_millis();
    println!(
        "malha: {malhas} geradas, {} triângulos, {vazias} vazias, {com_pixels} texturas com pixels, {wad_total} de WAD, {skyboxes} skybox, pior payload {:.1} MB ({pior_nome}), em {ms} ms",
        total_tris,
        pior_payload as f64 / (1024.0 * 1024.0)
    );
    for f in &falhas {
        println!("  ! {f}");
    }

    // O que não pode é o parser quebrar em massa: 90% das malhas precisam existir.
    assert!(
        malhas * 10 >= files.len() * 9,
        "menos de 90% dos mapas geraram malha ({malhas} de {})",
        files.len()
    );
}

#[test]
#[ignore = "precisa de uma pasta de mapas de verdade"]
fn dump_textures() {
    let Ok(dir) = std::env::var("BSP_MUSEUM_MAPS") else {
        eprintln!("defina BSP_MUSEUM_MAPS");
        return;
    };
    let Ok(out) = std::env::var("BSP_MUSEUM_OUT") else {
        eprintln!("defina BSP_MUSEUM_OUT");
        return;
    };
    let _ = std::fs::create_dir_all(&out);
    let root = std::path::PathBuf::from(dir);
    let files = catalog::find_bsp_files(&root);
    let mut dumped = 0usize;
    for file in files.iter().take(40) {
        if let Ok(mesh) = catalog::mesh(file) {
            for (i, tex) in mesh.textures.iter().enumerate() {
                if let Some(data_url) = &tex.png {
                    if data_url.len() < 64 {
                        continue;
                    }
                    let b64 = data_url.strip_prefix("data:image/png;base64,").unwrap_or(data_url);
                    let bytes = base64_decode(b64);
                    let name = format!(
                        "{}__{:02}__{}.png",
                        file.file_stem().unwrap_or_default().to_string_lossy(),
                        i,
                        sanitize(&tex.name)
                    );
                    let _ = std::fs::write(std::path::Path::new(&out).join(&name), bytes);
                    dumped += 1;
                    if dumped >= 6 {
                        return;
                    }
                }
            }
        }
    }
}

fn base64_decode(s: &str) -> Vec<u8> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .unwrap_or_default()
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect()
}

#[test]
#[ignore = "precisa de uma pasta de mapas de verdade"]
fn diagnostica_chao_ceu() {
    let Ok(dir) = std::env::var("BSP_MUSEUM_MAPS") else {
        eprintln!("defina BSP_MUSEUM_MAPS");
        return;
    };
    let root = std::path::PathBuf::from(dir);
    let files = catalog::find_bsp_files(&root);
    assert!(!files.is_empty());

    // 1) Um mapa real: o que usam as faces "de chão" (normal +Z) e por que somem.
    let path = &files[0];
    let bytes = std::fs::read(path).unwrap();
    let bsp = Bsp::parse(&bytes).unwrap();
    let parsed = crate::bsp::entities::parse(&bsp.entities_raw);
    let summary = crate::bsp::entities::summarize(&parsed);

    let mut floors = 0usize;
    let mut floor_invisible = 0usize;
    let mut floor_no_tex = 0usize;
    let mut floor_embedded = 0usize;
    let mut floor_names = std::collections::BTreeSet::new();
    for face in &bsp.faces {
        let Some(p) = bsp.face_polygon(face) else { continue };
        if p.len() < 3 {
            continue;
        }
        // Newell + eu só quero o sinal de Z.
        let mut nz = 0.0f32;
        for i in 0..p.len() {
            let a = p[i];
            let b = p[(i + 1) % p.len()];
            nz += (a[0] - b[0]) * (a[1] + b[1]);
        }
        if nz < 0.0 {
            continue; // teto
        }
        if nz.abs() < (f32::EPSILON * 100.0) {
            // parede vertical
            continue;
        }
        floors += 1;
        match bsp.texture_of(face) {
            Some(tex) => {
                floor_names.insert(tex.name.clone());
                if crate::bsp::render::is_invisible(&tex.name) {
                    floor_invisible += 1;
                } else if tex.embedded {
                    floor_embedded += 1;
                } else {
                    floor_no_tex += 1;
                }
            }
            None => floor_no_tex += 1,
        }
    }
    println!("diagno: mapa={} | chao(face +Z em cima)={} | invisivel={} | sem_textura={} | embutida={}", files[0].file_stem().unwrap_or_default().to_string_lossy(), floors, floor_invisible, floor_no_tex, floor_embedded);
    println!("diagno: nomes de textura do chao = {:?}", floor_names.into_iter().collect::<Vec<_>>());

    // 2) Na malha enviada ao frontend: a normal dos triângulos que estão num
    // plano baixo (y pequeno) aponta para cima (+Y) ou para baixo?
    let mesh = catalog::mesh(path).unwrap();
    let mut up = 0usize;
    let mut down = 0usize;
    let mut other = 0usize;
    for tri in 0..mesh.triangles {
        let p0 = tri * 9;
        let a = (&mesh.positions[p0], &mesh.positions[p0 + 1], &mesh.positions[p0 + 2]);
        let b = (&mesh.positions[p0 + 3], &mesh.positions[p0 + 4], &mesh.positions[p0 + 5]);
        let c = (&mesh.positions[p0 + 6], &mesh.positions[p0 + 7], &mesh.positions[p0 + 8]);
        let ymin = (*a.1).min(*b.1).min(*c.1);
        // Só olha o teto/chão (triângulos quase no mesmo plano horizontal).
        let dy1 = (*b.1 - *a.1).abs();
        let dy2 = (*c.1 - *a.1).abs();
        if dy1 > 0.5 || dy2 > 0.5 {
            other += 1;
            continue;
        }
        let (uax, uay, uaz) = (*b.0 - *a.0, *b.1 - *a.1, *b.2 - *a.2);
        let (ubx, uby, ubz) = (*c.0 - *a.0, *c.1 - *a.1, *c.2 - *a.2);
        let ny = uaz * ubx - uax * ubz; // componente Y do cross product
        if ny < 0.0 {
            down += 1;
        } else {
            up += 1;
        }
        let _ = ymin;
    }
    println!("diagno: tri perpendicular ao eixo Y: para_cima(+Y)={up} para_baixo(-Y)={down} (fora do plano)={other} | total={}", mesh.triangles);
    println!("diagno: texcom_pixels=`{}`", mesh.textures.iter().filter(|t| t.png.is_some()).count());
    println!("diagno: spawns={}", summary.spawns.len());
}

#[test]
#[ignore = "precisa de uma pasta de mapas de verdade"]
fn dump_sky() {
    let Ok(dir) = std::env::var("BSP_MUSEUM_MAPS") else {
        eprintln!("defina BSP_MUSEUM_MAPS");
        return;
    };
    let Ok(out) = std::env::var("BSP_MUSEUM_OUT") else {
        eprintln!("defina BSP_MUSEUM_OUT");
        return;
    };
    let _ = std::fs::create_dir_all(&out);
    let files = catalog::find_bsp_files(&std::path::PathBuf::from(dir));
    for file in files.iter().take(3) {
        if let Ok(mesh) = catalog::mesh(file) {
            let Some(sb) = &mesh.skybox else { continue };
            let base = file.file_stem().unwrap_or_default().to_string_lossy();
            let faces: [(&str, &String); 6] = [
                ("up", &sb.up),
                ("down", &sb.down),
                ("left", &sb.left),
                ("right", &sb.right),
                ("front", &sb.front),
                ("back", &sb.back),
            ];
            for (kind, data_url) in faces {
                if let Some(b64) = data_url.strip_prefix("data:image/png;base64,") {
                    let name = format!("{base}__sky_{kind}.png");
                    let _ = std::fs::write(std::path::Path::new(&out).join(&name), base64_decode(b64));
                }
            }
        }
    }
}

#[test]
#[ignore = "precisa de uma pasta de mapas de verdade"]
fn gera_prints_mapa() {
    let Ok(dir) = std::env::var("BSP_MUSEUM_MAPS") else {
        eprintln!("defina BSP_MUSEUM_MAPS");
        return;
    };
    let Ok(out) = std::env::var("BSP_MUSEUM_OUT") else {
        eprintln!("defina BSP_MUSEUM_OUT");
        return;
    };
    let _ = std::fs::create_dir_all(&out);
    let path = std::path::Path::new(&dir).join("de_dust2.bsp");
    let mesh = catalog::mesh(&path).expect("malha do de_dust2");

    // Planta: polígonos do SVG (já ordenados por desenho = painter's algorithm).
    let svg = render::top_down(
        &Bsp::parse(&std::fs::read(&path).unwrap()).unwrap(),
        &mesh.spawns,
        RenderOptions::detail(),
    )
    .svg;
    let plant = svg_to_png(&svg, 1280);
    let _ = std::fs::write(
        std::path::Path::new(&out).join("dust2-planta.png"),
        plant,
    );

    // 3D: isométrico por altura, com z-buffer, a partir da malha (vetor do app).
    let tri = mesh_triangles(&mesh);
    let png = render_isometric(&tri, &mesh, 1280);
    let _ = std::fs::write(std::path::Path::new(&out).join("dust2-3d.png"), png);

    println!("prints em: {}", out);
}

/// Converte o SVG da planta em PNG (rasterizeira de <polygon>).
fn svg_to_png(svg: &str, target_w: i32) -> Vec<u8> {
    let mut view = [0f32; 4];
    if let Some(vb) = attr(svg, "viewBox") {
        let p: Vec<f32> = vb.split_whitespace().map(|v| v.parse().unwrap_or(0.0)).collect();
        if p.len() == 4 {
            view = [p[0], p[1], p[2], p[3]];
        }
    }
    let vw = (view[2] - view[0]).max(1.0);
    let vh = (view[3] - view[1]).max(1.0);
    let scale = target_w as f32 / vw;
    let w = target_w as u32;
    let h = (vh * scale).round().max(1.0) as u32;

    let mut img = vec![0u8; (w * h * 4) as usize];
    for px in img.chunks_exact_mut(4) {
        px.copy_from_slice(&[13, 17, 23, 255]);
    }

    let mut zbuf = vec![f32::MAX; (w * h) as usize];
    let mut idx: i32 = 0;
    let mut cursor = svg;
    while let Some(start) = cursor.find("<polygon") {
        let after = &cursor[start..];
        let Some(pts) = attr(after, "points") else {
            cursor = &after[8..];
            continue;
        };
        let fill = attr(after, "fill")
            .and_then(|hx| u32::from_str_radix(hx.trim_start_matches('#'), 16).ok())
            .unwrap_or(0x8b98a5);
        let coords: Vec<(f32, f32)> = pts
            .split_whitespace()
            .filter_map(|pair| {
                let mut it = pair.split(',');
                Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
            })
            .collect();
        if coords.len() < 3 {
            cursor = &after[8..];
            continue;
        }
        // draw order = profundidade (painter's): o último do SVG fica por cima.
        // Por isso o mais recente precisa vencer o z-buffer => profundidade negativa.
        let depth = -(idx as f32);
        // projeta para a tela (y do SVG cresce para baixo).
        let s: Vec<(f32, f32)> = coords
            .iter()
            .map(|(x, y)| ((x - view[0]) * scale, (y - view[1]) * scale))
            .collect();
        let r = ((fill >> 16) & 0xff) as u8;
        let g = ((fill >> 8) & 0xff) as u8;
        let b = (fill & 0xff) as u8;
        // Triangula o polígono (fan a partir do 1º vértice) e preenche cada triângulo.
        for i in 1..s.len() - 1 {
            fill_tri(
                &mut img,
                &mut zbuf,
                w,
                h,
                &[s[0], s[i], s[i + 1]],
                &[depth, depth, depth],
                [r, g, b, 255],
            );
        }
        idx += 1;
        cursor = &after[7..];
    }

    // Paredes (traço fino) e spawns (bolinha) — como na vista detalhada do app.
    let mut c2 = svg;
    while let Some(start) = c2.find("<polyline") {
        let after = &c2[start..];
        let Some(pts) = attr(after, "points") else {
            c2 = &after[9..];
            continue;
        };
        let coords: Vec<(f32, f32)> = pts
            .split_whitespace()
            .filter_map(|pair| {
                let mut it = pair.split(',');
                Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
            })
            .collect();
        for i in 0..coords.len().saturating_sub(1) {
            let (x0, y0) = coords[i];
            let (x1, y1) = coords[i + 1];
            line_on(
                &mut img,
                w,
                h,
                (x0 - view[0]) * scale,
                (y0 - view[1]) * scale,
                (x1 - view[0]) * scale,
                (y1 - view[1]) * scale,
            );
        }
        c2 = &after[9..];
    }
    let mut c3 = svg;
    while let Some(start) = c3.find("<circle") {
        let after = &c3[start..];
        let cx = attr(after, "cx").and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let cy = attr(after, "cy").and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let r = attr(after, "r").and_then(|v| v.parse().ok()).unwrap_or(1.0);
        let fill = attr(after, "fill").unwrap_or_default();
        let crgb = hex_or(&fill, 0x4c7cf3);
        disc_on(
            &mut img,
            w,
            h,
            (cx - view[0]) * scale,
            (cy - view[1]) * scale,
            r * scale,
            crgb,
        );
        c3 = &after[7..];
    }
    crate::bsp::rgba_png(w as usize, h as usize, &img)
        .map(|url| base64_decode(url.strip_prefix("data:image/png;base64,").unwrap_or(&url)))
        .unwrap_or_default()
}

fn hex_or(s: &str, d: u32) -> u32 {
    let hx: String = s.trim_start_matches('#').chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    u32::from_str_radix(&hx, 16).unwrap_or(d)
}

/// Desenha uma linha na tela (espessura de ~1.4 px).
fn line_on(img: &mut [u8], w: u32, h: u32, x0: f32, y0: f32, x1: f32, y1: f32) {
    let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1.0) as i32;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = (x0 + (x1 - x0) * t).round() as i32;
        let y = (y0 + (y1 - y0) * t).round() as i32;
        if x >= 0 && x < w as i32 && y >= 0 && y < h as i32 {
            let di = (y as usize * w as usize + x as usize) * 4;
            img[di] = 139;
            img[di + 1] = 152;
            img[di + 2] = 165;
            img[di + 3] = 255;
        }
    }
}

/// Desenha um disco (spawn) preenchido.
fn disc_on(img: &mut [u8], w: u32, h: u32, cx: f32, cy: f32, r: f32, color: u32) {
    if r <= 0.0 {
        return;
    }
    let rr = (r as i32).max(1);
    for dy in -rr..=rr {
        for dx in -rr..=rr {
            if (dx as f32) * (dx as f32) + (dy as f32) * (dy as f32) > r * r {
                continue;
            }
            let x = (cx as i32) + dx;
            let y = (cy as i32) + dy;
            if x >= 0 && x < w as i32 && y >= 0 && y < h as i32 {
                let di = (y as usize * w as usize + x as usize) * 4;
                img[di] = ((color >> 16) & 0xff) as u8;
                img[di + 1] = ((color >> 8) & 0xff) as u8;
                img[di + 2] = (color & 0xff) as u8;
                img[di + 3] = 255;
            }
        }
    }
}

/// Triângulos da malha: [(vértice 3D goldsrc), [screen-ish]] — aqui mantemos 3D.
fn mesh_triangles(mesh: &crate::catalog::MeshDetail) -> Vec<([[f32; 3]; 3], usize)> {
    let mut out = Vec::with_capacity(mesh.triangles);
    for tri in 0..mesh.triangles {
        let p = tri * 9;
        let a = [mesh.positions[p], mesh.positions[p + 1], mesh.positions[p + 2]];
        let b = [mesh.positions[p + 3], mesh.positions[p + 4], mesh.positions[p + 5]];
        let c = [mesh.positions[p + 6], mesh.positions[p + 7], mesh.positions[p + 8]];
        out.push(([a, b, c], tri));
    }
    out
}

/// Renderiza isométrico por altura com z-buffer; devolve bytes de PNG.
fn render_isometric(
    tris: &[([[f32; 3]; 3], usize)],
    mesh: &crate::catalog::MeshDetail,
    target_w: i32,
) -> Vec<u8> {
    let b = mesh.bounds.as_ref().expect("bounds");
    let yaw = -0.75f32; // ~ -43°
    let pitch = 0.5f32; // ~ 29°
    let cp = pitch.cos();
    let fwd = [yaw.sin() * cp, pitch.sin(), -yaw.cos() * cp];
    let upv = [0.0f32, 1.0, 0.0];
    let right = norm(cross(fwd, upv));
    let up2 = norm(cross(right, fwd));

    let center = [
        (b.mins[0] + b.maxs[0]) / 2.0,
        (b.mins[1] + b.maxs[1]) / 2.0,
        (b.mins[2] + b.maxs[2]) / 2.0,
    ];
    // goldsrc Z-up -> Three Y-up: (x, z, -y)
    let to_world = |p: [f32; 3]| [p[0], p[2], -p[1]];
    let proj = |w: [f32; 3]| -> [f32; 3] {
        let d = [w[0] - center[0], w[1] - center[1], w[2] - center[2]];
        [dot(d, right), dot(d, up2), dot(d, fwd)]
    };
    let projected: Vec<([[f32; 3]; 3], [f32; 3])> = tris
        .iter()
        .map(|([a, b, c], _)| {
            let (wa, wb, wc) = (to_world(*a), to_world(*b), to_world(*c));
            ([proj(wa), proj(wb), proj(wc)], [a[2], b[2], c[2]])
        })
        .collect();

    let mut minx = f32::MAX;
    let mut maxx = f32::MIN;
    let mut miny = f32::MAX;
    let mut maxy = f32::MIN;
    for (t, _) in &projected {
        for v in t {
            minx = minx.min(v[0]);
            maxx = maxx.max(v[0]);
            miny = miny.min(v[1]);
            maxy = maxy.max(v[1]);
        }
    }
    let vw = (maxx - minx).max(1.0);
    let vh = (maxy - miny).max(1.0);
    let scale = target_w as f32 / vw;
    let wpx = target_w as u32;
    let hpx = (vh * scale).round().max(1.0) as u32;

    let mut img = vec![0u8; (wpx * hpx * 4) as usize];
    for px in img.chunks_exact_mut(4) {
        px.copy_from_slice(&[13, 17, 23, 255]);
    }
    let mut zbuf = vec![f32::MAX; (wpx * hpx) as usize];

    let zmin = b.mins[2];
    let zmax = b.maxs[2].max(zmin + 1.0);
    let map_to = |p: [f32; 3]| ((p[0] - minx) * scale, (p[1] - miny) * scale);

    for (t, gold) in &projected {
        let s: Vec<(f32, f32)> = t.iter().map(|v| map_to(*v)).collect();
        // cor pela altura goldsrc do triângulo
        let c = height_rgb_3(((gold[0] + gold[1] + gold[2]) / 3.0 - zmin) / (zmax - zmin));
        fill_tri(
            &mut img,
            &mut zbuf,
            wpx,
            hpx,
            &s,
            &[t[0][2], t[1][2], t[2][2]],
            [c[0], c[1], c[2], 255],
        );
    }
    crate::bsp::rgba_png(wpx as usize, hpx as usize, &img)
        .map(|url| base64_decode(url.strip_prefix("data:image/png;base64,").unwrap_or(&url)))
        .unwrap_or_default()
}

/// Altura -> cor (mesma paleta da planta).
fn height_rgb_3(t: f32) -> [u8; 3] {
    let low = [26u8, 42, 71];
    let mid = [31u8, 122, 140];
    let high = [242u8, 193, 78];
    let lerp = |a: [u8; 3], b: [u8; 3], k: f32| -> [u8; 3] {
        [
            (a[0] as f32 + (b[0] as f32 - a[0] as f32) * k) as u8,
            (a[1] as f32 + (b[1] as f32 - a[1] as f32) * k) as u8,
            (a[2] as f32 + (b[2] as f32 - a[2] as f32) * k) as u8,
        ]
    };
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 { lerp(low, mid, t * 2.0) } else { lerp(mid, high, (t - 0.5) * 2.0) }
}

/// Extrai valores de um `key="..."` na string via procura simples.
fn attr(s: &str, key: &str) -> Option<String> {
    let pat = format!("{key}=\"");
    let start = s.find(&pat)? + pat.len();
    let rest = &s[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if l > f32::EPSILON { [a[0] / l, a[1] / l, a[2] / l] } else { a }
}

/// Rasteriza um triângulo com z-buffer em `img` (RGBA) e `zbuf`.
#[allow(clippy::too_many_arguments)]
fn fill_tri(
    img: &mut [u8],
    zbuf: &mut [f32],
    w: u32,
    h: u32,
    pts: &[(f32, f32)],
    z: &[f32; 3],
    rgba: [u8; 4],
) {
    let (x0, y0) = (pts[0].0, pts[0].1);
    let (x1, y1) = (pts[1].0, pts[1].1);
    let (x2, y2) = (pts[2].0, pts[2].1);
    let minx = x0.min(x1).min(x2).max(0.0).floor() as i32;
    let maxx = x0.max(x1).max(x2).min((w - 1) as f32).ceil() as i32;
    let miny = y0.min(y1).min(y2).max(0.0).floor() as i32;
    let maxy = y0.max(y1).max(y2).min((h - 1) as f32).ceil() as i32;
    let area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0);
    if area.abs() < f32::EPSILON {
        return;
    }
    for py in miny..=maxy {
        for px in minx..=maxx {
            let fx = px as f32;
            let fy = py as f32;
            let w0 = ((x1 - fx) * (y2 - fy) - (x2 - fx) * (y1 - fy)) / area;
            let w1 = ((x2 - fx) * (y0 - fy) - (x0 - fx) * (y2 - fy)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                let depth = w0 * z[0] + w1 * z[1] + w2 * z[2];
                let idx = (py as usize) * w as usize + px as usize;
                if depth < zbuf[idx] {
                    zbuf[idx] = depth;
                    let di = idx * 4;
                    img[di] = rgba[0];
                    img[di + 1] = rgba[1];
                    img[di + 2] = rgba[2];
                    img[di + 3] = rgba[3];
                }
            }
        }
    }
}

#[test]
fn detalhe_de_arquivo_invalido_devolve_erro_legivel() {

    let path = write_temp("ruim.bsp", b"lixo");
    let err = catalog::detail(&path, RenderOptions::detail(), 32).unwrap_err();
    assert!(err.contains("pequeno") || err.contains("versão"), "veio: {err}");
}

