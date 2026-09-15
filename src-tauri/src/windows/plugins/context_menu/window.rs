use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, WebviewWindow, WebviewWindowBuilder};

const LABEL: &str = "context-menu";
const INITIAL_WIDTH: f64 = 300.0;
const INITIAL_HEIGHT: f64 = 400.0;

fn default_item_kind() -> String {
    "item".to_string()
}

fn default_anchor() -> String {
    "cursor".to_string()
}

fn default_coordinate_space() -> String {
    "physical".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuButton {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favicon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    #[serde(default)]
    pub disabled: bool,
}

impl MenuButton {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            favicon: None,
            icon_color: None,
            disabled: false,
        }
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuItem {
    #[serde(rename = "type", default = "default_item_kind")]
    pub kind: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favicon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buttons: Option<Vec<MenuButton>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<MenuItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_image: Option<String>,
}

impl MenuItem {
    pub fn item(id: impl Into<String>, label: impl Into<String>, icon: Option<&str>) -> Self {
        Self {
            kind: "item".to_string(),
            id: id.into(),
            label: label.into(),
            icon: icon.map(|s| s.to_string()),
            favicon: None,
            icon_color: None,
            disabled: false,
            buttons: None,
            children: None,
            preview_image: None,
        }
    }

    pub fn separator() -> Self {
        Self {
            kind: "separator".to_string(),
            id: String::new(),
            label: String::new(),
            icon: None,
            favicon: None,
            icon_color: None,
            disabled: false,
            buttons: None,
            children: None,
            preview_image: None,
        }
    }

    pub fn button_row(
        id: impl Into<String>,
        label: impl Into<String>,
        buttons: Vec<MenuButton>,
    ) -> Self {
        Self {
            kind: "button_row".to_string(),
            id: id.into(),
            label: label.into(),
            icon: None,
            favicon: None,
            icon_color: None,
            disabled: false,
            buttons: Some(buttons),
            children: None,
            preview_image: None,
        }
    }

    pub fn submenu(
        id: impl Into<String>,
        label: impl Into<String>,
        icon: Option<&str>,
        children: Vec<MenuItem>,
    ) -> Self {
        Self::item(id, label, icon).with_children(children)
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn with_children(mut self, children: Vec<MenuItem>) -> Self {
        self.children = Some(children);
        self
    }

    pub fn with_preview_image(mut self, preview_image: impl Into<String>) -> Self {
        self.preview_image = Some(preview_image.into());
        self
    }

}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuAppearance {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light_theme_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dark_theme_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_animation_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_font_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_font_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_font_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_font_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_font_family: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuPlacement {
    #[serde(default = "default_anchor")]
    pub anchor: String,
    #[serde(default = "default_coordinate_space")]
    pub coordinate_space: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    #[serde(default)]
    pub cursor: MenuPoint,
}

impl Default for MenuPlacement {
    fn default() -> Self {
        Self::cursor()
    }
}

impl MenuPlacement {
    pub fn cursor() -> Self {
        Self {
            anchor: "cursor".to_string(),
            coordinate_space: "physical".to_string(),
            x: None,
            y: None,
            cursor: MenuPoint::default(),
        }
    }

    pub fn physical_point(x: i32, y: i32) -> Self {
        Self {
            anchor: "point".to_string(),
            coordinate_space: "physical".to_string(),
            x: Some(x),
            y: Some(y),
            cursor: MenuPoint::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuBehavior {
    #[serde(default)]
    pub is_tray_menu: bool,
    #[serde(default)]
    pub force_focus: bool,
}

impl MenuBehavior {
    pub fn tray() -> Self {
        Self {
            is_tray_menu: true,
            force_focus: false,
        }
    }

    pub fn focused() -> Self {
        Self {
            is_tray_menu: false,
            force_focus: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuLayout {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuMonitor {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextMenuRequest {
    pub items: Vec<MenuItem>,
    #[serde(default)]
    pub placement: MenuPlacement,
    #[serde(default)]
    pub appearance: MenuAppearance,
    #[serde(default)]
    pub behavior: MenuBehavior,
    #[serde(default)]
    pub layout: MenuLayout,
    #[serde(default)]
    pub session_id: u64,
    #[serde(default)]
    pub monitor: MenuMonitor,
}

impl ContextMenuRequest {
    pub fn new(items: Vec<MenuItem>) -> Self {
        Self {
            items,
            placement: MenuPlacement::cursor(),
            appearance: MenuAppearance::default(),
            behavior: MenuBehavior::default(),
            layout: MenuLayout::default(),
            session_id: 0,
            monitor: MenuMonitor::default(),
        }
    }

    pub fn with_placement(mut self, placement: MenuPlacement) -> Self {
        self.placement = placement;
        self
    }

    pub fn with_appearance(mut self, appearance: MenuAppearance) -> Self {
        self.appearance = appearance;
        self
    }

    pub fn with_behavior(mut self, behavior: MenuBehavior) -> Self {
        self.behavior = behavior;
        self
    }

    fn apply_monitor_context(&mut self, monitor: MonitorContext) {
        self.placement.cursor = MenuPoint {
            x: monitor.cursor_x,
            y: monitor.cursor_y,
        };
        self.monitor = MenuMonitor {
            x: monitor.monitor_x,
            y: monitor.monitor_y,
            width: monitor.monitor_width,
            height: monitor.monitor_height,
        };
    }
}

#[derive(Debug, Clone, Copy)]
struct MonitorContext {
    monitor_x: f64,
    monitor_y: f64,
    monitor_width: f64,
    monitor_height: f64,
    scale: f64,
    cursor_x: i32,
    cursor_y: i32,
}

impl MonitorContext {
    fn detect_for_request(app: &AppHandle, request: &ContextMenuRequest) -> Self {
        if request.placement.anchor == "point" && request.placement.coordinate_space == "physical" {
            if let (Some(x), Some(y)) = (request.placement.x, request.placement.y) {
                return Self::from_physical_point(app, x, y);
            }
        }
        let (cursor_x, cursor_y) = crate::mouse::get_cursor_position();
        Self::from_physical_point(app, cursor_x, cursor_y)
    }

    fn from_physical_point(app: &AppHandle, point_x: i32, point_y: i32) -> Self {
        let (monitor_phys_x, monitor_phys_y, monitor_phys_w, monitor_phys_h, scale) =
            app.available_monitors()
                .ok()
                .and_then(|monitors| {
                    monitors.into_iter().find(|m| {
                        let pos = m.position();
                        let size = m.size();
                        let right = pos.x + size.width as i32;
                        let bottom = pos.y + size.height as i32;
                        point_x >= pos.x && point_x < right && point_y >= pos.y && point_y < bottom
                    })
                })
                .or_else(|| app.primary_monitor().ok().flatten())
                .map(|m| {
                    let pos = m.position();
                    let size = m.size();
                    (
                        pos.x as f64,
                        pos.y as f64,
                        size.width as f64,
                        size.height as f64,
                        m.scale_factor(),
                    )
                })
                .unwrap_or((0.0, 0.0, 1920.0, 1080.0, 1.0));

        let cursor_rel_x = (point_x as f64 - monitor_phys_x) / scale;
        let cursor_rel_y = (point_y as f64 - monitor_phys_y) / scale;

        Self {
            monitor_x: monitor_phys_x,
            monitor_y: monitor_phys_y,
            monitor_width: monitor_phys_w / scale,
            monitor_height: monitor_phys_h / scale,
            scale,
            cursor_x: cursor_rel_x as i32,
            cursor_y: cursor_rel_y as i32,
        }
    }
}

async fn run_on_main_thread_result<T, F>(app: &AppHandle, task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(task());
    })
    .map_err(|e| format!("调度主线程任务失败: {}", e))?;

    rx.await.map_err(|_| "主线程任务被取消".to_string())?
}

fn start_cursor_passthrough_monitor(window: WebviewWindow, session_id: u64) {
    let app = window.app_handle().clone();
    std::thread::spawn(move || {
        let mut last = true;
        while super::get_active_menu_session() == session_id {
            let (mx, my) = crate::utils::mouse::get_cursor_position();
            let in_region = super::is_point_in_menu_region(mx, my);
            if in_region != last {
                last = in_region;
                let window_for_task = window.clone();
                let _ = app.run_on_main_thread(move || {
                    let _ = window_for_task.set_ignore_cursor_events(!in_region);
                });
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // 退出时无条件 set_ignore_cursor_events(false) 有与 w1 同款
        // 竞态——while 因新菜单接管而退出时,新会话刚把菜单建出来、其 monitor
        // 首 tick 尚未接管穿透,旧线程立即 false 会把新菜单的穿透恢复成交互,
        // 短暂拦截新菜单下方点击。与 clear_menu_regions 同款加会话比对。
        if super::get_active_menu_session() == session_id {
            let window_for_task = window.clone();
            let _ = app.run_on_main_thread(move || {
                let _ = window_for_task.set_ignore_cursor_events(false);
            });
        }
        // w1:只在会话仍属于本线程时才清区域——while 循环可能因会话被新
        // 菜单接管(next_menu_session_id 推进 + set_active_menu_session)而
        // 退出,此时新会话刚 update_menu_regions 写入的区域属于新菜单,
        // 旧线程无条件 clear 会把新菜单的穿透区域一并清掉,新菜单穿透
        // 计算全失效。
        if super::get_active_menu_session() == session_id {
            super::clear_menu_regions();
        }
    });
}

async fn hide_existing_window(app: &AppHandle) -> Result<(), String> {
    let app_for_task = app.clone();
    run_on_main_thread_result(app, move || {
        if let Some(w) = app_for_task.get_webview_window(LABEL) {
            let _ = w.hide();
        }
        Ok(())
    }).await
}

fn build_or_reuse_window(app: &AppHandle, monitor: MonitorContext, is_tray: bool) -> Result<WebviewWindow, String> {
    let init_phys_x = monitor.monitor_x as i32;
    let init_phys_y = monitor.monitor_y as i32;

    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.hide();
        let _ = w.set_always_on_top(false);
        let _ = w.set_position(tauri::PhysicalPosition::new(init_phys_x, init_phys_y));
        let _ = w.set_size(LogicalSize::new(INITIAL_WIDTH, INITIAL_HEIGHT));
        let _ = w.set_focusable(is_tray);
        let _ = w.set_ignore_cursor_events(false);
        return Ok(w);
    }

    let init_logical_x = init_phys_x as f64 / monitor.scale;
    let init_logical_y = init_phys_y as f64 / monitor.scale;
    let window = WebviewWindowBuilder::new(
        app,
        LABEL,
        tauri::WebviewUrl::App("plugins/context_menu/contextMenu.html".into()),
    )
    .title("菜单")
    .inner_size(INITIAL_WIDTH, INITIAL_HEIGHT)
    .position(init_logical_x, init_logical_y)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .focused(is_tray)
    .focusable(is_tray)
    .visible(false)
    .skip_taskbar(true)
    .drag_and_drop(false)
    .build()
    .map_err(|e| format!("创建菜单窗口失败: {}", e))?;

    let _ = window.set_ignore_cursor_events(false);
    let _ = window.set_position(tauri::PhysicalPosition::new(init_phys_x, init_phys_y));
    Ok(window)
}

async fn prepare_menu_window(
    app: &AppHandle,
    mut request: ContextMenuRequest,
) -> Result<WebviewWindow, String> {
    let app_for_task = app.clone();
    run_on_main_thread_result(app, move || {
        let monitor = MonitorContext::detect_for_request(&app_for_task, &request);
        request.apply_monitor_context(monitor);
        let is_tray = request.behavior.is_tray_menu;
        super::set_options(request);

        let window = build_or_reuse_window(&app_for_task, monitor, is_tray)?;
        let _ = window.emit("reload-menu", ());
        Ok(window)
    }).await
}

async fn show_prepared_window(
    app: &AppHandle,
    window: WebviewWindow,
    is_tray: bool,
    force_focus: bool,
) -> Result<(), String> {
    run_on_main_thread_result(app, move || {
        let _ = window.set_always_on_top(true);
        let _ = window.show();
        if is_tray || force_focus {
            let _ = window.set_focus();
        }
        let _ = window.set_always_on_top(true);
        Ok(())
    }).await
}

async fn wait_for_menu_result(session_id: u64) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if super::has_result() || super::get_active_menu_session() != session_id {
                let _ = tx.send(());
                break;
            }
        }
    });
    let _ = rx.await;
}

pub async fn show_menu(
    app: AppHandle,
    mut request: ContextMenuRequest,
) -> Result<Option<String>, String> {
    let old_session = super::get_active_menu_session();
    if old_session != 0 {
        super::clear_active_menu_session(old_session);
        hide_existing_window(&app).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    super::clear_result();
    super::clear_options();

    let session_id = super::next_menu_session_id();
    request.session_id = session_id;
    super::set_active_menu_session(session_id);

    let is_tray = request.behavior.is_tray_menu;
    let force_focus = request.behavior.force_focus;
    let window = prepare_menu_window(&app, request).await?;

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    show_prepared_window(&app, window.clone(), is_tray, force_focus).await?;
    start_cursor_passthrough_monitor(window, session_id);
    wait_for_menu_result(session_id).await;

    let result = if super::get_active_menu_session() == session_id {
        super::get_result()
    } else {
        None
    };

    super::clear_active_menu_session(session_id);
    super::clear_options_for_session(session_id);
    Ok(result)
}

#[cfg(test)]
mod w1_menu_region_guard {
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    // w1(区域被旧会话线程误清):start_cursor_passthrough_monitor 退出时必须
    // 只在"活跃会话仍是本线程 session"时才 clear_menu_regions——while 退出
    // 可能因为新菜单接管,此时区域已被新会话写入,无条件清会连新菜单的
    // 穿透区域一起清掉。
    #[test]
    fn passthrough_monitor_clears_regions_only_for_own_session() {
        let src = strip_line_comments(&source_file("src/windows/plugins/context_menu/window.rs"));
        let body = fn_body(&src, "start_cursor_passthrough_monitor");
        // 退出循环后的收尾段必须存在会话比对
        let clear_pos = body
            .find("clear_menu_regions()")
            .expect("收尾必须清菜单区域");
        // 比对必须位于 clear 之前且引用同一 session_id
        let cmp_pos = body
            .find("super::get_active_menu_session() == session_id")
            .expect("清区域前必须比对活跃会话仍属本线程");
        assert!(
            cmp_pos < clear_pos,
            "必须先在会话仍属本线程时才 clear_menu_regions"
        );
        // 负向:比对之前不得再出现裸 clear_menu_regions(清区域)——修复后
        // 唯一一次调用被 `if super::get_active_menu_session() == session_id`
        // 守卫,且该 if 的 `{` 就在调用同一行(无重排余地)。
        assert!(
            !body[..cmp_pos].contains("clear_menu_regions()"),
            "clear_menu_regions 必须被会话比对守卫"
        );
        let guarded = &body[cmp_pos..clear_pos + "super::clear_menu_regions();".len()];
        assert!(
            guarded.contains("if super::get_active_menu_session() == session_id {"),
            "clear_menu_regions 必须位于 if 会话比对分支内"
        );
        // 退出时的 set_ignore_cursor_events(false)(恢复交互)也
        // 必须被同一会话比对守卫——新菜单接管瞬间旧线程无条件恢复交互会
        // 短暂拦截新菜单下方点击,直到新 monitor 首 tick 再穿透。
        let reset_offset = body.find("set_ignore_cursor_events(false)").expect("收尾必须恢复交互");
        assert!(
            cmp_pos < reset_offset,
            "set_ignore_cursor_events(false) 必须在会话比对之后"
        );
        let reset_guarded = &body[cmp_pos..reset_offset + "set_ignore_cursor_events(false)".len()];
        assert!(
            reset_guarded.contains("if super::get_active_menu_session() == session_id"),
            "set_ignore_cursor_events(false) 必须被会话比对守卫"
        );
        // 负向:比对之前不得出现 set_ignore_cursor_events(false)(新会话接管时旧线程不得恢复交互)
        assert!(
            !body[..cmp_pos].contains("set_ignore_cursor_events(false)"),
            "恢复交互必须被会话比对守卫"
        );
    }
}
