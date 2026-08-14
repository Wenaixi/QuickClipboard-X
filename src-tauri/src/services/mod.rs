pub mod clipboard;
pub mod screenshot;
pub mod database;
pub mod data_management;
pub mod notification;
pub mod settings;
pub mod system;
pub mod paste;
pub mod sound;
pub mod image_library;
pub mod low_memory;
pub mod memory;
pub mod store;
pub mod sync_transfer;
pub mod secure_credentials;
pub mod webdav_sync;

pub use settings::{AppSettings, get_settings, update_settings, get_data_directory};
pub use notification::show_startup_notification;
pub use system::hotkey;
pub use sound::{SoundPlayer, AppSounds, mark_paste_operation};

pub fn normalize_path_for_hash(path: &str) -> String {
    let normalized = path.replace("\\", "/");
    for prefix in ["clipboard_images/", "pin_images/"] {
        if let Some(idx) = normalized.find(prefix) {
            return normalized[idx..].to_string();
        }
    }
    normalized
}

// 解析存储的路径为实际绝对路径
pub fn resolve_stored_path(stored_path: &str) -> String {
    let normalized_input = stored_path.replace("/", "\\");
    
    if normalized_input.starts_with("clipboard_images\\") 
        || normalized_input.starts_with("pin_images\\")
        || normalized_input.starts_with("image_library\\") {
        if let Ok(data_dir) = get_data_directory() {
            return data_dir.join(&normalized_input).to_string_lossy().to_string();
        }
    }
    
    let search_path = stored_path.replace("\\", "/");
    for prefix in ["clipboard_images/", "pin_images/", "image_library/"] {
        if let Some(idx) = search_path.find(prefix) {
            if let Ok(data_dir) = get_data_directory() {
                let relative = search_path[idx..].replace("/", "\\");
                let new_path = data_dir.join(&relative);
                if new_path.exists() {
                    return new_path.to_string_lossy().to_string();
                }
            }
        }
    }
    
    stored_path.to_string()
}

pub fn is_portable_build() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase()))
        .map(|name| name.contains("portable"))
        .unwrap_or(false)
}

/// 统一便携运行时检测:exe 名含 portable,或同目录有 portable.flag / portable.txt。
/// 所有调用点必须走这里,禁止各自内联 flag/txt 判定(语义漂移见 A2)。
pub fn is_portable_runtime() -> bool {
    if is_portable_build() {
        return true;
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            exe.parent().map(|p| {
                p.join("portable.flag").exists() || p.join("portable.txt").exists()
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    // A2 护栏:is_portable_runtime 必须是唯一判定入口,
    // 禁止调用点各自内联 portable.flag / portable.txt 判定(语义漂移)。
    // 剥注释后再匹配,避免注释字面误命中。
    use crate::services::system::hotkey::test_utils::{source_file, strip_line_comments};

    fn bare_source(rel: &str) -> String {
        strip_line_comments(&source_file(&format!("src/{rel}")))
    }

    #[test]
    fn is_portable_runtime_is_single_source_of_truth() {
        // 1. helper 自身必须同时检查 flag + txt(语义完整)
        let helper = bare_source("services/mod.rs");
        let fn_start = helper
            .find("pub fn is_portable_runtime")
            .expect("找不到 is_portable_runtime");
        let fn_end = helper[fn_start..]
            .find("\npub fn ")
            .or_else(|| helper[fn_start..].find("\nfn "))
            .or_else(|| helper[fn_start..].find("\n#[cfg"))
            .unwrap_or(helper.len() - fn_start);
        let body = &helper[fn_start..fn_start + fn_end];
        assert!(
            body.contains("portable.flag") && body.contains("portable.txt"),
            "is_portable_runtime 必须同时检查 portable.flag 与 portable.txt"
        );
        assert!(
            body.contains("is_portable_build()"),
            "is_portable_runtime 必须先委托 is_portable_build"
        );

        // 2. 6 处调用点不得再内联 flag/txt 判定(必须走 helper)
        let call_sites: &[(&str, &str)] = &[
            ("services/settings/storage.rs", "fn is_portable_mode"),
            ("commands/settings.rs", "pub fn is_portable_mode"),
            ("services/data_management/mod.rs", "fn import_data_package"), // 近似锚,下面用全文
            ("windows/updater_window/creator.rs", "let mut is_portable"),
        ];
        // storage.rs: 私有 is_portable_mode 必须委托 helper,不得内联 join
        {
            let s = bare_source("services/settings/storage.rs");
            let start = s.find("fn is_portable_mode").expect("storage 缺 is_portable_mode");
            let end = s[start..]
                .find("\n    fn ")
                .or_else(|| s[start..].find("\n    pub fn "))
                .unwrap_or(200);
            let body = &s[start..start + end];
            assert!(
                body.contains("is_portable_runtime()"),
                "storage::is_portable_mode 必须委托 is_portable_runtime"
            );
            assert!(
                !body.contains("portable.flag") && !body.contains("portable.txt"),
                "storage::is_portable_mode 不得再内联 flag/txt 判定"
            );
            let _ = call_sites; // silence
        }
        // commands/settings.rs: 公开 is_portable_mode 必须委托 helper
        {
            let s = bare_source("commands/settings.rs");
            let start = s
                .find("pub fn is_portable_mode")
                .expect("commands 缺 is_portable_mode");
            let end = s[start..]
                .find("\n#[tauri::command]")
                .or_else(|| s[start..].find("\npub fn "))
                .unwrap_or(300);
            let body = &s[start..start + end];
            assert!(
                body.contains("is_portable_runtime()"),
                "commands::is_portable_mode 必须委托 is_portable_runtime"
            );
            assert!(
                !body.contains("portable.txt") && !body.contains("portable.flag"),
                "commands::is_portable_mode 不得再内联 flag/txt 判定"
            );
        }
        // data_management 三处:全文不得再出现内联 portable.txt 判定
        {
            let s = bare_source("services/data_management/mod.rs");
            // 允许注释,但 bare 已剥注释;生产代码不得再 join("portable.txt")
            assert!(
                !s.contains("join(\"portable.txt\")") && !s.contains("join(\"portable.flag\")"),
                "data_management 不得再内联 portable.txt/flag 判定,必须走 is_portable_runtime"
            );
            let count = s.matches("is_portable_runtime()").count();
            assert!(
                count >= 3,
                "data_management 至少 3 处调用 is_portable_runtime,实际 {}",
                count
            );
        }
        // updater creator: 必须走 helper,不得内联
        {
            let s = bare_source("windows/updater_window/creator.rs");
            assert!(
                s.contains("is_portable_runtime()"),
                "updater creator 必须调用 is_portable_runtime"
            );
            assert!(
                !s.contains("join(\"portable.txt\")") && !s.contains("join(\"portable.flag\")"),
                "updater creator 不得再内联 portable.txt/flag 判定"
            );
        }
        // lib.rs setup 写 portable.flag 的路径只看 is_portable_build(写 marker 不是检测)
        // —— 不强制改,setup 语义是"若是 portable 构建则落 flag",不是 runtime 检测
    }
}
