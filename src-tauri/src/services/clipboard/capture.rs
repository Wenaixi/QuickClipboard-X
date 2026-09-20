use clipboard_rs::{common::RustImage, Clipboard, ClipboardContext, RustImageData};

use crate::services::database::ClipboardDataSeed;
use crate::utils::is_image_only_html;

// 剪贴板内容类型
#[derive(Debug, Clone, PartialEq)]
pub enum ContentType {
    Text,
    RichText,
    Files,
}

// 剪贴板内容
#[derive(Debug, Clone)]
pub struct ClipboardContent {
    pub content_type: ContentType,
    pub text: Option<String>,
    pub html: Option<String>,
    pub files: Option<Vec<String>>,
    pub image_path: Option<String>,
    pub raw_formats: Vec<ClipboardDataSeed>,
}

// 保存剪贴板图片到缓存目录
fn save_clipboard_image(rust_image: RustImageData) -> Result<String, String> {
    use crate::services::get_data_directory;
    use image::{codecs::png::PngEncoder, ImageEncoder};
    use sha2::{Digest, Sha256};

    let images_dir = get_data_directory()?.join("clipboard_images");
    std::fs::create_dir_all(&images_dir).map_err(|e| format!("创建目录失败: {}", e))?;

    let rgba_image = rust_image.to_rgba8().map_err(|e| e.to_string())?;
    let (width, height) = (rgba_image.width(), rgba_image.height());

    let mut png_data = Vec::new();
    let encoder = PngEncoder::new(&mut png_data);
    encoder
        .write_image(
            rgba_image.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;

    let hash = format!("{:x}", Sha256::digest(&png_data));
    let filename = format!("{}.png", &hash[..16]);
    let final_path = images_dir.join(&filename);

    if final_path.exists() {
        return Ok(format!("clipboard_images/{}", filename));
    }

    std::fs::write(&final_path, &png_data).map_err(|e| e.to_string())?;

    Ok(format!("clipboard_images/{}", filename))
}

fn append_internal_image_raw_format(raw_formats: &mut Vec<ClipboardDataSeed>, image_path: &str) {
    raw_formats.push(ClipboardDataSeed {
        format_name: crate::services::clipboard::INTERNAL_IMAGE_PATH_FORMAT.to_string(),
        raw_data: image_path.as_bytes().to_vec(),
        is_primary: false,
        format_order: raw_formats.len() as i64,
    });
}

impl ClipboardContent {
    // 从剪贴板捕获内容
    pub fn capture() -> Result<Vec<Self>, String> {
        // 图片分段写入时给更充足窗口，避免大图遗漏。
        const RETRY_DELAYS_MS: [u64; 7] = [0, 40, 80, 140, 220, 360, 560];
        const IMAGE_SETTLE_EXTRA_RETRIES: u8 = 2;
        let mut best_results = Vec::new();
        let mut cached_image_path: Option<String> = None;
        let mut image_settle_retries: u8 = 0;
        let mut best_image_results: Option<Vec<Self>> = None;
        let mut best_image_raw_count: usize = 0;

        for (attempt, delay_ms) in RETRY_DELAYS_MS.iter().enumerate() {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(*delay_ms));
            }

            let results = match Self::capture_internal(cached_image_path.as_deref()) {
                Ok(results) => results,
                Err(_) => continue,
            };

            if results.is_empty() {
                continue;
            }

            if cached_image_path.is_none() {
                cached_image_path = results.iter().find_map(|content| content.image_path.clone());
            }

            // 图片已拿到后额外补几轮，尽量等到 Excel 晚到的文本/HTML/RTF 格式。
            if results.iter().any(|content| content.image_path.is_some()) {
                let current_raw_count = results
                    .iter()
                    .map(|content| {
                        content
                            .raw_formats
                            .iter()
                            .filter(|item| {
                                item.format_name != crate::services::clipboard::INTERNAL_IMAGE_PATH_FORMAT
                            })
                            .count()
                    })
                    .max()
                    .unwrap_or(0);

                if current_raw_count >= best_image_raw_count {
                    best_image_raw_count = current_raw_count;
                    best_image_results = Some(results.clone());
                }

                let is_last_attempt = attempt + 1 >= RETRY_DELAYS_MS.len();
                if image_settle_retries >= IMAGE_SETTLE_EXTRA_RETRIES || is_last_attempt {
                    let final_results = best_image_results.unwrap_or(results);
                    return Ok(final_results);
                }

                image_settle_retries += 1;
                best_results = best_image_results.clone().unwrap_or(results);
                continue;
            }

            if results
                .iter()
                .all(|content| !needs_capture_retry(content))
            {
                return Ok(results);
            }

            best_results = results;
        }

        Ok(best_results)
    }

    fn capture_internal(cached_image_path: Option<&str>) -> Result<Vec<Self>, String> {
        let ctx = ClipboardContext::new().map_err(|e| format!("创建剪贴板上下文失败: {}", e))?;

        let available_formats = ctx.available_formats().unwrap_or_default();

        let mut content = capture_primary_content(&ctx, cached_image_path)?;

        if !has_meaningful_capture(&content) {
            return Ok(Vec::new());
        }

        let mut raw_formats = collect_supported_raw_formats(&ctx, &available_formats);

        if let Some(image_path) = content.image_path.as_deref() {
            append_internal_image_raw_format(&mut raw_formats, image_path);
        }

        let primary_format_name = pick_primary_format_name(&content.content_type, &raw_formats);
        for item in &mut raw_formats {
            item.is_primary = Some(item.format_name.as_str()) == primary_format_name.as_deref();
        }
        content.raw_formats = raw_formats;

        Ok(vec![content])
    }

    // 计算内容哈希值
    // 用主内容简哈希(与 monitor 侧 set_last_hash_text/file/files 同口径),
    // 这样粘贴后预置的哈希能命中捕获侧去重:
    // Text/RichText→sha256(text), Files→sha256(normalize 路径串联),
    // 纯图(被收敛成 Files)也按路径——粘贴时 resolve_item_image_path 返回的
    // 就是本地缓存 png 路径,两侧 normalize 后一致。
    pub fn calculate_hash(&self) -> String {
        use sha2::{Digest, Sha256};

        let mut hasher = Sha256::new();

        match &self.content_type {
            ContentType::Text | ContentType::RichText => {
                if let Some(text) = &self.text {
                    return format!("{:x}", {
                        let mut h = Sha256::new();
                        h.update(text.as_bytes());
                        h.finalize()
                    });
                }
            }
            ContentType::Files => {
                if let Some(files) = &self.files {
                    for file in files {
                        let normalized = crate::services::normalize_path_for_hash(file);
                        hasher.update(normalized.as_bytes());
                        // 路径间必须插入分隔字节,与 monitor.rs 的
                        // set_last_hash_files/hash_clipboard_content 同口径;
                        // 缺分隔符会让 {"a,b","c"} 与 {"a","b,c"} 哈希相同,
                        // 且文件/图片自粘贴 fast-path 永远失配(fast-path
                        // 命中才走原位不置顶;失配落 find_duplicate 兜底
                        // 无条件置顶,违背 pasteToTop=false)。
                        hasher.update([0u8]);
                    }
                    return format!("{:x}", hasher.finalize());
                }
            }
        }

        // 主内容缺失时的回退:raw_formats 全量串联(保底,很少走到)
        for raw in &self.raw_formats {
            hasher.update(raw.format_name.as_bytes());
            hasher.update([0u8]);
            hasher.update(&raw.raw_data);
            hasher.update([0u8]);
        }

        format!("{:x}", hasher.finalize())
    }
}

fn capture_primary_content(
    ctx: &ClipboardContext,
    cached_image_path: Option<&str>,
) -> Result<ClipboardContent, String> {
    let mut content = ClipboardContent {
        content_type: ContentType::Text,
        text: None,
        html: None,
        files: None,
        image_path: None,
        raw_formats: Vec::new(),
    };

    if let Ok(files) = ctx.get_files() {
        if !files.is_empty() {
            content.content_type = ContentType::Files;
            content.text = Some(files.join("\n"));
            content.files = Some(files);
            return Ok(content);
        }
    }

    if let Ok(html) = ctx.get_html() {
        if !html.trim().is_empty() {
            content.content_type = ContentType::RichText;
            content.text = ctx.get_text().ok();
            content.html = Some(html);
        }
    }

    if content.html.is_none() {
        if let Ok(text) = ctx.get_text() {
            if !text.trim().is_empty() {
                content.content_type = ContentType::Text;
                content.text = Some(text);
            }
        }
    }

    if let Some(path) = cached_image_path {
        if content.text.is_none() && content.html.is_none() {
            content.content_type = ContentType::Files;
            content.files = Some(vec![path.to_string()]);
        }
        content.image_path = Some(path.to_string());
    } else {
        if let Ok(rust_image) = ctx.get_image() {
            let image_path = save_clipboard_image(rust_image)?;
            if content.text.is_none() && content.html.is_none() {
                content.content_type = ContentType::Files;
                content.files = Some(vec![image_path.clone()]);
            }
            content.image_path = Some(image_path);
        }
    }

    // 只有图片、没有可见文本时，收敛成纯图片
    if content.image_path.is_some()
        && content.files.is_none()
        && content
            .text
            .as_deref()
            .map(|text| text.trim().is_empty())
            .unwrap_or(true)
        && is_image_only_html(content.html.as_deref())
    {
        let image_path = content.image_path.clone().unwrap();
        content.content_type = ContentType::Files;
        content.text = None;
        content.html = None;
        content.files = Some(vec![image_path]);
    }
    Ok(content)
}

fn has_meaningful_capture(content: &ClipboardContent) -> bool {
    content
        .text
        .as_deref()
        .map(|text| !text.trim().is_empty())
        .unwrap_or(false)
        || content
            .html
            .as_deref()
            .map(|html| !html.trim().is_empty())
            .unwrap_or(false)
        || content
            .files
            .as_ref()
            .map(|files| !files.is_empty())
            .unwrap_or(false)
        || content.image_path.is_some()
}

fn needs_rich_text_retry(content: &ClipboardContent) -> bool {
    content.image_path.is_none()
        && content.files.is_none()
        && content.html.is_none()
        && has_rich_text_candidate_format(&content.raw_formats)
}

fn needs_image_retry(content: &ClipboardContent) -> bool {
    content.image_path.is_none()
        && content.files.is_none()
        && is_image_only_html(content.html.as_deref())
}

fn needs_capture_retry(content: &ClipboardContent) -> bool {
    needs_rich_text_retry(content) || needs_image_retry(content)
}

fn has_rich_text_candidate_format(raw_formats: &[ClipboardDataSeed]) -> bool {
    raw_formats.iter().any(|item| {
        matches!(
            item.format_name.as_str(),
            "HTML Format" | "Rich Text Format"
        )
    })
}

fn collect_supported_raw_formats(
    ctx: &ClipboardContext,
    available_formats: &[String],
) -> Vec<ClipboardDataSeed> {
    let mut result = Vec::new();

    for (idx, format_name) in available_formats.iter().enumerate() {
        if !is_supported_raw_format(format_name) {
            continue;
        }

        if let Ok(raw_data) = ctx.get_buffer(format_name) {
            result.push(ClipboardDataSeed {
                format_name: format_name.clone(),
                raw_data,
                is_primary: false,
                format_order: idx as i64,
            });
        }
    }

    result
}

fn pick_primary_format_name(
    content_type: &ContentType,
    raw_formats: &[ClipboardDataSeed],
) -> Option<String> {
    let preferred_formats: &[&str] = match content_type {
        ContentType::Files => &["CF_HDROP", "text/uri-list"],
        ContentType::RichText => &[
            "HTML Format",
            "Rich Text Format",
            "CF_UNICODETEXT",
            "CF_TEXT",
        ],
        ContentType::Text => &["CF_UNICODETEXT", "CF_TEXT"],
    };

    for preferred in preferred_formats {
        if raw_formats
            .iter()
            .any(|item| item.format_name == *preferred)
        {
            return Some((*preferred).to_string());
        }
    }

    raw_formats.first().map(|item| item.format_name.clone())
}

fn is_supported_raw_format(format_name: &str) -> bool {
    matches!(
        format_name,
        "CF_HDROP" | "CF_TEXT" | "CF_UNICODETEXT" | "HTML Format" | "Rich Text Format"
    )
}

#[cfg(test)]
mod hash_guards {
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    // 主内容简哈希必须优先于 raw_formats 全量串联,否则粘贴后预置的
    // 简哈希与捕获侧全量哈希失配,自粘贴去重 fast-path 失效。
    #[test]
    fn calculate_hash_prioritizes_primary_content_over_raw_formats() {
        let src = strip_line_comments(&source_file("src/services/clipboard/capture.rs"));
        let body = fn_body(&src, "calculate_hash");
        let text_start = body.find("ContentType::Text | ContentType::RichText");
        let files_start = body.find("ContentType::Files");
        let raw_start = body.find("raw_formats");
        assert!(
            text_start.is_some() && files_start.is_some() && raw_start.is_some(),
            "calculate_hash 必须同时包含主内容与 raw_formats 回退分支"
        );
        assert!(
            text_start.unwrap() < raw_start.unwrap(),
            "主内容(Text)分支必须先于 raw_formats 回退,否则粘贴预置简哈希命中不了"
        );
        assert!(
            files_start.unwrap() < raw_start.unwrap(),
            "主内容(Files)分支必须先于 raw_formats 回退"
        );
    }

    // 文件/图片自粘贴 fast-path 依赖 capture 侧简哈希与粘贴侧预置哈希
    // 完全一致:Files 分支每段路径后必须插入 [0u8] 分隔字节(与
    // monitor.rs set_last_hash_files 同口径),缺分隔符会让多文件集合
    // 哈希合糊且自粘贴永远落 find_duplicate 兜底无条件置顶。
    #[test]
    fn files_hash_branch_uses_zero_byte_separators_like_paste_side() {
        let src = strip_line_comments(&source_file("src/services/clipboard/capture.rs"));
        let body = fn_body(&src, "calculate_hash");
        let files_seg = {
            let start = body
                .find("ContentType::Files =>")
                .expect("缺 Files 哈希分支");
            let raw = body
                .find("raw_formats")
                .expect("缺 raw_formats 回退分支");
            &body[start..raw]
        };
        assert!(
            files_seg.contains("hasher.update([0u8])"),
            "Files 分支每段路径后必须插入零字节分隔符(与粘贴侧同口径)"
        );
    }
}
