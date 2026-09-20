use tauri::{Manager, Monitor, PhysicalPosition, WebviewWindow};

// 将窗口定位到鼠标位置
pub fn position_at_cursor(window: &WebviewWindow) -> Result<(), String> {
    let monitor = crate::screen::ScreenUtils::get_monitor_at_cursor(window.app_handle())?;
    let (cursor_x, cursor_y) = crate::mouse::get_cursor_position();
    let window_size = window.outer_size().map_err(|e| e.to_string())?;

    let best_pos = calculate_best_position(
        PhysicalPosition::new(cursor_x, cursor_y),
        window_size,
        &monitor,
    );

    window.set_position(best_pos).map_err(|e| e.to_string())
}

// 将记住的位置恢复到可见屏幕范围内，无法判断时回退到智能鼠标定位
pub fn position_at_saved_or_cursor(window: &WebviewWindow, x: i32, y: i32) -> Result<(), String> {
    let window_size = match window.outer_size() {
        Ok(size) => size,
        Err(_) => return position_at_cursor(window),
    };
    let width = window_size.width.min(i32::MAX as u32) as i32;
    let height = window_size.height.min(i32::MAX as u32) as i32;

    match crate::screen::ScreenUtils::resolve_visible_window_position(
        window.app_handle(),
        x,
        y,
        width,
        height,
    ) {
        Ok((visible_x, visible_y)) => window
            .set_position(PhysicalPosition::new(visible_x, visible_y))
            .map_err(|e| e.to_string()),
        Err(_) => position_at_cursor(window),
    }
}

pub fn calculate_popup_position(
    cursor_x: i32,
    cursor_y: i32,
    width: i32,
    height: i32,
    monitor: &Monitor,
) -> PhysicalPosition<i32> {
    calculate_best_position(
        PhysicalPosition::new(cursor_x, cursor_y),
        tauri::PhysicalSize::new(width.max(0) as u32, height.max(0) as u32),
        monitor,
    )
}

fn calculate_best_position(
    cursor: PhysicalPosition<i32>,
    window_size: tauri::PhysicalSize<u32>,
    monitor: &Monitor,
) -> PhysicalPosition<i32> {
    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();

    let margin = 12;
    let w = window_size.width as i32;
    let h = window_size.height as i32;

    let work_x = monitor_pos.x;
    let work_y = monitor_pos.y;
    let work_w = monitor_size.width as i32;
    let work_h = monitor_size.height as i32;

    // 默认位置：鼠标右下方
    let mut x = cursor.x + margin;
    let mut y = cursor.y + margin;

    // 如果右边超出，移到左边
    if x + w > work_x + work_w {
        x = cursor.x - w - margin;
    }

    // 如果下边超出，移到上边
    if y + h > work_y + work_h {
        y = cursor.y - h - margin;
    }

    x = x.max(work_x).min(work_x + work_w - w).max(work_x);
    y = y.max(work_y).min(work_y + work_h - h).max(work_y);

    PhysicalPosition::new(x, y)
}

// 将窗口居中显示
pub fn center_window(window: &WebviewWindow) -> Result<(), String> {
    window.center().map_err(|e| e.to_string())
}

// 将窗口中心对齐到鼠标位置
pub fn center_at_cursor(window: &WebviewWindow) -> Result<(), String> {
    let monitor = crate::screen::ScreenUtils::get_monitor_at_cursor(window.app_handle())?;
    let (cursor_x, cursor_y) = crate::mouse::get_cursor_position();
    let window_size = window.outer_size().map_err(|e| e.to_string())?;

    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();

    let w = window_size.width as i32;
    let h = window_size.height as i32;

    let work_x = monitor_pos.x;
    let work_y = monitor_pos.y;
    let work_w = monitor_size.width as i32;
    let work_h = monitor_size.height as i32;

    // 窗口中心对齐鼠标
    let mut x = cursor_x - w / 2;
    let mut y = cursor_y - h / 2;

    // 确保不超出屏幕边界:窗口比屏还宽/高(高 DPI 面板或副屏小于主屏)时
    // 直接 min 会把钳到负坐标屏外,末尾补 max 兜底保证落在工作区内。
    x = x.max(work_x).min(work_x + work_w - w).max(work_x);
    y = y.max(work_y).min(work_y + work_h - h).max(work_y);

    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

// 获取窗口边界
pub fn get_window_bounds(window: &WebviewWindow) -> Result<(i32, i32, u32, u32), String> {
    let pos = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    Ok((pos.x, pos.y, size.width, size.height))
}

#[cfg(test)]
mod tests {
    // 护栏:窗口比工作区还大(高 DPI 面板落在低分辨率屏/副屏小于主屏)时,
    // clamp 末尾必须保留二次 max 兜底,否则窗口被钳到负坐标屏外不可见。
    // 删掉末尾 .max 兜底,把窗口宽加大到超过屏宽即见红。
    #[test]
    fn clamp_keeps_trailing_max_fallback() {
        let source = std::fs::read_to_string(format!(
            "{}/src/utils/positioning.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读 positioning.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let x_line = stripped
            .lines()
            .find(|l| l.contains("min(work_x + work_w - w).max(work_x)"))
            .expect("calculate_best_position clamp 末尾必须带 max 兜底");
        let y_line = stripped
            .lines()
            .find(|l| l.contains("min(work_y + work_h - h).max(work_y)"))
            .expect("clamp 纵向末尾必须带 max 兜底");
        assert!(x_line.contains(".max(work_x)"), "横向 clamp 必须二次 max");
        assert!(y_line.contains(".max(work_y)"), "纵向 clamp 必须二次 max");
        // center_at_cursor 同族修复,防只补一处漏另一处。
        assert!(stripped.contains("min(work_x + work_w - w).max(work_x)"), "center_at_cursor 横向必须同款");
    }
}
