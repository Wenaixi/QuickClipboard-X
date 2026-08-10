use super::model::{
    AppSettings, SETTINGS_MIGRATION_VERSION_V1, SETTINGS_MIGRATION_VERSION_V2,
    SETTINGS_MIGRATION_VERSION_V3,
};
use std::{
    env,
    fs,
    path::{Path, PathBuf},
};

pub struct SettingsStorage;

impl SettingsStorage {
    fn migrate_settings(settings: &mut AppSettings) -> bool {
        let mut migrated = false;
        let migration_version = settings.settings_migration_version.unwrap_or(0);

        if migration_version < SETTINGS_MIGRATION_VERSION_V1 {
            settings.image_preview = true;
            settings.text_preview = true;
            settings.file_preview = true;
            settings.settings_migration_version = Some(SETTINGS_MIGRATION_VERSION_V1);
            migrated = true;
        }

        if migration_version < SETTINGS_MIGRATION_VERSION_V2 {
            settings.settings_migration_version = Some(SETTINGS_MIGRATION_VERSION_V2);
            migrated = true;
        }

        if migration_version < SETTINGS_MIGRATION_VERSION_V3 {
            let _ = settings.normalize_app_filter_blocklist();
            settings.settings_migration_version = Some(SETTINGS_MIGRATION_VERSION_V3);
            migrated = true;
        }

        migrated
    }

    fn is_portable_mode() -> bool {
        // 统一走 is_portable_runtime,避免与 commands/data_management/updater 语义漂移
        crate::services::is_portable_runtime()
    }

    fn get_data_dir() -> Result<PathBuf, String> {
        if Self::is_portable_mode() {
            let exe_dir = env::current_exe()
                .map_err(|e| e.to_string())?
                .parent()
                .ok_or("无法获取执行目录")?
                .to_path_buf();
            return Ok(exe_dir.join("data"));
        }

        Ok(dirs::data_local_dir()
            .ok_or("无法获取数据目录")?
            .join("quickclipboard"))
    }

    pub fn get_settings_path() -> Result<PathBuf, String> {
        let dir = Self::get_data_dir()?;
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(dir.join("settings.json"))
    }

    fn legacy_settings_paths(target: &Path) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(exe_dir) = env::current_exe().and_then(|exe| {
            exe.parent()
                .map(|path| path.to_path_buf())
                .ok_or_else(|| std::io::Error::other("无法获取执行目录"))
        }) {
            let path = exe_dir.join("data").join("settings.json");
            if path != target {
                paths.push(path);
            }
        }
        if let Some(local_app_data) = dirs::data_local_dir() {
            let path = local_app_data.join("quickclipboard").join("settings.json");
            if path != target && !paths.contains(&path) {
                paths.push(path);
            }
        }
        paths
    }

    fn read_settings_file(path: &Path) -> Result<AppSettings, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    }

    fn load_legacy_settings(target: &Path) -> Result<Option<AppSettings>, String> {
        for source in Self::legacy_settings_paths(target) {
            if !source.exists() {
                continue;
            }
            let settings = Self::read_settings_file(&source)?;
            let content = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
            fs::write(target, content).map_err(|e| e.to_string())?;
            return Ok(Some(settings));
        }
        Ok(None)
    }

    pub fn load() -> Result<AppSettings, String> {
        let path = Self::get_settings_path()?;

        let (mut settings, content) = if path.exists() {
            let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let settings = Self::read_settings_file(&path)?;
            (settings, content)
        } else if let Some(settings) = Self::load_legacy_settings(&path)? {
            let content = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
            (settings, content)
        } else {
            return Ok(AppSettings::default());
        };

        let has_legacy_lan_sync_settings = content.contains("\"lanSync");
        let had_legacy_webdav_password = !settings.webdav_password.is_empty();
        if had_legacy_webdav_password {
            if !settings.webdav_url.trim().is_empty() && !settings.webdav_username.trim().is_empty() {
                if let Err(e) = crate::services::secure_credentials::set_webdav_password(
                    &settings.webdav_url,
                    &settings.webdav_username,
                    &settings.webdav_password,
                ) {
                    eprintln!("迁移 WebDAV 密码到系统凭据库失败: {}", e);
                }
            }
            settings.webdav_password.clear();
        }
        // 守不变量:手改/旧 JSON 可能留下 hide=false/hover=true 违规组合,
        // 加载时统一归一化,防止下次开启 hide 时意外弹出触发条
        settings.normalize_edge_hover_invariant();
        let normalized = settings.normalize_app_filter_blocklist();
        let migrated = Self::migrate_settings(&mut settings)
            || normalized
            || has_legacy_lan_sync_settings
            || had_legacy_webdav_password;

        if migrated {
            let _ = Self::save(&settings);
        }
        
        Ok(settings)
    }

    pub fn exists() -> Result<bool, String> {
        let path = Self::get_settings_path()?;
        Ok(path.exists())
    }

    pub fn save(settings: &AppSettings) -> Result<(), String> {
        let path = Self::get_settings_path()?;
        let content = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
        fs::write(&path, content).map_err(|e| e.to_string())
    }

    pub fn get_data_directory(settings: &AppSettings) -> Result<PathBuf, String> {
        if settings.use_custom_storage {
            if let Some(ref path) = settings.custom_storage_path {
                let custom_dir = PathBuf::from(path);
                fs::create_dir_all(&custom_dir).map_err(|e| e.to_string())?;
                return Ok(custom_dir);
            }
        }
        
        let dir = Self::get_data_dir()?;
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Ok(dir)
    }
}
