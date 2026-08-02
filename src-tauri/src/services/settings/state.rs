use super::{AppSettings, storage::SettingsStorage};
use once_cell::sync::Lazy;
use parking_lot::RwLock;

static SETTINGS: Lazy<RwLock<AppSettings>> = Lazy::new(|| {
    RwLock::new(SettingsStorage::load().unwrap_or_default())
});

pub fn get_settings() -> AppSettings {
    SETTINGS.read().clone()
}

// 热路径只读单字段:edge_monitor 50ms 轮询若走 get_settings() 会深克隆 ~135 字段,
// 这里只读锁 + bool 拷贝,20Hz × 2 路径降到常数级。
pub fn is_edge_hover_popup_enabled() -> bool {
    SETTINGS.read().edge_hover_popup_enabled
}

pub fn update_settings(mut settings: AppSettings) -> Result<(), String> {
    // 守不变量:所有写入路径统一入口,杜绝 hide=false/hover=true 违规组合落地
    settings.normalize_edge_hover_invariant();
    *SETTINGS.write() = settings.clone();
    SettingsStorage::save(&settings)
}

pub fn update_with<F>(updater: F) -> Result<(), String>
where
    F: FnOnce(&mut AppSettings),
{
    let mut settings = SETTINGS.write();
    updater(&mut settings);
    // 守不变量:update_with 是与 update_settings 并列的写入入口,
    // 必须共享归一化,杜绝 hide=false/hover=true 违规组合落地
    settings.normalize_edge_hover_invariant();
    SettingsStorage::save(&settings)
}

pub fn get_data_directory() -> Result<std::path::PathBuf, String> {
    let settings = SETTINGS.read();
    SettingsStorage::get_data_directory(&settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // 串行化所有写 SETTINGS 的测试。
    // 否则并发跑时 update_settings / update_with 互相覆盖,
    // 读到对方刚写入的 hide=true/hover=true,误判归一化失败。
    static SERIAL: Mutex<()> = Mutex::new(());

    fn lock_serial() -> std::sync::MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    // update_settings(所有写入统一入口)必须执行归一化
    #[test]
    fn update_settings_normalizes_edge_hover_invariant() {
        let _g = lock_serial();
        let mut settings = AppSettings::default();
        settings.edge_hide_enabled = false;
        settings.edge_hover_popup_enabled = true;
        update_settings(settings.clone()).unwrap();
        let loaded = get_settings();
        assert!(
            !loaded.edge_hover_popup_enabled,
            "update_settings 必须归一化,杜绝 hide=false/hover=true 落地"
        );
        // 恢复默认,避免污染其他测试
        update_settings(AppSettings::default()).unwrap();
    }

    // 热路径 accessor:只读 edge_hover_popup_enabled,与 get_settings 同源但零深克隆。
    // edge_monitor 50ms 轮询必须走此入口(源码护栏在 edge_monitor 测试模块)。
    #[test]
    fn is_edge_hover_popup_enabled_reads_live_field() {
        let _g = lock_serial();
        let mut settings = AppSettings::default();
        settings.edge_hide_enabled = true;
        settings.edge_hover_popup_enabled = true;
        update_settings(settings).unwrap();
        assert!(
            is_edge_hover_popup_enabled(),
            "写入 true 后 accessor 必须读到 true"
        );

        let mut settings = AppSettings::default();
        settings.edge_hide_enabled = true;
        settings.edge_hover_popup_enabled = false;
        update_settings(settings).unwrap();
        assert!(
            !is_edge_hover_popup_enabled(),
            "写入 false 后 accessor 必须读到 false"
        );

        update_settings(AppSettings::default()).unwrap();
    }

    // update_with 也必须与 update_settings 共享归一化
    #[test]
    fn update_with_normalizes_edge_hover_invariant() {
        let _g = lock_serial();
        let mut seed = AppSettings::default();
        seed.edge_hide_enabled = true;
        seed.edge_hover_popup_enabled = true;
        update_settings(seed).unwrap();

        update_with(|s| {
            s.edge_hide_enabled = false;
        })
        .expect("update_with 必须支持 hide 字段变更");

        let loaded = get_settings();
        assert!(
            !loaded.edge_hover_popup_enabled,
            "update_with 必须归一化 hide=false ⇒ hover=false,现状 hover={}",
            loaded.edge_hover_popup_enabled
        );
        assert!(
            !loaded.edge_hide_enabled,
            "update_with 应保留 hide=false 写入"
        );

        update_settings(AppSettings::default()).unwrap();
    }
}
