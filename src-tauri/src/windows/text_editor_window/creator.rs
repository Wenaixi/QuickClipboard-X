use tauri::AppHandle;
use std::sync::atomic::{AtomicU64, Ordering};

// 同类型窗口连续创建用单调序号区分——时间戳无法保证唯一(同毫秒
// 创建两次就冲突,第二个窗口 build 直接失败),且与用户可见时间无关。
static TEXT_EDITOR_WINDOW_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn create_text_editor_window(
    app: &AppHandle,
    item_id: &str,
    item_type: &str,
    item_index: Option<i32>,
    group_name: Option<String>,
) -> Result<String, String> {
    let sequence = TEXT_EDITOR_WINDOW_SEQ.fetch_add(1, Ordering::Relaxed);
    let window_label = format!("text-editor-{}-{}", item_type, sequence);
    
    let mut url = format!("windows/textEditor/index.html?id={}&type={}", item_id, item_type);
    if let Some(index) = item_index {
        url = format!("{}&index={}", url, index);
    }
    if let Some(group) = group_name {
        url = format!("{}&group={}", url, group);
    }
    
    let editor_window = tauri::WebviewWindowBuilder::new(
        app,
        &window_label,
        tauri::WebviewUrl::App(url.into()),
    )
    .title("文本编辑器 - 快速剪贴板")
    .inner_size(900.0, 700.0)
    .min_inner_size(600.0, 400.0)
    .center()
    .resizable(true)
    .maximizable(true)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .skip_taskbar(false)
    .visible(true)
    .focused(true)
    .drag_and_drop(false)
    .build()
    .map_err(|e| format!("创建文本编辑器窗口失败: {}", e))?;

    let editor_window_for_events = editor_window.clone();
    editor_window.on_window_event(move |event| match event {
        tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed => {
            crate::services::memory::schedule_cleanup_after_main_window_hide();
        }
        _ => {
            if editor_window_for_events.is_minimized().unwrap_or(false) {
                crate::services::memory::schedule_cleanup_after_main_window_hide();
            }
        }
    });

    Ok(window_label)
}

#[cfg(test)]
mod tests {
    // 源码护栏:文本编辑器窗口标签必须用单调序号,不能用时间戳——
    // 同毫秒创建两次时间戳相同,label 冲突会让第二个窗口 build 失败;
    // 单调计数从 0 起步,同进程内永不重复。
    #[test]
    fn text_editor_label_uses_monotonic_sequence_not_timestamp() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/text_editor_window/creator.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 creator.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = stripped
            .find("fn create_text_editor_window")
            .expect("缺 create_text_editor_window");
        let rest = &stripped[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(stripped.len());
        let body = &stripped[start..end];
        assert!(
            body.contains("TEXT_EDITOR_WINDOW_SEQ.fetch_add(1, Ordering::Relaxed)"),
            "标签必须用静态单调计数器"
        );
        assert!(
            !body.contains("SystemTime"),
            "标签不得用时间戳——同毫秒创建冲突"
        );
        assert!(
            !body.contains("format!(\"text-editor-{}-{}\", item_type, timestamp)"),
            "标签格式不得依赖时间戳后缀"
        );
    }
}

