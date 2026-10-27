//! Fonte dos ícones de "Configurações" (`assets/settings_light.ico` e
//! `assets/settings_dark.ico`): rasteriza a engrenagem e monta os `.ico`.
//!
//! Núcleo sem E/S, exceto [`write_settings_icons`], que grava num diretório
//! recebido. Compartilhado pelo example `settings_icons` (que regrava os assets)
//! e pelo teste de guarda `tests/settings_icons.rs` (que compara com eles).

use std::f64::consts::PI;
use std::path::{Path, PathBuf};
use std::{error, fmt, io};

use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder};

/// Geometria da engrenagem. Mudar o glifo é mudar [`SETTINGS_GEAR`] e rodar
/// `cargo run -p duplicata-core --example settings_icons`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GearParams {
    /// Número de dentes.
    pub teeth: u32,
    /// Raio da ponta do dente, em fração do lado do ícone.
    pub tip: f64,
    /// Raio da raiz (entre dentes), em fração do lado.
    pub root: f64,
    /// Raio do furo central, em fração do lado.
    pub hole: f64,
    /// Meia-largura do dente, em fração de π/dentes, nos tamanhos até `small_max`.
    pub half_small: f64,
    /// Meia-largura do dente, em fração de π/dentes, acima de `small_max`.
    pub half_large: f64,
    /// Maior tamanho (px) que usa `half_small`.
    pub small_max: u32,
    /// Subamostras por eixo em cada pixel.
    pub supersample: u32,
}

/// A engrenagem versionada em `assets/`.
pub const SETTINGS_GEAR: GearParams = GearParams {
    teeth: 8,
    tip: 0.430,
    root: 0.320,
    hole: 0.145,
    half_small: 0.56,
    half_large: 0.46,
    small_max: 20,
    supersample: 4,
};

/// Tamanhos (px) de cada `.ico`, na ordem das entradas.
pub const SIZES: [u32; 4] = [16, 20, 24, 32];

/// Nome do arquivo e cor RGB do traço de cada tema.
pub const VARIANTS: [(&str, [u8; 3]); 2] = [
    ("settings_light.ico", [0x1F, 0x1F, 0x22]),
    ("settings_dark.ico", [0xF0, 0xF0, 0xF2]),
];

/// Alfa (cobertura da engrenagem, 0..=255) de cada pixel de um ícone `size`×`size`,
/// linha a linha.
///
/// A ordem das operações em ponto flutuante é a da fonte original e não deve
/// mudar: um bit de diferença numa subamostra de borda muda um alfa, e o teste
/// de guarda exige pixels idênticos aos versionados.
pub fn gear_alpha(size: u32, params: &GearParams) -> Vec<u8> {
    let s = f64::from(size);
    let ss = params.supersample;
    let n = f64::from(params.teeth);
    let cx = s / 2.0;
    let cy = s / 2.0;
    let r_tip = params.tip * s;
    let r_root = params.root * s;
    let r_hole = params.hole * s;
    let k = if size <= params.small_max {
        params.half_small
    } else {
        params.half_large
    };
    let half = (PI / n) * k;
    let per = 2.0 * PI / n;
    let samples = f64::from(ss * ss);

    let mut out = Vec::with_capacity((size * size) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut hit = 0u32;
            for sy in 0..ss {
                for sx in 0..ss {
                    let x = (f64::from(px) + (f64::from(sx) + 0.5) / f64::from(ss)) - cx;
                    let y = (f64::from(py) + (f64::from(sy) + 0.5) / f64::from(ss)) - cy;
                    // x e y são múltiplos exatos de 1/ss: x*x + y*y é exato e a
                    // raiz sai corretamente arredondada, como um hypot exato.
                    let d = (x * x + y * y).sqrt();
                    if d < r_hole {
                        continue;
                    }
                    let th = y.atan2(x);
                    // Módulo com piso (o ângulo pode ser negativo), não o `%` truncado.
                    let mut ph = th.rem_euclid(per);
                    if ph > per / 2.0 {
                        ph -= per;
                    }
                    let r = if ph.abs() <= half { r_tip } else { r_root };
                    if d <= r {
                        hit += 1;
                    }
                }
            }
            let coverage = f64::from(hit) / samples;
            // 0 <= coverage <= 1: o arredondamento cabe em u8.
            out.push((coverage * 255.0).round() as u8);
        }
    }
    out
}

/// Bytes de um `.ico` com uma entrada PNG RGBA por tamanho de [`SIZES`], traço na cor `rgb`.
pub fn settings_ico(rgb: [u8; 3], params: &GearParams) -> Vec<u8> {
    let images: Vec<(u32, Vec<u8>)> = SIZES
        .iter()
        .map(|&size| {
            // RGB do tema em todo pixel, inclusive nos de alfa 0.
            let rgba: Vec<u8> = gear_alpha(size, params)
                .into_iter()
                .flat_map(|a| [rgb[0], rgb[1], rgb[2], a])
                .collect();
            (size, encode_png(size, &rgba))
        })
        .collect();
    build_ico(&images)
}

/// PNG RGBA 8 bits de uma imagem `size`×`size` (`rgba`, linha a linha).
///
/// # Panics
///
/// Se `rgba` não tiver `size * size * 4` bytes (erro de quem chama).
pub fn encode_png(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mut png = Vec::new();
    // Sem filtro e compressão máxima: com cor única por imagem, é o menor arquivo
    // (o filtro adaptativo do padrão quase dobra o .ico embutido no app).
    PngEncoder::new_with_quality(&mut png, CompressionType::Best, FilterType::NoFilter)
        .write_image(rgba, size, size, ExtendedColorType::Rgba8)
        .expect("codificar PNG em memória só falha com buffer de tamanho errado");
    png
}

/// Monta um `.ico` com uma entrada PNG por `(tamanho, png)`, na ordem recebida.
///
/// # Panics
///
/// Com mais de `u16::MAX` imagens, tamanho acima de 256 ou PNG acima de 4 GiB.
pub fn build_ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    const HEADER: usize = 6;
    const ENTRY: usize = 16;
    let count = u16::try_from(images.len()).expect("no máximo u16::MAX imagens num .ico");
    let mut header = Vec::with_capacity(HEADER + ENTRY * images.len());
    header.extend_from_slice(&0u16.to_le_bytes());
    header.extend_from_slice(&1u16.to_le_bytes());
    header.extend_from_slice(&count.to_le_bytes());
    let mut blobs = Vec::new();
    let mut offset = HEADER + ENTRY * images.len();
    for (size, png) in images {
        // No .ico, 256 px é gravado como 0.
        let side = if *size == 256 {
            0
        } else {
            u8::try_from(*size).expect("ícone de no máximo 256 px")
        };
        let len = u32::try_from(png.len()).expect("PNG de no máximo 4 GiB");
        let off = u32::try_from(offset).expect(".ico de no máximo 4 GiB");
        header.extend_from_slice(&[side, side, 0, 0]);
        header.extend_from_slice(&1u16.to_le_bytes()); // planos
        header.extend_from_slice(&32u16.to_le_bytes()); // bits por pixel
        header.extend_from_slice(&len.to_le_bytes());
        header.extend_from_slice(&off.to_le_bytes());
        offset += png.len();
        blobs.extend_from_slice(png);
    }
    header.extend_from_slice(&blobs);
    header
}

/// Falha ao gravar um dos `.ico`.
#[derive(Debug)]
pub struct WriteError {
    /// Arquivo que não pôde ser gravado.
    pub path: PathBuf,
    /// Causa do sistema.
    pub source: io::Error,
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "não foi possível gravar {}", self.path.display())
    }
}

impl error::Error for WriteError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Gera os dois `.ico` de [`VARIANTS`] e grava em `dir`. Devolve o nome e os
/// tamanhos de cada arquivo gravado.
///
/// # Errors
///
/// [`WriteError`] com o caminho do arquivo que não pôde ser gravado.
///
/// Os dois arquivos são gerados em memória antes da primeira gravação, e cada um
/// é gravado direto (sem temporário): uma falha no meio deixa o arquivo pela
/// metade, e o teste de guarda acusa até o comando ser rodado de novo.
pub fn write_settings_icons(dir: &Path) -> Result<Vec<(&'static str, Vec<u32>)>, WriteError> {
    let icons: Vec<(&'static str, Vec<u8>)> = VARIANTS
        .iter()
        .map(|&(name, rgb)| (name, settings_ico(rgb, &SETTINGS_GEAR)))
        .collect();
    let mut written = Vec::with_capacity(icons.len());
    for (name, bytes) in icons {
        let path = dir.join(name);
        std::fs::write(&path, bytes).map_err(|source| WriteError { path, source })?;
        written.push((name, SIZES.to_vec()));
    }
    Ok(written)
}
