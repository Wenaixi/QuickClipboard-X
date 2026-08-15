use super::model::{
    AppSettings, SETTINGS_MIGRATION_VERSION_V1, SETTINGS_MIGRATION_VERSION_V2,
    SETTINGS_MIGRATION_VERSION_V3,
};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(target_os = "windows")]
use windows::core::HSTRING;
#[cfg(target_os = "windows")]
use windows::Win32::Storage::FileSystem::{
    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
};

static NEXT_SETTINGS_TEMP_FILE_ID: AtomicU64 = AtomicU64::new(0);

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
            for directory in ["quickclipboard", "com.quickclipboard.app"] {
                let path = local_app_data.join(directory).join("settings.json");
                if path != target && !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
        paths
    }

    fn load_settings_from_paths(
        target: &Path,
        legacy_paths: &[PathBuf],
    ) -> Result<Option<(AppSettings, String)>, String> {
        let mut last_error = None;
        if target.exists() {
            return match fs::read_to_string(target) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(settings) => Ok(Some((settings, content))),
                    Err(error) => Err(error.to_string()),
                },
                Err(error) => Err(error.to_string()),
            };
        }

        for source in legacy_paths {
            if !source.exists() {
                continue;
            }
            let content = match fs::read_to_string(source) {
                Ok(content) => content,
                Err(error) => {
                    last_error = Some(error.to_string());
                    continue;
                }
            };
            let settings = match serde_json::from_str::<AppSettings>(&content) {
                Ok(settings) => settings,
                Err(error) => {
                    last_error = Some(error.to_string());
                    continue;
                }
            };
            if let Err(error) = Self::save_to_path(target, &settings) {
                // 目标路径可能暂时不可写,先返回已恢复的设置,下次启动继续尝试迁移。
                eprintln!("迁移设置到目标路径失败: {}", error);
            }
            return Ok(Some((settings, content)));
        }

        match last_error {
            Some(error) => Err(error),
            None => Ok(None),
        }
    }

    pub fn load() -> Result<AppSettings, String> {
        let path = Self::get_settings_path()?;
        let (mut settings, content) =
            match Self::load_settings_from_paths(&path, &Self::legacy_settings_paths(&path))? {
                Some(settings) => settings,
                None => return Ok(AppSettings::default()),
            };

        let has_legacy_lan_sync_settings = content.contains("\"lanSync");
        let had_legacy_webdav_password = !settings.webdav_password.is_empty();
        if had_legacy_webdav_password {
            if !settings.webdav_url.trim().is_empty() && !settings.webdav_username.trim().is_empty()
            {
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

    fn save_to_path(path: &Path, settings: &AppSettings) -> Result<(), String> {
        let temporary_path = path.with_extension(format!(
            "json.{}.tmp",
            NEXT_SETTINGS_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let content = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
        fs::write(&temporary_path, content).map_err(|e| e.to_string())?;

        #[cfg(target_os = "windows")]
        {
            let source = HSTRING::from(temporary_path.as_os_str().to_string_lossy().into_owned());
            let destination = HSTRING::from(path.as_os_str().to_string_lossy().into_owned());
            let result = unsafe {
                MoveFileExW(
                    &source,
                    &destination,
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            };
            if result.is_ok() {
                return Ok(());
            }
            let _ = fs::remove_file(&temporary_path);
            return Err(result.err().map_or_else(
                || "Windows 设置文件原子替换失败".to_string(),
                |error| error.to_string(),
            ));
        }

        #[cfg(not(target_os = "windows"))]
        {
            fs::rename(&temporary_path, path).map_err(|error| error.to_string())
        }
    }

    pub fn save(settings: &AppSettings) -> Result<(), String> {
        let path = Self::get_settings_path()?;
        Self::save_to_path(&path, settings)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn test_dir() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = env::temp_dir().join(format!(
            "quickclipboard-settings-storage-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_settings(path: &Path, language: &str) {
        let mut settings = AppSettings::default();
        settings.language = language.to_string();
        fs::write(path, serde_json::to_string(&settings).unwrap()).unwrap();
    }

    #[test]
    fn valid_target_is_preferred_over_legacy_candidates() {
        let dir = test_dir();
        let target = dir.join("settings.json");
        let legacy = dir.join("legacy.json");
        write_settings(&target, "target");
        write_settings(&legacy, "legacy");

        let loaded = SettingsStorage::load_settings_from_paths(&target, &[legacy]).unwrap();
        assert_eq!(loaded.unwrap().0.language, "target");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn damaged_target_is_preserved_when_no_legacy_is_usable() {
        let dir = test_dir();
        let target = dir.join("settings.json");
        let original = "{ damaged settings";
        fs::write(&target, original).unwrap();

        assert!(SettingsStorage::load_settings_from_paths(&target, &[]).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), original);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unreadable_target_is_preserved_instead_of_being_replaced_by_legacy_settings() {
        let dir = test_dir();
        let target = dir.join("settings.json");
        let legacy = dir.join("legacy.json");
        fs::create_dir(&target).unwrap();
        write_settings(&legacy, "legacy");

        assert!(SettingsStorage::load_settings_from_paths(&target, &[legacy]).is_err());
        assert!(target.is_dir());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn save_replaces_a_complete_settings_document() {
        let dir = test_dir();
        let path = dir.join("settings.json");
        let mut settings = AppSettings::default();
        settings.language = "persisted-language".to_string();
        write_settings(&path, "old-language");

        SettingsStorage::save_to_path(&path, &settings).unwrap();

        let loaded: AppSettings = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.language, "persisted-language");
        assert!(
            fs::read_dir(&dir)
                .unwrap()
                .all(|entry| !entry.unwrap().file_name().to_string_lossy().ends_with(".tmp")),
            "成功写入后不应遗留临时设置文件"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_settings_replacement_uses_write_through_replace_existing() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/settings/storage.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取设置存储源码失败");
        assert!(source.contains("MoveFileExW"));
        assert!(source.contains("MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH"));
    }

    #[test]
    fn application_identifier_settings_are_migrated_to_current_storage() {
        let dir = test_dir();
        let target = dir.join("settings.json");
        let legacy = dir.join("com.quickclipboard.app-settings.json");
        write_settings(&legacy, "identifier-legacy");

        let loaded = SettingsStorage::load_settings_from_paths(&target, &[legacy]).unwrap();

        assert_eq!(loaded.unwrap().0.language, "identifier-legacy");
        assert_eq!(
            serde_json::from_str::<AppSettings>(&fs::read_to_string(&target).unwrap())
                .unwrap()
                .language,
            "identifier-legacy"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn damaged_target_is_preserved_even_when_a_legacy_candidate_is_valid() {
        let dir = test_dir();
        let target = dir.join("settings.json");
        let valid = dir.join("valid.json");
        let original = "{ damaged target";
        fs::write(&target, original).unwrap();
        write_settings(&valid, "legacy");

        assert!(SettingsStorage::load_settings_from_paths(&target, &[valid]).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), original);
        fs::remove_dir_all(dir).unwrap();
    }
}
