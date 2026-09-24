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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("QuickClipboard 重构壳");
            ui.label("阶段 0.5 骨架：gui 壳 + core 壳 + workspace + CI 改造已就位");
            ui.label(format!(
                "core 连接: {}",
                if quickclipboard_core::placeholder_exists() {
                    "OK"
                } else {
                    "MISSING"
                }
            ));
        });
    }
}
