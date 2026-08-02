pub mod main_window;
pub mod settings_window;
pub mod text_editor_window;
pub mod quickpaste;
pub mod tray;
pub mod community_window;
pub mod plugins;
pub mod pin_image_window;
pub mod updater_window;
pub mod preview_window;
pub mod transfer_shelf;
pub mod receive_box;
pub mod drop_proxy;

#[cfg(feature = "gpu-image-viewer")]
pub mod native_pin_window;

#[cfg(test)]
mod poison_recovery_guards {
    // §10.3 源码字面护栏:native_pin_window 在 no-default-features 下不编译,
    // 所以用 fs 读源 + contains 锁死 helper 与无裸 unwrap 不变量。
    // 运行时 poison 恢复测试见 native_pin_window::tests(开 feature 时跑)。

    fn strip_line_comments(src: &str) -> String {
        src.lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // 只取生产代码(#[cfg(test)] 之前),避免测试体内制造 poison 的 .lock().unwrap() 误命中
    fn production_body(src: &str) -> String {
        let cut = src.find("#[cfg(test)]").unwrap_or(src.len());
        strip_line_comments(&src[..cut])
    }

    fn read_src(rel: &str) -> String {
        std::fs::read_to_string(format!("{}/src/{}", env!("CARGO_MANIFEST_DIR"), rel))
            .unwrap_or_else(|e| panic!("找不到源文件 {}: {}", rel, e))
    }

    #[test]
    fn native_pin_window_uses_lock_window_data_helper() {
        let body = production_body(&read_src("windows/native_pin_window/mod.rs"));
        assert!(
            body.contains("fn lock_window_data()"),
            "必须有 lock_window_data helper"
        );
        assert!(
            body.contains("unwrap_or_else(|p| p.into_inner())"),
            "lock_window_data 必须 poison 恢复"
        );
        assert!(
            !body.contains("WINDOW_DATA.lock().unwrap()"),
            "不得再有 WINDOW_DATA.lock().unwrap() 裸调用"
        );
    }

    #[test]
    fn pin_image_window_uses_lock_pin_data_helper() {
        let body = production_body(&read_src("windows/pin_image_window/pin_image_window.rs"));
        assert!(
            body.contains("fn lock_pin_data()"),
            "必须有 lock_pin_data helper"
        );
        assert!(
            body.contains("unwrap_or_else(|p| p.into_inner())"),
            "lock_pin_data 必须 poison 恢复"
        );
        assert!(
            !body.contains(".lock().unwrap()"),
            "pin_image_window 不得再有 .lock().unwrap() 裸调用(PIN_IMAGE_DATA_MAP)"
        );
    }
}
