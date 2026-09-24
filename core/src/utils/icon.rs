use file_icon_provider::get_file_icon;
use image::{RgbaImage, ImageFormat};
use std::io::Cursor;
use sha2::{Sha256, Digest};

// 获取文件图标并转换为 Base64 Data URL
pub fn get_file_icon_base64(path: &str) -> Option<String> {
    match get_file_icon(path, 32) {
        Ok(icon) => {
            if let Ok(png_data) = icon_to_png(&icon) {
                use base64::{Engine as _, engine::general_purpose};
                let base64_str = general_purpose::STANDARD.encode(&png_data);
                return Some(format!("data:image/png;base64,{}", base64_str));
            }
            None
        }
        Err(_) => None,
    }
}

// 将 Icon 转换为 PNG 格式
pub fn icon_to_png(icon: &file_icon_provider::Icon) -> Result<Vec<u8>, String> {
    let img = RgbaImage::from_raw(icon.width, icon.height, icon.pixels.clone())
        .ok_or("创建图像失败")?;
    
    let mut png_data = Vec::new();
    let mut cursor = Cursor::new(&mut png_data);
    img.write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| format!("PNG编码失败: {}", e))?;
    
    Ok(png_data)
}

// 计算图标哈希
fn calculate_icon_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = format!("{:x}", hasher.finalize());
    hash[..16].to_string()
}

// 保存应用图标到 app_icons 目录
pub fn save_app_icon(exe_path: &str) -> Option<String> {
    let icon = match get_file_icon(exe_path, 32) {
        Ok(icon) => icon,
        Err(_) => return None,
    };

    let png_data = match icon_to_png(&icon) {
        Ok(data) => data,
        Err(_) => return None,
    };

    let hash = calculate_icon_hash(&png_data);

    let data_dir = match crate::services::get_data_directory() {
        Ok(dir) => dir,
        Err(_) => return None,
    };

    let icons_dir = data_dir.join("app_icons");
    if !icons_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&icons_dir) {
            eprintln!("创建图标目录失败: {}", e);
            return None;
        }
    }

    let icon_path = icons_dir.join(format!("{}.png", hash));
    // 已存在且非零字节才跳过:半写/损坏残留会被 exists() 误判为"已有"而
    // 永久复用坏图标,存在性判断需同时校验文件大小。
    if !icon_path.exists() || icon_path.metadata().map(|m| m.len()).unwrap_or(0) == 0 {
        if let Err(e) = std::fs::write(&icon_path, &png_data) {
            eprintln!("写入应用图标失败: {}", e);
            return None;
        }
    }

    Some(hash)
}

#[cfg(test)]
mod tests {
    use super::calculate_icon_hash;

    // 图标哈希是可复用缓存键:同一数据哈希稳定且长度为 16 十六进制字符。
    #[test]
    fn icon_hash_is_stable_and_16_hex_chars() {
        let data = b"icon-bytes";
        let h1 = calculate_icon_hash(data);
        let h2 = calculate_icon_hash(data);
        assert_eq!(h1, h2, "同一图标数据哈希必须稳定");
        assert_eq!(h1.len(), 16, "哈希取前 16 位十六进制");
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit()), "必须是十六进制字符");
    }

    // 不同图标数据必须映射到不同哈希,否则缓存互相覆盖。
    #[test]
    fn icon_hash_differs_for_different_data() {
        let h1 = calculate_icon_hash(b"first");
        let h2 = calculate_icon_hash(b"second");
        assert_ne!(h1, h2, "不同图标数据必须生成不同哈希");
    }

    // 空数据也可哈希(哈希函数对任意输入定义完整,不 panic)。
    #[test]
    fn icon_hash_handles_empty_data() {
        let h = calculate_icon_hash(b"");
        assert_eq!(h.len(), 16);
    }
}
