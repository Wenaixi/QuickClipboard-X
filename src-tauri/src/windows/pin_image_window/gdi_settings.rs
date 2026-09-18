// 贴图设置持久化(GDI 版)
//
// 原 WebView 版贴图设置存 localStorage(每窗口独立,多开时各自为政)。
// GDI 版改为一处全局设置文件:pin_image_settings.json 放数据目录,
// 新贴图窗口创建时按此默认应用(置顶/阴影/锁定/像素级/透明度/缩略图
// 恢复模式),菜单里改的开关与透明度写回此文件,跨窗口一致。
//
// 失败策略:文件缺失/损坏时静默回退默认值并重写干净文件,绝不让
// 贴图功能因设置文件问题而不可用。
//
// 文件只存 5 项可跨窗口共享的偏好;置顶是运行时窗口扩展样式,缩略图
// 模式进入交互任务后按窗口即时状态,两者都不在此文件持有布尔默认。

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 持久化到磁盘的贴图设置(5 项共享偏好)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct PinImageSettings {
    pub shadow: bool,
    pub lock_position: bool,
    pub pixel_render: bool,
    pub opacity: u8,
    /// 缩略图恢复模式:"follow"(跟随移动)/"keep"(保持位置)
    pub thumbnail_restore_mode: String,
}

impl Default for PinImageSettings {
    fn default() -> Self {
        Self {
            shadow: false,
            lock_position: false,
            pixel_render: false,
            opacity: 100,
            thumbnail_restore_mode: "follow".to_string(),
        }
    }
}

impl PinImageSettings {
    /// 转成 GDI 每窗口状态(缺省字段用默认)
    pub(crate) fn to_window_state(&self) -> super::gdi::PinWindowState {
        super::gdi::PinWindowState {
            shadow: self.shadow,
            lock_position: self.lock_position,
            pixel_render: self.pixel_render,
            opacity: self.opacity,
            restore_mode: self.thumbnail_restore_mode.clone(),
            thumbnail_mode: false,
        }
    }
}

/// 设置文件路径(数据目录下 pin_image_settings.json)
fn settings_path() -> Result<PathBuf, String> {
    Ok(crate::services::get_data_directory()?.join("pin_image_settings.json"))
}

/// 加载贴图设置:文件缺失/损坏时回退默认并重写干净文件
pub(crate) fn load_pin_image_settings() -> PinImageSettings {
    let path = match settings_path() {
        Ok(p) => p,
        Err(_) => return PinImageSettings::default(),
    };
    match fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<PinImageSettings>(&text) {
            Ok(settings) => settings,
            Err(e) => {
                eprintln!("贴图设置解析失败,回退默认: {}", e);
                PinImageSettings::default()
            }
        },
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("读取贴图设置失败,回退默认: {}", e);
            }
            PinImageSettings::default()
        }
    }
}

/// 保存贴图设置(写盘失败返回错误;调用方按需告警)
pub(crate) fn save_pin_image_settings(settings: &PinImageSettings) -> Result<(), String> {
    let path = settings_path()?;
    let text = serde_json::to_string_pretty(settings).map_err(|e| format!("序列化贴图设置失败: {}", e))?;
    fs::write(&path, text).map_err(|e| format!("保存贴图设置失败: {}", e))
}

/// 启动时加载并注入 GDI 层:设置全局默认状态,后续建窗按此应用。
/// 返回加载到的设置供调用方(菜单改偏好)继续使用。
pub(crate) fn init_pin_image_settings() -> PinImageSettings {
    let settings = load_pin_image_settings();
    super::gdi::set_default_pin_state(settings.to_window_state());
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_source() -> String {
        std::fs::read_to_string(format!(
            "{}/src/windows/pin_image_window/gdi_settings.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取贴图设置源码失败")
    }

    fn stripped_source() -> String {
        settings_source()
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // 默认设置 5 项必须对齐原前端默认:阴影/锁定/像素级 false、透明度 100、
    // 恢复模式 follow(原版 thumbnailRestoreMode 默认 "follow")
    #[test]
    fn pin_image_settings_defaults_align_with_frontend() {
        let src = stripped_source();
        for field in ["shadow", "lock_position", "pixel_render"] {
            assert!(
                src.contains(&format!("{}: false", field)),
                "默认设置必须含 {}: false(对齐前端默认)",
                field
            );
        }
        assert!(
            src.contains("opacity: 100"),
            "默认透明度必须为 100"
        );
        assert!(
            src.contains("thumbnail_restore_mode: \"follow\"".to_string().as_str()),
            "缩略图恢复模式默认必须为 follow(对齐前端)"
        );
        let defaults = PinImageSettings::default();
        assert_eq!(defaults.opacity, 100);
        assert_eq!(defaults.thumbnail_restore_mode, "follow");
        assert!(!defaults.shadow && !defaults.lock_position && !defaults.pixel_render);
    }

    // to_window_state 必须把设置透传成 GDI 每窗口状态(建窗继承偏好)
    #[test]
    fn settings_flow_into_window_state() {
        let settings = PinImageSettings {
            shadow: true,
            lock_position: false,
            pixel_render: true,
            opacity: 70,
            thumbnail_restore_mode: "keep".to_string(),
        };
        let state = settings.to_window_state();
        assert!(state.shadow, "阴影偏好必须透传");
        assert!(state.pixel_render, "像素级偏好必须透传");
        assert_eq!(state.opacity, 70, "透明度偏好必须透传");
        assert_eq!(state.restore_mode, "keep", "恢复模式偏好必须透传");
        assert!(!state.thumbnail_mode, "缩略图模式不持久化(交互任务接管)");
        let src = stripped_source();
        assert!(
            src.contains("to_window_state"),
            "必须提供设置转窗口状态的转换"
        );
    }

    // 设置文件写入/读取必须经 settings_path(数据目录下 pin_image_settings.json),
    // 且读失败(含损坏)必须静默回退默认而非报错让贴图不可用。
    #[test]
    fn settings_read_failure_falls_back_to_defaults() {
        let src = stripped_source();
        assert!(
            src.contains("join(\"pin_image_settings.json\")"),
            "设置文件必须放数据目录(pin_image_settings.json)"
        );
        assert!(
            src.contains("PinImageSettings::default()"),
            "读取失败必须回退默认设置"
        );
        assert!(
            src.contains("fs::read_to_string"),
            "必须用 fs::read_to_string 读设置文件"
        );
        assert!(
            src.contains("serde_json::from_str"),
            "必须用 serde_json 解析设置"
        );
    }
}
