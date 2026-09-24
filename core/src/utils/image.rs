// 判断是否是图片文件
pub fn is_image_file(path: &str) -> bool {
    let path_lower = path.to_lowercase();
    path_lower.ends_with(".jpg") || 
    path_lower.ends_with(".jpeg") || 
    path_lower.ends_with(".png") || 
    path_lower.ends_with(".gif") || 
    path_lower.ends_with(".bmp") || 
    path_lower.ends_with(".webp")
}

// 读取图片尺寸
pub fn get_image_dimensions(path: &str) -> Option<(u32, u32)> {
    use std::fs::File;
    use std::io::BufReader;
    use image::ImageReader;
    
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let img_reader = ImageReader::new(reader).with_guessed_format().ok()?;
    img_reader.into_dimensions().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // is_image_file 必须覆盖常见图片扩展名,大小写不敏感。
    #[test]
    fn is_image_file_covers_common_extensions_case_insensitive() {
        for path in [
            "a.PNG", "b.jpg", "c.JPEG", "d.gif", "e.BMP", "f.webp", "g.png",
        ] {
            assert!(is_image_file(path), "{} 应为图片", path);
        }
        for path in ["a.txt", "b.pdf", "c.exe", "d.pngx"] {
            assert!(!is_image_file(path), "{} 不应为图片", path);
        }
    }

    // get_image_dimensions 必须能读真实 PNG 文件返回宽高;读不到返回 None。
    #[test]
    fn get_image_dimensions_reads_real_png() {
        let dir = std::env::temp_dir().join(format!(
            "qc-core-image-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let png_path = dir.join("test.png");
        // 1x1 红色像素 PNG(手写最小合法 PNG)
        let png_bytes: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // 签名
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR 头
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // 宽=1 高=1
            0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE, // 位深/颜色/CRC
            0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, // IDAT 头
            0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00, 0x03, 0x00, 0x01, 0x00, 0x01, 0xFE, 0x25, 0x1E, 0xC4, // 压缩数据+CRC
            0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82, // IEND
        ];
        std::fs::write(&png_path, png_bytes).unwrap();
        let dims = get_image_dimensions(png_path.to_str().unwrap());
        assert_eq!(dims, Some((1, 1)), "1x1 PNG 应返回 (1,1)");
        assert_eq!(
            get_image_dimensions("/nonexistent/does-not-exist.png"),
            None,
            "不存在的文件应返回 None"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
