#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([360.0, 520.0])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top(),
        ..Default::default()
    };
    eframe::run_native(
        "QuickClipboard",
        options,
        Box::new(|cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(RefactorShell::default()))
        }),
    )
}

struct RefactorShell;

impl Default for RefactorShell {
    fn default() -> Self {
        Self
    }
}

impl eframe::App for RefactorShell {
    // eframe 0.34 起 App trait 拆为 logic + ui 两方法，update 不再要求实现
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.heading("QuickClipboard 重构壳");
        ui.label("阶段 1：core 剥壳(database/settings/secure_credentials)已迁入");
        ui.label(format!(
            "core 就绪: 历史上限 {}",
            quickclipboard_core::services::settings::get_settings().history_limit
        ));
        // 事件总线接线：core 收尾时在启动主循环注入
        // quickclipboard_core::events::set_repaint_hook(Box::new(move || {
        //     ctx.request_repaint();
        // }));
        let _ = quickclipboard_core::events::drain();
    }
}