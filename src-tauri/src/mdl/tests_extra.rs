//! Testes dos extras do `.mdl`: bone controllers, blends, bodygroups, remap de
//! cor, flags de material, hitboxes, attachments, eventos e textura externa.
//! O modelo é montado byte a byte (nenhum arquivo da Valve no repositório).

use super::tests::{assert_close, no_ext, put_f32s, put_i16s, put_i32, put_str, Lcg};
use super::*;

fn off(b: &[u8]) -> i32 {
    b.len() as i32
}

fn set_i32(v: &mut [u8], at: usize, x: i32) {
    v[at..at + 4].copy_from_slice(&x.to_le_bytes());
}

fn set_f32(v: &mut [u8], at: usize, x: f32) {
    v[at..at + 4].copy_from_slice(&x.to_le_bytes());
}

/// `mstudiobone_t`: `ctl` liga cada canal (X,Y,Z,XR,YR,ZR) a um controlador (-1 = nenhum).
fn bone(name: &str, parent: i32, value: [f32; 6], ctl: [i32; 6]) -> Vec<u8> {
    let mut b = Vec::new();
    put_str(&mut b, name, 32);
    put_i32(&mut b, parent);
    put_i32(&mut b, 0); // flags
    for c in ctl {
        put_i32(&mut b, c);
    }
    put_f32s(&mut b, &value);
    put_f32s(&mut b, &[1.0, 1.0, 1.0, 0.01, 0.01, 0.01]); // scale
    assert_eq!(b.len(), BONE_SIZE);
    b
}

fn controller(bone: i32, kind: i32, start: f32, end: f32, rest: i32, index: i32) -> Vec<u8> {
    let mut c = Vec::new();
    put_i32(&mut c, bone);
    put_i32(&mut c, kind);
    put_f32s(&mut c, &[start, end]);
    put_i32(&mut c, rest);
    put_i32(&mut c, index);
    assert_eq!(c.len(), CONTROLLER_SIZE);
    c
}

struct Sd<'a> {
    name: &'a str,
    frames: i32,
    flags: i32,
    num_events: i32,
    event_index: i32,
    num_blends: i32,
    anim_index: i32,
    blend_type0: i32,
    blend_range0: [f32; 2],
}

fn seqdesc(sd: &Sd) -> Vec<u8> {
    let mut s = vec![0u8; SEQDESC_SIZE];
    s[..sd.name.len()].copy_from_slice(sd.name.as_bytes());
    set_f32(&mut s, 32, 10.0); // fps
    set_i32(&mut s, 36, sd.flags);
    set_i32(&mut s, 40, 7); // activity
    set_i32(&mut s, 44, 3); // actweight
    set_i32(&mut s, 48, sd.num_events);
    set_i32(&mut s, 52, sd.event_index);
    set_i32(&mut s, 56, sd.frames);
    set_i32(&mut s, 120, sd.num_blends);
    set_i32(&mut s, 124, sd.anim_index);
    set_i32(&mut s, 128, sd.blend_type0);
    set_f32(&mut s, 136, sd.blend_range0[0]);
    set_f32(&mut s, 144, sd.blend_range0[1]);
    s
}

fn submodel_header(name: &str, mesh: i32, vertinfo: i32, verts: i32) -> Vec<u8> {
    let mut m = Vec::new();
    put_str(&mut m, name, 64);
    put_i32(&mut m, 0); // type
    put_f32s(&mut m, &[0.0]); // boundingradius
    put_i32(&mut m, 1); // nummesh
    put_i32(&mut m, mesh);
    put_i32(&mut m, 3); // numverts
    put_i32(&mut m, vertinfo);
    put_i32(&mut m, verts);
    m.resize(MODEL_SIZE, 0);
    m
}

struct Rich {
    data: Vec<u8>,
    /// offset do `mstudioseqdesc_t` de `walk`
    walk: usize,
}

/// Modelo com tudo: 2 bones (`root`, `head` em z=10), 3 controladores, 2 texturas
/// (remap + material especial), 2 famílias de skin, 2 bodyparts (`body` com 2
/// submodels, `gun` com 1), 1 hitbox, 1 attachment, 3 eventos e 2 sequências:
/// - `walk`: 2 quadros, 2 blends (eixo YR -45..45). Blend 0 = repouso; blend 1 =
///   X do root +4 e rotZ do head 1.0 rad;
/// - `idle`: 1 quadro, 1 blend.
///
/// Controladores: 0 = X do root (0..10, slot 0); 1 = ZR do head (0..45°, boca);
/// 2 = YR do head com wrap (0..360°, slot 1).
fn rich_mdl() -> Rich {
    let mut b = vec![0u8; 244];

    let bone_index = off(&b);
    b.extend(bone("root", -1, [0.0; 6], [0, -1, -1, -1, -1, -1]));
    b.extend(bone("head", 0, [0.0, 0.0, 10.0, 0.0, 0.0, 0.0], [-1, -1, -1, -1, 2, 1]));

    let ctl_index = off(&b);
    b.extend(controller(0, 0x0001, 0.0, 10.0, 0, 0));
    b.extend(controller(1, 0x0020, 0.0, 45.0, 0, 4));
    b.extend(controller(1, 0x0010 | 0x8000, 0.0, 360.0, 0, 1));

    let hit_index = off(&b);
    put_i32(&mut b, 1); // bone
    put_i32(&mut b, 3); // group
    put_f32s(&mut b, &[0.0, 0.0, 0.0, 4.0, 2.0, 2.0]);

    let att_index = off(&b);
    put_str(&mut b, "muzzle", 32);
    put_i32(&mut b, 0); // type
    put_i32(&mut b, 1); // bone
    put_f32s(&mut b, &[1.0, 2.0, 3.0]); // org
    put_f32s(&mut b, &[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]); // vectors
    assert_eq!(b.len() as i32 - att_index, ATTACHMENT_SIZE as i32);

    // texturas: 0 = "Remap1_160_191_223" 2x1 (pixels 170 e 200, ambos azul puro);
    // 1 = "shiny" 1x1, com chrome|flatshade|fullbright|additive|masked
    let tex_index = off(&b);
    let pix0 = tex_index + 2 * TEXTURE_SIZE as i32;
    let pix1 = pix0 + 2 + 768;
    for (name, flags, w, px) in [("Remap1_160_191_223", 0, 2, pix0), ("shiny", 0x67, 1, pix1)] {
        put_str(&mut b, name, 64);
        put_i32(&mut b, flags);
        put_i32(&mut b, w);
        put_i32(&mut b, 1);
        put_i32(&mut b, px);
    }
    b.extend_from_slice(&[170, 200]);
    let mut pal = vec![0u8; 768];
    pal[170 * 3..170 * 3 + 3].copy_from_slice(&[0, 0, 255]);
    pal[200 * 3..200 * 3 + 3].copy_from_slice(&[0, 0, 255]);
    b.extend(pal);
    b.push(5);
    let mut pal = vec![0u8; 768];
    pal[15..18].copy_from_slice(&[10, 20, 30]);
    b.extend(pal);

    let skin_index = off(&b);
    put_i16s(&mut b, &[0, 1, 1, 0]); // família 0 = [0,1], família 1 = [1,0]

    // bodyparts + submodels
    let bp_index = off(&b);
    let models = bp_index + 2 * BODYPART_SIZE as i32;
    let data0 = models + 3 * MODEL_SIZE as i32;
    // por submodel: vertinfo (3) + verts (36) + mesh (20) + stream (28) = 87 bytes
    let per = 87;
    put_str(&mut b, "body", 64);
    put_i32(&mut b, 2);
    put_i32(&mut b, 1); // base
    put_i32(&mut b, models);
    put_str(&mut b, "gun", 64);
    put_i32(&mut b, 1);
    put_i32(&mut b, 2); // base = nummodels do anterior
    put_i32(&mut b, models + 2 * MODEL_SIZE as i32);
    for (i, name) in ["a", "b", "g"].into_iter().enumerate() {
        let d = data0 + i as i32 * per;
        b.extend(submodel_header(name, d + 39, d, d + 3));
    }
    // (bones por vértice, vértices locais, skinref, s do vértice 1)
    type Sub = ([u8; 3], [[f32; 3]; 3], i32, i16);
    let subs: [Sub; 3] = [
        ([0, 1, 1], [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], 0, 2),
        ([0, 0, 1], [[5.0, 0.0, 0.0], [6.0, 0.0, 0.0], [5.0, 1.0, 0.0]], 1, 0),
        ([1, 1, 1], [[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 1.0]], 0, 0),
    ];
    for (i, (bones, verts, skinref, s1)) in subs.into_iter().enumerate() {
        let d = data0 + i as i32 * per;
        assert_eq!(off(&b), d);
        b.extend_from_slice(&bones);
        for v in verts {
            put_f32s(&mut b, &v);
        }
        // mesh: numtris, triindex (= d + 3 + 36 + 20), skinref, numnorms, normindex
        for v in [1, d + 59, skinref, 0, 0] {
            put_i32(&mut b, v);
        }
        assert_eq!(off(&b), d + 59);
        put_i16s(&mut b, &[3]);
        for (vi, s) in [(0i16, 0i16), (1, s1), (2, 0)] {
            put_i16s(&mut b, &[vi, 0, s, 0]);
        }
        put_i16s(&mut b, &[0]);
    }

    // eventos de `walk`
    let ev_index = off(&b);
    for (frame, event, opt) in [(0, 5001, "1"), (1, 5004, "weapons/x.wav"), (1, 3, "")] {
        put_i32(&mut b, frame);
        put_i32(&mut b, event);
        put_i32(&mut b, 0);
        put_str(&mut b, opt, 64);
    }

    // sequências
    let seq_index = off(&b);
    let anim0 = seq_index + 2 * SEQDESC_SIZE as i32;
    let anim1 = anim0 + 60;
    b.extend(seqdesc(&Sd {
        name: "walk",
        frames: 2,
        flags: 1,
        num_events: 3,
        event_index: ev_index,
        num_blends: 2,
        anim_index: anim0,
        blend_type0: 0x0010,
        blend_range0: [-45.0, 45.0],
    }));
    b.extend(seqdesc(&Sd {
        name: "idle",
        frames: 1,
        flags: 0,
        num_events: 0,
        event_index: 0,
        num_blends: 1,
        anim_index: anim1,
        blend_type0: 0,
        blend_range0: [0.0, 0.0],
    }));
    // anim de `walk`: blend 0 (2 x 12 bytes zerados), blend 1 e os trechos RLE
    b.extend_from_slice(&[0u8; 24]);
    let mut a = [[0u16; 6]; 2];
    a[0][0] = 24; // X do root: stream em anim0 + 48, base do bone em anim0 + 24
    a[1][5] = 18; // rotZ do head: stream em anim0 + 54, base do bone em anim0 + 36
    for bone in a {
        for v in bone {
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    b.extend_from_slice(&[2, 2]);
    put_i16s(&mut b, &[4, 4]);
    b.extend_from_slice(&[2, 2]);
    put_i16s(&mut b, &[100, 100]);
    assert_eq!(off(&b), anim1);
    b.extend_from_slice(&[0u8; 24]); // anim de `idle`

    let mut h = Vec::new();
    h.extend_from_slice(IDENT);
    put_i32(&mut h, VERSION);
    put_str(&mut h, "rich", 64);
    put_i32(&mut h, b.len() as i32);
    h.extend_from_slice(&[0u8; 60]); // eyeposition, min, max, bbmin, bbmax
    put_i32(&mut h, 0); // flags
    for v in [
        2, bone_index, 3, ctl_index, 1, hit_index, 2, seq_index, 0, 0, 2, tex_index, 0, 2, 2, skin_index, 2, bp_index, 1,
        att_index,
    ] {
        put_i32(&mut h, v);
    }
    h.resize(244, 0);
    b[..244].copy_from_slice(&h);

    Rich { data: b, walk: seq_index as usize }
}

fn pose(frames: &SeqFrames, frame: usize, bone: usize) -> [f32; 7] {
    let nb = frames.bones as usize;
    let mut p = [0.0; 7];
    p.copy_from_slice(&frames.data[(frame * nb + bone) * POSE_STRIDE..][..POSE_STRIDE]);
    p
}

fn walk(r: &Rich, opts: &SeqOptions) -> SeqFrames {
    sequence_frames_with(&r.data, 0, &no_ext, opts).expect("walk válido")
}

#[test]
fn extras_do_modelo_rico() {
    let r = rich_mdl();
    let m = parse(&r.data).expect("modelo rico válido");
    let x = &m.extra;
    assert!(x.warnings.is_empty(), "{:?}", x.warnings);
    assert_eq!(x.bone_info.iter().map(|b| (b.name.as_str(), b.parent)).collect::<Vec<_>>(), [("root", -1), ("head", 0)]);

    // controladores
    assert_eq!(x.controllers.len(), 3);
    let c = &x.controllers[0];
    assert_eq!((c.index, c.name.as_str(), c.kind.as_str(), c.unit.as_str(), c.wrap), (0, "controller0", "x", "units", false));
    assert_eq!((c.start, c.end, c.rest, c.input_max, c.bone), (0.0, 10.0, 0, 255, 0));
    let mouth = &x.controllers[1];
    assert_eq!((mouth.name.as_str(), mouth.kind.as_str(), mouth.unit.as_str(), mouth.input_max), ("mouth", "zr", "deg", 64));
    let wrap = &x.controllers[2];
    assert_eq!((wrap.name.as_str(), wrap.kind.as_str(), wrap.wrap, wrap.raw_type), ("controller1", "yr", true, 0x8010));

    // hitbox e attachment na pose de repouso (head em z=10)
    assert_eq!(x.hitboxes.len(), 1);
    let hb = &x.hitboxes[0];
    assert_eq!((hb.bone, hb.group), (1, 3));
    assert_close(&hb.rest.center, &[2.0, 1.0, 11.0]);
    assert_close(&hb.rest.half, &[2.0, 1.0, 1.0]);
    assert_close(&hb.rest.rot, &[0.0, 0.0, 0.0, 1.0]);
    assert_eq!(x.attachments.len(), 1);
    let at = &x.attachments[0];
    assert_eq!((at.name.as_str(), at.bone, at.kind), ("muzzle", 1, 0));
    assert_close(&at.rest, &[1.0, 2.0, 13.0]);

    // sequências
    assert_eq!(x.seq_details.len(), 2);
    let walk = &x.seq_details[0];
    assert_eq!((walk.flags, walk.activity, walk.act_weight), (1, 7, 3));
    assert_eq!((walk.blend_type, walk.blend_axis.clone()), ([0x10, 0], ["yr".to_string(), String::new()]));
    assert_eq!((walk.blend_start[0], walk.blend_end[0]), (-45.0, 45.0));
    assert_eq!(m.seq_info[0].blends, 2);
    assert_eq!(x.seq_details[1].events.len(), 0);
    assert_eq!(x.selection, Selection { body: 0, skin: 0, top: None, bottom: None });
}

#[test]
fn eventos_ganham_nome_e_lado() {
    let m = parse(&rich_mdl().data).unwrap();
    let ev = &m.extra.seq_details[0].events;
    assert_eq!(ev.len(), 3);
    assert_eq!((ev[0].frame, ev[0].event, ev[0].options.as_str()), (0, 5001, "1"));
    assert_eq!((ev[0].name.as_deref(), ev[0].side.as_str(), ev[0].attachment), (Some("muzzleflash"), "client", Some(0)));
    assert_eq!((ev[1].name.as_deref(), ev[1].options.as_str()), (Some("sound"), "weapons/x.wav"));
    // o "3" do pedido não existe no SDK: fica sem nome, mas o ID e o lado vão
    assert_eq!((ev[2].event, ev[2].name.clone(), ev[2].side.as_str(), ev[2].attachment), (3, None, "server", None));

    assert_eq!(event_info(5011).map(|e| (e.name, e.attachment)), Some(("muzzleflash", Some(1))));
    assert_eq!(event_info(5031).and_then(|e| e.attachment), Some(3));
    assert_eq!(event_info(5002).map(|e| e.name), Some("spark"));
    assert_eq!(event_info(1000).map(|e| e.name), Some("SCRIPT_EVENT_DEAD"));
    assert_eq!(event_info(1010).map(|e| e.name), Some("SCRIPT_EVENT_NOT_DEAD"));
    assert!(event_info(1011).is_none() && event_info(0).is_none() && event_info(5003).is_none());
}

#[test]
fn bodygroup_escolhe_um_submodel_por_bodypart() {
    let r = rich_mdl();
    let m0 = parse(&r.data).unwrap();
    let parts = &m0.extra.bodyparts;
    assert_eq!(parts.len(), 2);
    assert_eq!((parts[0].name.as_str(), parts[0].base, parts[0].models.clone()), ("body", 1, vec!["a".to_string(), "b".to_string()]));
    assert_eq!((parts[1].name.as_str(), parts[1].base, parts[1].models.len()), ("gun", 2, 1));

    // body 0: submodel a + gun; 2 triângulos
    assert_eq!(m0.texindex, vec![0, 0]);
    assert_close(&m0.positions[0..9], &[0.0, 0.0, 0.0, 1.0, 0.0, 10.0, 0.0, 1.0, 10.0]);
    assert_close(&m0.positions[9..18], &[0.0, 0.0, 10.0, 0.0, 0.0, 11.0, 0.0, 1.0, 11.0]);
    assert_eq!(m0.vert_bones, vec![0, 1, 1, 1, 1, 1]);

    // body 1: (1/1)%2 = 1 -> submodel b; (1/2)%1 = 0 -> gun
    let opts = ParseOptions { body: 1, ..Default::default() };
    let m1 = parse_with(&r.data, None, &opts).unwrap();
    assert_eq!(m1.texindex, vec![1, 0]);
    assert_close(&m1.positions[0..9], &[5.0, 0.0, 0.0, 6.0, 0.0, 0.0, 5.0, 1.0, 10.0]);
    assert_eq!(m1.extra.selection.body, 1);

    // body 2 e 3: o gun só tem 1 modelo (sempre o mesmo); body 2 volta ao submodel a
    let m2 = parse_with(&r.data, None, &ParseOptions { body: 2, ..Default::default() }).unwrap();
    assert_eq!(m2.positions, m0.positions);
    let m3 = parse_with(&r.data, None, &ParseOptions { body: 3, ..Default::default() }).unwrap();
    assert_eq!(m3.positions, m1.positions);
    // valor absurdo não quebra
    assert!(parse_with(&r.data, None, &ParseOptions { body: u32::MAX, ..Default::default() }).is_ok());

    // API body_value <-> escolha
    for (choices, body) in [([0u32, 0], 0u32), ([1, 0], 1), ([0, 5], 0), ([9, 0], 1)] {
        assert_eq!(body_value(parts, &choices), body, "{choices:?}");
    }
    for body in 0..8u32 {
        let want = [parts[0].choice(body) as u32, parts[1].choice(body) as u32];
        assert_eq!(parts[0].choice(body_value(parts, &want)), want[0] as usize);
    }
    // base 0/negativo vale 1 e bodypart sem modelos não divide por zero
    let odd = BodyPartInfo { name: String::new(), base: 0, models: vec!["x".into(), "y".into()] };
    assert_eq!((odd.choice(0), odd.choice(1), odd.choice(2)), (0, 1, 0));
    let empty = BodyPartInfo { name: String::new(), base: -3, models: vec![] };
    assert_eq!(empty.choice(7), 0);
    assert_eq!(body_value(&[empty], &[4]), 0);
}

#[test]
fn skin_troca_a_textura_e_o_uv_usa_a_textura_certa() {
    let r = rich_mdl();
    // família 0: o submodel a usa a textura 0 (2x1): s = 2 -> u = 1
    let m0 = parse(&r.data).unwrap();
    assert_eq!(m0.skin_families, vec![vec![0, 1], vec![1, 0]]);
    assert_close(&m0.uvs[0..6], &[0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
    // família 1: textura 1 (1x1) no lugar -> u = 2 e texindex já trocado
    let m1 = parse_with(&r.data, None, &ParseOptions { skin: 1, ..Default::default() }).unwrap();
    assert_eq!(m1.texindex, vec![1, 1]);
    assert_close(&m1.uvs[0..6], &[0.0, 0.0, 2.0, 0.0, 0.0, 0.0]);
    assert_eq!(m1.extra.selection.skin, 1);
    // skin fora da faixa vale 0 (como o SDK)
    let m9 = parse_with(&r.data, None, &ParseOptions { skin: 9, ..Default::default() }).unwrap();
    assert_eq!((m9.texindex, m9.extra.selection.skin), (vec![0, 0], 0));
}

#[test]
fn flags_de_material_por_textura() {
    let m = parse(&rich_mdl().data).unwrap();
    let t = &m.extra.texture_info;
    assert_eq!(t.len(), m.textures.len());
    assert!(!t[0].chrome && !t[0].additive && !t[0].masked && t[0].blend == "opaque");
    assert_eq!(t[0].remap, Some(RemapRange { top_start: 160, top_end: 191, bottom_end: 223 }));
    let s = &t[1];
    assert_eq!(s.name, "shiny");
    assert!(s.chrome && s.flatshade && s.fullbright && s.additive && s.masked);
    assert!(!s.nomips && !s.alpha);
    assert_eq!((s.blend.as_str(), s.flags, s.remap), ("additive", 0x67, None));
    // cada flag sozinha
    for (flag, check) in [
        (NF_FLATSHADE, TextureInfo::new("x", NF_FLATSHADE).flatshade),
        (NF_CHROME, TextureInfo::new("x", NF_CHROME).chrome),
        (NF_FULLBRIGHT, TextureInfo::new("x", NF_FULLBRIGHT).fullbright),
        (NF_NOMIPS, TextureInfo::new("x", NF_NOMIPS).nomips),
        (NF_ALPHA, TextureInfo::new("x", NF_ALPHA).alpha),
        (NF_ADDITIVE, TextureInfo::new("x", NF_ADDITIVE).additive),
        (NF_MASKED, TextureInfo::new("x", NF_MASKED).masked),
    ] {
        assert!(check, "flag {flag:#x}");
    }
    assert_eq!(TextureInfo::new("x", NF_MASKED).blend, "masked");
    assert_eq!((NF_FLATSHADE, NF_CHROME, NF_FULLBRIGHT, NF_ADDITIVE, NF_MASKED), (0x1, 0x2, 0x4, 0x20, 0x40));
}

#[test]
fn nomes_de_textura_de_remap() {
    let r = |n: &str| remap_range(n);
    assert_eq!(r("Remap1_160_191_223"), Some(RemapRange { top_start: 160, top_end: 191, bottom_end: 223 }));
    assert_eq!(r("remap2_000_015_031"), Some(RemapRange { top_start: 0, top_end: 15, bottom_end: 31 }));
    assert_eq!(r("REMAP9_300_191_223").map(|x| x.top_start), Some(255)); // limitado, como o bound() do motor
    let dm = Some(RemapRange { top_start: 160, top_end: 191, bottom_end: 223 });
    assert_eq!((r("DM_Base"), r("dm_base2"), r("DM_Basement")), (dm, dm, dm));
    for n in ["skin1", "", "Remap", "Remap1_160_191", "Remap1_xyz_191_223", "Remap1_160_19a_223", "emap1_160_191_223", "xRemap1_160_191_223"] {
        assert_eq!(r(n), None, "{n:?}");
    }
}

#[test]
fn hue_replace_e_remap_de_paleta() {
    let mut pal = vec![0u8; 768];
    pal[170 * 3..170 * 3 + 3].copy_from_slice(&[0, 0, 255]); // azul puro, faixa de cima
    pal[200 * 3..200 * 3 + 3].copy_from_slice(&[0, 0, 255]); // azul puro, faixa de baixo
    pal[100 * 3..100 * 3 + 3].copy_from_slice(&[0, 0, 255]); // fora das faixas
    pal[171 * 3..171 * 3 + 3].copy_from_slice(&[100, 50, 50]); // vermelho escuro (val 100, sat .5)
    let range = RemapRange { top_start: 160, top_end: 191, bottom_end: 223 };

    let mut p = pal.clone();
    remap_palette(&mut p, range, Some(85), Some(0)); // 85 -> 120° (verde); 0 -> 0° (vermelho)
    assert_eq!(&p[170 * 3..170 * 3 + 3], &[0, 255, 0]);
    assert_eq!(&p[200 * 3..200 * 3 + 3], &[255, 0, 0]);
    assert_eq!(&p[100 * 3..100 * 3 + 3], &[0, 0, 255], "fora da faixa não muda");
    // o brilho e a saturação são mantidos: (100,50,50) -> verde com val 100 e mínimo 50
    // (±1: o motor trunca o float para byte)
    for (got, want) in p[171 * 3..171 * 3 + 3].iter().zip([50u8, 100, 50]) {
        assert!(got.abs_diff(want) <= 1, "{:?}", &p[171 * 3..171 * 3 + 3]);
    }
    assert_eq!(&p[..3], &[0, 0, 0], "preto fica como está");

    // só uma das cores: a outra faixa não muda
    let mut only_top = pal.clone();
    remap_palette(&mut only_top, range, Some(85), None);
    assert_eq!(&only_top[200 * 3..200 * 3 + 3], &[0, 0, 255]);
    assert_eq!(&only_top[170 * 3..170 * 3 + 3], &[0, 255, 0]);

    // matiz 170 (240°) é azul; 255 (360°) volta ao vermelho; todos os 256 matizes terminam sem NaN/panic
    let mut c = vec![0u8; 768];
    c[160 * 3..160 * 3 + 3].copy_from_slice(&[255, 0, 0]);
    hue_replace(&mut c, 170, 160, 160);
    assert_eq!(&c[160 * 3..160 * 3 + 3], &[0, 0, 255]);
    for hue in 0..=255u8 {
        let mut q = pal.clone();
        hue_replace(&mut q, hue, 0, 255);
        let mx = q[170 * 3..170 * 3 + 3].iter().copied().max().unwrap();
        assert!(mx >= 254, "matiz {hue} perdeu brilho: {:?}", &q[170 * 3..170 * 3 + 3]);
    }
    // paleta curta e faixa invertida não dão panic
    hue_replace(&mut [0u8; 9], 10, 0, 255);
    hue_replace(&mut pal.clone(), 10, 200, 100);
}

#[test]
fn remap_recolore_so_a_textura_de_remap() {
    let r = rich_mdl();
    let png = |rgba: &[u8], w| crate::bsp::rgba_png(w, 1, rgba).unwrap();
    let plain = parse(&r.data).unwrap();
    assert_eq!(plain.textures[0].png, png(&[0, 0, 255, 255, 0, 0, 255, 255], 2));
    let opts = ParseOptions { top: Some(85), bottom: Some(0), ..Default::default() };
    let team = parse_with(&r.data, None, &opts).unwrap();
    assert_eq!(team.textures[0].png, png(&[0, 255, 0, 255, 255, 0, 0, 255], 2));
    assert_eq!(team.textures[1].png, plain.textures[1].png, "textura comum não é tocada");
    assert_eq!((team.extra.selection.top, team.extra.selection.bottom), (Some(85), Some(0)));
}

#[test]
fn controladores_convertem_byte_em_valor_e_voltam() {
    let m = parse(&rich_mdl().data).unwrap();
    let [x, mouth, wrap] = [&m.extra.controllers[0], &m.extra.controllers[1], &m.extra.controllers[2]];
    // faixa normal: 0..255 -> start..end
    assert_eq!((x.value_for(0), x.value_for(255)), (0.0, 10.0));
    assert!((x.value_for(128) - 5.0196).abs() < 1e-3);
    // boca: byte/64 saturado
    assert_eq!((mouth.value_for(0), mouth.value_for(64), mouth.value_for(255)), (0.0, 45.0, 45.0));
    assert_eq!(mouth.value_for(32), 22.5);
    // wrap: 256 passos = 360°
    assert_eq!((wrap.value_for(0), wrap.value_for(64), wrap.value_for(128)), (0.0, 90.0, 180.0));
    // inversos
    assert_eq!((x.byte_for(0.0), x.byte_for(10.0), x.byte_for(99.0), x.byte_for(-5.0)), (0, 255, 255, 0));
    assert_eq!((mouth.byte_for(45.0), mouth.byte_for(22.5), mouth.byte_for(0.0)), (64, 32, 0));
    assert_eq!((wrap.byte_for(90.0), wrap.byte_for(-90.0), wrap.byte_for(360.0)), (64, 192, 0));
    for b in [0u8, 1, 50, 128, 200, 255] {
        assert!((i32::from(x.byte_for(x.value_for(b))) - i32::from(b)).abs() <= 1, "byte {b}");
    }
    // controlador sem faixa não divide por zero
    let flat = ControllerInfo { start: 3.0, end: 3.0, ..x.clone() };
    assert_eq!(flat.byte_for(3.0), 0);
}

#[test]
fn sem_opcoes_a_pose_e_a_de_antes() {
    let r = rich_mdl();
    let f = sequence_frames(&r.data, 0, &no_ext).unwrap();
    assert_eq!((f.frames, f.bones), (2, 2));
    for fr in 0..2 {
        assert_close(&pose(&f, fr, 0), &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        assert_close(&pose(&f, fr, 1), &[0.0, 0.0, 10.0, 0.0, 0.0, 0.0, 1.0]);
    }
    assert!(f.hitboxes.is_empty());
    assert_eq!(f.attachments.len(), 2 * 3, "attachments sempre vão (3 floats por quadro)");
    // `SeqOptions::default()` é o mesmo
    let g = walk(&r, &SeqOptions::default());
    assert_eq!(f.data, g.data);
}

#[test]
fn blend_interpola_entre_os_dois_blends() {
    let r = rich_mdl();
    // 255 = blend 1: X do root +4, rotZ do head 1.0 rad
    let f = walk(&r, &SeqOptions { blend: [255, 0], ..Default::default() });
    assert_close(&pose(&f, 0, 0), &[4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    assert_close(&pose(&f, 1, 1), &[4.0, 0.0, 10.0, 0.0, 0.0, 0.5f32.sin(), 0.5f32.cos()]);
    // 128 = 128/255 do caminho (slerp do head, lerp do root)
    let s = 128.0 / 255.0;
    let f = walk(&r, &SeqOptions { blend: [128, 0], ..Default::default() });
    let ang = s * 1.0;
    assert_close(&pose(&f, 0, 0)[..3], &[4.0 * s, 0.0, 0.0]);
    assert_close(&pose(&f, 0, 1)[3..], &[0.0, 0.0, (ang / 2.0).sin(), (ang / 2.0).cos()]);
    // o segundo blender não faz nada em sequência de 2 blends
    let g = walk(&r, &SeqOptions { blend: [128, 255], ..Default::default() });
    assert_eq!(f.data, g.data);
    // sequência de um blend ignora o valor
    let a = sequence_frames_with(&r.data, 1, &no_ext, &SeqOptions { blend: [255, 255], ..Default::default() }).unwrap();
    let b = sequence_frames(&r.data, 1, &no_ext).unwrap();
    assert_eq!(a.data, b.data);
    assert_eq!(a.frames, 1);
}

#[test]
fn blend_byte_converte_graus_em_byte() {
    let m = parse(&rich_mdl().data).unwrap();
    let d = &m.extra.seq_details[0];
    assert_eq!((d.blend_byte(0, -45.0), d.blend_byte(0, 45.0), d.blend_byte(0, 0.0)), (0, 255, 127));
    assert_eq!((d.blend_byte(0, 90.0), d.blend_byte(0, -90.0)), (255, 0)); // fora da faixa satura
    assert_eq!(d.blend_byte(1, 10.0), 0, "blender sem eixo");
    // wrap do ângulo: 300° equivale a -60° (vira 0 por estar abaixo de -45)
    assert_eq!(d.blend_byte(0, 300.0), 0);
}

#[test]
fn plano_de_blends_segue_o_sdk_para_2_e_4_e_generaliza_o_resto() {
    // cada folha vale o próprio índice (posição x): o resultado é o índice interpolado
    let eval = |n: usize, second: bool, blend: [u8; 2]| {
        let plan = Plan::new(n, second, blend);
        let mut leaves = Vec::new();
        plan.leaves(&mut leaves);
        let locals: Vec<Vec<Transform>> =
            leaves.iter().map(|&i| vec![Transform { q: [0.0, 0.0, 0.0, 1.0], pos: [i as f32, 0.0, 0.0] }]).collect();
        plan.eval(&locals, &leaves)[0].pos[0]
    };
    let close = |a: f32, b: f32| assert!((a - b).abs() < 1e-4, "{a} != {b}");
    close(eval(1, false, [255, 255]), 0.0);
    close(eval(2, false, [0, 0]), 0.0);
    close(eval(2, false, [255, 0]), 1.0);
    // 4 blends do SDK: (0,1) e (2,3) por blend[0]; os dois por blend[1]
    close(eval(4, false, [0, 0]), 0.0);
    close(eval(4, false, [255, 0]), 1.0);
    close(eval(4, false, [0, 255]), 2.0);
    close(eval(4, false, [255, 255]), 3.0);
    close(eval(4, false, [255, 255 / 2]), 1.0 * (1.0 - 127.0 / 255.0) + 3.0 * (127.0 / 255.0));
    // 9 blends com dois eixos: grade 3x3 (linha = blend[1], coluna = blend[0])
    close(eval(9, true, [0, 0]), 0.0);
    close(eval(9, true, [255, 0]), 2.0);
    close(eval(9, true, [0, 255]), 6.0);
    close(eval(9, true, [255, 255]), 8.0);
    // meio (128/255 nos dois eixos): célula (1,1) com fração f = 2*128/255 - 1
    let f = 2.0 * 128.0 / 255.0 - 1.0;
    close(eval(9, true, [128, 128]), (4.0 + f) * (1.0 - f) + (7.0 + f) * f);
    // 9 blends em um eixo só: 9 amostras percorridas por blend[0]
    close(eval(9, false, [0, 0]), 0.0);
    close(eval(9, false, [255, 0]), 8.0);
    close(eval(9, false, [255, 255]), 8.0);
    close(eval(9, false, [128, 0]), 8.0 * 128.0 / 255.0);
    // N absurdo não estoura
    let _ = eval(1 << 30, true, [255, 255]);
    let _ = eval(3, false, [200, 0]);
}

#[test]
fn blend_alem_do_arquivo_e_erro_nao_panic() {
    let r = rich_mdl();
    let mut data = r.data.clone();
    set_i32(&mut data, r.walk + 120, 9); // 9 blends, mas só há dados de 2 (e depois a sequência idle/fim do arquivo)
    for blend in [[0, 0], [255, 0], [255, 255]] {
        let _ = sequence_frames_with(&data, 0, &no_ext, &SeqOptions { blend, ..Default::default() });
    }
    set_i32(&mut data, r.walk + 120, i32::MAX);
    assert!(sequence_frames_with(&data, 0, &no_ext, &SeqOptions { blend: [255, 255], ..Default::default() }).is_err());
}

#[test]
fn slerp_do_sdk() {
    let id = [0.0, 0.0, 0.0, 1.0];
    let z90 = angle_quaternion([0.0, 0.0, std::f32::consts::FRAC_PI_2]);
    let mid = quat_slerp(&id, &z90, 0.5);
    assert_close(&mid, &angle_quaternion([0.0, 0.0, std::f32::consts::FRAC_PI_4]));
    assert_close(&quat_slerp(&id, &z90, 0.0), &id);
    assert_close(&quat_slerp(&id, &z90, 1.0), &z90);
    // quaternion oposto (mesma rotação) segue o caminho curto
    let neg = z90.map(|v| -v);
    assert_close(&quat_slerp(&id, &neg, 0.5), &mid);
    // iguais: sem divisão por zero
    assert_close(&quat_slerp(&id, &id, 0.3), &id);
}

#[test]
fn controladores_de_bone_entram_na_pose() {
    let r = rich_mdl();
    let with = |c: [u8; 5]| walk(&r, &SeqOptions { controllers: Some(c), ..Default::default() });

    // todos em zero: ajuste = start de cada um (0 aqui) = pose de antes
    let base = walk(&r, &SeqOptions::default());
    assert_eq!(with([0; 5]).data, base.data);

    // controlador 0 (X do root, 0..10): byte 255 -> +10 em X; o head, filho, acompanha
    let f = with([255, 0, 0, 0, 0]);
    assert_close(&pose(&f, 0, 0)[..3], &[10.0, 0.0, 0.0]);
    assert_close(&pose(&f, 0, 1)[..3], &[10.0, 0.0, 10.0]);
    let f = with([51, 0, 0, 0, 0]); // 51/255 = 0.2 -> 2
    assert_close(&pose(&f, 1, 0)[..3], &[2.0, 0.0, 0.0]);

    // boca (índice 4, 0..45° em rotZ do head): 64 = 45°; 32 = 22.5°; 200 satura em 45°
    for (mouth, deg) in [(64u8, 45.0f32), (32, 22.5), (200, 45.0)] {
        let f = with([0, 0, 0, 0, mouth]);
        let a = deg.to_radians();
        assert_close(&pose(&f, 0, 1)[3..], &[0.0, 0.0, (a / 2.0).sin(), (a / 2.0).cos()]);
    }

    // controlador 1 (YR com wrap no head): 64 -> 90° em torno de Y; (1,0,0) vai para (0,0,-1)
    let f = with([0, 64, 0, 0, 0]);
    let q = pose(&f, 0, 1);
    assert_close(&quat_rotate(&[q[3], q[4], q[5], q[6]], [1.0, 0.0, 0.0]), &[0.0, 0.0, -1.0]);
    // ... e o wrap dá a volta: 192 = 270° = -90°
    let f = with([0, 192, 0, 0, 0]);
    let q = pose(&f, 0, 1);
    assert_close(&quat_rotate(&[q[3], q[4], q[5], q[6]], [1.0, 0.0, 0.0]), &[0.0, 0.0, 1.0]);

    // blend e controlador somam: root X = 4 (blend 1) + 10
    let f = walk(&r, &SeqOptions { blend: [255, 0], controllers: Some([255, 0, 0, 0, 0]), ..Default::default() });
    assert_close(&pose(&f, 0, 0)[..3], &[14.0, 0.0, 0.0]);
}

#[test]
fn attachments_e_hitboxes_seguem_a_pose_por_quadro() {
    let r = rich_mdl();
    let f = walk(&r, &SeqOptions { blend: [255, 0], hitboxes: true, ..Default::default() });
    // head: posição (4,0,10), rotZ 1 rad
    let (s, c) = 1.0f32.sin_cos();
    let rot = |v: [f32; 3]| [4.0 + c * v[0] - s * v[1], s * v[0] + c * v[1], 10.0 + v[2]];
    for fr in 0..2 {
        assert_close(&f.attachments[fr * 3..fr * 3 + 3], &rot([1.0, 2.0, 3.0]));
        let hb = &f.hitboxes[fr * HITBOX_STRIDE..(fr + 1) * HITBOX_STRIDE];
        assert_close(&hb[..3], &rot([2.0, 1.0, 1.0])); // centro de (0,0,0)-(4,2,2)
        assert_close(&hb[3..6], &[2.0, 1.0, 1.0]);
        assert_close(&hb[6..], &[0.0, 0.0, 0.5f32.sin(), 0.5f32.cos()]);
    }
    // sem pedir, não vão caixas
    assert!(walk(&r, &SeqOptions::default()).hitboxes.is_empty());
    // o JSON só traz os campos quando existem
    let json = serde_json::to_value(walk(&r, &SeqOptions::default())).unwrap();
    assert!(json.get("hitboxes").is_none() && json.get("attachments").is_some());
}

#[test]
fn motiontype_zera_so_xyz_e_movimento_linear_nao_mexe_na_pose() {
    let r = rich_mdl();
    let opts = SeqOptions { blend: [255, 0], ..Default::default() };
    let base = walk(&r, &opts);

    // STUDIO_LX (0x40) + linearmovement: o SDK multiplica por 0, nada muda
    let mut d = r.data.clone();
    set_i32(&mut d, r.walk + 68, 0x40);
    set_i32(&mut d, r.walk + 72, 0);
    set_f32(&mut d, r.walk + 76, 100.0);
    let lin = sequence_frames_with(&d, 0, &no_ext, &opts).unwrap();
    assert_eq!(lin.data, base.data);
    let m = parse(&d).unwrap();
    assert_eq!((m.extra.seq_details[0].motion_type, m.extra.seq_details[0].linear_movement), (0x40, [100.0, 0.0, 0.0]));

    // STUDIO_X no root: o +4 do blend 1 some, o head (filho) vai junto
    set_i32(&mut d, r.walk + 68, STUDIO_X);
    let z = sequence_frames_with(&d, 0, &no_ext, &opts).unwrap();
    assert_close(&pose(&z, 0, 0)[..3], &[0.0, 0.0, 0.0]);
    assert_close(&pose(&z, 0, 1)[..3], &[0.0, 0.0, 10.0]);
    // motionbone fora da faixa é ignorado
    set_i32(&mut d, r.walk + 72, 77);
    assert_eq!(sequence_frames_with(&d, 0, &no_ext, &opts).unwrap().data, base.data);
    set_i32(&mut d, r.walk + 72, -1);
    assert_eq!(sequence_frames_with(&d, 0, &no_ext, &opts).unwrap().data, base.data);
}

// ---------------------------------------------------------------------------
// Textura externa (nomeT.mdl)

/// `synthetic_mdl()` sem as texturas (`numtextures = 0`): o par é o próprio
/// modelo completo, que faz o papel do `nomeT.mdl`.
fn without_textures() -> (Vec<u8>, Vec<u8>) {
    let full = synthetic_mdl();
    let mut main = full.clone();
    set_i32(&mut main, 180, 0); // numtextures
    (main, full)
}

#[test]
fn textura_externa_vem_do_arquivo_t() {
    let (main, t) = without_textures();
    assert!(needs_external_textures(&main) && !needs_external_textures(&t));

    let bare = parse(&main).expect("sem T: malha sem textura, não erro");
    assert_eq!((bare.textures.len(), bare.texindex.len()), (0, 1));
    assert!(!bare.extra.external_textures);

    let m = parse_with(&main, Some(t.as_slice()), &ParseOptions::default()).unwrap();
    assert_eq!((m.textures.len(), m.textures[0].name.as_str()), (1, "skin1"));
    assert!(m.extra.external_textures);
    assert_eq!(m.extra.texture_info.len(), 1);
    // T só é usado quando o principal não tem textura
    let full = parse_with(&t, Some(main.as_slice()), &ParseOptions::default()).unwrap();
    assert!(!full.extra.external_textures);

    // T ilegível: aviso e segue sem textura
    let broken = parse_with(&main, Some(&b"lixo"[..]), &ParseOptions::default()).unwrap();
    assert_eq!(broken.textures.len(), 0);
    assert_eq!(broken.extra.warnings.len(), 1, "{:?}", broken.extra.warnings);
}

#[test]
fn arquivos_irmaos_sao_achados_na_pasta_sem_diferenciar_maiusculas() {
    let dir = std::env::temp_dir().join("bsp-museum-tests-mdl-extra");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (main, t) = without_textures();
    let path = dir.join("Prop.mdl");
    std::fs::write(&path, &main).unwrap();

    // sem T: carrega, sem textura
    let m = crate::catalog::load_model(&path).unwrap();
    assert_eq!(m.textures.len(), 0);

    // `PropT.mdl` com a caixa do motor
    std::fs::write(dir.join("PropT.mdl"), &t).unwrap();
    assert_eq!(crate::catalog::load_model(&path).unwrap().textures.len(), 1);
    std::fs::remove_file(dir.join("PropT.mdl")).unwrap();
    // `propt.mdl` (outra caixa)
    std::fs::write(dir.join("propt.mdl"), &t).unwrap();
    assert_eq!(files::texture_file(&path).and_then(|p| p.file_name().map(|n| n.to_owned())).unwrap(), "propt.mdl");
    let m = crate::catalog::load_model(&path).unwrap();
    assert_eq!((m.textures.len(), m.extra.external_textures), (1, true));

    // grupos: prop01.mdl / PROP02.MDL
    assert!(files::group_file(&path, 1).is_none() && files::group_file(&path, 0).is_none());
    std::fs::write(dir.join("PROP01.mdl"), b"IDSQ").unwrap();
    assert_eq!(files::group_file(&path, 1).and_then(|p| p.file_name().map(|n| n.to_owned())).unwrap(), "PROP01.mdl");
    // o arquivo irmão precisa ser arquivo, não pasta
    std::fs::create_dir_all(dir.join("Prop02.mdl")).unwrap();
    assert!(files::group_file(&path, 2).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Contrato JSON (o que o frontend recebe)

#[test]
fn json_do_resumo_so_acrescenta_campos() {
    let dir = std::env::temp_dir().join("bsp-museum-tests-mdl-json");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rich.mdl");
    std::fs::write(&path, rich_mdl().data).unwrap();

    let s = crate::catalog::load_model_ex(&path, Some(1), Some(1), Some(85), None).unwrap();
    let j = serde_json::to_value(&s).unwrap();
    for k in ["positions", "uvs", "texindex", "textures", "sequences", "sequence_info", "local_positions", "vert_bones", "num_bones", "skin_families"] {
        assert!(j.get(k).is_some(), "campo antigo {k} sumiu");
    }
    for k in ["bone_info", "controllers", "bodyparts", "texture_info", "hitboxes", "attachments", "seq_details", "selection"] {
        assert!(j.get(k).is_some(), "campo novo {k} ausente");
    }
    assert!(j.get("warnings").is_none() && j.get("external_textures").is_none(), "vazios não são serializados");
    assert_eq!(j["selection"], serde_json::json!({ "body": 1, "skin": 1, "top": 85, "bottom": null }));
    assert_eq!(j["controllers"][1]["name"], "mouth");
    assert_eq!(j["bodyparts"][0]["models"], serde_json::json!(["a", "b"]));
    assert_eq!(j["texture_info"][1]["blend"], "additive");
    assert_eq!(j["texture_info"][0]["remap"]["top_end"], 191);
    assert_eq!(j["seq_details"][0]["events"][0]["name"], "muzzleflash");
    assert_eq!(j["seq_details"][0]["events"][0]["type"], 0);
    assert_eq!(j["attachments"][0]["type"], 0);
    assert_eq!(j["sequence_info"][0]["blends"], 2);

    // carregamento da sequência pelo caminho: blend + controlador + caixas
    let f = crate::catalog::load_sequence_ex(&path, 0, Some([255, 0]), Some([0, 0, 0, 0, 64]), true).unwrap();
    assert_eq!(f.hitboxes.len(), 2 * HITBOX_STRIDE);
    assert!((f.data[0] - 4.0).abs() < 1e-4);
    // a assinatura antiga continua igual
    let old = crate::catalog::load_sequence(&path, 0).unwrap();
    assert_eq!((old.frames, old.bones), (2, 2));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn modelo_simples_nao_traz_tabelas_vazias() {
    let s = serde_json::to_value(parse(&synthetic_mdl()).unwrap().extra).unwrap();
    for k in ["controllers", "hitboxes", "attachments", "warnings", "external_textures"] {
        assert!(s.get(k).is_none(), "{k} vazio não deveria ir pro JSON");
    }
    assert!(s.get("bodyparts").is_some() && s.get("selection").is_some());
}

// ---------------------------------------------------------------------------
// Robustez

fn exercita(data: &[u8], tex: &[u8]) {
    for opts in [
        ParseOptions::default(),
        ParseOptions { body: 1, skin: 1, top: Some(30), bottom: Some(200) },
        ParseOptions { body: u32::MAX, skin: u32::MAX, ..Default::default() },
    ] {
        let _ = parse_with(data, Some(tex), &opts);
    }
    let _ = needs_external_textures(data);
    let ext = tex.to_vec();
    for seq in 0..3 {
        for opts in [
            SeqOptions::default(),
            SeqOptions { blend: [255, 255], controllers: Some([255; 5]), hitboxes: true },
            SeqOptions { blend: [97, 3], controllers: Some([0, 64, 128, 200, 64]), hitboxes: false },
        ] {
            let _ = sequence_frames_with(data, seq, &|_| Some(ext.clone()), &opts);
        }
    }
}

#[test]
fn truncar_o_modelo_rico_em_qualquer_ponto_nao_da_panic() {
    let r = rich_mdl();
    for len in 0..r.data.len() {
        exercita(&r.data[..len], &r.data);
    }
    for len in (0..r.data.len()).step_by(7) {
        exercita(&r.data, &r.data[..len]);
    }
    // sem cabeçalho completo (< 244 bytes) o formato antigo de 212 bytes também é recusado sem panic
    assert!(parse(&r.data[..220]).is_err());
}

#[test]
fn campos_extremos_no_modelo_rico_nao_dao_panic() {
    let r = rich_mdl();
    for at in (0..r.data.len() - 3).step_by(4) {
        for v in [0i32, -1, 1, 255, 1 << 16, 1 << 30, i32::MAX, i32::MIN] {
            let mut data = r.data.clone();
            set_i32(&mut data, at, v);
            exercita(&data, &r.data);
        }
    }
}

#[test]
fn bytes_aleatorios_no_modelo_rico_nao_dao_panic() {
    let r = rich_mdl();
    let mut rng = Lcg(0xBADC0DE);
    for _ in 0..1500 {
        let mut data = r.data.clone();
        for _ in 0..1 + rng.next() % 8 {
            let at = rng.next() as usize % data.len();
            data[at] = rng.next() as u8;
        }
        exercita(&data, &r.data);
    }
}

#[test]
fn tabelas_opcionais_corrompidas_viram_aviso_e_a_malha_continua() {
    let r = rich_mdl();
    let mut data = r.data.clone();
    set_i32(&mut data, 148, 1 << 20); // numbonecontrollers absurdo
    set_i32(&mut data, 156, 1 << 20); // numhitboxes
    set_i32(&mut data, 212, 1 << 20); // numattachments
    set_i32(&mut data, r.walk + 48, -4); // numevents negativo
    let m = parse(&data).expect("tabelas opcionais não derrubam a malha");
    assert_eq!(m.texindex.len(), 2);
    assert!(m.extra.controllers.is_empty() && m.extra.hitboxes.is_empty() && m.extra.attachments.is_empty());
    assert_eq!(m.extra.warnings.len(), 4, "{:?}", m.extra.warnings);
    // a animação continua funcionando sem os extras
    let f = sequence_frames_with(&data, 0, &no_ext, &SeqOptions { controllers: Some([0; 5]), hitboxes: true, ..Default::default() }).unwrap();
    assert_eq!(f.frames, 2);
    assert!(f.hitboxes.is_empty() && f.attachments.is_empty());
}
