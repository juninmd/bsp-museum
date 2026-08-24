//! Decodificação de pixels indexados (8bpp + paleta de 256 cores) para RGBA.
//!
//! O mesmo miptex do Quake alimenta três formatos diferentes neste projeto: textura
//! embutida no BSP, textura de `.wad` externo e skin de `.mdl`. Os três compartilham
//! esta função — um só lugar decide o alpha por pixel, em vez de três cópias que podem
//! divergir (foi exatamente isso que deixou o bug de transparência escondido).

/// Decodifica pixels indexados numa paleta de 256 cores (768 bytes, RGB) para RGBA.
///
/// `transparent` liga o buraco do índice 255: no BSP/WAD isso só vale pra textura cujo
/// nome começa com `{` (convenção do Quake/GoldSrc — grade, cerca, vidro); em textura
/// comum o índice 255 é só mais uma cor da paleta. No `.mdl` quem decide é a flag
/// `STUDIO_NF_MASKED` da textura, não o nome. Sem isso, o pixel 255 furava em qualquer
/// textura — o bug que faz parede/chão ficarem com buracos que não existem no jogo.
pub fn decode_indexed(pixels: &[u8], palette: &[u8], transparent: bool) -> Option<Vec<u8>> {
    if palette.len() < 768 {
        return None;
    }
    let mut rgba = Vec::with_capacity(pixels.len() * 4);
    for &idx in pixels {
        let p = (idx as usize) * 3;
        let r = *palette.get(p)?;
        let g = *palette.get(p + 1)?;
        let b = *palette.get(p + 2)?;
        let a = if transparent && idx == 255 { 0 } else { 255 };
        rgba.extend_from_slice(&[r, g, b, a]);
    }
    Some(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette_with(idx255: [u8; 3]) -> Vec<u8> {
        let mut p = vec![0u8; 768];
        p[765..768].copy_from_slice(&idx255);
        p
    }

    #[test]
    fn indice_255_fura_so_quando_transparente() {
        let palette = palette_with([200, 10, 10]);
        let opaco = decode_indexed(&[255], &palette, false).unwrap();
        assert_eq!(opaco, vec![200, 10, 10, 255], "sem `transparent`, 255 é só uma cor");

        let furado = decode_indexed(&[255], &palette, true).unwrap();
        assert_eq!(furado, vec![200, 10, 10, 0], "com `transparent`, 255 vira buraco");
    }

    #[test]
    fn paleta_curta_devolve_none() {
        assert!(decode_indexed(&[0], &[0u8; 10], false).is_none());
    }
}
