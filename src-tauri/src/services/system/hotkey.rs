mod global;
mod navigation;

pub use global::{
    get_shortcut_status,
    get_shortcut_statuses,
    init_hotkey_manager,
    is_hotkeys_enabled,
    ShortcutStatus,
};

pub fn reload_from_settings() -> Result<(), String> {
    let result = global::reload_from_settings();
    navigation::reload_navigation_hotkeys_from_settings();
    result
}

pub fn enable_hotkeys() -> Result<(), String> {
    let result = global::enable_hotkeys();
    navigation::sync_navigation_hotkeys_for_foreground();
    result
}

pub fn disable_hotkeys() {
    global::disable_hotkeys();
    navigation::sync_navigation_hotkeys_for_foreground();
}

pub fn unregister_all() {
    // F1: 托盘菜单 toggle 关闭热键直接调本入口（不在 reload 持锁路径上），
    // 必须自己持 GLOBAL_HOTKEY_SYNC_LOCK 与持锁的 register_* 串行化；
    // global::unregister_all 假设锁已持，不得再 lock。
    let _guard = global::GLOBAL_HOTKEY_SYNC_LOCK.lock();
    global::unregister_all();
    navigation::sync_navigation_hotkeys_for_foreground();
}

pub fn sync_hotkeys_for_foreground() {
    global::sync_hotkeys_for_foreground();
    navigation::sync_navigation_hotkeys_for_foreground();
}

pub fn enable_navigation_hotkeys() {
    navigation::enable_navigation_hotkeys();
}

pub fn disable_navigation_hotkeys() {
    navigation::disable_navigation_hotkeys();
}

#[cfg(test)]
mod tests {
    use std::fs;

    fn hotkey_source() -> String {
        fs::read_to_string(format!(
            "{}/src/services/system/hotkey.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取 hotkey.rs 失败")
    }

    fn strip_line_comments(src: &str) -> String {
        src.lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // F1: 托盘菜单 toggle 关闭热键调用 crate::hotkey::unregister_all()
    // （hotkey.rs 顶层），该入口不在 reload 持锁路径上，必须自己持
    // GLOBAL_HOTKEY_SYNC_LOCK，否则与持锁的 register_* 并发撞车。
    // 锁位置必须早于 global::unregister_all() 调用。
    #[test]
    fn top_level_unregister_all_holds_sync_lock_before_global_call() {
        let src = strip_line_comments(&hotkey_source());
        let start = src
            .find("pub fn unregister_all()")
            .expect("缺 unregister_all");
        let rest = &src[start..];
        let end = rest.find("\n}\n").map(|i| start + i).unwrap_or(src.len());
        let b = &src[start..end];
        let lock_pos = b.find("GLOBAL_HOTKEY_SYNC_LOCK.lock()");
        let call_pos = b.find("global::unregister_all()");
        assert!(
            lock_pos.is_some() && call_pos.is_some() && lock_pos < call_pos,
            "顶层 unregister_all 必须在调 global::unregister_all 之前持 GLOBAL_HOTKEY_SYNC_LOCK"
        );
    }
}
