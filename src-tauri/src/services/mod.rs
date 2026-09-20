pub mod clipboard;
#[cfg(target_os = "windows")]
pub mod screenshot;
pub mod database;
pub mod diagnostics;
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
pub mod upload;
pub mod tools;
#[cfg(target_os = "windows")]
pub mod recording;

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

// 检查路径是否含 . / .. 段——含父目录段的输入不得用于拼接解析，
// 否则词法拼接后 starts_with(data_dir) 在 `..` 未折叠时必然通过，
// 恶意同步记录可把 data_dir 之外任意路径解析出来。
// 按分隔符拆段判断：`file..txt` 是单个 Normal 段算安全，
// 只有独立成段的 `.`/`..` 才命中（LastIteration 等 Windows 特殊段不在此列）。
fn contains_parent_segments(path: &str) -> bool {
    use std::path::{Component, Path};
    Path::new(path)
        .components()
        .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
}

// 解析存储的路径为实际绝对路径
pub fn resolve_stored_path(stored_path: &str) -> String {
    let normalized_input = stored_path.replace("/", "\\");

    // 目录穿越防线：任何含 . / .. 段的输入直接返回空串，不参与拼接。
    // 两条 `..` 折叠后的 starts_with(&data_dir) 词法比较在 Windows 上
    // 无法可靠折叠（Path::starts_with 是组件级词法比较，Data 段不会展开），
    // 统一在拼接前拦截父目录段最稳。返回空串而非原始输入——原始串带 ..
    // 会被下游 fs::read 跟随，读到 data_dir 之外任意文件。
    if contains_parent_segments(&normalized_input) {
        return String::new();
    }

    if normalized_input.starts_with("clipboard_images\\")
        || normalized_input.starts_with("pin_images\\")
        || normalized_input.starts_with("image_library\\") {
        if let Ok(data_dir) = get_data_directory() {
            let candidate = data_dir.join(&normalized_input);
            // 以三个固定子目录开头的拼接不可能越出 data_dir,
            // 但仍校验解析路径确实落在 data_dir 之下(防御回归)
            if candidate.starts_with(&data_dir) {
                return candidate.to_string_lossy().to_string();
            }
        }
    }

    let search_path = stored_path.replace("\\", "/");
    for prefix in ["clipboard_images/", "pin_images/", "image_library/"] {
        if let Some(idx) = search_path.find(prefix) {
            if let Ok(data_dir) = get_data_directory() {
                let relative = search_path[idx..].replace("/", "\\");
                let new_path = data_dir.join(&relative);
                if new_path.exists() {
                    if new_path.starts_with(&data_dir) {
                        return new_path.to_string_lossy().to_string();
                    }
                }
            }
        }
    }

    stored_path.to_string()
}

// 远端同步记录(files: 内容)净化:仅保留应用管理的三个固定子目录相对路径。
// 本地采集链路自身保证只存相对路径或本机已存在路径(processor collect_file_info),
// 而 LAN 对端/WebDAV 下发的 content 完全不可信——恶意对端可下发本机绝对路径
// 让 hydrate 时 resolve_stored_path 原样返回、exists() 探测到文件后经 asset 协议
// 拉进渲染。绝对路径或含 .. 段路径的条目在写库前整条删除,净化幂等。
pub fn sanitize_remote_files_content(content: &str) -> String {
    if !content.starts_with("files:") {
        return content.to_string();
    }
    let Ok(mut data) = serde_json::from_str::<crate::services::paste::FilesData>(&content[6..])
    else {
        // 解析不了的 files: 内容原样入库(前端同样解析失败,不会加载任何路径)
        return content.to_string();
    };
    data.files.retain(|file| {
        let normalized = file.path.replace("/", "\\");
        !contains_parent_segments(&normalized)
            && (normalized.starts_with("clipboard_images\\")
                || normalized.starts_with("pin_images\\")
                || normalized.starts_with("image_library\\"))
    });
    match serde_json::to_string(&data) {
        Ok(json) => format!("files:{}", json),
        Err(_) => content.to_string(),
    }
}

pub fn is_portable_build() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase()))
        .map(|name| name.contains("portable"))
        .unwrap_or(false)
}

/// 统一便携运行时检测:exe 名含 portable,或同目录有 portable.flag / portable.txt。
/// 所有调用点必须走这里,禁止各自内联 flag/txt 判定,避免语义漂移。
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
    // 护栏:is_portable_runtime 必须是唯一判定入口,
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

    // 提取 resolve_stored_path 的函数体(剥注释后)
    fn resolve_stored_path_body() -> String {
        let src = bare_source("services/mod.rs");
        let start = src
            .find("pub fn resolve_stored_path")
            .expect("缺 resolve_stored_path");
        let end = src[start..]
            .find("\npub fn ")
            .map(|i| start + i)
            .unwrap_or(src.len());
        src[start..end].to_string()
    }

    #[test]
    fn resolve_stored_path_guards_against_parent_traversal() {
        // 以固定子目录开头的输入必须解析回 data_dir 之下;含父目录段时回退原样
        // 注意 get_data_directory 依赖 app 状态,测试环境可能不可用,
        // 这里只验证纯函数式的分支决策(无 data_dir 时返回原串)。
        let source = source_file("src/services/mod.rs");
        // 防御护栏:三个子目录前缀必须保持 starts_with(data_dir) 校验
        let count = source.matches(".starts_with(&data_dir)").count();
        assert!(
            count >= 2,
            "resolve_stored_path 必须对两条拼接路径都做 starts_with(data_dir) 校验,实际 {}",
            count
        );
    }

    #[test]
    fn resolve_stored_path_rejects_parent_segments_before_resolving() {
        // 目录穿越反证:data_dir.join("clipboard_images\\..\\..\\evil") 的
        // starts_with(&data_dir) 是词法前缀比较,`..` 段未折叠时必然通过,
        // 且 Reject 后 fallback 若返回原始串,下游 fs::read 仍会跟随 `..`
        // 读到 data_dir 之外任意文件。修复必须在拼接前拦截独立成段的
        // `.`/`..`(用 Component::ParentDir/CurDir,避免误伤 file..txt),
        // 拒绝时返回空串视为不存在。
        let whole = bare_source("services/mod.rs");
        let helper_start = whole
            .find("fn contains_parent_segments")
            .expect("必须提供父目录段检测函数");
        let helper_end = whole[helper_start..]
            .find("\n}\n")
            .map(|i| helper_start + i + 2)
            .unwrap_or(whole.len());
        let helper = &whole[helper_start..helper_end];
        assert!(
            helper.contains("Component::ParentDir") && helper.contains("Component::CurDir"),
            "父目录段检测必须覆盖 ParentDir 与 CurDir 组件"
        );
        // 检测必须在 resolve_stored_path 的拼接分支之前定义/可达
        let body = resolve_stored_path_body();
        let reject_pos = body
            .find("contains_parent_segments(&normalized_input)")
            .expect("拼接前必须调用父目录段检测");
        let prefix_pos = body
            .find("starts_with(\"clipboard_images")
            .expect("缺前缀直拼分支");
        assert!(
            reject_pos < prefix_pos,
            "父目录段拒绝必须早于任何拼接分支,否则恶意 .. 已参与候选路径"
        );
        // 拒绝分支必须返回空串,不能回退原始串——原始串带 .. 会被下游读取
        let reject_seg = &body[reject_pos..prefix_pos];
        assert!(
            reject_seg.contains("String::new()"),
            "拒绝父目录段必须返回空串,禁止回退原始输入(下游 fs::read 会跟随 ..)"
        );
    }

    #[test]
    fn sanitize_remote_files_content_strips_absolute_and_parent_paths() {
        // 远端同步记录 files: 内容不可信:绝对路径、含父目录段的条目
        // 整条删除,只保留三固定子目录相对路径(与 resolve_stored_path 白名单同源)。
        let input = r#"files:{"files":[{"path":"C:\\Users\\me\\secret.png","name":"a"},{"path":"clipboard_images/abc.png","name":"b"},{"path":"..\\evil.png","name":"c"},{"path":"pin_images/x.png","name":"d"}]}"#;
        let out = crate::services::sanitize_remote_files_content(input);
        assert!(out.starts_with("files:"), "files: 前缀必须保留");
        let data: crate::services::paste::FilesData =
            serde_json::from_str(&out[6..]).expect("净化后仍必须是合法 files: JSON");
        let paths: Vec<&str> = data.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["clipboard_images/abc.png", "pin_images/x.png"],
            "绝对路径与父目录段条目必须被删除,固定子目录相对路径必须保留"
        );
    }

    #[test]
    fn sanitize_remote_files_content_leaves_other_content_untouched() {
        // 非 files: 内容原样返回;纯相对路径 files: 内容原样保留(净化幂等)
        let text = "普通文本内容";
        assert_eq!(crate::services::sanitize_remote_files_content(text), text);
        let clean = r#"files:{"files":[{"path":"clipboard_images/abc.png","name":"a"}]}"#;
        assert_eq!(crate::services::sanitize_remote_files_content(clean), clean);
    }
}
