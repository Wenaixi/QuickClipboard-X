// HTML 处理工具函数

use regex::Regex;
use std::sync::LazyLock;

/// 匹配任意 HTML 标签。静态编译,避免热路径反复 Regex::new。
pub static HTML_TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]*>").unwrap());
/// 匹配 HTML 实体(如 &nbsp;)。与 TAG 配对剥离可见文本。
pub static HTML_ENTITY_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&[a-zA-Z]+;").unwrap());

/// 判断 HTML 是否只含图片、无可见文本(剥标签与实体后 trim 为空)。
/// capture / paste 共用,禁止各自再裸 Regex::new。
pub fn is_image_only_html(html: Option<&str>) -> bool {
    let Some(html) = html else {
        return false;
    };

    if !html.contains("<img") {
        return false;
    }

    let mut text = HTML_TAG_RE.replace_all(html, " ").to_string();
    text = HTML_ENTITY_RE.replace_all(&text, " ").to_string();
    text.trim().is_empty()
}

pub fn truncate_html(html: String, max_visible_len: usize) -> String {
    if html.is_empty() {
        return html;
    }
    
    if max_visible_len == 0 {
        return "...(内容过长已截断)".to_string();
    }
    
    let mut visible_count: usize = 0;
    let mut in_tag = false;
    
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => {
                visible_count = visible_count.saturating_add(1);
                if visible_count > max_visible_len {
                    break;
                }
            }
            _ => {}
        }
    }
    
    if visible_count <= max_visible_len {
        return html;
    }
    
    let mut result = String::with_capacity(html.len().min(max_visible_len * 10));
    visible_count = 0;
    in_tag = false;
    let mut open_tags: Vec<String> = Vec::with_capacity(16);
    let mut current_tag = String::with_capacity(32);
    let mut is_closing_tag = false;
    let mut tag_started = false;
    
    for c in html.chars() {
        if c == '<' {
            in_tag = true;
            tag_started = false;
            is_closing_tag = false;
            current_tag.clear();
            result.push(c);
        } else if c == '>' {
            in_tag = false;
            result.push(c);
            
            if !current_tag.is_empty() {
                let tag_name = current_tag.to_lowercase();
                let is_self_closing = matches!(tag_name.as_str(), 
                    "br" | "hr" | "img" | "input" | "meta" | "link" | "area" | "base" | "col" | "embed" | "source" | "track" | "wbr");
                
                if !is_self_closing {
                    if is_closing_tag {
                        if let Some(pos) = open_tags.iter().rposition(|t| t == &tag_name) {
                            open_tags.remove(pos);
                        }
                    } else {
                        if open_tags.len() < 100 {
                            open_tags.push(tag_name);
                        }
                    }
                }
            }
        } else if in_tag {
            result.push(c);
            
            if c == '/' && !tag_started {
                is_closing_tag = true;
            } else if c.is_alphanumeric() && !tag_started {
                tag_started = true;
                if current_tag.len() < 50 {
                    current_tag.push(c);
                }
            } else if tag_started && (c.is_alphanumeric() || c == '-') {
                if current_tag.len() < 50 {
                    current_tag.push(c);
                }
            } else if tag_started {
                tag_started = false;
            }
        } else {
            visible_count = visible_count.saturating_add(1);
            if visible_count > max_visible_len {
                break;
            }
            result.push(c);
        }
    }
    
    for tag in open_tags.iter().rev().take(50) {
        result.push_str("</");
        result.push_str(tag);
        result.push('>');
    }
    result.push_str("...(内容过长已截断)");

    result
}

#[cfg(test)]
mod tests {
    use super::is_image_only_html;
    use std::fs;

    #[test]
    fn image_only_html_true_for_img_without_text() {
        assert!(is_image_only_html(Some(r#"<div><img src="a.png"/></div>"#)));
        assert!(is_image_only_html(Some(r#"<img src="a.png">&nbsp;"#)));
    }

    #[test]
    fn image_only_html_false_when_visible_text_or_no_img() {
        assert!(!is_image_only_html(None));
        assert!(!is_image_only_html(Some("<div>hello</div>")));
        assert!(!is_image_only_html(Some(r#"<img src="a.png"/>caption"#)));
    }

    /// C01/C03 护栏:capture 与 paste/options 不得各自再 Regex::new TAG/ENTITY。
    #[test]
    fn capture_and_paste_reuse_shared_is_image_only_html() {
        let root = env!("CARGO_MANIFEST_DIR");
        let capture = fs::read_to_string(format!("{root}/src/services/clipboard/capture.rs"))
            .expect("读 capture.rs");
        let options = fs::read_to_string(format!("{root}/src/services/paste/options.rs"))
            .expect("读 options.rs");

        // 剥行注释后再匹配,避免注释误命中
        let strip = |src: &str| -> String {
            src.lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let capture_body = strip(&capture);
        let options_body = strip(&options);

        assert!(
            capture_body.contains("is_image_only_html("),
            "capture 应调用共享 is_image_only_html"
        );
        assert!(
            !capture_body.contains("fn is_image_only_html"),
            "capture 不得再定义本地 is_image_only_html 副本"
        );
        assert!(
            !capture_body.contains("Regex::new"),
            "capture 不得再裸编译正则"
        );
        assert!(
            options_body.contains("is_image_only_html("),
            "options 应调用共享 is_image_only_html"
        );
        assert!(
            !options_body.contains("fn is_image_only_html"),
            "options 不得再定义本地 is_image_only_html 副本"
        );
        assert!(
            !options_body.contains("Regex::new"),
            "options 不得再裸编译正则"
        );
    }
}
