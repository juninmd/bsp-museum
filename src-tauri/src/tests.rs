//! Testes do parser e do renderizador.
//!
//! Os BSPs de teste são montados byte a byte aqui: dá para descrever exatamente
//! o mapa (um chão quadrado, uma rampa, um lump corrompido) sem depender de
//! nenhum arquivo de 3 MB no repositório.

use crate::bsp::entities::{self, GameMode};
use crate::bsp::reader::BspError;
use crate::bsp::render::{self, RenderOptions};
use crate::bsp::{Bsp, GOLDSRC_VERSION, LUMP_COUNT};
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
fn refem_sem_zona_nao_conta_como_modo_de_resgate() {
    let summary = entities::summarize(&entities::parse("{\"classname\" \"hostage_entity\"}"));
    assert_ne!(summary.mode_by_entities, GameMode::Hostage);
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
fn refem_sem_zona_de_resgate_e_critico() {
    let mut text = spawns_text(16, 16);
    text.push_str("{\"classname\" \"hostage_entity\"}");
    let findings = findings_for("cs_semzona.bsp", &text, 512);
    assert!(has(&findings, "cs-sem-resgate"));
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
fn detalhe_de_arquivo_invalido_devolve_erro_legivel() {
    let path = write_temp("ruim.bsp", b"lixo");
    let err = catalog::detail(&path, RenderOptions::detail(), 32).unwrap_err();
    assert!(err.contains("pequeno") || err.contains("versão"), "veio: {err}");
}

