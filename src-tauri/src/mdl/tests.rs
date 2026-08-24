//! Testes do parser de `.mdl`, mesmo espírito dos testes de BSP: o arquivo é
//! montado byte a byte, sem depender de nenhum `.mdl` real no repositório.

use super::*;

fn push_str_fixed(out: &mut Vec<u8>, s: &str, n: usize) {
    let mut buf = vec![0u8; n];
    let bytes = s.as_bytes();
    buf[..bytes.len().min(n)].copy_from_slice(&bytes[..bytes.len().min(n)]);
    out.extend_from_slice(&buf);
}

/// Modelo sintético: 1 bone raiz (só translação em X), 1 skin 1×1, 1 bodypart
/// com 1 submodel de 1 triângulo, 1 sequência. Layout (offsets absolutos):
/// header 0..212, bone 212..324, textura (entry) 324..404, pixel+paleta
/// 404..1173, bodypart 1173..1249, submodel 1249..1361, vertinfo 1361..1364,
/// vértices 1364..1400, mesh 1400..1420, stream de triângulo 1420..1448,
/// sequência 1448..1624.
fn synthetic_mdl() -> Vec<u8> {
    let mut out = Vec::new();

    // ---- header ----
    out.extend_from_slice(IDENT);
    out.extend_from_slice(&VERSION.to_le_bytes());
    push_str_fixed(&mut out, "test", 64); // name
    out.extend_from_slice(&0i32.to_le_bytes()); // length
    for _ in 0..5 {
        out.extend_from_slice(&[0.0f32; 3].map(f32::to_le_bytes).concat()); // eyeposition/min/max/bbmin/bbmax
    }
    out.extend_from_slice(&0i32.to_le_bytes()); // flags
    out.extend_from_slice(&1i32.to_le_bytes()); // numbones
    out.extend_from_slice(&212i32.to_le_bytes()); // boneindex
    out.extend_from_slice(&0i32.to_le_bytes()); // numbonecontrollers
    out.extend_from_slice(&0i32.to_le_bytes()); // bonecontrollerindex
    out.extend_from_slice(&0i32.to_le_bytes()); // numhitboxes
    out.extend_from_slice(&0i32.to_le_bytes()); // hitboxindex
    out.extend_from_slice(&1i32.to_le_bytes()); // numseq
    out.extend_from_slice(&1448i32.to_le_bytes()); // seqindex
    out.extend_from_slice(&0i32.to_le_bytes()); // numseqgroups
    out.extend_from_slice(&0i32.to_le_bytes()); // seqgroupindex
    out.extend_from_slice(&1i32.to_le_bytes()); // numtextures
    out.extend_from_slice(&324i32.to_le_bytes()); // textureindex
    out.extend_from_slice(&0i32.to_le_bytes()); // texturedataindex (não usado)
    out.extend_from_slice(&0i32.to_le_bytes()); // numskinref
    out.extend_from_slice(&0i32.to_le_bytes()); // numskinfamilies
    out.extend_from_slice(&0i32.to_le_bytes()); // skinindex
    out.extend_from_slice(&1i32.to_le_bytes()); // numbodyparts
    out.extend_from_slice(&1173i32.to_le_bytes()); // bodypartindex
    assert_eq!(out.len(), 212, "header do teste desalinhou");

    // ---- bone (só translação em X=10, sem rotação) ----
    push_str_fixed(&mut out, "root", 32);
    out.extend_from_slice(&(-1i32).to_le_bytes()); // parent
    out.extend_from_slice(&0i32.to_le_bytes()); // flags
    out.extend_from_slice(&[0u8; 24]); // bonecontroller[6]
    let value = [10.0f32, 0.0, 0.0, 0.0, 0.0, 0.0];
    for v in value {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&[0u8; 24]); // scale[6]
    assert_eq!(out.len(), 324, "bone do teste desalinhou");

    // ---- textura (entry de 80 bytes; pixels ficam em outro offset) ----
    push_str_fixed(&mut out, "skin1", 64);
    out.extend_from_slice(&0i32.to_le_bytes()); // flags (sem STUDIO_NF_MASKED)
    out.extend_from_slice(&1u32.to_le_bytes()); // width
    out.extend_from_slice(&1u32.to_le_bytes()); // height
    out.extend_from_slice(&404i32.to_le_bytes()); // offset do pixel
    assert_eq!(out.len(), 404, "entrada de textura do teste desalinhou");

    // pixel (índice 5) + paleta de 256 cores (cor 5 = vermelho puro)
    out.push(5);
    let mut palette = vec![0u8; 768];
    palette[15..18].copy_from_slice(&[255, 0, 0]); // índice 5 = offset 15
    out.extend_from_slice(&palette);
    assert_eq!(out.len(), 1173, "bloco de pixels do teste desalinhou");

    // ---- bodypart ----
    push_str_fixed(&mut out, "body", 64);
    out.extend_from_slice(&1i32.to_le_bytes()); // nummodels
    out.extend_from_slice(&0i32.to_le_bytes()); // base
    out.extend_from_slice(&1249i32.to_le_bytes()); // modelindex
    assert_eq!(out.len(), 1249, "bodypart do teste desalinhou");

    // ---- submodel ----
    push_str_fixed(&mut out, "sub", 64);
    out.extend_from_slice(&0i32.to_le_bytes()); // type
    out.extend_from_slice(&0f32.to_le_bytes()); // boundingradius
    out.extend_from_slice(&1i32.to_le_bytes()); // nummesh
    out.extend_from_slice(&1400i32.to_le_bytes()); // meshindex
    out.extend_from_slice(&3i32.to_le_bytes()); // numverts
    out.extend_from_slice(&1361i32.to_le_bytes()); // vertinfoindex
    out.extend_from_slice(&1364i32.to_le_bytes()); // vertindex
    out.extend_from_slice(&[0u8; 112 - 64 - 4 - 4 - 4 - 4 - 4 - 4 - 4]); // resto do struct (numnorms..groupindex)
    assert_eq!(out.len(), 1361, "submodel do teste desalinhou");

    // ---- vertinfo (bone por vértice) ----
    out.extend_from_slice(&[0u8, 0u8, 0u8]);
    assert_eq!(out.len(), 1364, "vertinfo do teste desalinhou");

    // ---- vértices locais: (0,0,0) (1,0,0) (0,1,0) ----
    for v in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for c in v {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    assert_eq!(out.len(), 1400, "vértices do teste desalinharam");

    // ---- mesh ----
    out.extend_from_slice(&1i32.to_le_bytes()); // numtris
    out.extend_from_slice(&1420i32.to_le_bytes()); // triindex
    out.extend_from_slice(&0i32.to_le_bytes()); // skinref
    out.extend_from_slice(&0i32.to_le_bytes()); // numnorms
    out.extend_from_slice(&0i32.to_le_bytes()); // normindex
    assert_eq!(out.len(), 1420, "mesh do teste desalinhou");

    // ---- stream de triângulo: fan de 3 vértices (0,1,2), depois 0 termina ----
    out.extend_from_slice(&3i16.to_le_bytes());
    for vi in [0i16, 1, 2] {
        out.extend_from_slice(&vi.to_le_bytes()); // vertindex
        out.extend_from_slice(&0i16.to_le_bytes()); // normindex
        out.extend_from_slice(&0i16.to_le_bytes()); // s
        out.extend_from_slice(&0i16.to_le_bytes()); // t
    }
    out.extend_from_slice(&0i16.to_le_bytes()); // terminador
    assert_eq!(out.len(), 1448, "stream de triângulo do teste desalinhou");

    // ---- sequência (só o nome importa aqui) ----
    push_str_fixed(&mut out, "idle", 32);
    out.extend_from_slice(&[0u8; SEQDESC_SIZE - 32]);
    assert_eq!(out.len(), 1624, "sequência do teste desalinhou");

    out
}

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
