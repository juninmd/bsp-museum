use std::fmt;

/// Erro de leitura de BSP. Arquivo de mapa é binário de terceiro:
/// todo acesso é checado e vira erro, nunca panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BspError {
    TooSmall { need: usize, have: usize },
    BadVersion(i32),
    LumpOutOfBounds { lump: &'static str, offset: usize, len: usize, file: usize },
    LumpMisaligned { lump: &'static str, len: usize, stride: usize },
    BadIndex { what: &'static str, index: usize, len: usize },
    Empty(&'static str),
}

impl fmt::Display for BspError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BspError::TooSmall { need, have } => {
                write!(f, "arquivo pequeno demais: precisa de {need} bytes, tem {have}")
            }
            BspError::BadVersion(v) => write!(
                f,
                "versão {v} não é BSP do GoldSrc (esperado 30 — Half-Life/CS 1.6)"
            ),
            BspError::LumpOutOfBounds { lump, offset, len, file } => write!(
                f,
                "lump {lump} aponta para {offset}..{} mas o arquivo tem {file} bytes",
                offset + len
            ),
            BspError::LumpMisaligned { lump, len, stride } => write!(
                f,
                "lump {lump} tem {len} bytes, que não é múltiplo de {stride}"
            ),
            BspError::BadIndex { what, index, len } => {
                write!(f, "índice de {what} fora da faixa: {index} de {len}")
            }
            BspError::Empty(what) => write!(f, "{what} vazio"),
        }
    }
}

impl std::error::Error for BspError {}

pub type Result<T> = std::result::Result<T, BspError>;

/// Leitor de little-endian sobre uma fatia, com limites checados.
pub struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(BspError::TooSmall {
            need: usize::MAX,
            have: self.data.len(),
        })?;
        let slice = self.data.get(self.pos..end).ok_or(BspError::TooSmall {
            need: end,
            have: self.data.len(),
        })?;
        self.pos = end;
        Ok(slice)
    }

    pub fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn i32(&mut self) -> Result<i32> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(self.i32()? as u32)
    }

    pub fn f32(&mut self) -> Result<f32> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn vec3(&mut self) -> Result<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }

    /// String de tamanho fixo terminada em NUL (nome de textura tem 16 bytes).
    pub fn fixed_str(&mut self, n: usize) -> Result<String> {
        let raw = self.take(n)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        Ok(String::from_utf8_lossy(&raw[..end]).trim().to_string())
    }
}
