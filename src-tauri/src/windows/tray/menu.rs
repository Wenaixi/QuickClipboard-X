use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use crate::windows::plugins::context_menu::window::{
    ContextMenuRequest,
    MenuAppearance,
    MenuBehavior,
    MenuButton as CtxMenuButton,
    MenuItem as CtxMenuItem,
    MenuPlacement,
    show_menu,
};
use crate::utils::app_links;

fn get_pin_images_dir() -> Result<PathBuf, String> {
    let data_dir = crate::services::get_data_directory()?;
    Ok(data_dir.join("pin_images"))
}

pub fn get_pin_images_list() -> Vec<(String, String)> {
    let pin_dir = match get_pin_images_dir() {
        Ok(dir) => dir,
        Err(_) => return vec![],
    };
    
    if !pin_dir.exists() {
        return vec![];
    }
    
    let mut images: Vec<(String, String, std::time::SystemTime)> = Vec::new();
    let image_extensions = ["png", "jpg", "jpeg", "gif", "bmp", "webp", "ico"];
    
    if let Ok(entries) = fs::read_dir(&pin_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if image_extensions.contains(&ext.to_lowercase().as_str()) {
                        let file_name = path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("未知")
                            .to_string();
                        let file_path = path.to_string_lossy().to_string();
                        let modified = entry.metadata()
                            .and_then(|m| m.modified())
                            .unwrap_or(std::time::UNIX_EPOCH);
                        images.push((file_name, file_path, modified));
                    }
                }
            }
        }
    }

    images.sort_by(|a, b| b.2.cmp(&a.2));
    images.into_iter().map(|(name, path, _)| (name, path)).collect()
}

const MAX_PIN_IMAGES_DISPLAY: usize = 20;

// 创建分隔线菜单项
fn separator_item() -> CtxMenuItem {
    CtxMenuItem::separator()
}

// 创建普通菜单项
fn menu_item(id: &str, label: &str, icon: Option<&str>) -> CtxMenuItem {
    menu_item_with_state(id, label, icon, false)
}

fn menu_item_with_state(id: &str, label: &str, icon: Option<&str>, disabled: bool) -> CtxMenuItem {
    CtxMenuItem::item(id, label, icon).with_disabled(disabled)
}

// 构建贴图子菜单项列表
fn build_pin_images_children() -> Vec<CtxMenuItem> {
    let images = get_pin_images_list();
    let total_count = images.len();
    let mut children = Vec::new();
    
    if images.is_empty() {
        children.push(menu_item_with_state("empty", "(暂无贴图)", None, true));
    } else {
        for (idx, (name, path)) in images.iter().take(MAX_PIN_IMAGES_DISPLAY).enumerate() {
            // 按字符截断而非按字节切片——&name[..27] 在中文/emoji 等
            // 多字节字符落在边界中间时直接 panic 崩进程。
            let display_name = if name.chars().count() > 30 {
                format!("{}...", name.chars().take(27).collect::<String>())
            } else {
                name.clone()
            };
            children.push(
                // 菜单项 id 用文件名而非排序下标——目录在菜单打开与点击之间
                // 变化时(idx 漂移),下标会指向另一张图;文件名目录内唯一且稳定。
                CtxMenuItem::item(format!("pin-image-{}", name), display_name, Some("ti ti-photo"))
                    .with_preview_image(path.clone()),
            );
        }
        
        if total_count > MAX_PIN_IMAGES_DISPLAY {
            children.push(separator_item());
            children.push(menu_item(
                "pin-open-folder",
                &format!("更多... (共{}张)", total_count),
                Some("ti ti-dots"),
            ));
        }
    }
    
    children.push(separator_item());
    children.push(menu_item("pin-open-folder", "打开贴图目录", Some("ti ti-folder")));
    
    children
}

// 托盘菜单
pub async fn show_tray_menu(app: AppHandle) -> Result<(), String> {
    let settings = crate::get_settings();
    let is_force_update = crate::windows::updater_window::is_force_update_mode();
    
    let hotkeys_label = if settings.hotkeys_enabled { "禁用快捷键" } else { "启用快捷键" };
    let monitor_label = if settings.clipboard_monitor { "禁用剪贴板监听" } else { "启用剪贴板监听" };
    let recording_label = if crate::services::recording::current_session().is_some() {
        "停止屏幕录制"
    } else {
        "开始屏幕录制"
    };

    let mut items = vec![
        menu_item_with_state("toggle", "显示/隐藏", Some("ti ti-app-window"), is_force_update),
        separator_item(),
        menu_item_with_state("settings", "设置", Some("ti ti-settings"), is_force_update),
        CtxMenuItem::submenu(
            "pin-images",
            "贴图",
            Some("ti ti-pinned"),
            build_pin_images_children(),
        )
        .with_disabled(is_force_update),
        separator_item(),
        CtxMenuItem::submenu(
            "file-hub",
            "文件中转",
            Some("ti ti-transfer"),
            vec![
                menu_item("transfer-shelf", "新建文件盒", Some("ti ti-package")),
                menu_item("receive-box", "打开收件盒", Some("ti ti-inbox")),
            ],
        )
        .with_disabled(is_force_update),
        separator_item(),
        #[cfg(target_os = "windows")]
        menu_item_with_state("recording", recording_label, Some("ti ti-video"), is_force_update),
        separator_item(),
        menu_item_with_state("toggle-hotkeys", hotkeys_label, Some("ti ti-keyboard"), is_force_update),
        menu_item_with_state("toggle-clipboard-monitor", monitor_label, Some("ti ti-clipboard"), is_force_update),
        separator_item(),
        menu_item_with_state("low-memory-mode", "进入低占用模式", Some("ti ti-leaf"), is_force_update),
        separator_item(),
        CtxMenuItem::button_row(
            "tray-links",
            "",
            vec![
                CtxMenuButton::new("open-website", "官网").with_icon("ti ti-world"),
                CtxMenuButton::new("open-github", "GitHub").with_icon("ti ti-brand-github"),
                CtxMenuButton::new("open-qq-group", "社区交流").with_icon("ti ti-users"),
            ],
        ),
        separator_item(),
        menu_item("restart", "重启程序", Some("ti ti-refresh")),
        menu_item("quit", "退出", Some("ti ti-power")),
    ];

    #[cfg(target_os = "windows")]
    items.insert(3, menu_item_with_state(
        "screenshot",
        "截屏",
        Some("ti ti-screenshot"),
        is_force_update || !settings.screenshot_enabled,
    ));

    let (cursor_x, cursor_y) = crate::mouse::get_cursor_position();
    let theme = if settings.theme.is_empty() { "auto".to_string() } else { settings.theme };
    
    let appearance = MenuAppearance {
        theme: Some(theme),
        light_theme_style: Some(settings.light_theme_style),
        dark_theme_style: Some(settings.dark_theme_style),
        ui_animation_enabled: Some(settings.ui_animation_enabled),
        custom_font_enabled: Some(settings.custom_font_enabled),
        custom_font_type: Some(settings.custom_font_type),
        custom_font_path: Some(settings.custom_font_path),
        custom_font_url: Some(settings.custom_font_url),
        custom_font_family: Some(settings.custom_font_family),
    };

    let options = ContextMenuRequest::new(items)
        .with_placement(MenuPlacement::physical_point(cursor_x, cursor_y))
        .with_appearance(appearance)
        .with_behavior(MenuBehavior::tray());
    
    if let Ok(Some(selected_id)) = show_menu(app.clone(), options).await {
        handle_tray_menu_selection(&app, &selected_id);
    }
    
    Ok(())
}

// 处理托盘菜单选择
fn handle_tray_menu_selection(app: &AppHandle, selected_id: &str) {
    match selected_id {
        "open-website" => {
            if let Ok(links) = app_links::app_links() {
                let _ = tauri_plugin_opener::open_url(&links.website, None::<&str>);
            }
        }
        "open-github" => {
            if let Ok(links) = app_links::app_links() {
                let _ = tauri_plugin_opener::open_url(&links.github, None::<&str>);
            }
        }
        "open-qq-group" => {
            let _ = crate::windows::community_window::open_community_window(app);
        }
        "transfer-shelf" => {
            if let Err(e) = crate::windows::transfer_shelf::open_or_create_shelf(app) {
                eprintln!("创建文件盒失败: {}", e);
            }
        }
        "receive-box" => {
            if let Err(e) = crate::windows::receive_box::open_receive_box(app) {
                eprintln!("打开收件盒失败: {}", e);
            }
        }
        "toggle" => {
            crate::toggle_main_window_visibility(app);
        }
        "settings" => {
            let _ = crate::windows::settings_window::open_settings_window(app);
        }
        #[cfg(target_os = "windows")]
        "screenshot" => {
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(150));
                if let Err(error) = crate::commands::screenshot::start_screenshot(app) {
                    eprintln!("启动截图窗口失败: {}", error);
                }
            });
        }
        // 录制开始/停止切换（与热键同语义）：有活动会话先停，否则开始。
        #[cfg(target_os = "windows")]
        "recording" => {
            let app = app.clone();
            std::thread::spawn(move || {
                let result = if crate::services::recording::current_session().is_some() {
                    crate::commands::screenshot::stop_screen_recording(app)
                } else {
                    crate::commands::screenshot::start_screen_recording(app)
                };
                if let Err(error) = result {
                    eprintln!("屏幕录制操作失败: {}", error);
                }
            });
        }
        "toggle-hotkeys" => {
            toggle_hotkeys(app);
        }
        "toggle-clipboard-monitor" => {
            if let Err(e) = crate::commands::settings::toggle_clipboard_monitor(app) {
                eprintln!("切换剪贴板监听状态失败: {}", e);
            }
        }
        "low-memory-mode" => {
            if let Err(e) = crate::services::low_memory::enter_low_memory_mode(app) {
                eprintln!("进入低占用模式失败: {}", e);
            }
        }
        "restart" => {
            super::restart_app_gracefully(app);
        }
        "quit" => {
            // 退出前必须标记用户主动退出,否则低占用模式下 ExitRequested
            // 会走 prevent_exit() 保活(is_low_memory_mode && !user_requested),
            // 点退出后进程僵死无任何提示。与 native_menu/handlers.rs 的
            // quit 分支(mark → exit)保持同一口径。
            crate::services::low_memory::set_user_requested_exit(true);
            app.exit(0);
        }
        "pin-open-folder" => {
            open_pin_images_folder();
        }
        id if id.starts_with("pin-image-") => {
            if let Some(file_name) = id.strip_prefix("pin-image-") {
                let images = get_pin_images_list();
                if let Some((_, file_path)) = images
                    .iter()
                    .find(|(name, _)| name == file_name)
                    .cloned()
                {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = crate::windows::pin_image_window::pin_image_from_file(
                            app, file_path, None, None, None, None, None, None, None, None, None, None, None,
                        ).await {
                            eprintln!("创建贴图窗口失败: {}", e);
                        }
                    });
                }
            }
        }
        _ => {}
    }
}

// 切换快捷键状态
fn toggle_hotkeys(app: &AppHandle) {
    let mut settings = crate::get_settings();
    settings.hotkeys_enabled = !settings.hotkeys_enabled;
    let enabled = settings.hotkeys_enabled;
    
    if let Err(e) = crate::update_settings(settings.clone()) {
        eprintln!("更新快捷键设置失败: {}", e);
        return;
    }
    
    if enabled {
        if let Err(e) = crate::hotkey::reload_from_settings() {
            eprintln!("重新加载快捷键失败: {}", e);
        }
    } else {
        crate::hotkey::unregister_all();
    }

    let message = if enabled { "快捷键已启用" } else { "快捷键已禁用" };
    let _ = crate::services::notification::show_notification(app, "QuickClipboard", message);
}

// 打开贴图目录
fn open_pin_images_folder() {
    if let Ok(data_dir) = crate::services::get_data_directory() {
        let pin_images_dir = data_dir.join("pin_images");
        if !pin_images_dir.exists() {
            let _ = std::fs::create_dir_all(&pin_images_dir);
        }
        let _ = tauri_plugin_opener::open_path(&pin_images_dir, None::<&str>);
    }
}

#[cfg(test)]
mod display_name_guard {
    // 托盘字节切片 panic:贴图文件名超过 30 字符时按字符截断
    // (chars().take(27)),不得用 &name[..27] 字节切片——中文/emoji
    // 多字节字符落在边界中间时 String 切片 panic 崩进程。
    #[test]
    fn pin_image_display_name_truncates_by_chars_not_bytes() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/tray/menu.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读 menu.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = stripped
            .find("build_pin_images_children")
            .expect("缺 build_pin_images_children");
        let rest = &stripped[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(stripped.len());
        let body = &stripped[start..end];
        assert!(
            body.contains("name.chars().take(27).collect::<String>()"),
            "截断必须按字符 take(27),禁止字节切片 &name[..27]"
        );
        assert!(
            !body.contains("&name[..27]"),
            "禁止字节下标切片 &name[..27](多字节字符边界 panic)"
        );
    }

    // 托盘贴图项 idx 漂移:菜单项 id 必须用文件名(pin-image-{name})而非
    // 排序下标(pin-image-{idx}),点击处理必须按文件名 find 匹配而非下标 get——
    // 菜单打开与点击之间目录变化(新增/删除/改名)时下标漂移会贴错图。
    // 文件名目录内唯一且稳定,不受排序变化影响。反证:id 改回 idx 下标 → FAILED。
    #[test]
    fn pin_image_menu_uses_file_name_not_sort_index() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/tray/menu.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读 menu.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        // 菜单构建段:item id 用 name(不得用 idx)
        let build_start = stripped
            .find("build_pin_images_children")
            .expect("缺 build_pin_images_children");
        let build_rest = &stripped[build_start..];
        let build_end = build_rest.find("\n}\n").map(|i| build_start + i).unwrap_or(stripped.len());
        let build_body = &stripped[build_start..build_end];
        assert!(
            build_body.contains("format!(\"pin-image-{}\", name)"),
            "菜单项 id 必须用文件名(pin-image-{{name}}),禁止下标 {{idx}} 漂移"
        );
        assert!(
            !build_body.contains("format!(\"pin-image-{}\", idx)"),
            "禁止用排序下标构造 id(pin-image-{{idx}}),目录变化时漂移贴错图"
        );
        // 点击处理段:按文件名 find 匹配,不得用下标 get
        // 锚点用 "fn handle_tray_menu_selection" 而非裸函数名——裸名会命中
        // show_tray_menu 里的调用处(:192),从调用处切到函数结尾拿错函数体。
        let handle_start = stripped
            .find("fn handle_tray_menu_selection")
            .expect("缺 handle_tray_menu_selection");
        let handle_rest = &stripped[handle_start..];
        let handle_end = handle_rest.find("\n}\n").map(|i| handle_start + i).unwrap_or(stripped.len());
        let handle_body = &stripped[handle_start..handle_end];
        assert!(
            handle_body.contains(".find(|(name, _)| name == file_name)"),
            "点击处理必须按文件名 find 匹配,禁止下标 get 漂移"
        );
        assert!(
            !handle_body.contains("images.get(idx)"),
            "禁止用排序下标 get 定位贴图"
        );
    }

    // WebView 托盘菜单 quit 分支必须与 native 菜单同一口径:先标记用户主动
    // 退出再 exit。ExitRequested 在低占用模式且未标记用户主动退出时
    // prevent_exit() 保活——缺 mark 会让「退出」变成僵尸进程。
    #[test]
    fn webview_tray_quit_marks_user_requested_exit_before_exit() {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/tray/menu.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读 menu.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        // 定位 quit 分支:选 quit 标签行到本函数最外层的闭包收尾前的片段。
        let quit_at = stripped.find("\"quit\" => {").expect("缺 quit 分支");
        let quit_body = &stripped[quit_at..];
        let end = quit_body.find("\n        \"pin-open-folder\"").map(|i| quit_at + i).unwrap_or(stripped.len());
        let body = &stripped[quit_at..end];
        assert!(
            body.contains("set_user_requested_exit(true)"),
            "quit 分支必须先标记用户主动退出,否则低占用模式退出变僵尸"
        );
        assert!(
            body.contains("app.exit(0)"),
            "quit 分支必须以 app.exit(0) 结束,不得只标记不退出"
        );
    }
}
