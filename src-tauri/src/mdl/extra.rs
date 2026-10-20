//! Metadados do `.mdl` além da malha e da pose: bone controllers, bodygroups,
//! flags de material, remap de cor de time, hitboxes, attachments, eventos e o
//! detalhe de cada sequência (blends, movimento).
//!
//! Tudo aqui é **adição compatível**: `MdlExtra` entra achatado (`#[serde(flatten)]`)
//! no `MdlSummary`, com campos que o frontend antigo simplesmente ignora. Os
//! campos opcionais (tabelas) só são serializados quando não estão vazios.
//!
//! Fontes conferidas (código aberto, baixadas neste ciclo):
//! - `studio.h` e `cl_dll/StudioModelRenderer.cpp` do HLSDK (`ValveSoftware/halflife`):
//!   layouts de `mstudiobonecontroller_t`/`mstudiobbox_t`/`mstudioattachment_t`/
//!   `mstudioevent_t`/`mstudiobodyparts_t`, flags `STUDIO_NF_*`, `StudioCalcBoneAdj`,
//!   `StudioCalcRotations`, `StudioSetupBones` (blends);
//! - `dlls/animation.cpp` (`SetController`, `SetBlending`, `Set/GetBodygroup`) e
//!   `dlls/scriptevent.h` (eventos de script 1000..1010);
//! - `cl_dll/entity.cpp` (`HUD_StudioEvent`: 5001/5011/5021/5031/5002/5004);
//! - Xash3D FWGS (GPL): `engine/client/cl_remap.c`, `ref/gl/gl_studio.c`
//!   (`R_StudioLoadTexture`, `R_StudioSetupChrome`), `engine/common/imagelib/
//!   img_utils.c` (`Image_PaletteHueReplace`), `common/com_model.h` (faixas de hue)
//!   e `engine/common/mod_studio.c` (`nomeT.mdl`, `nomeNN.mdl`).

use serde::Serialize;

// ---------------------------------------------------------------------------
// Flags (studio.h)

/// `STUDIO_NF_*`: opções de material da textura.
pub const NF_FLATSHADE: i32 = 0x0001;
pub const NF_CHROME: i32 = 0x0002;
pub const NF_FULLBRIGHT: i32 = 0x0004;
/// SDK: `STUDIO_NF_NOMIPS`; Xash3D usa o mesmo valor como "ignorar mipmaps".
pub const NF_NOMIPS: i32 = 0x0008;
/// SDK: `STUDIO_NF_ALPHA` (0x10). O Xash3D reaproveita o bit como `SMOOTH`.
/// Não afeta o desenho no motor aberto; só é repassado.
pub const NF_ALPHA: i32 = 0x0010;
pub const NF_ADDITIVE: i32 = 0x0020;
pub const NF_MASKED: i32 = 0x0040;

/// Tipos de controlador/blend (`STUDIO_X`..`STUDIO_AZR`) e o bit de wrap.
pub const T_X: i32 = 0x0001;
pub const T_Y: i32 = 0x0002;
pub const T_Z: i32 = 0x0004;
pub const T_XR: i32 = 0x0008;
pub const T_YR: i32 = 0x0010;
pub const T_ZR: i32 = 0x0020;
pub const T_TYPES: i32 = 0x7FFF;
/// `STUDIO_RLOOP`: controlador que dá a volta de 360° pelo caminho mais curto.
pub const T_RLOOP: i32 = 0x8000;

/// Faixas de hue do remap padrão (`common/com_model.h` do Xash3D): textura
/// `DM_Base*` troca 160..191 pela cor de cima e 192..223 pela de baixo.
pub const PLATE_HUE_START: u8 = 160;
pub const PLATE_HUE_END: u8 = 191;
pub const SUIT_HUE_END: u8 = 223;

// ---------------------------------------------------------------------------
// Tipos públicos (serde)

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BoneInfo {
    pub name: String,
    /// -1 = raiz
    pub parent: i32,
}

/// `mstudiobonecontroller_t` + o que a UI precisa para montar um slider.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ControllerInfo {
    /// slot de entrada: 0..3 = controladores do usuário (olhar/mira), 4 = boca
    pub index: i32,
    /// `"controller0".."controller3"` ou `"mouth"` (qualquer `index` > 3 é a boca, como no SDK)
    pub name: String,
    pub bone: i32,
    /// `"x" | "y" | "z" | "xr" | "yr" | "zr" | "outro"` (eixo que o controlador move)
    pub kind: String,
    /// `STUDIO_RLOOP`: o valor dá a volta (0..255 -> 0..360°) em vez de ir de `start` a `end`
    pub wrap: bool,
    pub start: f32,
    pub end: f32,
    /// byte (0..255) do repouso: valor inicial sugerido para o slider
    pub rest: i32,
    /// maior byte que ainda muda alguma coisa: 255, ou 64 na boca (`mouthopen / 64`, saturado)
    pub input_max: u8,
    /// `"deg"` (eixos de rotação) ou `"units"` (translação)
    pub unit: String,
    /// campo `type` cru (inclui o bit de wrap)
    pub raw_type: i32,
}

impl ControllerInfo {
    pub fn is_mouth(&self) -> bool {
        self.index > 3
    }

    /// Valor (graus ou unidades) que o byte `input` produz — `StudioCalcBoneAdj`
    /// do SDK, com o byte constante entre quadros (`dadt` irrelevante).
    pub fn value_for(&self, input: u8) -> f32 {
        let c = f32::from(input);
        if self.is_mouth() {
            let t = (c / 64.0).min(1.0);
            return (1.0 - t) * self.start + t * self.end;
        }
        if self.wrap {
            return c * (360.0 / 256.0) + self.start;
        }
        let t = (c / 255.0).clamp(0.0, 1.0);
        (1.0 - t) * self.start + t * self.end
    }

    /// Inverso de `value_for` (`SetController` do `dlls/animation.cpp`, sem o
    /// tratamento de envolvimento do valor de entrada): byte a enviar para
    /// obter `value` graus/unidades.
    pub fn byte_for(&self, value: f32) -> u8 {
        if self.wrap && !self.is_mouth() {
            let b = ((value - self.start) * (256.0 / 360.0)).round();
            return b.rem_euclid(256.0) as u8;
        }
        let span = if self.is_mouth() { 64.0 } else { 255.0 };
        let d = self.end - self.start;
        if d.abs() < f32::EPSILON {
            return 0;
        }
        (span * (value - self.start) / d).clamp(0.0, f32::from(self.input_max)) as u8
    }

    /// Ajuste que entra no canal do bone: radianos nos eixos de rotação,
    /// unidades na translação; outros tipos (`LX`, `AX`...) o SDK deixa sem valor.
    pub(super) fn adjust(&self, input: u8) -> f32 {
        let v = self.value_for(input);
        match self.raw_type & T_TYPES {
            T_XR | T_YR | T_ZR => v.to_radians(),
            T_X | T_Y | T_Z => v,
            _ => 0.0,
        }
    }
}

pub(super) fn type_kind(t: i32) -> &'static str {
    match t & T_TYPES {
        T_X => "x",
        T_Y => "y",
        T_Z => "z",
        T_XR => "xr",
        T_YR => "yr",
        T_ZR => "zr",
        _ => "outro",
    }
}

/// `mstudiobodyparts_t` + nomes dos submodels.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BodyPartInfo {
    pub name: String,
    /// multiplicador do valor de `body` neste bodypart
    pub base: i32,
    /// nome de cada submodel (`mstudiomodel_t.name`); a quantidade é `models.len()`
    pub models: Vec<String>,
}

impl BodyPartInfo {
    /// Submodel escolhido para o valor `body`: `(body / base) % nummodels`
    /// (`StudioSetupModel` do SDK / `GetBodygroup`). `base <= 0` vale 1.
    pub fn choice(&self, body: u32) -> usize {
        if self.models.is_empty() {
            return 0;
        }
        let base = u32::try_from(self.base).unwrap_or(1).max(1);
        ((body / base) as usize) % self.models.len()
    }
}

/// Valor de `body` que escolhe `choices[i]` em cada bodypart (`SetBodygroup`
/// aplicado a todos): soma de `escolha * base`. Escolha fora da faixa é
/// limitada ao último submodel; bodypart sem escolha fica no submodel 0.
pub fn body_value(parts: &[BodyPartInfo], choices: &[u32]) -> u32 {
    let mut body = 0u32;
    for (i, p) in parts.iter().enumerate() {
        if p.models.is_empty() {
            continue;
        }
        let c = choices.get(i).copied().unwrap_or(0).min(p.models.len() as u32 - 1);
        let base = u32::try_from(p.base).unwrap_or(1).max(1);
        body = body.saturating_add(c.saturating_mul(base));
    }
    body
}

/// Faixas de paleta do remap de cor de time (`Remap*`/`DM_Base*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RemapRange {
    pub top_start: u8,
    pub top_end: u8,
    /// a faixa de baixo começa em `top_end + 1`
    pub bottom_end: u8,
}

/// Material da textura (`mstudiotexture_t.flags`) já mastigado para o viewer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TextureInfo {
    pub name: String,
    pub flags: i32,
    pub flatshade: bool,
    /// UV = reflexão: `s = (n·right + 1) * 32 / width`, `t = (n·up + 1) * 32 / height`
    /// (`R_StudioSetupChrome`; `right`/`up` = base da câmera olhando o bone)
    pub chrome: bool,
    pub fullbright: bool,
    pub nomips: bool,
    pub alpha: bool,
    pub additive: bool,
    pub masked: bool,
    /// `"opaque" | "masked" | "additive"` (aditivo vence máscara)
    pub blend: String,
    /// textura de remap de cor de time (`DM_Base*` ou `Remap*`), com as faixas da paleta
    pub remap: Option<RemapRange>,
}

impl TextureInfo {
    pub(super) fn new(name: &str, flags: i32) -> Self {
        let masked = flags & NF_MASKED != 0;
        let additive = flags & NF_ADDITIVE != 0;
        Self {
            name: name.to_string(),
            flags,
            flatshade: flags & NF_FLATSHADE != 0,
            chrome: flags & NF_CHROME != 0,
            fullbright: flags & NF_FULLBRIGHT != 0,
            nomips: flags & NF_NOMIPS != 0,
            alpha: flags & NF_ALPHA != 0,
            additive,
            masked,
            blend: if additive { "additive" } else if masked { "masked" } else { "opaque" }.to_string(),
            remap: remap_range(name),
        }
    }
}

/// Caixa orientada em espaço do modelo: `rot` é o quaternion xyzw do bone,
/// `center`/`half` o centro e as meias-arestas da caixa dele.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct OrientedBox {
    pub center: [f32; 3],
    pub half: [f32; 3],
    pub rot: [f32; 4],
}

/// `mstudiobbox_t`; `rest` já está em espaço do modelo na pose de repouso.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HitboxInfo {
    pub bone: i32,
    /// grupo de acerto (1 = cabeça, 2 = peito, 3 = estômago, 4/5 = braços, 6/7 = pernas — convenção do jogo, não do formato)
    pub group: i32,
    pub bbmin: [f32; 3],
    pub bbmax: [f32; 3],
    pub rest: OrientedBox,
}

/// `mstudioattachment_t`; `rest` é `org` em espaço do modelo na pose de repouso.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AttachmentInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: i32,
    pub bone: i32,
    pub org: [f32; 3],
    pub vectors: [[f32; 3]; 3],
    pub rest: [f32; 3],
}

/// `mstudioevent_t` com o ID traduzido quando é um dos conhecidos.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SeqEvent {
    pub frame: i32,
    pub event: i32,
    #[serde(rename = "type")]
    pub kind: i32,
    pub options: String,
    /// nome do evento conhecido (ver `event_info`)
    pub name: Option<String>,
    /// `"client"` (>= 5000, tratado pelo cliente) ou `"server"` (< 5000, tratado pelo código do monstro/arma)
    pub side: String,
    /// attachment (0..3) onde o evento acontece, quando o SDK define um
    pub attachment: Option<u8>,
}

/// Evento conhecido: nome, attachment, origem da informação.
pub struct EventInfo {
    pub name: &'static str,
    pub attachment: Option<u8>,
    /// onde isto foi conferido
    pub source: &'static str,
}

const SRC_ENTITY: &str = "HLSDK cl_dll/entity.cpp (HUD_StudioEvent)";
const SRC_SCRIPT: &str = "HLSDK dlls/scriptevent.h";

/// Eventos conhecidos. Conferidos no código aberto: 5001/5011/5021/5031 (flash
/// de boca no attachment 0/1/2/3, `options` = tipo do flash), 5002 (faísca no
/// attachment 0), 5004 (som `options` no attachment 0) e os eventos de script
/// 1000..1010. IDs de mod (CS: ejeção de cápsula etc.) e eventos soltos como o
/// "3" não constam no SDK e ficam sem nome — o `event` cru sempre é repassado.
pub fn event_info(id: i32) -> Option<EventInfo> {
    let (name, attachment, source) = match id {
        5001 => ("muzzleflash", Some(0), SRC_ENTITY),
        5011 => ("muzzleflash", Some(1), SRC_ENTITY),
        5021 => ("muzzleflash", Some(2), SRC_ENTITY),
        5031 => ("muzzleflash", Some(3), SRC_ENTITY),
        5002 => ("spark", Some(0), SRC_ENTITY),
        5004 => ("sound", Some(0), SRC_ENTITY),
        1000 => ("SCRIPT_EVENT_DEAD", None, SRC_SCRIPT),
        1001 => ("SCRIPT_EVENT_NOINTERRUPT", None, SRC_SCRIPT),
        1002 => ("SCRIPT_EVENT_CANINTERRUPT", None, SRC_SCRIPT),
        1003 => ("SCRIPT_EVENT_FIREEVENT", None, SRC_SCRIPT),
        1004 => ("SCRIPT_EVENT_SOUND", None, SRC_SCRIPT),
        1005 => ("SCRIPT_EVENT_SENTENCE", None, SRC_SCRIPT),
        1006 => ("SCRIPT_EVENT_INAIR", None, SRC_SCRIPT),
        1007 => ("SCRIPT_EVENT_ENDANIMATION", None, SRC_SCRIPT),
        1008 => ("SCRIPT_EVENT_SOUND_VOICE", None, SRC_SCRIPT),
        1009 => ("SCRIPT_EVENT_SENTENCE_RND1", None, SRC_SCRIPT),
        1010 => ("SCRIPT_EVENT_NOT_DEAD", None, SRC_SCRIPT),
        _ => return None,
    };
    Some(EventInfo { name, attachment, source })
}

impl SeqEvent {
    pub(super) fn new(frame: i32, event: i32, kind: i32, options: String) -> Self {
        let info = event_info(event);
        Self {
            frame,
            event,
            kind,
            options,
            name: info.as_ref().map(|i| i.name.to_string()),
            // `EVENT_CLIENT` = 5000 (Xash3D studio.h): abaixo disso é evento do servidor
            side: if event >= 5000 { "client" } else { "server" }.to_string(),
            attachment: info.and_then(|i| i.attachment),
        }
    }
}

/// Detalhe de uma sequência além do `SeqInfo`: movimento, blends e eventos.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SeqDetail {
    /// `mstudioseqdesc_t.flags` cru (bit 0 = `STUDIO_LOOPING`, já em `SeqInfo.looping`)
    pub flags: i32,
    pub activity: i32,
    pub act_weight: i32,
    /// `STUDIO_X/Y/Z` zeram a translação do `motion_bone` nesses eixos (aplicado na pose,
    /// como `StudioCalcRotations`); `LX/LY/LZ` só informam — o SDK os multiplica por 0
    pub motion_type: i32,
    pub motion_bone: i32,
    pub linear_movement: [f32; 3],
    /// `blendtype[2]` cru (0 = sem eixo)
    pub blend_type: [i32; 2],
    /// eixo de cada blender: `"x" | "y" | "z" | "xr" | "yr" | "zr" | "outro" | ""`
    pub blend_axis: [String; 2],
    /// faixa em graus/unidades que cada blender (byte 0..255) cobre
    pub blend_start: [f32; 2],
    pub blend_end: [f32; 2],
    pub blend_parent: i32,
    /// sequência que entra sozinha ao fim desta (0 = nenhuma)
    pub next_seq: i32,
    pub events: Vec<SeqEvent>,
}

impl SeqDetail {
    /// Byte de blender (0..255) para o valor em graus/unidades `value` do
    /// blender `which` (0 ou 1) — `SetBlending` do `dlls/animation.cpp`.
    pub fn blend_byte(&self, which: usize, value: f32) -> u8 {
        let w = which.min(1);
        let (start, end) = (self.blend_start[w], self.blend_end[w]);
        let mut v = value;
        if self.blend_type[w] == 0 {
            return 0;
        }
        if self.blend_type[w] & (T_XR | T_YR | T_ZR) != 0 {
            if end < start {
                v = -v;
            }
            if start + 359.0 >= end {
                let mid = (start + end) / 2.0;
                if v > mid + 180.0 {
                    v -= 360.0;
                }
                if v < mid - 180.0 {
                    v += 360.0;
                }
            }
        }
        let d = end - start;
        if d.abs() < f32::EPSILON {
            return 0;
        }
        (255.0 * (v - start) / d).clamp(0.0, 255.0) as u8
    }
}

/// O que `load_model` aplicou (eco dos parâmetros, já limitados).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Selection {
    pub body: u32,
    /// família de skin aplicada; com `skin > 0` o `texindex` já vem trocado
    pub skin: u32,
    pub top: Option<u8>,
    pub bottom: Option<u8>,
}

/// Tudo o que o parser sabe além da malha. Achatado no `MdlSummary`.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MdlExtra {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bone_info: Vec<BoneInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub controllers: Vec<ControllerInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bodyparts: Vec<BodyPartInfo>,
    /// uma entrada por textura de `textures` (mesma ordem)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub texture_info: Vec<TextureInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hitboxes: Vec<HitboxInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AttachmentInfo>,
    /// um por sequência (mesma ordem de `sequence_info`)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub seq_details: Vec<SeqDetail>,
    pub selection: Selection,
    /// as texturas vieram de `nomeT.mdl` (o arquivo principal não tem texturas)
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub external_textures: bool,
    /// tabelas opcionais ilegíveis (corrompidas) que foram ignoradas
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Remap de cor de time

/// Lê um campo decimal de 3 caracteres em `name[at..at+3]` (o `Q_atoi` do motor
/// sobre 3 caracteres, aqui sem aceitar lixo): espaços à esquerda e dígitos.
fn three_digits(name: &[u8], at: usize) -> Option<u8> {
    let field = std::str::from_utf8(name.get(at..at + 3)?).ok()?.trim_start();
    if field.is_empty() || !field.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    field.parse::<u32>().ok().map(|v| v.min(255) as u8)
}

/// Faixas de paleta de uma textura de remap. Regra do motor aberto (Xash3D,
/// `R_StudioLoadTexture` em `ref/gl/gl_studio.c`; o `hw.dll` original não foi
/// aberto aqui):
/// - nome começando em `DM_Base` (sem diferenciar maiúsculas): top = 160..191,
///   bottom = 192..223 (`PLATE_HUE_*`/`SUIT_HUE_END`);
/// - nome começando em `Remap`: `Remap?_TTT_UUU_BBB` — os campos de 3 dígitos
///   ficam nas colunas 7, 11 e 15: top = TTT..UUU, bottom = (UUU+1)..BBB.
///   (O motor usa `atoi` e aceitaria lixo como 0; aqui um campo malformado
///   deixa de ser remap.)
pub fn remap_range(name: &str) -> Option<RemapRange> {
    let b = name.as_bytes();
    let starts = |p: &str| b.len() >= p.len() && b[..p.len()].eq_ignore_ascii_case(p.as_bytes());
    if starts("DM_Base") {
        return Some(RemapRange { top_start: PLATE_HUE_START, top_end: PLATE_HUE_END, bottom_end: SUIT_HUE_END });
    }
    if starts("remap") {
        let (top_start, top_end, bottom_end) = (three_digits(b, 7)?, three_digits(b, 11)?, three_digits(b, 15)?);
        return Some(RemapRange { top_start, top_end, bottom_end });
    }
    None
}

/// `Image_PaletteHueReplace` do Xash3D: troca o matiz das entradas `start..=end`
/// da paleta RGB (768 bytes) pelo matiz `color` (0..255 -> 0..360°), mantendo
/// valor (brilho) e saturação de cada cor. Preto puro fica como está.
pub fn hue_replace(palette: &mut [u8], color: u8, start: u8, end: u8) {
    let hue = f32::from(color) * (360.0 / 255.0);
    for i in usize::from(start)..=usize::from(end) {
        let Some(rgb) = palette.get_mut(i * 3..i * 3 + 3) else { return };
        let (r0, g0, b0) = (f32::from(rgb[0]), f32::from(rgb[1]), f32::from(rgb[2]));
        let maxcol = r0.max(g0).max(b0) / 255.0;
        let mincol_in = r0.min(g0).min(b0) / 255.0;
        if maxcol == 0.0 {
            continue;
        }
        let val = maxcol;
        let sat = (maxcol - mincol_in) / maxcol;
        let mincol = val * (1.0 - sat);
        let (r, g, b);
        if hue <= 120.0 {
            b = mincol;
            if hue < 60.0 {
                r = val;
                g = mincol + hue * (val - mincol) / (120.0 - hue);
            } else {
                g = val;
                r = mincol + (120.0 - hue) * (val - mincol) / hue;
            }
        } else if hue <= 240.0 {
            r = mincol;
            if hue < 180.0 {
                g = val;
                b = mincol + (hue - 120.0) * (val - mincol) / (240.0 - hue);
            } else {
                b = val;
                g = mincol + (240.0 - hue) * (val - mincol) / (hue - 120.0);
            }
        } else {
            g = mincol;
            if hue < 300.0 {
                b = val;
                r = mincol + (hue - 240.0) * (val - mincol) / (360.0 - hue);
            } else {
                r = val;
                b = mincol + (360.0 - hue) * (val - mincol) / (hue - 240.0);
            }
        }
        rgb[0] = (r * 255.0) as u8;
        rgb[1] = (g * 255.0) as u8;
        rgb[2] = (b * 255.0) as u8;
    }
}

/// Aplica as cores de time à paleta (`CL_CreateRawTextureFromPixels` do
/// Xash3D): `top` na faixa de cima, `bottom` na de baixo. Cada cor é opcional —
/// `None` deixa a faixa como veio no arquivo.
pub fn remap_palette(palette: &mut [u8], range: RemapRange, top: Option<u8>, bottom: Option<u8>) {
    if let Some(t) = top {
        hue_replace(palette, t, range.top_start, range.top_end);
    }
    if let Some(b) = bottom {
        hue_replace(palette, b, range.top_end.saturating_add(1), range.bottom_end);
    }
}
