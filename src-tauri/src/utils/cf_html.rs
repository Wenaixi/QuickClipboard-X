pub fn generate_cf_html(html: &str) -> String {
    let html_content = ensure_fragment_markers(html);

    let header = "Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n";
    let start_html = header.len();
    let end_html = start_html + html_content.len();

    // 标记由 ensure_fragment_markers 保证存在,find 失败是编程错误而非输入错误
    let start_fragment = start_html
        + html_content
            .find("<!--StartFragment-->")
            .expect("ensure_fragment_markers 必须插入 StartFragment");
    let end_fragment = start_html
        + html_content
            .find("<!--EndFragment-->")
            .expect("ensure_fragment_markers 必须插入 EndFragment");

    format!(
        "Version:0.9\r\nStartHTML:{:010}\r\nEndHTML:{:010}\r\nStartFragment:{:010}\r\nEndFragment:{:010}\r\n{}",
        start_html, end_html, start_fragment, end_fragment, html_content
    )
}

/// 保证 html 含 StartFragment/EndFragment 标记。
/// 1. 已有标记:原样返回
/// 2. 无 <html 外壳:包一层完整文档 + 标记
/// 3. 有外壳:大小写不敏感找 <body...> 与 </body>,在 body 内插入标记
/// 4. 找不到 body:把整个输入包在一对 Fragment 标记中
fn ensure_fragment_markers(html: &str) -> String {
    if html.contains("<!--StartFragment-->") {
        return html.to_string();
    }

    if !html.to_ascii_lowercase().contains("<html") {
        return format!(
            "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n</head>\n<body>\n<!--StartFragment-->{}\n<!--EndFragment-->\n</body>\n</html>",
            html
        );
    }

    // 大小写不敏感定位 <body...> 开标签末尾
    let lower = html.to_ascii_lowercase();
    if let Some(body_open_rel) = lower.find("<body") {
        // 从 <body 起找 '>' 作为开标签结束
        if let Some(gt_rel) = html[body_open_rel..].find('>') {
            let after_open = body_open_rel + gt_rel + 1;
            // 找 </body>
            if let Some(close_rel) = lower[after_open..].find("</body>") {
                let close_abs = after_open + close_rel;
                let mut out = String::with_capacity(html.len() + 40);
                out.push_str(&html[..after_open]);
                out.push_str("\n<!--StartFragment-->");
                out.push_str(&html[after_open..close_abs]);
                out.push_str("<!--EndFragment-->\n");
                out.push_str(&html[close_abs..]);
                return out;
            }
        }
    }

    // 找不到 body:整段包在 Fragment 标记里
    format!(
        "<!--StartFragment-->{}<!--EndFragment-->",
        html
    )
}

pub fn normalize_clipboard_html(input: &str) -> String {
    let s = input;

    if s.contains("StartFragment") || s.contains("StartHTML") {
        if let Some(fragment) = extract_cf_html_by_markers(s) {
            return fragment;
        }
        if let Some(fragment) = extract_cf_html_by_offsets(s) {
            return fragment;
        }
    }

    s.to_string()
}

fn extract_cf_html_by_markers(s: &str) -> Option<String> {
    let start_marker = "<!--StartFragment-->";
    let end_marker = "<!--EndFragment-->";

    let start = s.find(start_marker)? + start_marker.len();
    let end = s.find(end_marker)?;
    if end <= start {
        return None;
    }

    Some(s[start..end].to_string())
}

fn extract_cf_html_by_offsets(s: &str) -> Option<String> {
    fn parse_offset(s: &str, key: &str) -> Option<usize> {
        let idx = s.find(key)?;
        let rest = &s[idx + key.len()..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            return None;
        }
        digits.parse::<usize>().ok()
    }

    let bytes = s.as_bytes();
    let len = bytes.len();

    let start_fragment = parse_offset(s, "StartFragment:").or_else(|| parse_offset(s, "StartHTML:"));
    let end_fragment = parse_offset(s, "EndFragment:").or_else(|| parse_offset(s, "EndHTML:"));

    let (start, end) = match (start_fragment, end_fragment) {
        (Some(a), Some(b)) if a < b && b <= len => (a, b),
        _ => return None,
    };

    std::str::from_utf8(&bytes[start..end]).ok().map(|t| t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_cf_html_plain_fragment_has_markers() {
        let out = generate_cf_html("<b>hi</b>");
        assert!(out.contains("<!--StartFragment-->"));
        assert!(out.contains("<!--EndFragment-->"));
        // StartFragment 偏移必须指向标记本身,不能是 0
        let start_line = out.lines().find(|l| l.starts_with("StartFragment:")).unwrap();
        let offset: usize = start_line.split(':').nth(1).unwrap().parse().unwrap();
        assert!(offset > 0, "StartFragment 偏移不应是 0");
        assert_eq!(&out.as_bytes()[offset..offset + 20], b"<!--StartFragment-->");
    }

    #[test]
    fn generate_cf_html_case_insensitive_body() {
        let html = "<HTML><BODY class=x>hello</BODY></HTML>";
        let out = generate_cf_html(html);
        assert!(out.contains("<!--StartFragment-->"));
        assert!(out.contains("<!--EndFragment-->"));
        assert!(out.contains("hello"));
    }

    #[test]
    fn generate_cf_html_no_body_wraps_whole() {
        let html = "<html><head></head>orphan</html>";
        let out = generate_cf_html(html);
        assert!(out.contains("<!--StartFragment-->"));
        assert!(out.contains("<!--EndFragment-->"));
        // 找不到 body 时整段包在标记内
        assert!(out.contains("<!--StartFragment--><html>"));
    }

    #[test]
    fn generate_cf_html_already_has_markers_passthrough() {
        let html = "<html><body><!--StartFragment-->x<!--EndFragment--></body></html>";
        let out = generate_cf_html(html);
        assert_eq!(out.matches("<!--StartFragment-->").count(), 1);
    }
}
