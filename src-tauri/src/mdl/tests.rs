//! Testes do parser de `.mdl`, mesmo espírito dos testes de BSP: o arquivo é
//! montado byte a byte, sem depender de nenhum `.mdl` real no repositório.

use super::*;

#[test]
fn decodifica_bone_textura_e_triangulo() {
    let model = parse(&synthetic_mdl()).expect("modelo sintético válido");

    assert_eq!(model.textures.len(), 1);
    assert_eq!(model.textures[0].name, "skin1");
    assert_eq!((model.textures[0].width, model.textures[0].height), (1, 1));

    assert_eq!(model.sequences, vec!["idle".to_string()]);

    // 1 triângulo => 9 floats de posição, 6 de UV, 1 texindex.
    assert_eq!(model.positions.len(), 9);
    assert_eq!(model.uvs.len(), 6);
    assert_eq!(model.texindex, vec![0]);

    // Bone só translada em X=10: vértice local (0,0,0) vira mundo (10,0,0).
    assert_eq!(&model.positions[0..3], &[10.0, 0.0, 0.0]);
    assert_eq!(&model.positions[3..6], &[11.0, 0.0, 0.0]);
    assert_eq!(&model.positions[6..9], &[10.0, 1.0, 0.0]);
}

#[test]
fn recusa_magic_errado() {
    let mut data = synthetic_mdl();
    data[0..4].copy_from_slice(b"NOPE");
    assert_eq!(parse(&data).unwrap_err(), MdlError::BadMagic);
}

#[test]
fn recusa_versao_errada() {
    let mut data = synthetic_mdl();
    data[4..8].copy_from_slice(&7i32.to_le_bytes());
    assert_eq!(parse(&data).unwrap_err(), MdlError::BadVersion(7));
}

#[test]
fn recusa_arquivo_pequeno_demais() {
    let data = synthetic_mdl();
    assert!(matches!(parse(&data[..10]), Err(MdlError::TooSmall { .. })));
}

#[test]
fn indice_apontando_fora_do_arquivo_vira_erro_nao_panic() {
    let mut data = synthetic_mdl();
    // boneindex (offset 144 do header: ident+version+name+length+5*vec3+flags+numbones)
    // passa a apontar bem depois do fim do arquivo.
    data[144..148].copy_from_slice(&900_000i32.to_le_bytes());
    assert!(parse(&data).is_err());
}

#[test]
fn arquivo_de_lixo_nao_causa_panic() {
    for len in [0usize, 1, 10, 64, 300, 1000] {
        let junk: Vec<u8> = (0..len).map(|i| (i * 53 % 251) as u8).collect();
        let _ = parse(&junk);
    }
}

// ---------------------------------------------------------------------------
// Animação: modelo sintético de 2 bones com sequência comprimida em RLE.

/// Modelo montado byte a byte: `root` (posição X=10) e `arm` (filho, posição
/// (0,5,0)), 2 texturas 1x1 + 2 famílias de skin, 1 triângulo (v0 no root, v1/v2
/// no arm) e 2 sequências:
/// - `idle` (4 quadros, 10 fps, looping): X do root em RLE com 2 trechos
///   (`valid=2,total=2: 0,2` e `valid=1,total=2: 4`, ou seja quadros 0,2,4,4);
///   rotZ do arm num trecho só (`0,50,100,157` x escala 0,01 rad);
/// - `ext` (grupo externo 1): os dados vivem em `ext_file()`.
struct Syn {
    data: Vec<u8>,
    /// fim da tabela de sequências: o que `parse` precisa ler
    parse_end: usize,
    /// offset do `mstudioseqdesc_t` de `idle`
    seq0: usize,
}

fn put_str(out: &mut Vec<u8>, s: &str, n: usize) {
    let mut buf = vec![0u8; n];
    buf[..s.len()].copy_from_slice(s.as_bytes());
    out.extend_from_slice(&buf);
}

fn put_i32(out: &mut Vec<u8>, v: i32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_f32s(out: &mut Vec<u8>, vs: &[f32]) {
    for v in vs {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn put_i16s(out: &mut Vec<u8>, vs: &[i16]) {
    for v in vs {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn off(b: &[u8]) -> i32 {
    b.len() as i32
}

fn seqdesc(name: &str, frames: i32, flags: i32, anim_index: i32, group: i32) -> Vec<u8> {
    let mut s = Vec::new();
    put_str(&mut s, name, 32);
    put_f32s(&mut s, &[10.0]); // fps
    put_i32(&mut s, flags);
    s.resize(56, 0); // activity, actweight, numevents, eventindex
    put_i32(&mut s, frames); // 56 numframes
    s.resize(120, 0); // pivots, motiontype, motionbone, linearmovement, automove, bbmin/bbmax
    put_i32(&mut s, 1); // 120 numblends
    put_i32(&mut s, anim_index); // 124 animindex
    s.resize(156, 0);
    put_i32(&mut s, group); // 156 seqgroup
    s.resize(SEQDESC_SIZE, 0);
    s
}

fn bone(name: &str, parent: i32, value: [f32; 6]) -> Vec<u8> {
    let mut b = Vec::new();
    put_str(&mut b, name, 32);
    put_i32(&mut b, parent);
    put_i32(&mut b, 0); // flags
    b.extend_from_slice(&[0u8; 24]); // bonecontroller[6]
    put_f32s(&mut b, &value);
    put_f32s(&mut b, &[1.0, 1.0, 1.0, 0.01, 0.01, 0.01]); // scale
    assert_eq!(b.len(), BONE_SIZE);
    b
}

fn synthetic_anim_mdl() -> Syn {
    let mut b = vec![0u8; 212]; // header, preenchido no fim

    let bone_index = off(&b);
    b.extend(bone("root", -1, [10.0, 0.0, 0.0, 0.0, 0.0, 0.0]));
    b.extend(bone("arm", 0, [0.0, 5.0, 0.0, 0.0, 0.0, 0.0]));

    // texturas 1x1: a = vermelho (índice 5), b = verde (índice 6)
    let tex_index = off(&b);
    let pixel_a = tex_index + 2 * TEXTURE_SIZE as i32;
    let pixel_b = pixel_a + 1 + 768;
    for (name, px) in [("a", pixel_a), ("b", pixel_b)] {
        put_str(&mut b, name, 64);
        put_i32(&mut b, 0); // flags
        put_i32(&mut b, 1); // width
        put_i32(&mut b, 1); // height
        put_i32(&mut b, px);
    }
    for (idx, rgb) in [(5usize, [255u8, 0, 0]), (6, [0, 255, 0])] {
        b.push(idx as u8);
        let mut pal = vec![0u8; 768];
        pal[idx * 3..idx * 3 + 3].copy_from_slice(&rgb);
        b.extend(pal);
    }

    // skinref[2 famílias][2 slots]: família 0 = [0,1], família 1 = [1,0]
    let skin_index = off(&b);
    put_i16s(&mut b, &[0, 1, 1, 0]);

    let bodypart_index = off(&b);
    let model_index = bodypart_index + BODYPART_SIZE as i32;
    put_str(&mut b, "body", 64);
    put_i32(&mut b, 1); // nummodels
    put_i32(&mut b, 0); // base
    put_i32(&mut b, model_index);

    let vertinfo = model_index + MODEL_SIZE as i32;
    let verts = vertinfo + 3;
    let mesh_index = verts + 36;
    let tri_index = mesh_index + MESH_SIZE as i32;
    put_str(&mut b, "sub", 64);
    put_i32(&mut b, 0); // type
    put_f32s(&mut b, &[0.0]); // boundingradius
    put_i32(&mut b, 1); // nummesh
    put_i32(&mut b, mesh_index);
    put_i32(&mut b, 3); // numverts
    put_i32(&mut b, vertinfo);
    put_i32(&mut b, verts);
    b.resize(model_index as usize + MODEL_SIZE, 0);

    b.extend_from_slice(&[0, 1, 1]); // vertinfo: v0 -> root, v1/v2 -> arm
    put_f32s(&mut b, &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);

    // mesh: skinref = slot 1 (que a família 0 liga à textura 1)
    put_i32(&mut b, 1);
    put_i32(&mut b, tri_index);
    put_i32(&mut b, 1);
    put_i32(&mut b, 0);
    put_i32(&mut b, 0);
    assert_eq!(off(&b), tri_index);
    put_i16s(&mut b, &[3]);
    for vi in 0..3i16 {
        put_i16s(&mut b, &[vi, 0, 0, 0]);
    }
    put_i16s(&mut b, &[0]);

    // sequências
    let seq_index = off(&b);
    let anim_index = seq_index + 2 * SEQDESC_SIZE as i32;
    b.extend(seqdesc("idle", 4, 1, anim_index, 0));
    b.extend(seqdesc("ext", 2, 0, 76, 1));
    let parse_end = b.len();

    // anim data do `idle`: 2 x mstudioanim_t (12 bytes) + valores RLE
    let base0 = anim_index as usize;
    let stream_x = base0 + 24;
    let stream_rz = stream_x + 10;
    let mut a0 = [0u16; 6];
    a0[0] = (stream_x - base0) as u16; // X do root
    let mut a1 = [0u16; 6];
    a1[5] = (stream_rz - (base0 + 12)) as u16; // rotZ do arm
    for v in a0.into_iter().chain(a1) {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&[2, 2]);
    put_i16s(&mut b, &[0, 2]);
    b.extend_from_slice(&[1, 2]);
    put_i16s(&mut b, &[4]);
    b.extend_from_slice(&[4, 4]);
    put_i16s(&mut b, &[0, 50, 100, 157]);

    // header
    let mut h = Vec::new();
    h.extend_from_slice(IDENT);
    put_i32(&mut h, VERSION);
    put_str(&mut h, "anim", 64);
    put_i32(&mut h, b.len() as i32);
    h.extend_from_slice(&[0u8; 60]); // eyeposition, min, max, bbmin, bbmax
    put_i32(&mut h, 0); // flags
    for v in [2, bone_index, 0, 0, 0, 0, 2, seq_index, 0, 0, 2, tex_index, 0, 2, 2, skin_index, 1, bodypart_index] {
        put_i32(&mut h, v);
    }
    assert_eq!(h.len(), 212);
    b[..212].copy_from_slice(&h);

    Syn { data: b, parse_end, seq0: seq_index as usize }
}

/// Arquivo de grupo externo (`IDSQ`) da sequência `ext`: X do root 0 -> 6 em 2 quadros.
fn ext_file() -> Vec<u8> {
    let mut e = Vec::new();
    e.extend_from_slice(SEQ_IDENT);
    put_i32(&mut e, VERSION);
    put_str(&mut e, "ext", 64);
    put_i32(&mut e, 0);
    assert_eq!(e.len(), 76);
    let mut a0 = [0u16; 6];
    a0[0] = 24;
    for v in a0.into_iter().chain([0u16; 6]) {
        e.extend_from_slice(&v.to_le_bytes());
    }
    e.extend_from_slice(&[2, 2]);
    put_i16s(&mut e, &[0, 6]);
    e
}

fn no_ext(_: u32) -> Option<Vec<u8>> {
    None
}

/// Skinning de referência (o mesmo que o frontend faz): `pose[bone] * local`.
fn skin(frames: &SeqFrames, model: &MdlModel, frame: usize) -> Vec<f32> {
    let nb = frames.bones as usize;
    let mut out = Vec::new();
    for (i, &bone) in model.vert_bones.iter().enumerate() {
        let p = &frames.data[(frame * nb + bone as usize) * POSE_STRIDE..][..POSE_STRIDE];
        let local = [model.local_positions[i * 3], model.local_positions[i * 3 + 1], model.local_positions[i * 3 + 2]];
        let w = add(quat_rotate(&[p[3], p[4], p[5], p[6]], local), [p[0], p[1], p[2]]);
        out.extend_from_slice(&w);
    }
    out
}

fn assert_close(got: &[f32], want: &[f32]) {
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() < 1e-4, "got {got:?}, want {want:?}");
    }
}

#[test]
fn modelo_de_dois_bones_decodifica_pose_de_repouso_skins_e_metadados() {
    let syn = synthetic_anim_mdl();
    let model = parse(&syn.data).expect("modelo sintético válido");

    assert_eq!(model.num_bones, 2);
    assert_eq!(model.textures.len(), 2);
    // root em (10,0,0); arm em (10,5,0) no mundo; v1 local (1,0,0), v2 local (0,1,0).
    assert_close(&model.positions, &[10.0, 0.0, 0.0, 11.0, 5.0, 0.0, 10.0, 6.0, 0.0]);
    assert_close(&model.local_positions, &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    assert_eq!(model.vert_bones, vec![0, 1, 1]);

    assert_eq!(model.sequences, vec!["idle".to_string(), "ext".to_string()]);
    assert_eq!(
        model.seq_info[0],
        SeqInfo { name: "idle".into(), fps: 10.0, frames: 4, looping: true, blends: 1, group: 0 }
    );
    assert_eq!((model.seq_info[1].looping, model.seq_info[1].group), (false, 1));

    // O mesh usa o slot 1 de skinref: família 0 liga o slot 1 à textura 1.
    assert_eq!(model.texindex, vec![1]);
    // Família 1 troca as duas texturas de lugar (0 <-> 1).
    assert_eq!(model.skin_families, vec![vec![0, 1], vec![1, 0]]);
}

#[test]
fn quadro_zero_da_animacao_coincide_com_a_pose_de_repouso() {
    let syn = synthetic_anim_mdl();
    let model = parse(&syn.data).unwrap();
    let frames = sequence_frames(&syn.data, 0, &no_ext).expect("sequência válida");
    assert_eq!((frames.frames, frames.bones, frames.looping), (4, 2, true));
    assert_eq!(frames.data.len(), 4 * 2 * POSE_STRIDE);

    // bone 0: posição (10,0,0), rotação identidade; bone 1: (10,5,0), identidade.
    assert_close(&frames.data[..14], &[10.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 10.0, 5.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    assert_close(&skin(&frames, &model, 0), &model.positions);
}

#[test]
fn quadros_em_rle_trocam_posicao_e_rotacao_por_bone() {
    let syn = synthetic_anim_mdl();
    let model = parse(&syn.data).unwrap();
    let frames = sequence_frames(&syn.data, 0, &no_ext).unwrap();
    let pose = |f: usize, bone: usize| &frames.data[(f * 2 + bone) * POSE_STRIDE..][..POSE_STRIDE];

    // X do root: quadros 0,1 no 1º trecho (0,2); 2,3 no 2º (4 mantido).
    for (f, x) in [(0, 10.0), (1, 12.0), (2, 14.0), (3, 14.0)] {
        assert!((pose(f, 0)[0] - x).abs() < 1e-5, "quadro {f}: x = {}", pose(f, 0)[0]);
    }
    // rotZ do arm = 0, 0.5, 1.0, 1.57 rad -> quaternion (0,0,sin(a/2),cos(a/2)).
    for (f, a) in [(0, 0.0f32), (1, 0.5), (2, 1.0), (3, 1.57)] {
        let q = &pose(f, 1)[3..];
        assert_close(q, &[0.0, 0.0, (a / 2.0).sin(), (a / 2.0).cos()]);
    }

    // Quadro 3: root em X=14; arm em (14,5,0) girado ~90° em Z.
    // v1 local (1,0,0) -> (cos,sin,0) + (14,5,0); v2 local (0,1,0) -> (-sin,cos,0) + (14,5,0).
    let (s, c) = 1.57f32.sin_cos();
    assert_close(&skin(&frames, &model, 3), &[14.0, 0.0, 0.0, 14.0 + c, 5.0 + s, 0.0, 14.0 - s, 5.0 + c, 0.0]);
    // Quadro 1 (intermediário): a rotação gira só o arm, o root só translada.
    let (s, c) = 0.5f32.sin_cos();
    assert_close(&skin(&frames, &model, 1), &[12.0, 0.0, 0.0, 12.0 + c, 5.0 + s, 0.0, 12.0 - s, 5.0 + c, 0.0]);
}

#[test]
fn motiontype_zera_a_translacao_do_bone_de_movimento() {
    let mut syn = synthetic_anim_mdl();
    syn.data[syn.seq0 + 68..syn.seq0 + 72].copy_from_slice(&STUDIO_X.to_le_bytes()); // motiontype
    syn.data[syn.seq0 + 72..syn.seq0 + 76].copy_from_slice(&0i32.to_le_bytes()); // motionbone = root
    let frames = sequence_frames(&syn.data, 0, &no_ext).unwrap();
    // X do root zerado em todos os quadros (e o arm, filho dele, acompanha).
    for f in 0..4 {
        assert_eq!(frames.data[f * 2 * POSE_STRIDE], 0.0, "quadro {f}");
        assert_eq!(frames.data[(f * 2 + 1) * POSE_STRIDE], 0.0, "quadro {f}");
    }
}

#[test]
fn sequencia_em_grupo_externo_precisa_do_arquivo() {
    let syn = synthetic_anim_mdl();
    assert!(sequence_frames(&syn.data, 1, &no_ext).is_err());

    let ext = ext_file();
    let frames = sequence_frames(&syn.data, 1, &|g| (g == 1).then(|| ext.clone())).expect("grupo externo válido");
    assert_eq!(frames.frames, 2);
    // quadro 1, bone 0: X = 10 + 6 * escala 1.0
    assert!((frames.data[2 * POSE_STRIDE] - 16.0).abs() < 1e-5);

    // arquivo que não é IDSQ é recusado
    let mut bad = ext;
    bad[0..4].copy_from_slice(b"IDST");
    assert!(sequence_frames(&syn.data, 1, &|_| Some(bad.clone())).is_err());
}

#[test]
fn catalog_carrega_modelo_e_sequencia_com_grupo_externo_do_disco() {
    let dir = std::env::temp_dir().join("bsp-museum-tests-mdl-anim");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("rig.mdl");
    std::fs::write(&path, synthetic_anim_mdl().data).unwrap();

    let summary = crate::catalog::load_model(&path).expect("modelo válido");
    assert_eq!((summary.num_bones, summary.sequence_info.len(), summary.skin_families.len()), (2, 2, 2));
    assert_eq!(summary.vert_bones, vec![0, 1, 1]);

    let idle = crate::catalog::load_sequence(&path, 0).expect("idle");
    assert_eq!((idle.frames, idle.bones), (4, 2));
    // `ext` mora em rig01.mdl: sem o arquivo dá erro, com ele anima
    assert!(crate::catalog::load_sequence(&path, 1).is_err());
    std::fs::write(dir.join("rig01.mdl"), ext_file()).unwrap();
    let ext = crate::catalog::load_sequence(&path, 1).expect("ext");
    assert_eq!(ext.frames, 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sequencia_inexistente_e_erro() {
    let syn = synthetic_anim_mdl();
    assert!(sequence_frames(&syn.data, 2, &no_ext).is_err());
}

/// Matriz Rz(yaw) · Ry(pitch) · Rx(roll), escrita à mão e independente do
/// quaternion — a ordem GoldSrc (`AngleQuaternion`/`QuaternionMatrix`).
fn euler_matrix(roll: f32, pitch: f32, yaw: f32) -> [[f32; 3]; 3] {
    let (sx, cx) = roll.sin_cos();
    let (sy, cy) = pitch.sin_cos();
    let (sz, cz) = yaw.sin_cos();
    [
        [cy * cz, sx * sy * cz - cx * sz, cx * sy * cz + sx * sz],
        [cy * sz, sx * sy * sz + cx * cz, cx * sy * sz - sx * cz],
        [-sy, sx * cy, cx * cy],
    ]
}

#[test]
fn quaternion_do_sdk_bate_com_a_matriz_rz_ry_rx() {
    for angles in [[0.3f32, -0.7, 1.9], [1.2, 0.4, -2.5], [0.0, 0.0, std::f32::consts::FRAC_PI_2], [-0.9, 1.4, 0.2]] {
        let q = angle_quaternion(angles);
        let m = euler_matrix(angles[0], angles[1], angles[2]);
        for v in [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [2.0, -3.0, 5.0]] {
            assert_close(&quat_rotate(&q, v), &mat_vec(&m, v));
        }
    }
    // yaw de 90° leva +X em +Y (anti-horário visto de cima, Z pra cima)
    let q = angle_quaternion([0.0, 0.0, std::f32::consts::FRAC_PI_2]);
    assert_close(&quat_rotate(&q, [1.0, 0.0, 0.0]), &[0.0, 1.0, 0.0]);
}

#[test]
fn composicao_da_hierarquia_e_pai_vezes_filho() {
    let parent = Transform::from_euler([1.0, 2.0, 3.0], [0.4, -0.2, 0.9]);
    let local = Transform::from_euler([0.5, -1.0, 2.0], [-0.3, 0.8, 0.1]);
    let world = Transform::compose(&parent, &local);
    for v in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [-2.0, 4.0, 0.5]] {
        assert_close(&world.apply(v), &parent.apply(local.apply(v)));
    }
}

#[test]
fn rle_com_total_zero_e_erro_e_nao_trava() {
    let buf = [0u8, 0, 0, 0];
    assert!(rle_value(&buf, 0, 0).is_err());
    assert!(rle_value(&buf, 0, 5).is_err());
    // offset fora do buffer
    assert!(rle_value(&buf, 100, 0).is_err());
}

#[test]
fn truncar_em_qualquer_offset_da_erro_sem_panic() {
    let syn = synthetic_anim_mdl();
    let ext = ext_file();
    for len in 0..syn.data.len() {
        let cut = &syn.data[..len];
        let parsed = parse(cut);
        if len < syn.parse_end {
            assert!(parsed.is_err(), "parse deveria falhar com {len} bytes");
        }
        // as animações vivem no fim do arquivo: qualquer corte as quebra
        assert!(sequence_frames(cut, 0, &no_ext).is_err(), "sequence_frames deveria falhar com {len} bytes");
        let _ = sequence_frames(cut, 1, &|_| Some(ext.clone()));
    }
    for len in 0..ext.len() {
        assert!(sequence_frames(&syn.data, 1, &|_| Some(ext[..len].to_vec())).is_err(), "grupo externo com {len} bytes");
    }
}

/// Gerador pseudoaleatório determinístico (LCG) — sem dependência externa.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
}

fn exercita(data: &[u8], ext: &[u8]) {
    let ext = ext.to_vec();
    if let Ok(model) = parse(data) {
        for i in 0..model.sequences.len().min(4) {
            let _ = sequence_frames(data, i, &|_| Some(ext.clone()));
        }
    }
    let _ = sequence_frames(data, 0, &no_ext);
}

#[test]
fn campos_com_valores_extremos_nao_causam_panic() {
    let syn = synthetic_anim_mdl();
    let ext = ext_file();
    // Cada palavra de 4 bytes do arquivo (header, bones, tabelas, sequências...)
    // vira um valor absurdo, um de cada vez.
    for at in (0..syn.data.len() - 3).step_by(4) {
        for v in [0i32, -1, 1, 255, 1 << 16, 1 << 30, i32::MAX, i32::MIN] {
            let mut data = syn.data.clone();
            data[at..at + 4].copy_from_slice(&v.to_le_bytes());
            exercita(&data, &ext);
        }
    }
}

#[test]
fn bytes_corrompidos_e_lixo_aleatorio_nao_causam_panic() {
    let syn = synthetic_anim_mdl();
    let ext = ext_file();
    let mut rng = Lcg(0xC0FFEE);
    for _ in 0..3000 {
        let mut data = syn.data.clone();
        for _ in 0..1 + rng.next() % 6 {
            let at = rng.next() as usize % data.len();
            data[at] = rng.next() as u8;
        }
        exercita(&data, &ext);
    }
    // lixo puro e lixo com cabeçalho válido
    for round in 0..300 {
        let len = (rng.next() % 3000) as usize;
        let mut junk: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        if round % 2 == 0 && junk.len() > 8 {
            junk[0..4].copy_from_slice(IDENT);
            junk[4..8].copy_from_slice(&VERSION.to_le_bytes());
        }
        exercita(&junk, &ext);
    }
}
