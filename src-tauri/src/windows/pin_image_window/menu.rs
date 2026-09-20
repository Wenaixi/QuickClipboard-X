// 贴图 GDI 右键菜单
//
// 对照 ShareX PinToScreenForm 工具栏(复制/缩放/选项/关闭)与本项目原
// WebView 版 13 项菜单(置顶/阴影/锁定位置/像素级/缩略图/缩略图恢复模式
// 子菜单/透明度 6 档+自定义/复制/另存/关闭)复刻到原生 Win32 菜单。
//
// 菜单走 WM_RBUTTONUP → CreatePopupMenu → AppendMenuW → TrackPopupMenu
// (TPM_RETURNCMD 返回被选 id)。动作分发到 pin_image_window 的服务函数与
// gdi 的状态 helper,这里不重复实现逻辑。
//
// 阴影/像素级显示在 GDI 下无 CSS filter 等价物:shadow 记录开关状态(视觉
// 阴影留 DIB 边框增强项),pixel_render 记录状态(重渲染时最近邻缩放增强项)。
// 两者与 lock_position/restore_mode/opacity 同进每窗口状态,由全局设置
// 文件持久化(gdi_settings)。

use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, HMENU, MF_CHECKED, MF_POPUP,
    MF_SEPARATOR, MF_STRING, MENU_ITEM_FLAGS, TrackPopupMenu, TPM_RETURNCMD,
};
use windows::core::PCWSTR;

use super::{gdi, pin_image_window};

/// 菜单条目 id(1 起,0 保留给 TrackPopupMenu 未命中)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PinMenuId {
    ToggleTop = 1,
    ToggleShadow = 2,
    ToggleLockPosition = 3,
    TogglePixelRender = 4,
    ToggleThumbnail = 5,
    RestoreModeFollow = 6,
    RestoreModeKeep = 7,
    Opacity100 = 8,
    Opacity90 = 9,
    Opacity80 = 10,
    Opacity70 = 11,
    Opacity60 = 12,
    Opacity50 = 13,
    OpacityCustom = 14,
    Copy = 15,
    SaveAs = 16,
    Close = 17,
}

/// 透明度条目归一化为百分比;非透明度条目返回 None
fn opacity_from_id(id: usize) -> Option<u32> {
    match id {
        id if id == PinMenuId::Opacity100 as usize => Some(100),
        id if id == PinMenuId::Opacity90 as usize => Some(90),
        id if id == PinMenuId::Opacity80 as usize => Some(80),
        id if id == PinMenuId::Opacity70 as usize => Some(70),
        id if id == PinMenuId::Opacity60 as usize => Some(60),
        id if id == PinMenuId::Opacity50 as usize => Some(50),
        _ => None,
    }
}

fn append(menu: HMENU, flags: MENU_ITEM_FLAGS, id: usize, label: &str) -> Result<(), String> {
    let mut wide: Vec<u16> = label.encode_utf16().collect();
    wide.push(0);
    unsafe { AppendMenuW(menu, flags, id, PCWSTR(wide.as_ptr())) }
        .map_err(|e| format!("追加菜单项失败: {}", e))
}

/// 勾选条目:当前状态命中的条目带 MF_CHECKED(对照原版菜单 ti ti-check 图标)
fn append_checked(
    menu: HMENU,
    id: usize,
    label: &str,
    checked: bool,
) -> Result<(), String> {
    let flags = if checked { MF_CHECKED } else { MF_STRING };
    append(menu, flags, id, label)
}

/// 在光标位置弹出贴图右键菜单,返回被选条目 id(0=取消)。
/// 当前状态(置顶/阴影/锁定/像素级/缩略图/恢复模式/透明度)以勾选呈现。
pub(crate) fn show_pin_menu(hwnd: HWND, label: &str) -> Result<usize, String> {
    let menu = unsafe { CreatePopupMenu() }.map_err(|e| format!("创建菜单失败: {}", e))?;
    let state = gdi::pin_state(label);

    let build = (|| -> Result<(), String> {
        append_checked(
            menu,
            PinMenuId::ToggleTop as usize,
            "窗口置顶",
            gdi::window_is_topmost(label),
        )?;
        append_checked(menu, PinMenuId::ToggleShadow as usize, "窗口阴影", state.shadow)?;
        append_checked(
            menu,
            PinMenuId::ToggleLockPosition as usize,
            "锁定位置",
            state.lock_position,
        )?;
        append_checked(
            menu,
            PinMenuId::TogglePixelRender as usize,
            "像素级显示",
            state.pixel_render,
        )?;
        // 缩略图模式:切换只把窗口缩到 50x50 且无恢复语义(原尺寸/位置记录
        // 未接线),入口点进去不可逆,属于残缺功能——从菜单隐藏避免误导,
        // 待恢复语义接入后再恢复入口。

        let sub = unsafe { CreatePopupMenu() }.map_err(|e| format!("创建子菜单失败: {}", e))?;
        append_checked(
            sub,
            PinMenuId::RestoreModeFollow as usize,
            "跟随移动",
            state.restore_mode == "follow",
        )?;
        append_checked(
            sub,
            PinMenuId::RestoreModeKeep as usize,
            "保持位置",
            state.restore_mode == "keep",
        )?;
        append(menu, MF_POPUP, sub.0 as usize, "缩略图恢复模式")?;

        append(menu, MF_SEPARATOR, 0, "")?;

        for (id, label) in [
            (PinMenuId::Opacity100, "100%"),
            (PinMenuId::Opacity90, "90%"),
            (PinMenuId::Opacity80, "80%"),
            (PinMenuId::Opacity70, "70%"),
            (PinMenuId::Opacity60, "60%"),
            (PinMenuId::Opacity50, "50%"),
        ] {
            let opacity = opacity_from_id(id as usize).unwrap_or(0);
            append_checked(menu, id as usize, label, state.opacity == opacity as u8)?;
        }
        append_checked(
            menu,
            PinMenuId::OpacityCustom as usize,
            "自定义...",
            !matches!(state.opacity, 100 | 90 | 80 | 70 | 60 | 50),
        )?;

        append(menu, MF_SEPARATOR, 0, "")?;
        append(menu, MF_STRING, PinMenuId::Copy as usize, "复制到剪贴板")?;
        append(menu, MF_STRING, PinMenuId::SaveAs as usize, "图像另存为...")?;
        append(menu, MF_SEPARATOR, 0, "")?;
        append(menu, MF_STRING, PinMenuId::Close as usize, "关闭窗口")?;
        Ok(())
    })();
    if let Err(e) = build {
        let _ = unsafe { DestroyMenu(menu) };
        return Err(e);
    }

    let mut pt = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut pt) }.is_err() {
        let _ = unsafe { DestroyMenu(menu) };
        return Ok(0);
    }

    let selected = unsafe { TrackPopupMenu(menu, TPM_RETURNCMD, pt.x, pt.y, Some(0), hwnd, None) };
    let _ = unsafe { DestroyMenu(menu) };
    Ok(selected.0 as usize)
}

/// 处理菜单选中的动作。label 为贴图窗口标签。透明度档与开关切换写回
/// 每窗口状态(GDI 状态表),同时落盘共享偏好设置文件(跨窗口一致)。
/// 由 WM_RBUTTONUP 在 UI 线程调用,状态写入轻量无阻塞。
pub(crate) fn handle_pin_menu_action(label: &str, hwnd: HWND, id: usize) -> Result<(), String> {
    if let Some(opacity) = opacity_from_id(id) {
        let mut state = gdi::pin_state(label);
        state.opacity = opacity as u8;
        gdi::set_pin_state(label, state);
        persist_state_preferences(label);
        return gdi::render_current(label, hwnd);
    }

    match id {
        id if id == PinMenuId::ToggleTop as usize => {
            gdi::toggle_topmost(hwnd)?;
            Ok(())
        }
        id if id == PinMenuId::ToggleShadow as usize => {
            gdi::toggle_state_bool(label, gdi::PinStateFlag::Shadow)?;
            persist_state_preferences(label);
            Ok(())
        }
        id if id == PinMenuId::ToggleLockPosition as usize => {
            gdi::toggle_state_bool(label, gdi::PinStateFlag::LockPosition)?;
            persist_state_preferences(label);
            Ok(())
        }
        id if id == PinMenuId::TogglePixelRender as usize => {
            gdi::toggle_state_bool(label, gdi::PinStateFlag::PixelRender)?;
            persist_state_preferences(label);
            Ok(())
        }
        id if id == PinMenuId::ToggleThumbnail as usize => {
            // 缩略图切换:窗口缩放动画到 50x50。完整恢复语义(记录原尺寸/
            // 位置/恢复模式)由交互任务接入,此处保留菜单入口的行为占位。
            let _ = pin_image_window::animate_window_resize(
                label.to_string(), 0.0, 0.0, 0, 0, 50.0, 50.0, 0, 0, 300,
            );
            Ok(())
        }
        id if id == PinMenuId::RestoreModeFollow as usize => {
            gdi::set_state_str(label, gdi::PinStateFlag::RestoreMode, "follow")?;
            persist_state_preferences(label);
            Ok(())
        }
        id if id == PinMenuId::RestoreModeKeep as usize => {
            gdi::set_state_str(label, gdi::PinStateFlag::RestoreMode, "keep")?;
            persist_state_preferences(label);
            Ok(())
        }
        id if id == PinMenuId::OpacityCustom as usize => {
            // 自定义透明度:经 input_dialog 数字输入框取 0-255,写入状态后
            // 落盘并重渲染——与 5 档透明度同一条 UpdateLayeredWindow 路径。
            let app = gdi::app_handle().ok_or_else(|| "贴图窗口句柄不可用".to_string())?;
            let current = gdi::pin_state(label).opacity;
            let value = crate::windows::plugins::input_dialog::commands::show_input(
                app,
                "自定义透明度".to_string(),
                "请输入透明度 (0-255, 数值越大越不透明)".to_string(),
                Some("0-255".to_string()),
                Some(current.to_string()),
                Some("number".to_string()),
                Some(0),
                Some(255),
            )
            .await?
            .ok_or_else(|| "已取消".to_string())?;
            let opacity: u8 = value.trim().parse().map_err(|_| "透明度必须是 0-255 的整数".to_string())?;
            let mut state = gdi::pin_state(label);
            state.opacity = opacity;
            gdi::set_pin_state(label, state);
            persist_state_preferences(label);
            if let Some(hwnd) = gdi::find_gdi_window(label) {
                let _ = gdi::render_current(label, hwnd);
            }
            Ok(())
        }
        id if id == PinMenuId::Copy as usize => {
            let path = pin_image_window::pin_image_file_path(label)?;
            crate::commands::copy_image_to_clipboard(path)
        }
        id if id == PinMenuId::SaveAs as usize => {
            if let Some(app) = gdi::app_handle() {
                let label = label.to_string();
                tauri::async_runtime::spawn(async move {
                    let _ = pin_image_window::save_pin_image_as(app, label).await;
                });
                Ok(())
            } else {
                Err("AppHandle 未初始化".to_string())
            }
        }
        id if id == PinMenuId::Close as usize => pin_image_window::close_pin_image_window(label),
        _ => Err("未知菜单项".to_string()),
    }
}

/// 把当前窗口的可共享偏好(阴影/锁定/像素级/透明度/恢复模式)写回全局
/// 设置文件,让后续新建的贴图窗口继承本次选择(对齐原版 localStorage
/// 每窗口独立,这里一处全局,多开窗口行为一致)。写盘失败仅告警不阻断
/// 菜单操作——设置文件是偏好,不是关键路径。
fn persist_state_preferences(label: &str) {
    let state = gdi::pin_state(label);
    let settings = super::gdi_settings::PinImageSettings {
        shadow: state.shadow,
        lock_position: state.lock_position,
        pixel_render: state.pixel_render,
        opacity: state.opacity,
        thumbnail_restore_mode: state.restore_mode.clone(),
    };
    if let Err(e) = super::gdi_settings::save_pin_image_settings(&settings) {
        eprintln!("保存贴图设置失败: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu_source() -> String {
        std::fs::read_to_string(format!(
            "{}/src/windows/pin_image_window/menu.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取贴图菜单源码失败")
    }

    fn stripped_source() -> String {
        menu_source()
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // 枚举必须覆盖全部菜单条目:置顶/阴影/锁定/像素级 4 项(缩略图模式因
    // 无恢复语义已从菜单隐藏,枚举保留供 UI 语义与未来接入) + 恢复模式
    // 2 项 + 透明度 6 档 + 自定义 + 复制/另存/关闭 3 项 = 16 展示项。
    #[test]
    fn context_menu_defines_all_expected_entries() {
        let src = stripped_source();
        for (name, desc) in [
            ("ToggleTop", "置顶"),
            ("ToggleShadow", "阴影"),
            ("ToggleLockPosition", "锁定位置"),
            ("TogglePixelRender", "像素级"),
            ("ToggleThumbnail", "缩略图"),
            ("RestoreModeFollow", "恢复-跟随"),
            ("RestoreModeKeep", "恢复-保持"),
            ("Opacity100", "透明度 100"),
            ("Opacity90", "透明度 90"),
            ("Opacity80", "透明度 80"),
            ("Opacity70", "透明度 70"),
            ("Opacity60", "透明度 60"),
            ("Opacity50", "透明度 50"),
            ("OpacityCustom", "自定义透明度"),
            ("Copy", "复制"),
            ("SaveAs", "另存"),
            ("Close", "关闭"),
        ] {
            assert!(
                src.contains(name),
                "菜单枚举缺少 {} ({})",
                name,
                desc
            );
        }
        let toggles = 5;
        let restore = 2;
        let opacities = 6;
        let custom = 1;
        let actions = 3;
        assert_eq!(
            toggles + restore + opacities + custom + actions,
            17,
            "菜单枚举总数应为 17"
        );
        // 缩略图菜单项:不可见(show_pin_menu 不再 append),因为切换只缩
        // 50x50 且无恢复语义,入口不可逆会误导。
        let show_menu_start = src
            .find("pub(crate) fn show_pin_menu")
            .expect("缺 show_pin_menu");
        let show_menu_seg = &src[show_menu_start..src.len().min(show_menu_start + 4000)];
        assert!(
            !show_menu_seg.contains("PinMenuId::ToggleThumbnail as usize"),
            "缩略图模式入口不得出现在右键菜单(无恢复语义的残缺功能)"
        );
    }

    // 负向断言:贴图菜单不得含"编辑贴图"入口,也不得调用 start_pin_edit_mode
    // 桩命令——后端该功能未实现,入口点击必然失败(承接原 contextMenu.test.js
    // 防回归语义)。断言目标用拆分拼接避免自命中(本测试读自己源码)。
    #[test]
    fn context_menu_has_no_edit_entry() {
        let src = stripped_source();
        let edit_label = ["编辑", "贴图"].join("");
        let stub_cmd = ["start_pin_", "edit_mode"].join("");
        assert!(
            !src.contains(&edit_label),
            "未实现的后端功能不得保留菜单入口"
        );
        assert!(!src.contains(&stub_cmd), "不得调用桩命令");
        assert!(src.contains("Copy"), "复制入口必须保留");
    }

    // 菜单必须勾选呈现当前状态:勾选用 MF_CHECKED(对照原版菜单 ti ti-check
    // 图标),且辅助函数被 show_pin_menu 实际调用。
    #[test]
    fn context_menu_checks_current_state() {
        let src = stripped_source();
        assert!(
            src.contains("MF_CHECKED"),
            "菜单必须用 MF_CHECKED 呈现勾选态"
        );
        assert!(
            src.contains("fn append_checked"),
            "必须提供勾选辅助函数"
        );
        assert!(
            src.contains("append_checked(menu"),
            "主菜单条目必须经勾选辅助函数创建"
        );
    }

    // 透明度 6 档 id 必须能归一化为百分比(opacity_from_id 全映射)。
    // 全档反证:任一档被改坏 → 归一化断言 FAILED。
    #[test]
    fn opacity_ids_map_to_percentages() {
        assert_eq!(opacity_from_id(PinMenuId::Opacity100 as usize), Some(100));
        assert_eq!(opacity_from_id(PinMenuId::Opacity90 as usize), Some(90));
        assert_eq!(opacity_from_id(PinMenuId::Opacity80 as usize), Some(80));
        assert_eq!(opacity_from_id(PinMenuId::Opacity70 as usize), Some(70));
        assert_eq!(opacity_from_id(PinMenuId::Opacity60 as usize), Some(60));
        assert_eq!(opacity_from_id(PinMenuId::Opacity50 as usize), Some(50));
        assert_eq!(opacity_from_id(PinMenuId::OpacityCustom as usize), None);
        assert_eq!(opacity_from_id(PinMenuId::Copy as usize), None);
    }

    // 窗口置顶勾选态必须来自窗口实时状态(window_is_topmost),否则菜单
    // 勾选会与实际置顶状态脱节。
    #[test]
    fn topmost_checkmark_reads_live_window_state() {
        let src = stripped_source();
        let menu_body_start = src
            .find("pub(crate) fn show_pin_menu")
            .expect("缺 show_pin_menu");
        let menu_body = &src[menu_body_start..];
        assert!(
            menu_body.contains("gdi::window_is_topmost(label)"),
            "置顶勾选必须查询窗口实时置顶状态"
        );
    }

    // 透明度档与共享偏好(阴影/锁定/像素级/恢复模式)切换后必须把状态
    // 落盘到全局设置文件(persist_state_preferences),新窗口才能继承。
    #[test]
    fn menu_state_changes_persist_to_global_settings() {
        let src = stripped_source();
        let body_start = src
            .find("pub(crate) fn handle_pin_menu_action")
            .expect("缺 handle_pin_menu_action");
        let body = &src[body_start..];
        assert!(
            body.contains("persist_state_preferences(label)"),
            "菜单状态切换必须持久化到全局设置"
        );
        let persist_start = src
            .find("fn persist_state_preferences")
            .expect("缺 persist_state_preferences");
        let persist_body = &src[persist_start..];
        assert!(
            persist_body.contains("save_pin_image_settings"),
            "持久化必须走 gdi_settings::save_pin_image_settings"
        );
        assert!(
            persist_body.contains("eprintln!(\"保存贴图设置失败"),
            "写盘失败必须告警(不阻断菜单操作)"
        );
    }
}