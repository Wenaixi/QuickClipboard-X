use std::path::{Path, PathBuf};

use image::{codecs::png::PngEncoder, ColorType, ImageEncoder};
use sha2::{Digest, Sha256};

#[cfg(target_os = "windows")]
use windows::core::HSTRING;
#[cfg(target_os = "windows")]
use windows::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};

use crate::services::database::ClipboardDataSeed;

const CLIPBOARD_IMAGE_DIRECTORY: &str = "clipboard_images";
const PIN_IMAGE_DIRECTORY: &str = "pin_images";
const IMAGE_ID_LENGTH: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredScreenshot {
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub image_id: String,
    pub width: u32,
    pub height: u32,
    pub png_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageStoreError {
    InvalidDimensions,
    InvalidRgbaLength { expected: usize, actual: usize },
    DataDirectory(String),
    CreateDirectory(String),
    Read(String),
    Encode(String),
    AtomicWrite(String),
}

impl std::fmt::Display for ImageStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDimensions => write!(f, "截图尺寸无效"),
            Self::InvalidRgbaLength { expected, actual } => {
                write!(f, "截图像素数据长度无效: expected={expected}, actual={actual}")
            }
            Self::DataDirectory(error) => write!(f, "获取截图数据目录失败: {error}"),
            Self::CreateDirectory(error) => write!(f, "创建截图目录失败: {error}"),
            Self::Read(error) => write!(f, "读取截图文件失败: {error}"),
            Self::Encode(error) => write!(f, "编码截图 PNG 失败: {error}"),
            Self::AtomicWrite(error) => write!(f, "原子写入截图 PNG 失败: {error}"),
        }
    }
}

impl std::error::Error for ImageStoreError {}

pub(crate) fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, ImageStoreError> {
    if width == 0 || height == 0 {
        return Err(ImageStoreError::InvalidDimensions);
    }

    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(ImageStoreError::InvalidDimensions)?;
    if rgba.len() != expected {
        return Err(ImageStoreError::InvalidRgbaLength {
            expected,
            actual: rgba.len(),
        });
    }

    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes)
        .write_image(rgba, width, height, ColorType::Rgba8.into())
        .map_err(|error| ImageStoreError::Encode(error.to_string()))?;
    Ok(bytes)
}

fn content_image_id(png_bytes: &[u8]) -> String {
    let digest = Sha256::digest(png_bytes);
    format!("{digest:x}")[..IMAGE_ID_LENGTH].to_string()
}

fn atomic_write(
    path: &Path,
    bytes: &[u8],
    replace_existing: bool,
) -> Result<(), ImageStoreError> {
    let temporary = path.with_extension(format!(
        "png.{}.tmp",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&temporary, bytes)
        .map_err(|error| ImageStoreError::AtomicWrite(error.to_string()))?;

    #[cfg(target_os = "windows")]
    {
        let source_text = temporary.as_os_str().to_string_lossy().into_owned();
        let destination_text = path.as_os_str().to_string_lossy().into_owned();
        let source = HSTRING::from(source_text);
        let destination = HSTRING::from(destination_text);
        let mut flags = MOVEFILE_WRITE_THROUGH;
        if replace_existing {
            flags |= MOVEFILE_REPLACE_EXISTING;
        }

        let result = unsafe { MoveFileExW(&source, &destination, flags) };
        if result.is_ok() {
            return Ok(());
        }
        if !replace_existing && path.exists() {
            let _ = std::fs::remove_file(&temporary);
            return Ok(());
        }

        let _ = std::fs::remove_file(&temporary);
        return Err(ImageStoreError::AtomicWrite(result.err().map_or_else(
            || "Windows 原子替换失败".to_string(),
            |error| error.to_string(),
        )));
    }

    #[cfg(not(target_os = "windows"))]
    {
        if !replace_existing && path.exists() {
            let _ = std::fs::remove_file(&temporary);
            return Ok(());
        }
        if let Err(error) = std::fs::rename(&temporary, path) {
            if !replace_existing && path.exists() {
                let _ = std::fs::remove_file(&temporary);
                return Ok(());
            }
            let _ = std::fs::remove_file(&temporary);
            return Err(ImageStoreError::AtomicWrite(error.to_string()));
        }
        Ok(())
    }
}

pub fn encode_and_store_png(
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<StoredScreenshot, ImageStoreError> {
    let png_bytes = encode_png(width, height, rgba)?;
    store_png_bytes(width, height, png_bytes)
}

fn store_png_bytes(
    width: u32,
    height: u32,
    png_bytes: Vec<u8>,
) -> Result<StoredScreenshot, ImageStoreError> {
    let image_id = content_image_id(&png_bytes);
    let data_directory =
        crate::services::get_data_directory().map_err(ImageStoreError::DataDirectory)?;
    let images_directory = data_directory.join(CLIPBOARD_IMAGE_DIRECTORY);
    std::fs::create_dir_all(&images_directory)
        .map_err(|error| ImageStoreError::CreateDirectory(error.to_string()))?;

    let filename = format!("{image_id}.png");
    let absolute_path = images_directory.join(&filename);
    atomic_write(&absolute_path, &png_bytes, false)?;

    Ok(StoredScreenshot {
        relative_path: format!("{CLIPBOARD_IMAGE_DIRECTORY}/{filename}"),
        absolute_path,
        image_id,
        width,
        height,
        png_bytes: png_bytes.len(),
    })
}

pub fn copy_file_atomic(source: &Path, destination: &Path) -> Result<(), ImageStoreError> {
    let bytes = std::fs::read(source)
        .map_err(|error| ImageStoreError::Read(error.to_string()))?;
    atomic_write(destination, &bytes, true)
}

pub fn prepare_pin_path(stored: &StoredScreenshot) -> Result<PathBuf, ImageStoreError> {
    let data_directory =
        crate::services::get_data_directory().map_err(ImageStoreError::DataDirectory)?;
    let pin_directory = data_directory.join(PIN_IMAGE_DIRECTORY);
    std::fs::create_dir_all(&pin_directory)
        .map_err(|error| ImageStoreError::CreateDirectory(error.to_string()))?;

    let destination = pin_directory.join(format!("QC_{}.png", stored.image_id));
    copy_file_atomic(&stored.absolute_path, &destination)?;
    Ok(destination)
}

pub fn image_raw_format(relative_path: &str) -> ClipboardDataSeed {
    ClipboardDataSeed {
        format_name: crate::services::clipboard::INTERNAL_IMAGE_PATH_FORMAT.to_string(),
        raw_data: relative_path.as_bytes().to_vec(),
        is_primary: false,
        format_order: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_encoding_is_content_addressed_and_validates_rgba_length() {
        let first = encode_png(1, 1, &[255, 0, 0, 255]).unwrap();
        let second = encode_png(1, 1, &[255, 0, 0, 255]).unwrap();
        assert_eq!(first, second);
        assert_eq!(content_image_id(&first).len(), IMAGE_ID_LENGTH);
        assert!(matches!(
            encode_png(1, 1, &[0, 0, 0]),
            Err(ImageStoreError::InvalidRgbaLength { .. })
        ));
    }

    #[test]
    fn image_raw_format_keeps_only_relative_path_data() {
        let raw = image_raw_format("clipboard_images/0123456789abcdef.png");
        assert_eq!(
            raw.format_name,
            crate::services::clipboard::INTERNAL_IMAGE_PATH_FORMAT
        );
        assert_eq!(raw.raw_data, b"clipboard_images/0123456789abcdef.png");
        assert!(!raw.is_primary);
    }
}
