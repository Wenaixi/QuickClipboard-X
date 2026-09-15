// Windows 注册表管理，用于禁用/启用系统 Win+V 快捷键

#[cfg(windows)]
use winreg::enums::*;
#[cfg(windows)]
use winreg::RegKey;

#[cfg(windows)]
const EXPLORER_ADVANCED_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
#[cfg(windows)]
const DISABLED_HOTKEYS_VALUE: &str = "DisabledHotkeys";

#[cfg(windows)]
pub fn disable_win_v_hotkey() -> Result<(), String> {
    add_disabled_hotkey('V', true)
}

#[cfg(windows)]
pub fn disable_win_v_hotkey_silent() -> Result<(), String> {
    add_disabled_hotkey('V', false)
}

#[cfg(windows)]
pub fn enable_win_v_hotkey() -> Result<(), String> {
    remove_disabled_hotkey('V', true)
}

#[cfg(windows)]
pub fn enable_win_v_hotkey_silent() -> Result<(), String> {
    remove_disabled_hotkey('V', false)
}

#[cfg(windows)]
fn add_disabled_hotkey(key: char, restart_explorer: bool) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (reg_key, _) = hkcu
        .create_subkey(EXPLORER_ADVANCED_PATH)
        .map_err(|e| format!("无法打开注册表项: {}", e))?;

    let current_value: String = reg_key
        .get_value(DISABLED_HOTKEYS_VALUE)
        .unwrap_or_default();

    let key_upper = key.to_uppercase().to_string();
    if !current_value.contains(&key_upper) {
        let new_value = format!("{}{}", current_value, key_upper);
        reg_key
            .set_value(DISABLED_HOTKEYS_VALUE, &new_value)
            .map_err(|e| format!("无法设置注册表值: {}", e))?;
        // 值实际被写入才重启资源管理器——值早已存在时跳过重启,避免
        // 连续点击开关让桌面闪黑、关闭用户打开的窗口。
        if restart_explorer {
            restart_explorer_process()?;
        }
    }

    Ok(())
}

#[cfg(windows)]
fn remove_disabled_hotkey(key: char, restart_explorer: bool) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let reg_key = match hkcu.open_subkey_with_flags(EXPLORER_ADVANCED_PATH, KEY_READ | KEY_WRITE) {
        Ok(k) => k,
        Err(_) => return Ok(()),
    };

    let current_value: String = reg_key
        .get_value(DISABLED_HOTKEYS_VALUE)
        .unwrap_or_default();

    let key_upper = key.to_uppercase().to_string();
    let new_value = current_value.replace(&key_upper, "");

    // 记录本次调用是否实际改变了注册表;无变化(值里本就没有该键)则不必重启
    let mut changed = new_value != current_value;
    if new_value.is_empty() {
        // 值为空时若本来就有该值,delete 才算实际变更;无该值则无变更
        changed = current_value.contains(&key_upper);
        let _ = reg_key.delete_value(DISABLED_HOTKEYS_VALUE);
    } else if new_value != current_value {
        changed = true;
        reg_key
            .set_value(DISABLED_HOTKEYS_VALUE, &new_value)
            .map_err(|e| format!("无法更新注册表值: {}", e))?;
    }

    if restart_explorer && changed {
        restart_explorer_process()?;
    }

    Ok(())
}

#[cfg(windows)]
fn restart_explorer_process() -> Result<(), String> {
    use std::process::Command;

    // 先 taskkill /F /IM explorer.exe,失败立刻返回避免 sleep+start 产生双进程
    let out = Command::new("taskkill")
        .args(["/F", "/IM", "explorer.exe"])
        .output()
        .map_err(|e| format!("taskkill 启动失败: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "taskkill 退出失败: status={:?}, stderr={}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    std::thread::sleep(std::time::Duration::from_millis(1000));

    if Command::new("cmd")
        .args(["/C", "start", "explorer.exe"])
        .spawn()
        .is_err()
    {
        Command::new("explorer.exe")
            .spawn()
            .map_err(|e| format!("无法启动Explorer进程: {}", e))?;
    }

    std::thread::sleep(std::time::Duration::from_millis(1000));

    Ok(())
}

#[cfg(windows)]
pub fn is_win_v_hotkey_disabled() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let reg_key = match hkcu.open_subkey(EXPLORER_ADVANCED_PATH) {
        Ok(k) => k,
        Err(_) => return false,
    };

    let current_value: String = reg_key
        .get_value(DISABLED_HOTKEYS_VALUE)
        .unwrap_or_default();

    current_value.contains('V')
}

#[cfg(test)]
mod tests {
    // 源码字面护栏:禁用/启用 Win+V 只在实际改变注册表值时才重启
    // 资源管理器——值已存在/已不存在时再重启只会让桌面闪黑、关闭
    // 用户打开的窗口,且与注册表状态无关联。
    fn stripped() -> String {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/system/win_v_hotkey.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 win_v_hotkey.rs");
        source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn add_restarts_explorer_only_when_value_changed() {
        let s = stripped();
        let start = s.find("fn add_disabled_hotkey").expect("缺 add_disabled_hotkey");
        let end = s[start + 1..].find("\nfn remove_disabled_hotkey").map(|i| start + 1 + i).unwrap_or(s.len());
        let body = &s[start..end];
        // 重启调用必须嵌套在"值被写入"的 if 分支里,而不是函数尾部无条件执行
        let write_pos = body.find(".set_value(").expect("add 必须写注册表值");
        let restart_pos = body.find("restart_explorer_process()").expect("add 必须重启资源管理器");
        assert!(
            write_pos < restart_pos,
            "add 中重启必须发生在值写入之后(值未变化时不重启)"
        );
        assert!(
            body[..restart_pos].matches("if restart_explorer").count() >= 1,
            "add 中重启必须受条件守卫(值未变化时跳过)"
        );
    }

    #[test]
    fn remove_restarts_explorer_only_when_value_changed() {
        let s = stripped();
        let start = s.find("fn remove_disabled_hotkey").expect("缺 remove_disabled_hotkey");
        let end = s[start + 1..].find("\nfn restart_explorer_process").map(|i| start + 1 + i).unwrap_or(s.len());
        let body = &s[start..end];
        assert!(
            body.contains("restart_explorer && changed"),
            "remove 中重启必须带 changed 判定(值无变化时不重启)"
        );
    }
}

#[cfg(not(windows))]
pub fn disable_win_v_hotkey() -> Result<(), String> { Ok(()) }
#[cfg(not(windows))]
pub fn disable_win_v_hotkey_silent() -> Result<(), String> { Ok(()) }
#[cfg(not(windows))]
pub fn enable_win_v_hotkey() -> Result<(), String> { Ok(()) }
#[cfg(not(windows))]
pub fn enable_win_v_hotkey_silent() -> Result<(), String> { Ok(()) }
#[cfg(not(windows))]
pub fn is_win_v_hotkey_disabled() -> bool { false }
