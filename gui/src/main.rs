#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use quickclipboard_core::services::clipboard::start_clipboard_monitor;
use quickclipboard_core::services::database::{
    init_database, query_clipboard_items, ClipboardItem, QueryParams,
};
use quickclipboard_core::services::paste::copy_clipboard_item;
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
    // 启动剪贴板监听（core 完全自洽：监视器线程 + 事件总线 + AppSounds 均在 core 内），
    // 新复制内容经 core::events::post → gui 壳每帧 drain 收到 ClipboardUpdated 自动重载。
    if let Err(e) = start_clipboard_monitor() {
        eprintln!("启动剪贴板监听失败: {}", e);
    }
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
    history_limit: u64,
    items: Vec<ClipboardItem>,
    search: String,
    content_filter: String,
}

impl RefactorShell {
    fn load_history(&mut self) {
        let search = if self.search.trim().is_empty() {
            None
        } else {
            Some(self.search.trim().to_string())
        };
        // 内容类型过滤:空串=全部,否则按所选类型(与 content_type 逗号语义一致)
        let content_type = if self.content_filter.is_empty() {
            None
        } else {
            Some(self.content_filter.clone())
        };
        let params = QueryParams {
            offset: 0i64,
            limit: 50i64,
            search,
            content_type,
            paste_status: None,
        };
        match query_clipboard_items(params) {
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
            search: String::new(),
            content_filter: String::new(),
        }
    }
}

impl eframe::App for RefactorShell {
    // eframe 0.34 起 App trait 拆为 logic + ui 两方法，update 不再要求实现
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("QuickClipboard 重构壳");
        ui.horizontal(|ui| {
            let edited = ui.text_edit_singleline(&mut self.search).changed();
            ui.label("类型:");
            let types = ["", "text", "image", "file"];
            let mut changed = false;
            for t in types {
                if ui.selectable_label(self.content_filter == t, if t.is_empty() { "全部" } else { t }).clicked() {
                    self.content_filter = t.to_string();
                    changed = true;
                }
            }
            if ui.button("搜索").clicked() || edited || changed {
                self.load_history();
            }
        });
        ui.horizontal(|ui| {
            if ui.button("刷新历史").clicked() {
                self.load_history();
            }
            ui.label(format!("历史上限: {}", self.history_limit));
            ui.label(format!("历史条数: {}", self.items.len()));
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for item in &self.items {
                    ui.horizontal(|ui| {
                        let preview = item.content.chars().take(40).collect::<String>();
                        let preview = if preview.is_empty() { "(空内容)" } else { &preview };
                        ui.label(format!(
                            "[{:>3}] {}",
                            item.content_type.split(',').next().unwrap_or("?"),
                            preview
                        ));
                        if ui.small_button("复制").clicked() {
                            let clone = item.clone();
                            match copy_clipboard_item(&clone) {
                                Ok(()) => eprintln!("已复制到系统剪贴板: id={}", item.id),
                                Err(e) => eprintln!("复制失败: {}", e),
                            }
                        }
                    });
                }
            });
        // 每帧排空事件总线:收到事件(剪贴板监听器产生)后刷新历史。
        let events = quickclipboard_core::events::drain();
        if !events.is_empty() {
            self.load_history();
        }
    }
}
