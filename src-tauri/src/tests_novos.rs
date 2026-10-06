//! Testes de integração sobre o mapa sintético de `fixture.rs`.

use crate::bsp::Bsp;
use crate::catalog;
use crate::fixture::{museum_bsp, Options};
use std::path::PathBuf;

/// grava o mapa em `<tmp>/<mod>/maps/<nome>.bsp` (a estrutura que o app espera)
fn write_map(tag: &str, name: &str, opts: Options) -> PathBuf {
    let root = std::env::temp_dir().join(format!("bspm-novos-{tag}-{}", std::process::id()));
    let maps = root.join("cstrike").join("maps");
    std::fs::create_dir_all(&maps).unwrap();
    let path = maps.join(format!("{name}.bsp"));
    std::fs::write(&path, museum_bsp(opts)).unwrap();
    path
}

#[test]
fn fixture_abre_com_arvore_vis_e_luz() {
    let bsp = Bsp::parse(&museum_bsp(Options::default())).expect("fixture válida");
    assert_eq!(bsp.nodes.len(), 8);
    assert_eq!(bsp.leaves.len(), 4);
    assert!(!bsp.visibility.is_empty());
    assert!(!bsp.lighting.is_empty());
    assert_eq!(bsp.models[0].visleafs, 3);
}

#[test]
fn spawns_da_fixture_estao_em_espaco_vazio() {
    let bsp = Bsp::parse(&museum_bsp(Options::default())).unwrap();
    assert_eq!(bsp.contents_at([-420.0, 0.0, 36.0]), Some(-1));
    assert_eq!(bsp.contents_at([300.0, 0.0, 36.0]), Some(-1));
    // fora do mapa é sólido
    assert_eq!(bsp.contents_at([-300.0, 400.0, 36.0]), Some(-2));
}

#[test]
fn folhas_da_fixture_batem_com_as_salas() {
    let bsp = Bsp::parse(&museum_bsp(Options::default())).unwrap();
    assert_eq!(bsp.leaf_at(0, [-300.0, 0.0, 40.0]), Some(1));
    assert_eq!(bsp.leaf_at(0, [300.0, 0.0, 40.0]), Some(2));
    assert_eq!(bsp.leaf_at(0, [800.0, 0.0, 40.0]), Some(3));
    // uma linha de vis (1 byte) por folha não sólida
    assert_eq!(bsp.visibility, vec![0b011, 0b111, 0b110]);
}

#[test]
fn diagnostico_pega_spawn_em_solido_e_leak() {
    let path = write_map("solido", "de_museu", Options { spawn_in_wall: true, vis: false, ..Options::default() });
    let d = catalog::detail(&path, crate::bsp::render::RenderOptions::detail(), 2).unwrap();
    let ids: Vec<&str> = d.findings.iter().map(|f| f.id).collect();
    assert!(ids.contains(&"spawn-em-solido"), "ids: {ids:?}");
    assert!(ids.contains(&"sem-vis"), "ids: {ids:?}");
}

#[test]
fn mapa_sao_nao_dispara_spawn_em_solido_nem_sem_vis() {
    let path = write_map("sao", "de_museu", Options::default());
    let d = catalog::detail(&path, crate::bsp::render::RenderOptions::detail(), 2).unwrap();
    let ids: Vec<&str> = d.findings.iter().map(|f| f.id).collect();
    assert!(!ids.contains(&"spawn-em-solido") && !ids.contains(&"sem-vis"), "ids: {ids:?}");
    assert!(!d.summary.fullbright);
}

#[test]
fn malha_traz_lightmap_pvs_e_face_por_triangulo() {
    let path = write_map("malha", "de_museu", Options::default());
    let mesh = catalog::mesh(&path).unwrap();
    assert!(mesh.lightmap.is_some(), "atlas de luz");
    assert_eq!(mesh.lm_uvs.len(), mesh.triangles * 6);
    assert_eq!(mesh.tri_face.len(), mesh.triangles);
    let pvs = mesh.pvs.expect("pvs");
    assert_eq!(pvs.leaves.len(), 4 * 4);
    assert_eq!(pvs.world_face_count as usize, mesh.tri_face.iter().max().map(|m| *m as usize + 1).unwrap());
    // uv do atlas sempre dentro de [0,1]
    assert!(mesh.lm_uvs.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn mapa_fullbright_nao_gera_atlas() {
    let path = write_map("full", "de_museu", Options { lighting: false, ..Options::default() });
    let mesh = catalog::mesh(&path).unwrap();
    assert!(mesh.lightmap.is_none());
    assert!(mesh.lm_uvs.is_empty());
}

#[test]
fn bsp_do_quake_v29_abre() {
    let path = write_map("q1", "e1m1", Options { version: 29, ..Options::default() });
    let summary = catalog::summarize_file(&path);
    assert!(summary.error.is_none(), "{:?}", summary.error);
    assert_eq!(summary.bsp_version, 29);
    let mesh = catalog::mesh(&path).unwrap();
    assert!(mesh.triangles > 0);
}

/// WAD3 mínimo só com o diretório (nomes de lump `'C'`): é tudo que o diagnóstico lê.
fn wad_com(names: &[&str]) -> Vec<u8> {
    let mut out = b"WAD3".to_vec();
    out.extend_from_slice(&(names.len() as u32).to_le_bytes());
    out.extend_from_slice(&12u32.to_le_bytes());
    for name in names {
        out.extend_from_slice(&0u32.to_le_bytes()); // filepos
        out.extend_from_slice(&0u32.to_le_bytes()); // disksize
        out.extend_from_slice(&0u32.to_le_bytes()); // size
        out.push(0x43); // tipo miptex
        out.extend_from_slice(&[0u8; 3]);
        let mut raw = [0u8; 16];
        raw[..name.len()].copy_from_slice(name.as_bytes());
        out.extend_from_slice(&raw);
    }
    out
}

#[test]
fn textura_so_de_wad_e_conferida_nos_wads_achados() {
    let path = write_map("wadtex", "de_wad", Options { external_texture: true, ..Options::default() });
    let wad_path = path.parent().unwrap().parent().unwrap().join("museu.wad");
    let run = || {
        catalog::detail(&path, crate::bsp::render::RenderOptions::thumbnail(), 2)
            .unwrap()
            .findings
            .into_iter()
            .find(|f| f.id == "textura-ausente")
    };

    // WAD sem a textura: achado, com o nome
    std::fs::write(&wad_path, wad_com(&["outra"])).unwrap();
    let f = run().expect("textura ausente");
    assert!(f.detail.contains("externa"), "{}", f.detail);

    // WAD com a textura (nome em maiúsculas: a busca ignora caixa): some o achado
    std::fs::write(&wad_path, wad_com(&["EXTERNA"])).unwrap();
    assert!(run().is_none());
}

#[test]
fn indice_de_wad_recusa_arquivo_corrompido() {
    let dir = std::env::temp_dir().join(format!("bspm-wadidx-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bom = dir.join("bom.wad");
    std::fs::write(&bom, wad_com(&["a", "b"])).unwrap();
    let names = crate::bsp::wad::texture_names(&bom).unwrap();
    assert!(names.contains("a") && names.contains("b") && names.len() == 2);

    let ruim = dir.join("ruim.wad");
    let mut bytes = wad_com(&["a"]);
    bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes()); // número de lumps absurdo
    std::fs::write(&ruim, bytes).unwrap();
    assert!(crate::bsp::wad::texture_names(&ruim).is_none());

    std::fs::write(dir.join("lixo.wad"), b"nao sou wad").unwrap();
    assert!(crate::bsp::wad::texture_names(&dir.join("lixo.wad")).is_none());
}

#[test]
fn recursos_listam_mapa_wad_e_som_ausente() {
    let path = write_map("rec", "de_museu", Options::default());
    let d = catalog::detail(&path, crate::bsp::render::RenderOptions::thumbnail(), 2).unwrap();
    let kinds: Vec<&str> = d.resources.items.iter().map(|i| i.kind).collect();
    assert!(kinds.contains(&"mapa") && kinds.contains(&"wad") && kinds.contains(&"sound"));
    let sound = d.resources.items.iter().find(|i| i.kind == "sound").unwrap();
    assert!(!sound.found);
    assert!(d.findings.iter().any(|f| f.id == "recurso-ausente"));
    // o .bsp conta no download; o WAD padrão não
    assert!(d.resources.download_size >= d.resources.items[0].size);
    assert!(d.resources.items.iter().find(|i| i.kind == "wad").unwrap().shared);
}

#[test]
fn indice_persistente_so_relê_o_que_mudou() {
    let a = write_map("idx", "de_um", Options::default());
    let maps = a.parent().unwrap().to_path_buf();
    std::fs::write(maps.join("de_dois.bsp"), museum_bsp(Options::default())).unwrap();
    let root = maps.parent().unwrap().to_path_buf();

    let mut cache = catalog::IndexCache::default();
    let (first, hits) = catalog::scan_cached(&root, &mut cache);
    assert_eq!((first.len(), hits), (2, 0));

    // segunda varredura: tudo vem do índice, mesmo resultado
    let (second, hits) = catalog::scan_cached(&root, &mut cache);
    assert_eq!(hits, 2);
    assert_eq!(second.iter().map(|m| &m.name).collect::<Vec<_>>(), first.iter().map(|m| &m.name).collect::<Vec<_>>());

    // arquivo alterado (outro tamanho) invalida só ele
    std::fs::write(maps.join("de_dois.bsp"), museum_bsp(Options { lighting: false, ..Options::default() })).unwrap();
    let (third, hits) = catalog::scan_cached(&root, &mut cache);
    assert_eq!(hits, 1);
    assert!(third.iter().find(|m| m.name == "de_dois").unwrap().fullbright);

    // índice de versão antiga é descartado
    cache.version = 0;
    let (_, hits) = catalog::scan_cached(&root, &mut cache);
    assert_eq!(hits, 0);
}

#[test]
fn auditoria_acha_duplicados_e_resume_cada_mapa() {
    let a = write_map("aud", "de_original", Options::default());
    let maps = a.parent().unwrap().to_path_buf();
    std::fs::write(maps.join("de_copia.bsp"), museum_bsp(Options::default())).unwrap();
    std::fs::write(maps.join("de_leak.bsp"), museum_bsp(Options { vis: false, ..Options::default() })).unwrap();
    std::fs::write(maps.join("quebrado.bsp"), b"nao sou um bsp").unwrap();

    let report = crate::audit::run(maps.parent().unwrap(), 2);
    assert_eq!(report.rows.len(), 4);
    assert_eq!(report.duplicates.len(), 1, "{:?}", report.duplicates);
    assert!(report.duplicates[0].iter().any(|p| p.ends_with("de_copia.bsp")));
    let leak = report.rows.iter().find(|r| r.name == "de_leak").unwrap();
    assert!(leak.findings.iter().any(|f| f.id == "sem-vis"));
    assert!(report.rows.iter().find(|r| r.name == "quebrado").unwrap().error.is_some());
}

#[test]
fn comparacao_aponta_diferencas_de_lump_e_entidade() {
    let a = write_map("cmp", "de_antes", Options::default());
    let maps = a.parent().unwrap().to_path_buf();
    let b = maps.join("de_depois.bsp");
    std::fs::write(&b, museum_bsp(Options { lighting: false, spawn_in_wall: true, ..Options::default() })).unwrap();

    let c = crate::compare::compare(&a, &b, 2).unwrap();
    assert!(c.lumps.iter().any(|d| d.name == "lighting" && d.a > 0 && d.b == 0));
    let spawn = c.entities.iter().find(|d| d.name == "info_player_start").expect("spawn extra");
    assert_eq!((spawn.a, spawn.b), (16, 17));
    assert!(c.b.fullbright && !c.a.fullbright);
    assert!(c.b.findings.iter().any(|(id, _)| id == "spawn-em-solido"));
}

#[test]
fn lista_de_entidades_traz_origem_e_chaves() {
    let bsp = Bsp::parse(&museum_bsp(Options::default())).unwrap();
    let parsed = crate::bsp::entities::parse(&bsp.entities_raw);
    let rows = crate::entity_list::list(&bsp, &parsed);
    assert_eq!(rows.len(), parsed.len());
    let buy = rows.iter().find(|r| r.classname == "func_buyzone").unwrap();
    assert_eq!(buy.targetname.as_deref(), Some("loja_ct"));
    assert_eq!(buy.origin, Some([-256.0, 0.0, 64.0]));
    assert!(buy.keys.iter().any(|(k, _)| k == "targetname"));
}

// ------------------------------------------------------------------------------
// Dados das capturas de tela do README (não roda no `cargo test` normal):
//   FIXTURE_DIR=/tmp/fx cargo test gerar_dados_das_capturas -- --ignored
// Monta um acervo sintético e grava, em JSON, exatamente o que os comandos do
// backend devolveriam — o script `scripts/screenshots.mjs` serve isso ao frontend.

#[test]
#[ignore = "gera dados para as capturas: FIXTURE_DIR=<pasta> cargo test gerar_dados_das_capturas -- --ignored"]
fn gerar_dados_das_capturas() {
    use crate::bsp::render::RenderOptions;
    let out = PathBuf::from(std::env::var("FIXTURE_DIR").expect("defina FIXTURE_DIR"));
    let maps_dir = out.join("cstrike").join("maps");
    std::fs::create_dir_all(&maps_dir).unwrap();

    // arquivos auxiliares que o mapa referencia (conteúdo dummy: só o tamanho importa)
    let cs = out.join("cstrike");
    for (rel, size) in [("sound/ambience/hum.wav", 48_000usize), ("sprites/glow01.spr", 6_000)] {
        let p = cs.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, vec![0u8; size]).unwrap();
    }
    // WAD3 válido e vazio: o app o encontra, mas nenhuma textura do mapa depende dele
    let mut wad = b"WAD3".to_vec();
    wad.extend_from_slice(&0u32.to_le_bytes());
    wad.extend_from_slice(&12u32.to_le_bytes());
    std::fs::write(cs.join("museu.wad"), wad).unwrap();

    let base = Options::default();
    let variants: Vec<(&str, Options)> = vec![
        ("de_museu", base),
        ("de_museu_copia", base),
        ("de_leak", Options { vis: false, ..base }),
        ("de_parede", Options { spawn_in_wall: true, rooms: 2, ..base }),
        ("cs_sem_refens", Options { rooms: 2, ..base }),
        ("as_incompleto", Options { rooms: 2, ..base }),
        ("zm_escuro", Options { lighting: false, rooms: 1, ..base }),
        ("cs_resgate", Options { rooms: 2, objectives: true, ..base }),
    ];
    for (name, opts) in &variants {
        std::fs::write(maps_dir.join(format!("{name}.bsp")), museum_bsp(*opts)).unwrap();
    }

    let put = |file: &str, text: String| std::fs::write(out.join(file), text).unwrap();
    let json = |v: &dyn erased::Ser| v.to_json();

    let mut cache = catalog::IndexCache::default();
    let (scan, _) = catalog::scan_cached(&out.join("cstrike"), &mut cache);
    put("scan.json", json(&scan));

    for (name, _) in &variants {
        let path = maps_dir.join(format!("{name}.bsp"));
        let detail = catalog::detail(&path, RenderOptions::detail(), 32).unwrap();
        put(&format!("detail-{name}.json"), json(&detail));
        put(&format!("thumb-{name}.svg"), catalog::thumbnail(&path).unwrap());
    }
    for name in ["de_museu", "de_parede"] {
        let path = maps_dir.join(format!("{name}.bsp"));
        put(&format!("mesh-{name}.json"), json(&catalog::mesh(&path).unwrap()));
        let bytes = std::fs::read(&path).unwrap();
        let bsp = Bsp::parse(&bytes).unwrap();
        let parsed = crate::bsp::entities::parse(&bsp.entities_raw);
        put(&format!("entities-{name}.json"), json(&crate::entity_list::list(&bsp, &parsed)));
        let radar = crate::radar::render(&bsp, 512).unwrap();
        use base64::Engine as _;
        let png = base64::engine::general_purpose::STANDARD.encode(radar.png().unwrap());
        put(&format!("radar-{name}.txt"), format!("data:image/png;base64,{png}"));
    }
    put("audit.json", json(&crate::audit::run(&out.join("cstrike"), 32)));
    let a = maps_dir.join("de_museu.bsp");
    let b = maps_dir.join("de_parede.bsp");
    put("compare.json", json(&crate::compare::compare(&a, &b, 32).unwrap()));
    put("maps_dir.txt", out.join("cstrike").to_string_lossy().to_string());
}

mod erased {
    /// `serde_json::to_string` sem precisar nomear o tipo em cada chamada.
    pub trait Ser {
        fn to_json(&self) -> String;
    }
    impl<T: serde::Serialize> Ser for T {
        fn to_json(&self) -> String {
            serde_json::to_string(self).unwrap()
        }
    }
}
