// 剪贴板图片 ID 白名单校验。
// image_id 会直接拼进本地/云端路径(`{image_id}.png`),必须限制为短横线/下划线/字母数字,
// 杜绝 `../` 目录穿越与任意路径读写。与 lan/files.rs::is_valid_image_id 语义一致。

/// 校验剪贴板图片 ID 是否安全:非空、≤128 字符、仅字母数字 + `-` + `_`。
pub fn is_valid_image_id(image_id: &str) -> bool {
    !image_id.is_empty()
        && image_id.len() <= 128
        && image_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::is_valid_image_id;

    #[test]
    fn valid_ids_are_accepted() {
        for id in ["a", "A1", "image_01", "img-123", "aBc-9_"] {
            assert!(is_valid_image_id(id), "合法 id 被拒绝: {}", id);
        }
    }

    #[test]
    fn path_traversal_and_special_chars_are_rejected() {
        for id in ["", "../", "..", "..\\..", "a/b", "a\\b", "a:b", "a b", "a%2fb", "..hidden", "a/../b"] {
            assert!(!is_valid_image_id(id), "非法 id 被放行: {:?}", id);
        }
    }

    #[test]
    fn length_limit_is_128() {
        let ok = "a".repeat(128);
        assert!(is_valid_image_id(&ok));
        let too_long = "a".repeat(129);
        assert!(!is_valid_image_id(&too_long));
    }
}