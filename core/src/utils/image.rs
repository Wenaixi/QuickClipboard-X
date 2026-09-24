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
    // 用 image crate 编码生成 1x1 PNG(确定性,避免手写字节 CRC 不确定性)。
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
        let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        img.save(&png_path).expect("编码 1x1 PNG 失败");
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
