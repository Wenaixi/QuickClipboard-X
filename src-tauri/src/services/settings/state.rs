use super::{AppSettings, storage::SettingsStorage};
use once_cell::sync::Lazy;
use parking_lot::RwLock;

static SETTINGS: Lazy<RwLock<AppSettings>> = Lazy::new(|| {
    RwLock::new(SettingsStorage::load().unwrap_or_default())
});

pub fn get_settings() -> AppSettings {
    SETTINGS.read().clone()
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
    SettingsStorage::save(&settings)
}

pub fn get_data_directory() -> Result<std::path::PathBuf, String> {
    let settings = SETTINGS.read();
    SettingsStorage::get_data_directory(&settings)
}
