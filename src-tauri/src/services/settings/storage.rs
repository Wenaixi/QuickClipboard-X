use super::model::{
    AppSettings, SETTINGS_MIGRATION_VERSION_V1, SETTINGS_MIGRATION_VERSION_V2,
    SETTINGS_MIGRATION_VERSION_V3,
};
use serde_json::{Map, Value};
use std::{
    collections::HashSet,
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

fn deserialize_settings(document: &Map<String, Value>) -> Result<AppSettings, serde_json::Error> {
    serde_json::from_value(Value::Object(document.clone()))
}

fn legacy_setting_alias_target(key: &str) -> Option<&'static str> {
    match key {
        "history_limit" => Some("historyLimit"),
        "custom_storage_path" => Some("customStoragePath"),
        "use_custom_storage" => Some("useCustomStorage"),
        _ => None,
    }
}

fn recover_settings_from_content(content: &str) -> Result<(AppSettings, bool, bool), String> {
    let Value::Object(document) =
        serde_json::from_str::<Value>(content).map_err(|error| error.to_string())?
    else {
        return Err("设置文件根节点必须是 JSON 对象".to_string());
    };
    let Value::Object(mut recovered) =
        serde_json::to_value(AppSettings::default()).map_err(|error| error.to_string())?
    else {
        return Err("默认设置序列化结果不是 JSON 对象".to_string());
    };

    let known_keys: HashSet<String> = recovered.keys().cloned().collect();
    let mut rejected_any_field = false;
    let mut unknown_any_field = false;
    for (key, value) in document {
        let alias_target = legacy_setting_alias_target(&key);
        if !known_keys.contains(&key) && alias_target.is_none() {
            unknown_any_field = true;
        }
        let mut candidate = recovered.clone();
        if let Some(canonical_key) = alias_target {
            candidate.remove(canonical_key);
        }
        candidate.insert(key, value);
        if deserialize_settings(&candidate).is_ok() {
            recovered = candidate;
        } else {
            rejected_any_field = true;
        }
    }

    deserialize_settings(&recovered)
        .map(|settings| (settings, rejected_any_field, unknown_any_field))
        .map_err(|error| error.to_string())
}

fn incompatible_settings_backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.incompatible.bak")
}

fn incompatible_settings_backup_path_at(path: &Path, index: usize) -> PathBuf {
    if index == 0 {
        incompatible_settings_backup_path(path)
    } else {
        path.with_extension(format!("json.incompatible.{index}.bak"))
    }
}

fn preserve_incompatible_settings(path: &Path, content: &str) -> Result<(), String> {
    for index in 0..=999 {
        let backup = incompatible_settings_backup_path_at(path, index);
        match fs::read_to_string(&backup) {
            Ok(existing) if existing == content => return Ok(()),
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return fs::write(&backup, content)
                    .map_err(|error| format!("保留不兼容设置备份失败，已拒绝覆盖原设置: {error}"));
            }
            Err(error) => {
                return Err(format!("读取不兼容设置备份失败，已拒绝覆盖原设置: {error}"));
            }
        }
    }
    // 槽位耗尽：覆盖最旧备份（index 0），绝不因历史垃圾备份累积而拒绝启动
    fs::write(incompatible_settings_backup_path_at(path, 0), content)
        .map_err(|error| format!("保留不兼容设置备份失败，已拒绝覆盖原设置: {error}"))
}

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
                Ok(content) => match recover_settings_from_content(&content) {
                    Ok((settings, rejected_any_field, unknown_any_field)) => {
                        // 未知字段已由 #[serde(flatten)] extra_fields 保留并随保存写回，
                        // 不需要 backup 兜底；只有真正无法反序列化的字段才需要备份原文，
                        // 避免升级后旧配置里的已删除字段反复触发备份累积到 1000 个上限。
                        let _ = unknown_any_field;
                        if rejected_any_field {
                            preserve_incompatible_settings(target, &content)?;
                        }
                        if rejected_any_field {
                            Self::save_to_path(target, &settings)?;
                        }
                        Ok(Some((settings, content)))
                    }
                    Err(error) => Err(error),
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
            let settings = match recover_settings_from_content(&content) {
                Ok((settings, rejected_any_field, unknown_any_field)) => {
                    // 同主路径：仅真正拒绝的字段触发备份，未知字段由 extra_fields 保留。
                    let _ = unknown_any_field;
                    if rejected_any_field {
                        preserve_incompatible_settings(source, &content)?;
                    }
                    settings
                }
                Err(error) => {
                    last_error = Some(error);
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
            // 原子替换失败时尽力清理临时文件；若清理也失败，
            // 残留文件名含递增 ID 不会被后续保存复用，属理论性垃圾累积。
            let _ = fs::remove_file(&temporary_path).ok();
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

        let loaded: AppSettings =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.language, "persisted-language");
        assert!(
            fs::read_dir(&dir).unwrap().all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")),
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

    #[test]
    fn dead_screenshot_settings_are_not_reintroduced() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/settings/model.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到设置模型源文件");
        let code = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        // 死设置字段：无 UI 暴露、无业务消费，删除后不得回归（旧配置键由
        // extra_fields 保留，不触发备份）。
        assert!(
            !code.contains("screenshot_auto_save") && !code.contains("screenshot_show_hints"),
            "死设置字段 screenshot_auto_save / screenshot_show_hints 不得重新引入"
        );
    }

    #[test]
    fn incompatible_field_recovers_each_compatible_shortcut_instead_of_resetting_settings() {
        let dir = test_dir();
        let target = dir.join("settings.json");
        fs::write(
            &target,
            r#"{
                "toggleShortcut": "Ctrl+Alt+V",
                "screenshotShortcut": "Ctrl+Shift+S",
                "quickpasteShortcut": "Alt+Q",
                "navigateUpShortcut": "W",
                "screenshotHintsEnabled": "not-a-number",
                "hotkeysEnabled": { "invalid": true },
                "unknownFutureSetting": { "keepForFuture": true }
            }"#,
        )
        .unwrap();

        let loaded = SettingsStorage::load_settings_from_paths(&target, &[])
            .expect("单个字段不兼容时仍应恢复其余设置")
            .expect("存在设置文件时必须返回恢复结果");

        assert_eq!(loaded.0.toggle_shortcut, "Ctrl+Alt+V");
        assert_eq!(loaded.0.screenshot_shortcut, "Ctrl+Shift+S");
        assert_eq!(loaded.0.quickpaste_shortcut, "Alt+Q");
        assert_eq!(loaded.0.navigate_up_shortcut, "W");
        assert_eq!(
            loaded.0.screenshot_hints_enabled,
            AppSettings::default().screenshot_hints_enabled
        );
        let backup = incompatible_settings_backup_path(&target);
        assert!(backup.exists(), "不兼容配置必须先保留完整备份");
        assert!(
            fs::read_to_string(backup)
                .unwrap()
                .contains("unknownFutureSetting"),
            "备份必须保留未知字段，避免未来版本设置被本版删除"
        );
        let repaired: AppSettings = serde_json::from_str(&fs::read_to_string(&target).unwrap())
            .expect("恢复结果必须写回为下次可直接加载的有效配置");
        assert_eq!(repaired.toggle_shortcut, "Ctrl+Alt+V");
        assert_eq!(repaired.screenshot_shortcut, "Ctrl+Shift+S");
        assert!(
            !fs::read_to_string(&target)
                .unwrap()
                .contains("not-a-number"),
            "不兼容字段只能留在备份，不能让每次启动反复触发恢复"
        );
        assert!(
            fs::read_to_string(&target)
                .unwrap()
                .contains("unknownFutureSetting"),
            "恢复写回时也必须保留未知未来字段"
        );
        let mut saved_again = loaded.0.clone();
        saved_again.language = "en-US".to_string();
        SettingsStorage::save_to_path(&target, &saved_again)
            .expect("用户下次保存也不能删除未知未来字段");
        assert!(
            fs::read_to_string(&target)
                .unwrap()
                .contains("unknownFutureSetting"),
            "后续保存必须继续保留未知未来字段"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_new_incompatible_document_gets_its_own_backup_instead_of_reusing_a_stale_one() {
        let dir = test_dir();
        let target = dir.join("settings.json");
        fs::write(
            incompatible_settings_backup_path(&target),
            r#"{ "unknownFutureSetting": "old-value" }"#,
        )
        .unwrap();
        // 用真正无法反序列化的字段（toggleShortcut 给了数字）触发备份，验证新旧配置独立备份。
        let original = r#"{ "toggleShortcut": 12345, "unknownFutureSetting": "new-value" }"#;
        fs::write(&target, original).unwrap();

        SettingsStorage::load_settings_from_paths(&target, &[]).expect("不同原始配置仍应可恢复");

        let backups = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains("incompatible")
            })
            .collect::<Vec<_>>();
        assert!(
            backups
                .iter()
                .any(|path| fs::read_to_string(path).unwrap().contains("new-value")),
            "新配置的未知字段必须有独立备份，不能静默复用旧备份"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn legacy_setting_aliases_override_defaults_during_field_by_field_recovery() {
        let dir = test_dir();
        let target = dir.join("settings.json");
        fs::write(
            &target,
            r#"{
                "history_limit": 321,
                "custom_storage_path": "D:/QuickClipboardData",
                "use_custom_storage": true,
                "unknownFutureSetting": "bad"
            }"#,
        )
        .unwrap();

        let loaded = SettingsStorage::load_settings_from_paths(&target, &[])
            .expect("别名与坏字段并存时仍应恢复")
            .expect("存在设置文件时必须返回恢复结果");

        assert_eq!(loaded.0.history_limit, 321);
        assert_eq!(
            loaded.0.custom_storage_path.as_deref(),
            Some("D:/QuickClipboardData")
        );
        assert!(loaded.0.use_custom_storage);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unknown_field_does_not_create_backup_after_recover() {
        let dir = test_dir();
        let target = dir.join("settings.json");
        fs::write(
            &target,
            r#"{ "toggleShortcut": "Ctrl+Alt+V", "futureSetting": "keep-me" }"#,
        )
        .unwrap();

        let loaded = SettingsStorage::load_settings_from_paths(&target, &[])
            .expect("未知字段不应阻止恢复")
            .expect("存在设置文件时必须返回恢复结果");

        assert!(
            !incompatible_settings_backup_path(&target).exists(),
            "未知字段已被 extra_fields 保留，不应触发备份累积"
        );
        assert_eq!(loaded.0.toggle_shortcut, "Ctrl+Alt+V");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn upgrade_load_keeps_every_compatible_field_and_unknown_fields_without_backup() {
        let dir = test_dir();
        let target = dir.join("settings.json");
        // 模拟旧版本导出的设置：含一个已删除字段（未来未知键）、一个旧别名键、
        // 一个当前仍存在的常规键。升级加载必须全部兼容保留且不触发备份。
        let original = r#"{
          "language": "en-US",
          "edgeHideEnabled": false,
          "screenshotAutoSave": true,
          "toggleShortcut": "Ctrl+Shift+T"
        }"#;
        fs::write(&target, original).unwrap();

        let loaded = SettingsStorage::load_settings_from_paths(&target, &[]).unwrap().unwrap();
        assert_eq!(loaded.0.language, "en-US", "常规键必须保留");
        assert!(!loaded.0.edge_hide_enabled, "布尔键必须保留");
        // screenshotAutoSave 是已删除的旧字段：兼容恢复后进入 extra_fields 保留，
        // 保存回写时不得丢失，且不得触发任何备份文件（§7bd80d6b 契约）。
        let backup_exists = dir
            .read_dir()
            .unwrap()
            .any(|entry| entry.unwrap().file_name().to_string_lossy().contains("incompatible"));
        assert!(!backup_exists, "未知字段不得触发备份");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn backup_failure_refuses_to_overwrite_an_incompatible_settings_document() {
        let dir = test_dir();
        let target = dir.join("settings.json");
        // 用真正无法反序列化的字段（toggleShortcut 给了数字）验证备份失败时拒绝覆盖。
        let original = r#"{ "toggleShortcut": 12345, "unknownFutureSetting": "bad" }"#;
        fs::write(&target, original).unwrap();
        fs::create_dir(incompatible_settings_backup_path(&target)).unwrap();

        let loaded = SettingsStorage::load_settings_from_paths(&target, &[]);

        assert!(loaded.is_err(), "无法安全备份时必须拒绝恢复写回");
        assert_eq!(fs::read_to_string(&target).unwrap(), original);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn backup_slot_exhaustion_overwrites_oldest_instead_of_refusing_launch() {
        let dir = test_dir();
        let target = dir.join("settings.json");
        // 填满 1000 个备份槽位，内容各不相同，模拟历史垃圾备份累积到上限。
        for index in 0..=999 {
            fs::write(
                incompatible_settings_backup_path_at(&target, index),
                format!("stale-backup-{index}"),
            )
            .unwrap();
        }

        // 槽位耗尽后再保存新的不兼容内容：必须覆盖最旧备份而不是拒绝启动。
        preserve_incompatible_settings(&target, "new-incompatible-content")
            .expect("备份槽位耗尽时必须覆盖最旧备份而不是拒绝启动");

        assert_eq!(
            fs::read_to_string(incompatible_settings_backup_path(&target)).unwrap(),
            "new-incompatible-content",
            "最旧备份（index 0）必须被新内容覆盖"
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
