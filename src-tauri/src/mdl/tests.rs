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
