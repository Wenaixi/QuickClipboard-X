#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use quickclipboard_core::services::database::{init_database, query_clipboard_items, ClipboardItem, QueryParams};
use quickclipboard_core::services::settings::{get_data_directory, get_settings};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([360.0, 520.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top(),
        ..Default::default()
    };
    let _ = init_runtime();
    eframe::run_native(
        "QuickClipboard",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(RefactorShell::default()))
        }),
    )
}

/// 启动时初始化运行时数据目录与数据库（纯 Rust 壳，不依赖 Tauri）。
fn init_runtime() -> Result<(), String> {
    let data_dir = get_data_directory()?;
    let db_path = data_dir.join("quickclipboard.db");
    init_database(db_path.to_str().ok_or("数据库路径转换失败")?)?;
    Ok(())
}

struct RefactorShell {
    history_limit: i64,
    items: Vec<ClipboardItem>,
}

impl RefactorShell {
    fn load_history(&mut self) {
        match query_clipboard_items(QueryParams {
            offset: 0,
            limit: 50,
            search: None,
            content_type: None,
            paste_status: None,
        }) {
            Ok(result) => self.items = result.items,
            Err(e) => eprintln!("加载剪贴板历史失败: {}", e),
        }
    }
}

impl Default for RefactorShell {
    fn default() -> Self {
        Self {
            history_limit: get_settings().history_limit,
            items: Vec::new(),
        }
    }
}

impl eframe::App for RefactorShell {
    // eframe 0.34 起 App trait 拆为 logic + ui 两方法，update 不再要求实现
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("QuickClipboard 重构壳");
        if ui.button("刷新历史").clicked() {
            self.load_history();
        }
        ui.label(format!("历史上限: {}", self.history_limit));
        ui.label(format!("历史条数: {}", self.items.len()));
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for item in &self.items {
                    let preview = item.content.chars().take(40).collect::<String>();
                    let preview = if preview.is_empty() { "(空内容)" } else { &preview };
                    ui.label(format!(
                        "[{:>3}] {}",
                        item.content_type.split(',').next().unwrap_or("?"),
                        preview
                    ));
                }
            });
        let _ = quickclipboard_core::events::drain();
    }
}