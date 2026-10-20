//! Testes dos recursos extras (hulls, zonas, render props, texturas animadas, estilos de
//! luz e cena) sobre o mapa sintético de `fixture_extra.rs`.

use crate::bsp::entities;
use crate::bsp::light::build_atlas;
use crate::bsp::Bsp;
use crate::catalog;
use crate::fixture_extra::{build, extra_bsp, extra_fx};
use crate::hulls::{self, HullQuery};
use crate::scene::{self, MoverKind, SceneEntity, ScrollSource};
use crate::volumes::{self, ZoneKind};
use std::path::PathBuf;

fn parsed() -> (Bsp, Vec<entities::Entity>) {
    let bsp = Bsp::parse(&extra_bsp()).expect("fixture extra válida");
    let ents = entities::parse(&bsp.entities_raw);
    (bsp, ents)
}

fn write_map(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("bspm-extra-{tag}-{}", std::process::id()));
    let maps = root.join("cstrike").join("maps");
    std::fs::create_dir_all(&maps).unwrap();
    let path = maps.join("de_extras.bsp");
    std::fs::write(&path, extra_bsp()).unwrap();
    path
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// (área, normal, centróide) de cada triângulo da malha indexada.
fn tri_info(positions: &[f32], indices: &[u32]) -> Vec<(f32, [f32; 3], [f32; 3])> {
    let p = |i: u32| {
        let i = i as usize * 3;
        [positions[i], positions[i + 1], positions[i + 2]]
    };
    indices
        .chunks_exact(3)
        .map(|t| {
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            let n = cross(sub(b, a), sub(c, a));
            let len = dot(n, n).sqrt();
            let normal = if len > 0.0 { [n[0] / len, n[1] / len, n[2] / len] } else { [0.0; 3] };
            (len / 2.0, normal, [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0])
        })
        .collect()
}

// ------------------------------------------------------------------ 1. hulls

#[test]
fn clipnodes_e_cabecas_dos_hulls_sao_lidos() {
    let (bsp, _) = parsed();
    assert_eq!(bsp.clipnodes.len(), 12);
    assert_eq!(bsp.models[0].hull_heads, [0, 6, -1]);
    assert_eq!(bsp.clipnodes[0].children, [-2, 1]);
    // dentro do cubo é vazio, fora é sólido
    assert_eq!(bsp.hull_contents_at(0, 1, [0.0, 0.0, 0.0]), Some(-1));
    assert_eq!(bsp.hull_contents_at(0, 1, [100.0, 0.0, 0.0]), Some(-2));
    assert_eq!(bsp.hull_contents_at(0, 2, [60.0, 0.0, 0.0]), Some(-2));
    assert_eq!(bsp.hull_contents_at(0, 2, [40.0, 0.0, 0.0]), Some(-1));
    // hull sem raiz / inválido
    assert_eq!(bsp.hull_contents_at(0, 3, [0.0; 3]), None);
    assert_eq!(bsp.hull_contents_at(0, 0, [0.0; 3]), None);
    assert_eq!(bsp.hull_contents_at(9, 1, [0.0; 3]), None);
}

#[test]
fn tamanhos_dos_hulls_seguem_o_sdk() {
    use crate::bsp::hull::hull_size;
    let h1 = hull_size(1).unwrap();
    assert_eq!((h1.mins, h1.maxs), ([-16.0, -16.0, -36.0], [16.0, 16.0, 36.0]));
    let h2 = hull_size(2).unwrap();
    assert_eq!((h2.mins, h2.maxs), ([-32.0, -32.0, -32.0], [32.0, 32.0, 32.0]));
    let h3 = hull_size(3).unwrap();
    assert_eq!((h3.mins, h3.maxs), ([-16.0, -16.0, -18.0], [16.0, 16.0, 18.0]));
    assert!(hull_size(0).is_none() && hull_size(4).is_none());
}

#[test]
fn malha_do_hull_e_a_superficie_do_cubo_vazio() {
    let (bsp, _) = parsed();
    let mesh = hulls::mesh_of(&bsp, HullQuery::new(1)).unwrap();
    assert_eq!((mesh.triangles, mesh.total_triangles), (12, 12));
    assert!(mesh.next_offset.is_none() && !mesh.truncated);
    assert_eq!(mesh.indices.len(), 36);
    assert_eq!(mesh.positions.len() / 3, 8, "vértices deduplicados");
    let total: f32 = tri_info(&mesh.positions, &mesh.indices).iter().map(|t| t.0).sum();
    assert!((total - 6.0 * 160.0 * 160.0).abs() < 1.0, "área {total}");
    for (_, normal, centroid) in tri_info(&mesh.positions, &mesh.indices) {
        // a superfície olha para o lado vazio: para dentro do cubo
        assert!(dot(normal, centroid) < 0.0, "normal para fora: {normal:?} em {centroid:?}");
        assert!(centroid.iter().any(|c| (c.abs() - 80.0).abs() < 0.01 || c.abs() <= 80.0 + 0.01));
    }
    for v in mesh.positions.chunks_exact(3) {
        assert!(v.iter().all(|c| (c.abs() - 80.0).abs() < 0.01), "vértice fora do cubo: {v:?}");
    }
    let h2 = hulls::mesh_of(&bsp, HullQuery::new(2)).unwrap();
    assert!(h2.positions.iter().all(|c| (c.abs() - 48.0).abs() < 0.01));
    assert_eq!(h2.size.hull, 2);
}

#[test]
fn hull_invalido_ou_ausente_devolve_erro_legivel() {
    let (bsp, _) = parsed();
    assert!(hulls::mesh_of(&bsp, HullQuery::new(0)).unwrap_err().contains("inválido"));
    assert!(hulls::mesh_of(&bsp, HullQuery::new(4)).is_err());
    assert!(hulls::mesh_of(&bsp, HullQuery::new(3)).unwrap_err().contains("não tem hull 3"));
    assert!(hulls::mesh_of(&bsp, HullQuery { model: 99, ..HullQuery::new(1) }).is_err());
    let infos = hulls::hull_infos(&bsp);
    assert_eq!(infos.iter().map(|h| h.available).collect::<Vec<_>>(), vec![true, true, false]);
}

#[test]
fn malha_do_hull_pagina_em_janelas_independentes() {
    let (bsp, _) = parsed();
    let mut seen = 0usize;
    let mut offset = 0usize;
    let mut windows = 0;
    loop {
        let w = hulls::mesh_of(&bsp, HullQuery { hull: 1, model: 0, offset, limit: 5 }).unwrap();
        windows += 1;
        assert_eq!(w.total_triangles, 12);
        assert!(w.triangles <= 5);
        let nv = (w.positions.len() / 3) as u32;
        assert!(w.indices.iter().all(|&i| i < nv), "índice local fora da janela");
        seen += w.triangles;
        match w.next_offset {
            Some(n) => offset = n,
            None => break,
        }
    }
    assert_eq!((seen, windows), (12, 3));
    // janela depois do fim: vazia, sem erro
    let past = hulls::mesh_of(&bsp, HullQuery { hull: 1, model: 0, offset: 500, limit: 5 }).unwrap();
    assert_eq!((past.triangles, past.next_offset), (0, None));
}

#[test]
fn hull_pelo_caminho_do_arquivo() {
    let path = write_map("hull");
    let m = hulls::hull_mesh(&path, 1).unwrap();
    assert_eq!(m.triangles, 12);
    assert!(serde_json::to_string(&m).unwrap().contains("\"total_triangles\":12"));
}

/// Bloco sólido ±10 no meio de espaço vazio: as normais apontam para fora do bloco.
#[test]
fn bloco_solido_tem_normais_para_fora() {
    let mut fx = extra_fx();
    fx.planes.clear();
    fx.clipnodes.clear();
    for axis in 0..3 {
        let mut n = [0.0f32; 3];
        n[axis] = 1.0;
        fx.planes.push((n, 10.0));
        fx.planes.push((n, -10.0));
    }
    // x=+10: frente vazio; x=-10: trás vazio; ...; z=-10: frente sólido, trás vazio
    fx.clipnodes = vec![(0, [-1, 1]), (1, [2, -1]), (2, [-1, 3]), (3, [4, -1]), (4, [-1, 5]), (5, [-2, -1])];
    fx.models[0].hull_heads = [0, -1, -1];
    let bsp = Bsp::parse(&build(&fx)).unwrap();
    let mesh = hulls::mesh_of(&bsp, HullQuery::new(1)).unwrap();
    assert_eq!(mesh.triangles, 12);
    let info = tri_info(&mesh.positions, &mesh.indices);
    let total: f32 = info.iter().map(|t| t.0).sum();
    assert!((total - 6.0 * 400.0).abs() < 0.5, "área {total}");
    assert!(info.iter().all(|(_, n, c)| dot(*n, *c) > 0.0), "normais devem apontar para longe do sólido");
}

/// Plano oblíquo e com normal não unitária (o lump não garante normalização).
#[test]
fn plano_obliquo_e_normalizado() {
    let mut fx = extra_fx();
    fx.planes = vec![([-1.0, 0.0, 1.0], 0.0)]; // z = x, sólido abaixo
    fx.clipnodes = vec![(0, [-1, -2])];
    fx.models[0].hull_heads = [0, -1, -1];
    let bsp = Bsp::parse(&build(&fx)).unwrap();
    let polys = bsp.hull_polygons(0, 1, 1000).unwrap();
    assert!(!polys.polygons.is_empty());
    let n = [-std::f32::consts::FRAC_1_SQRT_2, 0.0, std::f32::consts::FRAC_1_SQRT_2];
    for poly in &polys.polygons {
        for p in poly {
            assert!(dot(n, *p).abs() < 0.01, "ponto fora do plano: {p:?}");
        }
        // enrolamento anti-horário visto do lado vazio (frente, +n)
        let c = cross(sub(poly[1], poly[0]), sub(poly[2], poly[0]));
        assert!(dot(c, n) > 0.0);
    }
}

#[test]
fn arvore_de_clipnodes_com_ciclo_nao_trava() {
    let mut fx = extra_fx();
    fx.clipnodes = vec![(0, [0, 0]), (1, [1, 0])];
    fx.models[0].hull_heads = [0, 1, -1];
    let bsp = Bsp::parse(&build(&fx)).unwrap();
    for hull in 1..=2 {
        let r = bsp.hull_polygons(0, hull, 10_000);
        assert!(r.is_some());
    }
    // índice de plano fora da faixa
    fx.clipnodes = vec![(99, [-1, -2])];
    fx.models[0].hull_heads = [0, -1, -1];
    let bsp = Bsp::parse(&build(&fx)).unwrap();
    assert!(bsp.hull_polygons(0, 1, 100).unwrap().polygons.is_empty());
}

#[test]
fn limite_de_poligonos_marca_truncado() {
    let (bsp, _) = parsed();
    let r = bsp.hull_polygons(0, 1, 3).unwrap();
    assert_eq!(r.polygons.len(), 3);
    assert!(r.truncated);
}

// ------------------------------------------------------------------ 2. zonas

#[test]
fn zonas_vem_classificadas_com_cor_e_rotulo() {
    let (bsp, ents) = parsed();
    let r = volumes::collect(&bsp, &ents);
    assert_eq!(r.volumes.len(), 2);

    let trig = r.volumes.iter().find(|v| v.classname == "trigger_multiple").unwrap();
    assert_eq!((trig.kind, trig.model, trig.triangles), (ZoneKind::Trigger, 1, 2));
    assert!(!trig.from_box);
    assert_eq!(trig.label, "gatilho1");
    assert_eq!(trig.target.as_deref(), Some("alvo"));
    assert_eq!(trig.color, "#b388ff");
    assert_eq!(trig.positions.len(), 18);
    assert_eq!((trig.mins, trig.maxs), ([1000.0, 0.0, 0.0], [1032.0, 32.0, 64.0]));

    let buy = r.volumes.iter().find(|v| v.classname == "func_buyzone").unwrap();
    assert_eq!(buy.kind, ZoneKind::BuyZone);
    assert!(buy.from_box, "sem faces: usa a caixa do AABB");
    assert_eq!(buy.triangles, 12);
    assert_eq!(buy.label, "func_buyzone");
    // todos os vértices da caixa estão sobre o AABB
    for v in buy.positions.chunks_exact(3) {
        assert!(v[0].abs() == 64.0 && v[1].abs() == 64.0 && (v[2] == 0.0 || v[2] == 128.0));
    }
    assert_eq!(r.counts.get("func_buyzone"), Some(&1));
    assert!(r.legend.iter().any(|l| l.kind == ZoneKind::Water));
}

#[test]
fn classifica_as_classes_do_cs() {
    use crate::volumes::classify;
    assert_eq!(classify("trigger_hurt"), Some(ZoneKind::Trigger));
    assert_eq!(classify("func_bomb_target"), Some(ZoneKind::BombTarget));
    assert_eq!(classify("func_hostage_rescue"), Some(ZoneKind::HostageRescue));
    assert_eq!(classify("func_vip_safetyzone"), Some(ZoneKind::VipSafety));
    assert_eq!(classify("func_escapezone"), Some(ZoneKind::EscapeZone));
    assert_eq!(classify("func_ladder"), Some(ZoneKind::Ladder));
    assert_eq!(classify("func_water"), Some(ZoneKind::Water));
    assert_eq!(classify("func_wall"), None);
}

#[test]
fn superficies_invisiveis_saem_separadas_do_mundo() {
    let (bsp, ents) = parsed();
    let r = volumes::collect(&bsp, &ents);
    let names: Vec<&str> = r.invisible.iter().map(|g| g.texture.as_str()).collect();
    assert_eq!(names, vec!["aaatrigger", "clip", "sky"]);
    // a face aaatrigger do modelo 1 é da zona, não do grupo
    let trig = r.invisible.iter().find(|g| g.texture == "aaatrigger").unwrap();
    assert_eq!((trig.faces, trig.triangles), (1, 2));
    let clip = r.invisible.iter().find(|g| g.texture == "clip").unwrap();
    assert_eq!(clip.color, "#ef5350");
    assert_eq!(clip.positions.len(), 18);
}

#[test]
fn malha_visual_continua_sem_as_faces_invisiveis() {
    let path = write_map("visual");
    let mesh = catalog::mesh(&path).unwrap();
    // 10 faces de 2 triângulos; 4 invisíveis (clip, sky, aaatrigger do mundo e do modelo 1)
    assert_eq!(mesh.skipped, 4);
    assert_eq!(mesh.triangles, 6 * 2);
    let (zones, invisible) = volumes::counts(&Bsp::parse(&extra_bsp()).unwrap(), &parsed().1);
    assert_eq!(invisible, 4);
    assert_eq!(zones.len(), 2);
}

// ------------------------------------------------------------------ 3. render props e rolagem

#[test]
fn render_props_da_brush_entity() {
    let (bsp, ents) = parsed();
    let models = scene::brush_models(&bsp, &ents);
    let door = models.iter().find(|m| m.model == 3).unwrap();
    let r = door.render.as_ref().expect("porta aditiva");
    assert_eq!((r.mode, r.mode_name, r.fx, r.fx_name), (5, "additive", 10, "strobe rápido"));
    assert_eq!((r.amount, r.color), (128.0, [255, 0, 0]));
    assert_eq!((door.first_face, door.num_faces), (7, 1));
    assert_eq!(door.targetname.as_deref(), Some("porta"));
    // gatilho e buyzone: render padrão some
    assert!(models.iter().find(|m| m.model == 1).unwrap().render.is_none());
    assert!(models.iter().all(|m| m.model != 0));
}

#[test]
fn scroll_do_conveyor_vem_do_speed_e_o_de_func_wall_do_rendercolor() {
    let (bsp, ents) = parsed();
    let models = scene::brush_models(&bsp, &ents);
    let belt = models.iter().find(|m| m.model == 4).unwrap().scroll.as_ref().unwrap();
    assert_eq!((belt.speed, belt.source), (32.0, ScrollSource::ConveyorSpeed));
    assert_eq!(belt.textures, vec!["scrollbelt"]);
    // rendercolor "1 1 0": R != 0 inverte; (G<<8 | B) = 256; 256/16 = 16
    let wall = models.iter().find(|m| m.model == 5).unwrap().scroll.as_ref().unwrap();
    assert_eq!((wall.speed, wall.source), (-16.0, ScrollSource::Rendercolor));
    // a porta não tem textura scroll: sem rolagem
    assert!(models.iter().find(|m| m.model == 3).unwrap().scroll.is_none());
}

#[test]
fn regra_do_rendercolor_e_do_conveyor() {
    let ent = |t: &str| entities::parse(t).remove(0);
    assert_eq!(scene::scroll_speed(&ent(r#"{"classname" "func_wall" "rendercolor" "0 0 160"}"#)).unwrap().0, 10.0);
    assert_eq!(scene::scroll_speed(&ent(r#"{"classname" "func_wall" "rendercolor" "255 1 0"}"#)).unwrap().0, -16.0);
    assert!(scene::scroll_speed(&ent(r#"{"classname" "func_wall" "rendercolor" "0 0 0"}"#)).is_none());
    assert!(scene::scroll_speed(&ent(r#"{"classname" "func_wall"}"#)).is_none());
    // conveyor: 4 bits fracionários e velocidade padrão 100
    assert_eq!(scene::scroll_speed(&ent(r#"{"classname" "func_conveyor" "speed" "-50"}"#)).unwrap().0, -50.0);
    assert_eq!(scene::scroll_speed(&ent(r#"{"classname" "func_conveyor"}"#)).unwrap().0, 100.0);
    assert_eq!(scene::scroll_speed(&ent(r#"{"classname" "func_conveyor" "speed" "0.03"}"#)), None);
    // teto de 16 bits
    assert_eq!(scene::scroll_speed(&ent(r#"{"classname" "func_conveyor" "speed" "999999"}"#)).unwrap().0, 4095.9375);
    assert!(scene::is_scroll_texture("SCROLLbelt") && !scene::is_scroll_texture("scrol"));
}

#[test]
fn nomes_de_modo_e_efeito() {
    assert_eq!(scene::render_mode_name(0), "normal");
    assert_eq!(scene::render_mode_name(5), "additive");
    assert_eq!(scene::render_mode_name(77), "desconhecido");
    assert_eq!(scene::render_fx_name(19), "glow shell");
    assert_eq!(scene::render_fx_name(-1), "desconhecido");
}

// ------------------------------------------------------------------ 4. texturas animadas

#[test]
fn malha_expoe_grupo_de_textura_animada_com_quadros() {
    let path = write_map("anim");
    let mesh = catalog::mesh(&path).unwrap();
    assert_eq!(mesh.texture_anims.len(), 1);
    let a = &mesh.texture_anims[0];
    assert_eq!(a.name, "fan");
    // +2fan está na tabela e +0/+1 também; +Afan é a alternativa
    assert_eq!(a.frames, vec!["+0fan", "+1fan", "+2fan"]);
    assert_eq!(a.alt_frames, vec!["+Afan"]);
    assert!((a.interval - 0.1).abs() < 1e-6);
    assert!(a.complete);
    assert_eq!(a.frame_slots.len(), 3);
    for (slot, name) in a.frame_slots.iter().zip(&a.frames) {
        assert!(*slot >= 0, "quadro {name} sem imagem");
        assert_eq!(&mesh.textures[*slot as usize].name, name);
        assert!(mesh.textures[*slot as usize].png.is_some());
    }
    assert_eq!(a.alt_slots.len(), 1);
    assert!(a.alt_slots[0] >= 0);
}

// ------------------------------------------------------------------ 5. estilos de luz

#[test]
fn atlas_principal_segue_igual_e_ganha_camada_do_estilo_1() {
    let (bsp, _) = parsed();
    let atlas = build_atlas(&bsp).unwrap();
    let f = atlas.faces[0].expect("chão com lightmap");
    let at = |rgba: &[u8], x: usize, y: usize| {
        let o = (y * atlas.width + x) * 4;
        [rgba[o], rgba[o + 1], rgba[o + 2], rgba[o + 3]]
    };
    // atlas principal: só o estilo do slot 0 (vermelho), como antes
    assert_eq!(at(&atlas.rgba, f.x, f.y), [100, 0, 0, 255]);
    assert_eq!(at(&atlas.rgba, 0, 0), [255, 255, 255, 255], "texel branco intacto");

    assert_eq!(atlas.layers.len(), 1);
    let layer = &atlas.layers[0];
    assert_eq!(layer.slot, 1);
    assert_eq!(at(&layer.rgba, f.x, f.y), [0, 50, 0, 255]);
    assert_eq!(at(&layer.rgba, f.x + 16, f.y + 16), [0, 50, 0, 255]);
    assert_eq!(at(&layer.rgba, f.x - 1, f.y - 1), [0, 50, 0, 255], "borda replicada");
    // fora do retângulo da face e o texel branco: preto (a camada é aditiva)
    assert_eq!(at(&layer.rgba, 0, 0), [0, 0, 0, 255]);
    // face de um estilo só (5): sem camada, mas com o atlas principal
    let single = atlas.faces[5].expect("face com estilo 5");
    assert_eq!(at(&atlas.rgba, single.x, single.y), [10, 20, 30, 255]);
    assert_eq!(at(&layer.rgba, single.x, single.y), [0, 0, 0, 255]);
}

#[test]
fn bloco_do_segundo_estilo_fora_do_lump_e_ignorado() {
    let mut fx = extra_fx();
    // corta o segundo bloco (verde): só o primeiro cabe
    fx.quads[0].light.truncate(17 * 17 * 3);
    let bsp = Bsp::parse(&build(&fx)).unwrap();
    let atlas = build_atlas(&bsp).unwrap();
    assert!(atlas.faces[0].is_some());
    assert!(atlas.layers.is_empty());
}

#[test]
fn malha_traz_estilos_por_face_camadas_e_padroes() {
    let path = write_map("estilos");
    let mesh = catalog::mesh(&path).unwrap();
    assert_eq!(mesh.face_styles.len(), 10 * 4);
    assert_eq!(&mesh.face_styles[0..4], &[0, 1, 255, 255]);
    assert_eq!(&mesh.face_styles[4..8], &[255; 4], "face sem lightmap não anuncia estilo");
    assert_eq!(&mesh.face_styles[20..24], &[5, 255, 255, 255]);
    assert!(mesh.lightmap.is_some());
    assert_eq!(mesh.lightmap_layers.len(), 1);
    assert_eq!(mesh.lightmap_layers[0].slot, 1);
    assert!(mesh.lightmap_layers[0].png.starts_with("data:image/png;base64,"));

    let ids: Vec<u8> = mesh.lightstyles.iter().map(|d| d.id).collect();
    assert_eq!(ids, vec![0, 1, 5, 33]);
    let s1 = &mesh.lightstyles[1];
    assert_eq!(s1.pattern, "mmnmmommommnonmmonqnmmo");
    assert_eq!(s1.values.len(), s1.pattern.len());
    let s33 = &mesh.lightstyles[3];
    assert_eq!((s33.pattern.as_str(), s33.switchable, s33.targetname.as_deref()), ("az", true, Some("lampada")));
    // uvs do atlas principal continuam por vértice
    assert_eq!(mesh.lm_uvs.len(), mesh.uvs.len());
}

#[test]
fn mapa_fullbright_nao_anuncia_estilos_por_face() {
    let mut fx = extra_fx();
    for q in &mut fx.quads {
        q.light.clear();
    }
    let bsp = Bsp::parse(&build(&fx)).unwrap();
    assert!(build_atlas(&bsp).is_none());
}

// ------------------------------------------------------------------ 6. cena

fn find<'a, T>(scene: &'a scene::SceneEntities, pick: impl Fn(&'a SceneEntity) -> Option<&'a T>) -> Vec<&'a T> {
    scene.entities.iter().filter_map(pick).collect()
}

#[test]
fn cena_traz_fog_sol_decal_e_luzes() {
    let (bsp, ents) = parsed();
    let s = scene::collect(&bsp, &ents);

    let fog = find(&s, |e| if let SceneEntity::EnvFog(f) = e { Some(f) } else { None });
    assert_eq!((fog[0].color, fog[0].start, fog[0].end), ([10, 20, 30], 100.0, 900.0));

    let sun = &find(&s, |e| if let SceneEntity::LightEnvironment(l) = e { Some(l) } else { None })[0];
    assert_eq!((sun.color, sun.brightness), ([255, 240, 200], 300.0));
    assert_eq!((sun.ambient_color, sun.ambient_brightness), (Some([100, 110, 120]), Some(80.0)));
    assert_eq!(sun.pitch, Some(-60.0));
    let d = sun.direction;
    assert!((d[0] - 0.5 * 45f32.to_radians().cos()).abs() < 1e-4);
    assert!((d[2] + 0.866_025_4).abs() < 1e-4, "pitch -60 aponta para baixo: {d:?}");
    assert!((dot(d, d) - 1.0).abs() < 1e-4);

    let decal = &find(&s, |e| if let SceneEntity::Decal(d) = e { Some(d) } else { None })[0];
    assert_eq!((decal.texture.as_deref(), decal.base.origin, decal.brush), (Some("{shot1"), [10.0, 20.0, 30.0], false));

    let lights = find(&s, |e| if let SceneEntity::Light(l) = e { Some(l) } else { None });
    assert_eq!(lights.len(), 3);
    let pulsing = lights.iter().find(|l| l.style == 1).unwrap();
    assert_eq!((pulsing.color, pulsing.brightness, pulsing.switchable), ([255, 200, 100], 400.0, false));
    let lamp = lights.iter().find(|l| l.style == 33).unwrap();
    assert!(lamp.switchable);
    assert_eq!(lamp.pattern.as_deref(), Some("az"));
    let spot = lights.iter().find(|l| l.spot).unwrap();
    let dir = spot.direction.unwrap();
    assert!((dir[2] + 1.0).abs() < 1e-5, "o spot mira no alvo, para baixo: {dir:?}");
    assert_eq!((spot.cone, spot.cone2), (Some(30.0), Some(45.0)));
}

#[test]
fn cena_traz_sprite_raio_e_som() {
    let (bsp, ents) = parsed();
    let s = scene::collect(&bsp, &ents);
    let sp = &find(&s, |e| if let SceneEntity::Sprite(x) = e { Some(x) } else { None })[0];
    assert_eq!((sp.model.as_deref(), sp.scale, sp.glow), (Some("sprites/glow01.spr"), 0.5, false));
    assert_eq!((sp.render.mode, sp.render.amount, sp.render.color), (5, 200.0, [255, 128, 0]));

    let beam = &find(&s, |e| if let SceneEntity::Beam(b) = e { Some(b) } else { None })[0];
    assert_eq!((beam.start, beam.end), (Some([0.0, 0.0, 50.0]), Some([100.0, 0.0, 50.0])));
    assert_eq!((beam.width, beam.noise), (8.0, 2.0));
    assert_eq!(beam.start_target.as_deref(), Some("ponta_a"));

    let amb = &find(&s, |e| if let SceneEntity::Ambient(a) = e { Some(a) } else { None })[0];
    assert_eq!(amb.sound.as_deref(), Some("ambience/hum.wav"));
    assert_eq!(amb.volume, 0.5);
    assert_eq!(amb.radius, Some(800.0), "flag 4 = raio médio (ATTN_STATIC 1.25)");
}

#[test]
fn cena_traz_movers_com_deslocamento_da_porta() {
    let (bsp, ents) = parsed();
    let s = scene::collect(&bsp, &ents);
    let movers = find(&s, |e| if let SceneEntity::Mover(m) = e { Some(m) } else { None });
    assert_eq!(movers.len(), 3);

    let door = movers.iter().find(|m| m.kind == MoverKind::Door).unwrap();
    assert_eq!((door.model, door.speed, door.lip), (Some(3), 50.0, 8.0));
    assert!(door.movedir[0].abs() < 1e-5 && (door.movedir[1] - 1.0).abs() < 1e-5);
    // tamanho em Y = 16; (16 - 2) - lip 8 = 6 unidades ao longo de +Y
    let off = door.open_offset.unwrap();
    assert!(off[0].abs() < 1e-4 && (off[1] - 6.0).abs() < 1e-4 && off[2].abs() < 1e-4, "{off:?}");
    assert_eq!(door.wait, 3.0, "padrão do func_door");
    assert!(!door.start_open);
    assert_eq!(door.render.as_ref().map(|r| r.mode), Some(5));

    let belt = movers.iter().find(|m| m.kind == MoverKind::Conveyor).unwrap();
    assert_eq!(belt.scroll_speed, Some(32.0));
    // func_wall não é mover
    assert!(movers.iter().all(|m| m.base.classname != "func_wall"));
}

#[test]
fn movedir_segue_a_convencao_do_motor() {
    assert_eq!(scene::movedir_of([0.0, -1.0, 0.0]), [0.0, 0.0, 1.0]);
    assert_eq!(scene::movedir_of([0.0, -2.0, 0.0]), [0.0, 0.0, -1.0]);
    let d = scene::movedir_of([0.0, 90.0, 0.0]);
    assert!(d[0].abs() < 1e-6 && (d[1] - 1.0).abs() < 1e-6);
    // pitch positivo olha para baixo no motor
    assert!(scene::movedir_of([45.0, 0.0, 0.0])[2] < 0.0);
}

#[test]
fn rota_do_trem_segue_target_ate_fechar_o_circuito() {
    let (bsp, ents) = parsed();
    let s = scene::collect(&bsp, &ents);
    assert_eq!(s.routes.len(), 1);
    let r = &s.routes[0];
    assert_eq!(r.targetname.as_deref(), Some("trem"));
    assert_eq!(r.points, vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0]]);
    assert_eq!(r.speeds, vec![120.0, 0.0, 0.0]);
    assert_eq!(r.waits, vec![0.0, 2.0, 0.0]);
    assert!(r.looped && !r.broken);
    assert_eq!(r.corners.len(), 3);
    let train = &ents[r.train];
    assert_eq!(train.get("classname").map(String::as_str), Some("func_train"));
    assert_eq!(s.counts.get("path_corner"), Some(&3));
    assert_eq!(s.counts.get("mover"), Some(&3));
}

#[test]
fn rota_quebrada_e_marcada() {
    let ents = entities::parse(
        r#"{"classname" "func_train" "target" "a"}
           {"classname" "path_corner" "targetname" "a" "origin" "1 2 3" "target" "sumiu"}"#,
    );
    let s = scene::collect(&Bsp::default(), &ents);
    assert_eq!(s.routes.len(), 1);
    assert!(s.routes[0].broken && !s.routes[0].looped);
    assert_eq!(s.routes[0].points, vec![[1.0, 2.0, 3.0]]);
}

#[test]
fn cena_serializa_como_lista_tipada() {
    let path = write_map("json");
    let mesh = catalog::mesh(&path).unwrap();
    let json = serde_json::to_value(&mesh).unwrap();
    let ents = json["scene"]["entities"].as_array().unwrap();
    let types: std::collections::BTreeSet<&str> = ents.iter().map(|e| e["type"].as_str().unwrap()).collect();
    for t in ["env_fog", "light_environment", "decal", "light", "sprite", "beam", "ambient", "mover", "path_corner"] {
        assert!(types.contains(t), "falta o tipo {t}: {types:?}");
    }
    let door = ents.iter().find(|e| e["type"] == "mover" && e["kind"] == "door").unwrap();
    // campos comuns achatados no mesmo objeto
    assert_eq!(door["classname"], "func_door");
    assert_eq!(door["targetname"], "porta");
    assert!(json["scene"]["routes"].as_array().unwrap().len() == 1);
    assert!(json["brush_models"].as_array().unwrap().iter().any(|m| m["model"] == 3));
    assert_eq!(json["texture_anims"][0]["name"], "fan");
}

#[test]
fn entidades_com_valores_podres_nao_derrubam_a_cena() {
    let ents = entities::parse(
        r#"{"classname" "env_fog" "rendercolor" "a b" "startdist" "x"}
           {"classname" "light" "_light" "1e99 -4 nan" "style" "999" "origin" "x y z"}
           {"classname" "light_spot" "target" "nada" "angles" "1 2"}
           {"classname" "func_door" "model" "*99" "speed" "inf" "lip" "-nan"}
           {"classname" "func_door" "model" "*" "angles" "q"}
           {"classname" "func_rotating" "model" "*-1" "spawnflags" "99999999999"}
           {"classname" "env_beam" "LightningStart" "x" "BoltWidth" "?"}
           {"classname" "ambient_generic" "health" "9e99" "spawnflags" "-1"}
           {"classname" "path_corner" "target" "ele_mesmo" "targetname" "ele_mesmo"}
           {"classname" "func_train" "target" "ele_mesmo"}"#,
    );
    let s = scene::collect(&Bsp::default(), &ents);
    assert!(s.entities.len() >= 9);
    assert!(serde_json::to_string(&s).is_ok());
    assert!(s.routes[0].looped);
    let bm = scene::brush_models(&Bsp::default(), &ents);
    assert!(bm.is_empty());
}

// ------------------------------------------------------------------ 7. detalhe e lixo

#[test]
fn detalhe_do_mapa_resume_os_extras() {
    let path = write_map("detail");
    let d = catalog::detail(&path, crate::bsp::render::RenderOptions::thumbnail(), 32).unwrap();
    let x = &d.extra;
    assert_eq!(x.clipnodes, 12);
    assert_eq!(x.hulls.iter().map(|h| h.available).collect::<Vec<_>>(), vec![true, true, false]);
    assert_eq!(x.zones.get("trigger_multiple"), Some(&1));
    assert_eq!(x.zones.get("func_buyzone"), Some(&1));
    assert_eq!(x.invisible_faces, 4);
    assert_eq!(x.animated_textures, 1);
    assert_eq!(x.lightstyles, vec![0, 1, 5, 33]);
    assert_eq!(x.switchable_lights, 1);
    assert_eq!((x.rendered_models, x.scrolling_models, x.train_routes), (1, 2, 1));
    assert_eq!(x.scene.get("light"), Some(&3));
    assert!(serde_json::to_string(&d).unwrap().contains("\"extra\""));
}

/// Gerador pseudo-aleatório determinístico (xorshift) para estragar bytes.
fn rng(seed: u64) -> impl FnMut() -> u64 {
    let mut s = seed | 1;
    move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    }
}

fn exercita_tudo(bsp: &Bsp) {
    let ents = entities::parse(&bsp.entities_raw);
    for hull in 0..=4 {
        let _ = hulls::mesh_of(bsp, HullQuery::new(hull));
        let _ = bsp.hull_polygons(0, hull, 1000);
    }
    let _ = hulls::hull_infos(bsp);
    let _ = volumes::collect(bsp, &ents);
    let _ = volumes::counts(bsp, &ents);
    let _ = scene::collect(bsp, &ents);
    let _ = scene::brush_models(bsp, &ents);
    let _ = crate::bsp::lightstyle::collect(bsp, &ents);
    let _ = build_atlas(bsp);
    let _ = catalog::extra_summary(bsp, &ents);
    let names = bsp.raw_texture_name.iter().map(String::as_str);
    let _ = crate::bsp::texanim::group(names, &|_| false);
}

#[test]
fn lixo_aleatorio_nao_causa_panic() {
    let mut next = rng(0xC0FFEE);
    // bytes totalmente aleatórios
    for len in [0usize, 10, 124, 500, 4000] {
        let junk: Vec<u8> = (0..len).map(|_| next() as u8).collect();
        if let Ok(bsp) = Bsp::parse(&junk) {
            exercita_tudo(&bsp);
        }
    }
    // mapa válido com bytes estragados: cabeçalho preservado, lixo no meio
    let good = extra_bsp();
    let mut parsed_ok = 0;
    for round in 0..150 {
        let mut bytes = good.clone();
        let hits = 1 + (round % 12);
        for _ in 0..hits {
            let at = 124 + (next() as usize) % (bytes.len() - 124);
            bytes[at] = next() as u8;
        }
        if let Ok(bsp) = Bsp::parse(&bytes) {
            parsed_ok += 1;
            exercita_tudo(&bsp);
        }
    }
    assert!(parsed_ok > 50, "o teste precisa exercitar mapas que abrem ({parsed_ok})");
}

#[test]
fn lump_de_clipnodes_torto_nao_derruba_a_abertura() {
    let mut bytes = extra_bsp();
    // lump 9 (clipnodes): tamanho que não é múltiplo de 8
    let at = 4 + 9 * 8 + 4;
    let len = i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    bytes[at..at + 4].copy_from_slice(&(len - 3).to_le_bytes());
    let bsp = Bsp::parse(&bytes).expect("clipnode torto não impede abrir o mapa");
    assert!(bsp.clipnodes.is_empty());
    assert!(hulls::mesh_of(&bsp, HullQuery::new(1)).is_err());
}

#[test]
fn malha_do_mapa_estragado_nao_da_panic() {
    let mut next = rng(42);
    let good = extra_bsp();
    let path = write_map("lixo");
    for _ in 0..30 {
        let mut bytes = good.clone();
        for _ in 0..8 {
            let at = 124 + (next() as usize) % (bytes.len() - 124);
            bytes[at] = next() as u8;
        }
        std::fs::write(&path, &bytes).unwrap();
        let _ = catalog::mesh(&path);
        let _ = hulls::hull_mesh(&path, 1);
        let _ = volumes::volumes(&path);
        let _ = scene::scene_of(&path);
    }
}

#[test]
fn modelo_com_num_faces_gigante_nao_trava() {
    let mut fx = extra_fx();
    fx.models[1].first_face = 2;
    fx.models[1].num_faces = i32::MAX;
    fx.models[2].first_face = i32::MAX;
    fx.models[2].num_faces = i32::MAX;
    let bsp = Bsp::parse(&build(&fx)).unwrap();
    let ents = entities::parse(&bsp.entities_raw);
    let r = volumes::collect(&bsp, &ents);
    assert!(r.volumes.iter().all(|v| v.triangles <= 4000));
    let _ = scene::brush_models(&bsp, &ents);
}

/// Árvore k-d aleatória com `n` clipnodes: mede o custo da varredura num mapa "grande".
/// Rodar com `cargo test --release -- --ignored arvore_grande --nocapture`.
#[test]
#[ignore]
fn arvore_grande_cabe_no_orcamento() {
    use crate::fixture_extra::Fx;
    fn grow(fx: &mut Fx, next: &mut dyn FnMut() -> u64, mins: [f32; 3], maxs: [f32; 3], target: usize, depth: usize) -> i16 {
        let axis = (next() % 3) as usize;
        let span = maxs[axis] - mins[axis];
        if fx.clipnodes.len() >= target || span < 24.0 || depth > 60 {
            return if next() % 3 == 0 { -1 } else { -2 };
        }
        let cut = (mins[axis] + span * (0.25 + (next() % 50) as f32 / 100.0)).round();
        let mut n = [0.0f32; 3];
        n[axis] = 1.0;
        fx.planes.push((n, cut));
        let plane = (fx.planes.len() - 1) as i32;
        let me = fx.clipnodes.len();
        fx.clipnodes.push((plane, [0, 0]));
        let mut hi_mins = mins;
        hi_mins[axis] = cut;
        let mut lo_maxs = maxs;
        lo_maxs[axis] = cut;
        let front = grow(fx, next, hi_mins, maxs, target, depth + 1);
        let back = grow(fx, next, mins, lo_maxs, target, depth + 1);
        fx.clipnodes[me].1 = [front, back];
        me as i16
    }
    let mut next = rng(7);
    let mut fx = extra_fx();
    fx.planes.clear();
    fx.clipnodes.clear();
    grow(&mut fx, &mut next, [-2000.0; 3], [2000.0; 3], 20_000, 0);
    fx.models[0].hull_heads = [0, -1, -1];
    fx.models[0].mins = [-2000.0; 3];
    fx.models[0].maxs = [2000.0; 3];
    let bsp = Bsp::parse(&build(&fx)).unwrap();
    let t = std::time::Instant::now();
    let r = bsp.hull_polygons(0, 1, 600_000).unwrap();
    eprintln!("{} clipnodes -> {} poligonos em {:?}, truncado={}", bsp.clipnodes.len(), r.polygons.len(), t.elapsed(), r.truncated);
}

// ------------------------------------------------------------------------------
// Dados das capturas da vista 3D (não roda no `cargo test` normal):
//   FIXTURE_DIR=/tmp/fx cargo test gerar_dados_extras_das_capturas -- --ignored
// Grava, em JSON, o que os comandos `map_detail`, `map_mesh`, `map_hulls`, `map_volumes` e
// `map_entities` devolvem para o mapa sintético dos recursos extras. O script
// `scripts/screenshots.mjs` serve isso ao frontend junto com os dados do acervo básico.

#[test]
#[ignore = "gera dados para as capturas: FIXTURE_DIR=<pasta> cargo test gerar_dados_extras_das_capturas -- --ignored"]
fn gerar_dados_extras_das_capturas() {
    use crate::bsp::render::RenderOptions;
    let out = PathBuf::from(std::env::var("FIXTURE_DIR").expect("defina FIXTURE_DIR"));
    let maps = out.join("cstrike").join("maps");
    std::fs::create_dir_all(&maps).unwrap();
    let path = maps.join("de_extras.bsp");
    std::fs::write(&path, extra_bsp()).unwrap();
    let put = |file: &str, text: String| std::fs::write(out.join(file), text).unwrap();
    let detail = catalog::detail(&path, RenderOptions::detail(), 32).unwrap();
    put("summary-de_extras.json", serde_json::to_string(&detail.summary).unwrap());
    put("detail-de_extras.json", serde_json::to_string(&detail).unwrap());
    put("thumb-de_extras.svg", catalog::thumbnail(&path).unwrap());
    put("mesh-de_extras.json", serde_json::to_string(&catalog::mesh(&path).unwrap()).unwrap());
    put("volumes-de_extras.json", serde_json::to_string(&volumes::volumes(&path).unwrap()).unwrap());
    for hull in [1usize, 2] {
        let w = hulls::hull_mesh_with(&path, HullQuery { hull, model: 0, offset: 0, limit: 200_000 }).unwrap();
        put(&format!("hulls-de_extras-{hull}.json"), serde_json::to_string(&w).unwrap());
    }
    let bsp = Bsp::parse(&extra_bsp()).unwrap();
    let parsed = entities::parse(&bsp.entities_raw);
    put("entities-de_extras.json", serde_json::to_string(&crate::entity_list::list(&bsp, &parsed)).unwrap());
}
