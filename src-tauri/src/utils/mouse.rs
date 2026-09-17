use once_cell::sync::Lazy;
use parking_lot::Mutex;

static MOUSE_POSITION: Lazy<Mutex<(f64, f64)>> = Lazy::new(|| Mutex::new((0.0, 0.0)));

// 获取鼠标位置
#[cfg(target_os = "windows")]
pub fn get_cursor_position() -> (i32, i32) {
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    use windows::Win32::Foundation::POINT;
    
    let mut point = POINT { x: 0, y: 0 };
    unsafe {
        if GetCursorPos(&mut point).is_ok() {
            return (point.x, point.y);
        }
    }
    let pos = MOUSE_POSITION.lock();
    (pos.0 as i32, pos.1 as i32)
}

#[cfg(not(target_os = "windows"))]
pub fn get_cursor_position() -> (i32, i32) {
    if let Ok(pos) = get_system_cursor_position() {
        return pos;
    }
    let pos = MOUSE_POSITION.lock();
    (pos.0 as i32, pos.1 as i32)
}

#[cfg(not(target_os = "windows"))]
fn get_system_cursor_position() -> Result<(i32, i32), String> {
    use enigo::{Enigo, Mouse, Settings};
    
    let enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("初始化失败: {}", e))?;
    
    let (x, y) = enigo.location()
        .map_err(|e| format!("获取位置失败: {}", e))?;
    
    Ok((x, y))
}

