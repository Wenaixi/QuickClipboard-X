// 获取 Windows 系统文本缩放比例
#[cfg(windows)]
pub fn get_text_scale_factor() -> f64 {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey("SOFTWARE\\Microsoft\\Accessibility") {
        if let Ok(value) = key.get_value::<u32, _>("TextScaleFactor") {
            // 注册表被改到异常大值(如 400)会返回 4.0 缩放,前端字号随之离谱膨胀。
            // 限幅到 0.5~2.0 的合理区间,超界取边界值,防异常输入放大 UI。
            return (value.clamp(50, 200) as f64) / 100.0;
        }
    }
    1.0
}

#[cfg(not(windows))]
pub fn get_text_scale_factor() -> f64 {
    1.0
}

#[tauri::command]
pub fn get_system_text_scale() -> f64 {
    get_text_scale_factor()
}
