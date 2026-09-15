use std::{collections::HashMap, fs, path::{Path, PathBuf}, time::SystemTime};
use chrono::Local;
use serde::Serialize;

use crate::services::{get_data_directory, get_settings, update_settings};
use crate::services::settings::storage::SettingsStorage;
use crate::services::database::{init_database};
use crate::services::database::connection::{close_database, with_connection};
use crate::services::database::tombstones::record_sync_tombstone_in_conn;
use crate::services::system::hotkey::reload_from_settings;

#[derive(Debug, Clone, Serialize)]
pub struct TargetDataInfo {
    pub has_data: bool,
    pub has_database: bool,
    pub has_images: bool,
    pub has_image_library: bool,
    pub database_size: u64,
    pub images_count: usize,
    pub images_size: u64,
    pub image_library_count: usize,
    pub image_library_size: u64,
}

pub fn check_target_has_data(target_dir: &Path) -> Result<TargetDataInfo, String> {
    let db_path = target_dir.join("quickclipboard.db");
    let images_dir = target_dir.join("clipboard_images");
    let image_library_dir = target_dir.join("image_library");
    
    let has_database = db_path.exists();
    let database_size = if has_database {
        fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };
    
    let (has_images, images_count, images_size) = if images_dir.exists() {
        let mut count = 0usize;
        let mut size = 0u64;
        if let Ok(entries) = fs::read_dir(&images_dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        count += 1;
                        size += meta.len();
                    }
                }
            }
        }
        (count > 0, count, size)
    } else {
        (false, 0, 0)
    };
    
    let (has_image_library, image_library_count, image_library_size) = if image_library_dir.exists() {
        let mut count = 0usize;
        let mut size = 0u64;
        if let Ok(entries) = fs::read_dir(&image_library_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Ok(sub_entries) = fs::read_dir(&path) {
                        for sub_entry in sub_entries.flatten() {
                            if let Ok(meta) = sub_entry.metadata() {
                                if meta.is_file() {
                                    count += 1;
                                    size += meta.len();
                                }
                            }
                        }
                    }
                } else if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        count += 1;
                        size += meta.len();
                    }
                }
            }
        }
        (count > 0, count, size)
    } else {
        (false, 0, 0)
    };
    
    Ok(TargetDataInfo {
        has_data: has_database || has_images || has_image_library,
        has_database,
        has_images,
        has_image_library,
        database_size,
        images_count,
        images_size,
        image_library_count,
        image_library_size,
    })
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    if !dst.exists() {
        fs::create_dir_all(dst).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| format!("复制文件失败: {}", e))?;
        }
    }
    Ok(())
}

fn backup_full_zip(dir: &Path) -> Result<Option<PathBuf>, String> {
    let db = dir.join("quickclipboard.db");
    let images_dir = dir.join("clipboard_images");
    let app_icons_dir = dir.join("app_icons");
    // D3:备份清单必须覆盖全部会被 reset/替换导入清理的目录——重置全部数据
    // 会删 clipboard_images + image_library + app_icons + db,替换导入会删
    // clipboard_images + image_library + app_icons;若备份缺 image_library
    // 与 pin_images,执行这两类操作前图库与贴图数据永久丢失,备份里没有。
    let image_library_dir = dir.join("image_library");
    let pin_images_dir = dir.join("pin_images");
    if !db.exists()
        && !images_dir.exists()
        && !image_library_dir.exists()
        && !pin_images_dir.exists()
        && !app_icons_dir.exists()
    {
        return Ok(None);
    }

    let backups = dir.join("backups");
    fs::create_dir_all(&backups).map_err(|e| e.to_string())?;
    let ts_str = Local::now().format("%Y%m%d-%H%M%S").to_string();
    let name = format!("quickclipboard-backup-{}.zip", ts_str);
    let target = backups.join(&name);

    let file = fs::File::create(&target).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    if db.exists() {
        let mut f = fs::File::open(&db).map_err(|e| e.to_string())?;
        zip.start_file("quickclipboard.db", options).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, &mut zip).map_err(|e| e.to_string())?;
    }

    if images_dir.exists() {
        for entry in fs::read_dir(&images_dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
                    let zip_path = format!("clipboard_images/{}", fname);
                    let mut f = fs::File::open(&path).map_err(|e| e.to_string())?;
                    zip.start_file(&zip_path, options).map_err(|e| e.to_string())?;
                    std::io::copy(&mut f, &mut zip).map_err(|e| e.to_string())?;
                }
            }
        }
    }

    if image_library_dir.exists() {
        for entry in fs::read_dir(&image_library_dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
                    let zip_path = format!("image_library/{}", fname);
                    let mut f = fs::File::open(&path).map_err(|e| e.to_string())?;
                    zip.start_file(&zip_path, options).map_err(|e| e.to_string())?;
                    std::io::copy(&mut f, &mut zip).map_err(|e| e.to_string())?;
                }
            }
        }
    }

    if pin_images_dir.exists() {
        for entry in fs::read_dir(&pin_images_dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
                    let zip_path = format!("pin_images/{}", fname);
                    let mut f = fs::File::open(&path).map_err(|e| e.to_string())?;
                    zip.start_file(&zip_path, options).map_err(|e| e.to_string())?;
                    std::io::copy(&mut f, &mut zip).map_err(|e| e.to_string())?;
                }
            }
        }
    }

    if app_icons_dir.exists() {
        for entry in fs::read_dir(&app_icons_dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
                    let zip_path = format!("app_icons/{}", fname);
                    let mut f = fs::File::open(&path).map_err(|e| e.to_string())?;
                    zip.start_file(&zip_path, options).map_err(|e| e.to_string())?;
                    std::io::copy(&mut f, &mut zip).map_err(|e| e.to_string())?;
                }
            }
        }
    }

    if let Ok(settings_path) = SettingsStorage::get_settings_path() {
        if settings_path.exists() {
            if let Ok(mut f) = fs::File::open(&settings_path) {
                let _ = zip.start_file("settings.json", options);
                let _ = std::io::copy(&mut f, &mut zip);
            }
        }
    }

    zip.finish().map_err(|e| e.to_string())?;
    enforce_backup_retention(&backups, 10)?;
    Ok(Some(target))
}

// 获取备份列表
#[derive(Debug, Clone, Serialize)]
pub struct BackupInfo {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub created_at: String,
}

pub fn list_backups() -> Result<Vec<BackupInfo>, String> {
    let current_dir = get_current_storage_dir()?;
    let backups_dir = current_dir.join("backups");
    if !backups_dir.exists() { return Ok(vec![]); }
    
    let mut items: Vec<BackupInfo> = Vec::new();
    for e in fs::read_dir(&backups_dir).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let p = e.path();
        let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
        if !fname.starts_with("quickclipboard-backup-") || !fname.ends_with(".zip") { continue; }
        let md = e.metadata().map_err(|e| e.to_string())?;
        let size = md.len();
        let modified = md.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let datetime: chrono::DateTime<Local> = modified.into();
        items.push(BackupInfo {
            path: p.to_string_lossy().to_string(),
            name: fname,
            size,
            created_at: datetime.format("%Y-%m-%d %H:%M:%S").to_string(),
        });
    }
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(items)
}

fn enforce_backup_retention(backups_dir: &Path, keep: usize) -> Result<(), String> {
    let mut items: Vec<(SystemTime, PathBuf)> = Vec::new();
    for e in fs::read_dir(backups_dir).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let p = e.path();
        let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !fname.starts_with("quickclipboard-backup-") || !fname.ends_with(".zip") { continue; }
        let md = e.metadata().map_err(|e| e.to_string())?;
        let t = md.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        items.push((t, p));
    }
    items.sort_by(|a, b| b.0.cmp(&a.0));
    if items.len() > keep {
        for (_, p) in items.into_iter().skip(keep) {
            let _ = fs::remove_file(p);
        }
    }
    Ok(())
}

pub fn reset_all_data() -> Result<String, String> {
    let current_dir = get_current_storage_dir()?;
    let default_dir = get_default_data_dir()?;

    let _ = crate::services::database::connection::with_connection(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(FULL); PRAGMA wal_checkpoint(TRUNCATE);")
    });
    let _ = backup_full_zip(&current_dir);
    if current_dir != default_dir { let _ = backup_full_zip(&default_dir); }

    close_database();

    fn clean_dir(dir: &Path) -> Result<(), String> {
        let images = dir.join("clipboard_images");
        if images.exists() { let _ = fs::remove_dir_all(&images); }
        let image_library = dir.join("image_library");
        if image_library.exists() { let _ = fs::remove_dir_all(&image_library); }
        let app_icons = dir.join("app_icons");
        if app_icons.exists() { let _ = fs::remove_dir_all(&app_icons); }
        // r6-db-2:重置所有数据必须清 pin_images——与 D3/M2 已修的"备份/替换导入/
        // 合并"目录清单对齐。此前遗漏:重置后贴图文件残留磁盘,与"全部清空"
        // 语义不符,且下次重置的备份里不会有这份残留(旧备份才有)。
        let pin_images = dir.join("pin_images");
        if pin_images.exists() { let _ = fs::remove_dir_all(&pin_images); }
        for name in ["quickclipboard.db", "quickclipboard.db-shm", "quickclipboard.db-wal"] {
            let p = dir.join(name);
            if p.exists() { let _ = fs::remove_file(&p); }
        }
        Ok(())
    }

    clean_dir(&current_dir)?;
    if current_dir != default_dir { clean_dir(&default_dir)?; }

    let mut defaults = crate::services::AppSettings::default();
    defaults.use_custom_storage = false;
    defaults.custom_storage_path = None;
    update_settings(defaults.clone())?;

    let db_path = default_dir.join("quickclipboard.db");
    init_database(db_path.to_str().ok_or("数据库路径无效")?)?;
    let _ = crate::services::database::connection::with_connection(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
    });

    let _ = reload_from_settings();

    Ok(default_dir.to_string_lossy().to_string())
}

// zip 条目名安全检查:拒绝目录穿越、根相对/绝对路径与盘符路径,
// 保证 join 到 temp_root 后仍在其下,杜绝 zip-slip 写出任意文件。
// 返回可安全 join 的相对路径;不合法返回 None(该条目整体跳过)。
fn safe_zip_entry_name(name: &str) -> Option<&str> {
    if name.is_empty() {
        return None;
    }
    if name.contains("..") {
        return None;
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return None;
    }
    // Windows 盘符形式(C:、C:/ 等)与 UNC 都含冒号,一律拒绝
    if name.contains(':') {
        return None;
    }
    Some(name)
}

pub fn import_data_zip(zip_path: PathBuf, mode: &str) -> Result<String, String> {
    if !zip_path.exists() {
        return Err("导入文件不存在".into());
    }

    let temp_root = std::env::temp_dir().join(format!("quickclipboard_import_{}", fastrand::u32(..)));
    fs::create_dir_all(&temp_root).map_err(|e| e.to_string())?;
    let file = fs::File::open(&zip_path).map_err(|e| format!("打开导入文件失败: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("读取压缩包失败: {}", e))?;
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = f.name().to_string();
        if safe_zip_entry_name(&name).is_none() { continue; }
        let out_path = temp_root.join(&name);
        if f.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = out_path.parent() { fs::create_dir_all(p).map_err(|e| e.to_string())?; }
            let mut out = fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut f, &mut out).map_err(|e| e.to_string())?;
        }
    }

    let imported_db = temp_root.join("quickclipboard.db");
    let imported_images = temp_root.join("clipboard_images");
    let imported_image_library = temp_root.join("image_library");
    let imported_pin_images = temp_root.join("pin_images");
    let imported_app_icons = temp_root.join("app_icons");
    let imported_settings = temp_root.join("settings.json");

    match mode {
        "replace" => {
            let current_dir_for_backup = get_current_storage_dir()?;
            let _ = crate::services::database::connection::with_connection(|conn| {
                conn.execute_batch("PRAGMA wal_checkpoint(FULL); PRAGMA wal_checkpoint(TRUNCATE);")
            });
            let _ = backup_full_zip(&current_dir_for_backup);
            let mut new_settings = if imported_settings.exists() {
                let s = fs::read_to_string(&imported_settings).map_err(|e| e.to_string())?;
                serde_json::from_str::<crate::services::AppSettings>(&s).map_err(|e| e.to_string())?
            } else {
                get_settings()
            };
            // 守不变量:导入的 settings.json 可能带 hide=false/hover=true 违规组合,
            // 落地前统一归一化,防止下次开启 hide 时意外弹出触发条
            new_settings.normalize_edge_hover_invariant();
            if crate::services::is_portable_runtime() {
                new_settings.use_custom_storage = false;
                new_settings.custom_storage_path = None;
            }

            let target_dir = if new_settings.use_custom_storage {
                if let Some(ref path) = new_settings.custom_storage_path {
                    let p = PathBuf::from(path);
                    if p.exists() { p } else {
                        new_settings.use_custom_storage = false;
                        new_settings.custom_storage_path = None;
                        get_default_data_dir()?
                    }
                } else {
                    get_default_data_dir()?
                }
            } else {
                get_default_data_dir()?
            };

            // H1:修复"关库后散布 ? 早返,失败时既不重开库又已改设置"——
            // 与 D2 export_data_zip 同款病(export 修了,import replace 漏了)。
            // ①close_database() 之后的替换步骤收进闭包,全部错误经闭包返回;
            // ②闭包无论成败外层无条件 init_database(target 库),绝不留下关闭态;
            // ③update_settings 后置到替换与重开都成功之后,失败时不改设置,
            //   避免下次启动指向半替换目录。
            close_database();
            let result = (|| -> Result<(), String> {
                let target_images = target_dir.join("clipboard_images");
                if target_images.exists() { fs::remove_dir_all(&target_images).map_err(|e| e.to_string())?; }
                if imported_images.exists() { copy_dir_all(&imported_images, &target_images)?; }
                let target_image_library = target_dir.join("image_library");
                if target_image_library.exists() { fs::remove_dir_all(&target_image_library).map_err(|e| e.to_string())?; }
                if imported_image_library.exists() { copy_dir_all(&imported_image_library, &target_image_library)?; }
                let target_app_icons = target_dir.join("app_icons");
                if target_app_icons.exists() { fs::remove_dir_all(&target_app_icons).map_err(|e| e.to_string())?; }
                if imported_app_icons.exists() { copy_dir_all(&imported_app_icons, &target_app_icons)?; }
                // M2:pin_images 与 clipboard_images/image_library 同列——导出已收,
                // 替换导入漏收会让贴图文件成为"数据要清但备份没有"的孤儿(类比
                // D3 目录清单),必须与目标库同清同补。
                let target_pin_images = target_dir.join("pin_images");
                if target_pin_images.exists() { fs::remove_dir_all(&target_pin_images).map_err(|e| e.to_string())?; }
                if imported_pin_images.exists() { copy_dir_all(&imported_pin_images, &target_pin_images)?; }

                let src_db = temp_root.join("quickclipboard.db");
                let dst_db = target_dir.join("quickclipboard.db");
                if src_db.exists() {
                    if let Some(p) = dst_db.parent() { fs::create_dir_all(p).map_err(|e| e.to_string())?; }
                    fs::copy(&src_db, &dst_db).map_err(|e| e.to_string())?;
                }
                for name in ["quickclipboard.db-shm", "quickclipboard.db-wal"] {
                    let p = target_dir.join(name);
                    if p.exists() { let _ = fs::remove_file(&p); }
                }
                Ok(())
            })();

            // 无论成败都重开库（成功切新库,失败至少保持本会话 DB 可用）。
            let db_path = target_dir.join("quickclipboard.db");
            let reopen = init_database(db_path.to_str().ok_or("数据库路径无效")?);
            let _ = crate::services::database::connection::with_connection(|conn| {
                conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            });
            let _ = reload_from_settings();

            let _ = fs::remove_dir_all(&temp_root);
            // 替换成功且重开成功后才持久化设置;任一失败都不改设置。
            result?;
            reopen?;
            update_settings(new_settings.clone())?;
            Ok(target_dir.to_string_lossy().to_string())
        }
        "merge" => {
            let current_dir = get_current_storage_dir()?;

            let target_images = current_dir.join("clipboard_images");
            if imported_images.exists() {
                if !target_images.exists() { fs::create_dir_all(&target_images).map_err(|e| e.to_string())?; }
                merge_dir_overwrite(&imported_images, &target_images)?;
            }

            let target_image_library = current_dir.join("image_library");
            if imported_image_library.exists() {
                if !target_image_library.exists() { fs::create_dir_all(&target_image_library).map_err(|e| e.to_string())?; }
                merge_dir_overwrite(&imported_image_library, &target_image_library)?;
            }

            let target_app_icons = current_dir.join("app_icons");
            if imported_app_icons.exists() {
                if !target_app_icons.exists() { fs::create_dir_all(&target_app_icons).map_err(|e| e.to_string())?; }
                merge_dir_overwrite(&imported_app_icons, &target_app_icons)?;
            }

            let target_pin_images = current_dir.join("pin_images");
            if imported_pin_images.exists() {
                if !target_pin_images.exists() { fs::create_dir_all(&target_pin_images).map_err(|e| e.to_string())?; }
                merge_dir_overwrite(&imported_pin_images, &target_pin_images)?;
            }

            if imported_db.exists() {
                merge_database(&imported_db)?;
            }

            let _ = fs::remove_dir_all(&temp_root);
            Ok(current_dir.to_string_lossy().to_string())
        }
        _ => {
            let _ = fs::remove_dir_all(&temp_root);
            Err("不支持的导入模式".into())
        }
    }
}
fn safe_move_item(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() { return Ok(()); }
    if fs::rename(src, dst).is_err() {
        if src.is_dir() {
            copy_dir_all(src, dst)?;
            fs::remove_dir_all(src).map_err(|e| format!("删除源目录失败: {}", e))?;
        } else {
            if let Some(parent) = dst.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
            fs::copy(src, dst).map_err(|e| format!("复制文件失败: {}", e))?;
            fs::remove_file(src).map_err(|e| format!("删除源文件失败: {}", e))?;
        }
    }
    Ok(())
}

pub fn get_default_data_dir() -> Result<PathBuf, String> {
    let settings_path = SettingsStorage::get_settings_path()?;
    settings_path.parent().map(|p| p.to_path_buf()).ok_or("无法获取默认数据目录".to_string())
}

pub fn get_current_storage_dir() -> Result<PathBuf, String> {
    get_data_directory()
}

// mode: "source_only" | "target_only" | "merge"
pub fn change_storage_dir(new_dir: PathBuf, mode: &str) -> Result<PathBuf, String> {
    if crate::services::is_portable_runtime() {
        return Err("便携版不支持更改存储路径".into());
    }
    if !new_dir.exists() { fs::create_dir_all(&new_dir).map_err(|e| e.to_string())?; }

    let current_dir = get_current_storage_dir()?;
    if new_dir == current_dir {
        return Err("新位置与当前存储位置相同，无需迁移".to_string());
    }

    change_storage_dir_internal(&current_dir, &new_dir, mode)?;

    let mut settings = get_settings();
    settings.use_custom_storage = true;
    settings.custom_storage_path = Some(new_dir.to_string_lossy().to_string());
    update_settings(settings.clone())?;

    let db_path = new_dir.join("quickclipboard.db");
    init_database(db_path.to_str().ok_or("数据库路径无效")?)?;
    let _ = crate::services::database::connection::with_connection(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
    });

    Ok(new_dir)
}

fn merge_dir_overwrite(src: &Path, dst: &Path) -> Result<(), String> {
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            fs::create_dir_all(&dst_path).map_err(|e| e.to_string())?;
            merge_dir_overwrite(&src_path, &dst_path)?;
        } else {
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(&src_path, &dst_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn merge_dir_no_overwrite(src: &Path, dst: &Path) -> Result<(), String> {
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            if !dst_path.exists() { fs::create_dir_all(&dst_path).map_err(|e| e.to_string())?; }
            merge_dir_no_overwrite(&src_path, &dst_path)?;
        } else if !dst_path.exists() {
            if let Some(parent) = dst_path.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
            fs::copy(&src_path, &dst_path).map_err(|e| format!("复制文件失败: {}", e))?;
        }
    }
    Ok(())
}

fn merge_database(src_db: &Path) -> Result<(), String> {
    with_connection(|conn| {
        let import_path = src_db
            .to_str()
            .ok_or(rusqlite::Error::InvalidPath("bad path".into()))?;
        conn.execute("ATTACH DATABASE ?1 AS importdb", [import_path])?;

        // M1:ATTACH 导入必须包事务——merge_* 里任一失败(如撞 uuid 唯一索引)
        // 前面已落地的 groups/favorites/clipboard 都要回滚,否则半合并数据
        // 无法回滚,重复导入越积越多(INSERT OR IGNORE 幂等只兜部分)。
        let tx = conn.unchecked_transaction()?;
        merge_groups_from_importdb(&tx)?;
        merge_favorites_from_importdb(&tx)?;

        // 记录导入库 clipboard.id 到当前库新 id 的映射，用于迁移 clipboard_data。
        let id_mapping = merge_clipboard_from_importdb(&tx)?;
        merge_clipboard_data_from_importdb(&tx, &id_mapping)?;

        // r7-db-4:删除墓碑也必须并入——源库已删除的记录不能在导入后复活。
        // 只同步"比本地更新的墓碑"(源端 deleted_at > 本地),避免旧的删除
        // 反向盖掉本地较新的复活。逐行取源端 deleted_at,应用全部是
        // LWW 语义(每行最少删一次>插入一次)。
        if importdb_has_table(conn, "sync_tombstones")? {
            let mut tombstone_stmt = conn.prepare(
                "SELECT collection, item_id, source_device_id, deleted_at
                 FROM importdb.sync_tombstones",
            )?;
            let tombstones = tombstone_stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?;
            for tombstone in tombstones {
                let (collection, item_id, source_device_id, deleted_at) = tombstone?;
                if collection.trim().is_empty() || item_id.trim().is_empty() {
                    continue;
                }
                // 应用墓碑(含 LWW 语义:源端 deleted_at > 本地才生效)
                let _ = record_sync_tombstone_in_conn(
                    conn,
                    &collection,
                    &item_id,
                    &source_device_id,
                    deleted_at,
                );
            }
        }

        reorder_clipboard_by_time(&tx);
        tx.commit()?;

        // r6-db-1:无论成败都必须拆离 importdb——M1 修过"失败时 DETACH 不执行"使
        // importdb 残留 ATTACH 在全局连接上,下次 merge_database 再 ATTACH 撞
        // "already in use",后续所有 merge 永久失败。事务已包 commit(成功=DETACH 前
        // 已落盘;失败=tx Drop 回滚),DETACH 与事务成败解耦,独立无条件执行。
        let _ = conn.execute("DETACH DATABASE importdb", []);
        Ok(())
    })?;
    Ok(())
}

fn importdb_has_table(conn: &rusqlite::Connection, table: &str) -> rusqlite::Result<bool> {
    let sql = "SELECT COUNT(*) FROM importdb.sqlite_master WHERE type = 'table' AND name = ?1";
    let count: i64 = conn.query_row(sql, [table], |row| row.get(0))?;
    Ok(count > 0)
}

fn importdb_table_columns(
    conn: &rusqlite::Connection,
    table: &str,
) -> rusqlite::Result<std::collections::HashSet<String>> {
    let pragma_sql = format!("PRAGMA importdb.table_info({})", table);
    let mut stmt = conn.prepare(&pragma_sql)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut columns = std::collections::HashSet::new();
    for col in rows {
        columns.insert(col?);
    }
    Ok(columns)
}

fn has_col(columns: &std::collections::HashSet<String>, name: &str) -> bool {
    columns.contains(name)
}

fn merge_groups_from_importdb(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    if !importdb_has_table(conn, "groups")? {
        return Ok(());
    }

    let cols = importdb_table_columns(conn, "groups")?;
    let icon_expr = if has_col(&cols, "icon") {
        "icon"
    } else {
        "'ti ti-folder'"
    };
    let color_expr = if has_col(&cols, "color") {
        "color"
    } else {
        "'#dc2626'"
    };
    let order_expr = if has_col(&cols, "order_index") {
        "order_index"
    } else {
        "0"
    };
    let created_expr = if has_col(&cols, "created_at") {
        "created_at"
    } else {
        "CAST(strftime('%s','now') AS INTEGER)"
    };
    let updated_expr = if has_col(&cols, "updated_at") {
        "updated_at"
    } else {
        "CAST(strftime('%s','now') AS INTEGER)"
    };

    let sql = format!(
        "INSERT OR IGNORE INTO groups (name, icon, color, order_index, created_at, updated_at)
         SELECT name, {icon}, {color}, {order_index}, {created_at}, {updated_at}
         FROM importdb.groups",
        icon = icon_expr,
        color = color_expr,
        order_index = order_expr,
        created_at = created_expr,
        updated_at = updated_expr
    );
    let _ = conn.execute(&sql, []);
    Ok(())
}

fn merge_favorites_from_importdb(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    if !importdb_has_table(conn, "favorites")? {
        return Ok(());
    }

    let cols = importdb_table_columns(conn, "favorites")?;
    let html_expr = if has_col(&cols, "html_content") {
        "html_content"
    } else {
        "NULL"
    };
    let content_type_expr = if has_col(&cols, "content_type") {
        "content_type"
    } else {
        "'text'"
    };
    let image_id_expr = if has_col(&cols, "image_id") {
        "image_id"
    } else {
        "NULL"
    };
    let group_expr = if has_col(&cols, "group_name") {
        "group_name"
    } else {
        "'全部'"
    };
    let item_order_expr = if has_col(&cols, "item_order") {
        "item_order"
    } else {
        "0"
    };
    let created_expr = if has_col(&cols, "created_at") {
        "created_at"
    } else {
        "CAST(strftime('%s','now') AS INTEGER)"
    };
    let updated_expr = if has_col(&cols, "updated_at") {
        "updated_at"
    } else {
        "CAST(strftime('%s','now') AS INTEGER)"
    };
    let paste_count_expr = if has_col(&cols, "paste_count") {
        "paste_count"
    } else {
        "0"
    };
    let char_count_expr = if has_col(&cols, "char_count") {
        "char_count"
    } else {
        "NULL"
    };

    let sql = format!(
        "INSERT OR IGNORE INTO favorites
         (id, title, content, html_content, content_type, image_id, group_name, item_order, paste_count, char_count, created_at, updated_at)
         SELECT
            id,
            title,
            content,
            {html},
            {content_type},
            {image_id},
            {group_name},
            {item_order},
            {paste_count},
            {char_count},
            {created_at},
            {updated_at}
         FROM importdb.favorites",
        html = html_expr,
        content_type = content_type_expr,
        image_id = image_id_expr,
        group_name = group_expr,
        item_order = item_order_expr,
        paste_count = paste_count_expr,
        char_count = char_count_expr,
        created_at = created_expr,
        updated_at = updated_expr
    );
    let _ = conn.execute(&sql, []);
    Ok(())
}

fn merge_clipboard_from_importdb(
    conn: &rusqlite::Connection,
) -> rusqlite::Result<std::collections::HashMap<i64, i64>> {
    let mut id_map = HashMap::new();
    if !importdb_has_table(conn, "clipboard")? {
        return Ok(id_map);
    }

    let cols = importdb_table_columns(conn, "clipboard")?;
    let id_expr = if has_col(&cols, "id") { "id" } else { "NULL" };
    let html_expr = if has_col(&cols, "html_content") {
        "html_content"
    } else {
        "NULL"
    };
    let content_type_expr = if has_col(&cols, "content_type") {
        "content_type"
    } else {
        "'text'"
    };
    let image_id_expr = if has_col(&cols, "image_id") {
        "image_id"
    } else {
        "NULL"
    };
    let is_pinned_expr = if has_col(&cols, "is_pinned") {
        "is_pinned"
    } else {
        "0"
    };
    let paste_count_expr = if has_col(&cols, "paste_count") {
        "paste_count"
    } else {
        "0"
    };
    let source_app_expr = if has_col(&cols, "source_app") {
        "source_app"
    } else {
        "NULL"
    };
    let source_icon_hash_expr = if has_col(&cols, "source_icon_hash") {
        "source_icon_hash"
    } else {
        "NULL"
    };
    let char_count_expr = if has_col(&cols, "char_count") {
        "char_count"
    } else {
        "NULL"
    };
    let uuid_expr = if has_col(&cols, "uuid") { "uuid" } else { "NULL" };
    let source_device_id_expr = if has_col(&cols, "source_device_id") {
        "source_device_id"
    } else {
        "NULL"
    };
    let is_remote_expr = if has_col(&cols, "is_remote") {
        "is_remote"
    } else {
        "0"
    };
    let created_expr = if has_col(&cols, "created_at") {
        "created_at"
    } else {
        "CAST(strftime('%s','now') AS INTEGER)"
    };
    let updated_expr = if has_col(&cols, "updated_at") {
        "updated_at"
    } else {
        "CAST(strftime('%s','now') AS INTEGER)"
    };

    let select_sql = format!(
        "SELECT
            {id},
            content,
            {html},
            {content_type},
            {image_id},
            {is_pinned},
            {paste_count},
            {source_app},
            {source_icon_hash},
            {char_count},
            {uuid},
            {source_device_id},
            {is_remote},
            {created_at},
            {updated_at}
         FROM importdb.clipboard",
        id = id_expr,
        html = html_expr,
        content_type = content_type_expr,
        image_id = image_id_expr,
        is_pinned = is_pinned_expr,
        paste_count = paste_count_expr,
        source_app = source_app_expr,
        source_icon_hash = source_icon_hash_expr,
        char_count = char_count_expr,
        uuid = uuid_expr,
        source_device_id = source_device_id_expr,
        is_remote = is_remote_expr,
        created_at = created_expr,
        updated_at = updated_expr
    );

    let mut query_stmt = conn.prepare(&select_sql)?;
    let mut insert_stmt = conn.prepare(
        "INSERT OR IGNORE INTO clipboard
         (content, html_content, content_type, image_id, is_pinned, paste_count, source_app, source_icon_hash, char_count, uuid, source_device_id, is_remote, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
    )?;

    let rows = query_stmt.query_map([], |row| {
        Ok((
            row.get::<_, Option<i64>>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<i64>>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, Option<String>>(11)?,
            row.get::<_, i64>(12)?,
            row.get::<_, i64>(13)?,
            row.get::<_, i64>(14)?,
        ))
    })?;

    for row in rows {
        let (
            old_id,
            content,
            html_content,
            content_type,
            image_id,
            is_pinned,
            paste_count,
            source_app,
            source_icon_hash,
            char_count,
            uuid,
            source_device_id,
            is_remote,
            created_at,
            updated_at,
        ) = row?;

        insert_stmt.execute(rusqlite::params![
            content,
            html_content,
            content_type,
            image_id,
            is_pinned,
            paste_count,
            source_app,
            source_icon_hash,
            char_count,
            uuid,
            source_device_id,
            is_remote,
            created_at,
            updated_at
        ])?;

        if let Some(old_id) = old_id {
            id_map.insert(old_id, conn.last_insert_rowid());
        }
    }

    Ok(id_map)
}

fn merge_clipboard_data_from_importdb(
    conn: &rusqlite::Connection,
    clipboard_id_map: &std::collections::HashMap<i64, i64>,
) -> rusqlite::Result<()> {
    if !importdb_has_table(conn, "clipboard_data")? {
        return Ok(());
    }

    // 收藏原始格式：目标 id 不变，合并时不覆盖现有格式。
    let _ = conn.execute(
        "INSERT OR IGNORE INTO clipboard_data
         (target_kind, target_id, format_name, raw_data, is_primary, format_order, created_at, updated_at)
         SELECT d.target_kind, d.target_id, d.format_name, d.raw_data, d.is_primary, d.format_order, d.created_at, d.updated_at
         FROM importdb.clipboard_data d
         WHERE d.target_kind = 'favorite' AND EXISTS (SELECT 1 FROM favorites f WHERE f.id = d.target_id)",
        [],
    );

    if clipboard_id_map.is_empty() {
        return Ok(());
    }

    let mut source_stmt = conn.prepare(
        "SELECT format_name, raw_data, is_primary, format_order, created_at, updated_at
         FROM importdb.clipboard_data
         WHERE target_kind = 'clipboard' AND target_id = ?1
         ORDER BY format_order, id",
    )?;
    let mut insert_stmt = conn.prepare(
        "INSERT OR IGNORE INTO clipboard_data
         (target_kind, target_id, format_name, raw_data, is_primary, format_order, created_at, updated_at)
         VALUES ('clipboard', ?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;

    for (old_id, new_id) in clipboard_id_map {
        let old_target = old_id.to_string();
        let rows = source_stmt.query_map([old_target], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?;

        for row in rows {
            let (format_name, raw_data, is_primary, format_order, created_at, updated_at) = row?;
            insert_stmt.execute(rusqlite::params![
                new_id.to_string(),
                format_name,
                raw_data,
                is_primary,
                format_order,
                created_at,
                updated_at
            ])?;
        }
    }

    Ok(())
}

fn reorder_clipboard_by_time(conn: &rusqlite::Connection) {
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id FROM clipboard ORDER BY is_pinned DESC, created_at DESC"
    ) {
        let ids: Vec<i64> = stmt.query_map([], |row| row.get(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        let count = ids.len() as i64;
        for (i, id) in ids.iter().enumerate() {
            conn.execute("UPDATE clipboard SET item_order = ? WHERE id = ?",
                rusqlite::params![count - i as i64, id]).ok();
        }
    }
}

pub fn reset_storage_dir_to_default(mode: &str) -> Result<PathBuf, String> {
    if crate::services::is_portable_runtime() {
        return Err("便携版不支持重置存储路径".into());
    }
    let default_dir = get_default_data_dir()?;
    let current_dir = get_current_storage_dir()?;

    if current_dir == default_dir {
        return Err("当前已在默认存储位置".to_string());
    }

    change_storage_dir_internal(&current_dir, &default_dir, mode)?;

    let mut settings = get_settings();
    settings.use_custom_storage = false;
    settings.custom_storage_path = None;
    update_settings(settings.clone())?;

    let db_path = default_dir.join("quickclipboard.db");
    init_database(db_path.to_str().ok_or("数据库路径无效")?)?;
    let _ = crate::services::database::connection::with_connection(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
    });

    Ok(default_dir)
}

fn change_storage_dir_internal(src_dir: &Path, dst_dir: &Path, mode: &str) -> Result<(), String> {
    let _ = crate::services::database::connection::with_connection(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(FULL); PRAGMA wal_checkpoint(TRUNCATE);")
    });
    let _ = backup_full_zip(src_dir);
    if check_target_has_data(dst_dir)?.has_data { let _ = backup_full_zip(dst_dir); }

    close_database();

    // H2:关库后的迁移操作收进闭包,全部错误经闭包返回;外层无论成败统一
    // 重开 src 库,绝不留下关闭态——磁盘满/跨盘回退复制失败时本会话 DB
    // 仍可用,且设置未改(update_settings 在 internal 成功后才执行)。
    let src_db = src_dir.join("quickclipboard.db");
    let dst_db = dst_dir.join("quickclipboard.db");
    let result = (|| -> Result<(), String> {
        let src_images = src_dir.join("clipboard_images");
        let dst_images = dst_dir.join("clipboard_images");
        let src_pin_images = src_dir.join("pin_images");
        let dst_pin_images = dst_dir.join("pin_images");
        let src_image_library = src_dir.join("image_library");
        let dst_image_library = dst_dir.join("image_library");
        let src_app_icons = src_dir.join("app_icons");
        let dst_app_icons = dst_dir.join("app_icons");

        match mode {
            "source_only" => {
                if dst_images.exists() {
                    fs::remove_dir_all(&dst_images).map_err(|e| format!("删除目标图片目录失败: {}", e))?;
                }
                if dst_pin_images.exists() {
                    fs::remove_dir_all(&dst_pin_images).map_err(|e| format!("删除目标贴图目录失败: {}", e))?;
                }
                if dst_image_library.exists() {
                    fs::remove_dir_all(&dst_image_library).map_err(|e| format!("删除目标图库目录失败: {}", e))?;
                }
                if dst_app_icons.exists() {
                    fs::remove_dir_all(&dst_app_icons).map_err(|e| format!("删除目标图标目录失败: {}", e))?;
                }
                if dst_db.exists() {
                    fs::remove_file(&dst_db).map_err(|e| format!("删除目标数据库失败: {}", e))?;
                }
                if src_images.exists() {
                    safe_move_item(&src_images, &dst_images)?;
                }
                if src_pin_images.exists() {
                    safe_move_item(&src_pin_images, &dst_pin_images)?;
                }
                if src_image_library.exists() {
                    safe_move_item(&src_image_library, &dst_image_library)?;
                }
                if src_app_icons.exists() {
                    safe_move_item(&src_app_icons, &dst_app_icons)?;
                }
                if src_db.exists() {
                    safe_move_item(&src_db, &dst_db)?;
                }
            }
            "target_only" => {
                if src_images.exists() {
                    fs::remove_dir_all(&src_images).map_err(|e| format!("删除源图片目录失败: {}", e))?;
                }
                if src_pin_images.exists() {
                    fs::remove_dir_all(&src_pin_images).map_err(|e| format!("删除源贴图目录失败: {}", e))?;
                }
                if src_image_library.exists() {
                    fs::remove_dir_all(&src_image_library).map_err(|e| format!("删除源图库目录失败: {}", e))?;
                }
                if src_app_icons.exists() {
                    fs::remove_dir_all(&src_app_icons).map_err(|e| format!("删除源图标目录失败: {}", e))?;
                }
                if src_db.exists() {
                    fs::remove_file(&src_db).map_err(|e| format!("删除源数据库失败: {}", e))?;
                }
            }
            "merge" => {
                // 源数据优先：先把目标数据合并到源，再移动源到目标
                if src_images.exists() {
                    if !dst_images.exists() { fs::create_dir_all(&dst_images).map_err(|e| e.to_string())?; }
                    if dst_images.exists() { merge_dir_no_overwrite(&dst_images, &src_images)?; }
                    if dst_images.exists() { fs::remove_dir_all(&dst_images).map_err(|e| format!("删除目标图片目录失败: {}", e))?; }
                    safe_move_item(&src_images, &dst_images)?;
                }
                if src_pin_images.exists() {
                    if !dst_pin_images.exists() { fs::create_dir_all(&dst_pin_images).map_err(|e| e.to_string())?; }
                    if dst_pin_images.exists() { merge_dir_no_overwrite(&dst_pin_images, &src_pin_images)?; }
                    if dst_pin_images.exists() { fs::remove_dir_all(&dst_pin_images).map_err(|e| format!("删除目标贴图目录失败: {}", e))?; }
                    safe_move_item(&src_pin_images, &dst_pin_images)?;
                }
                if src_image_library.exists() {
                    if !dst_image_library.exists() { fs::create_dir_all(&dst_image_library).map_err(|e| e.to_string())?; }
                    if dst_image_library.exists() { merge_dir_no_overwrite(&dst_image_library, &src_image_library)?; }
                    if dst_image_library.exists() { fs::remove_dir_all(&dst_image_library).map_err(|e| format!("删除目标图库目录失败: {}", e))?; }
                    safe_move_item(&src_image_library, &dst_image_library)?;
                }
                if src_app_icons.exists() {
                    if !dst_app_icons.exists() { fs::create_dir_all(&dst_app_icons).map_err(|e| e.to_string())?; }
                    if dst_app_icons.exists() { merge_dir_no_overwrite(&dst_app_icons, &src_app_icons)?; }
                    if dst_app_icons.exists() { fs::remove_dir_all(&dst_app_icons).map_err(|e| format!("删除目标图标目录失败: {}", e))?; }
                    safe_move_item(&src_app_icons, &dst_app_icons)?;
                }
                if src_db.exists() {
                    if dst_db.exists() {
                        init_database(src_db.to_str().ok_or("数据库路径无效")?)?;
                        merge_database(&dst_db)?;
                        close_database();
                        fs::remove_file(&dst_db).map_err(|e| format!("删除目标数据库失败: {}", e))?;
                    }
                    safe_move_item(&src_db, &dst_db)?;
                }
            }
            _ => {
                return Err(format!("不支持的迁移模式: {}", mode));
            }
        }

        for name in ["quickclipboard.db-shm", "quickclipboard.db-wal"] {
            let p = dst_dir.join(name);
            if p.exists() { let _ = fs::remove_file(&p); }
            let sp = src_dir.join(name);
            if sp.exists() { let _ = fs::remove_file(&sp); }
        }

        Ok(())
    })();

    // 无论成败都重开 src 库(成功路径 src 库已迁移到 dst 或删除,src 路径
    // 重开会新建空库——但设置未改,下次启动仍读旧路径,数据安全由备份保证;
    // 失败路径 src 库原样保留,重开后本会话 DB 立即恢复可用)。
    if let Some(src_db_str) = src_db.to_str() {
        let _ = init_database(src_db_str);
    }
    result?;
    let _ = crate::services::database::connection::with_connection(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
    });
    Ok(())
}

pub fn export_data_zip(target_path: PathBuf) -> Result<PathBuf, String> {
    let current_dir = get_current_storage_dir()?;
    let _ = crate::services::database::connection::with_connection(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(FULL); PRAGMA wal_checkpoint(TRUNCATE);")
    });
    // D2:close_database() 之后必须保证 init_database() 一定能执行到——旧代码
    // 在关库后散布多个 `?` 早返,任一失败(建目录/建文件/读图库/zip 收尾)都会
    // 跳过末尾 init_database,剪贴板监听与所有 DB 命令本会话全部失效且无恢复
    // 入口。抽闭包收敛:导出体全部错误都经闭包返回,外层无论成败统一重开库。
    close_database();
    let result = (|| -> Result<(), String> {
        let images_dir = current_dir.join("clipboard_images");
        let image_library_dir = current_dir.join("image_library");
        let app_icons_dir = current_dir.join("app_icons");
        let db_files = [
            "quickclipboard.db",
        ];
        let settings_path = crate::services::settings::storage::SettingsStorage::get_settings_path()?;

        if let Some(parent) = target_path.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
        let file = fs::File::create(&target_path).map_err(|e| format!("创建导出文件失败: {}", e))?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);

        for name in &db_files {
            let src = current_dir.join(name);
            if src.exists() {
                let mut f = fs::File::open(&src).map_err(|e| format!("读取文件失败: {}", e))?;
                zip.start_file(name, options).map_err(|e| e.to_string())?;
                std::io::copy(&mut f, &mut zip).map_err(|e| e.to_string())?;
            }
        }

        fn add_dir_to_zip(base: &Path, dir: &Path, prefix: &str, zip: &mut zip::ZipWriter<fs::File>, options: zip::write::SimpleFileOptions) -> Result<(), String> {
            for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                let rel = path.strip_prefix(base).map_err(|e| e.to_string())?;
                if path.is_dir() {
                    add_dir_to_zip(base, &path, prefix, zip, options)?;
                } else {
                    let zip_path = Path::new(prefix).join(rel);
                    let mut f = fs::File::open(&path).map_err(|e| format!("读取文件失败: {}", e))?;
                    let zip_name = zip_path.to_string_lossy();
                    zip.start_file(zip_name.as_ref(), options).map_err(|e| e.to_string())?;
                    std::io::copy(&mut f, zip).map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        }

        if images_dir.exists() {
            add_dir_to_zip(&images_dir, &images_dir, "clipboard_images", &mut zip, options)?;
        }

        if image_library_dir.exists() {
            add_dir_to_zip(&image_library_dir, &image_library_dir, "image_library", &mut zip, options)?;
        }

        if app_icons_dir.exists() {
            add_dir_to_zip(&app_icons_dir, &app_icons_dir, "app_icons", &mut zip, options)?;
        }

        let pin_images_dir = current_dir.join("pin_images");
        if pin_images_dir.exists() {
            add_dir_to_zip(&pin_images_dir, &pin_images_dir, "pin_images", &mut zip, options)?;
        }

        if settings_path.exists() {
            let mut f = fs::File::open(&settings_path).map_err(|e| format!("读取settings失败: {}", e))?;
            zip.start_file("settings.json", options).map_err(|e| e.to_string())?;
            std::io::copy(&mut f, &mut zip).map_err(|e| e.to_string())?;
        }

        zip.finish().map_err(|e| e.to_string())?;
        Ok(())
    })();

    // 无论导出成败都重开数据库,绝不留下关闭态
    let db_path = current_dir.join("quickclipboard.db");
    if db_path.exists() {
        init_database(db_path.to_str().ok_or("数据库路径无效")?)?;
    }

    result?;
    Ok(target_path)
}

#[cfg(test)]
mod tests {
    use super::safe_zip_entry_name;
    use crate::services::system::hotkey::test_utils::{fn_body, source_file, strip_line_comments};

    // D3(备份缺图库/贴图):backup_full_zip 必须覆盖全部会被 reset/替换导入
    // 清理的目录——清理侧删 clipboard_images + image_library + app_icons,
    // 备份若缺 image_library 与 pin_images,执行重置/替换导入前图库与贴图
    // 数据永久丢失,备份里没有可回滚。
    #[test]
    fn backup_full_zip_covers_image_library_and_pin_images() {
        let src = strip_line_comments(&source_file("src/services/data_management/mod.rs"));
        let body = fn_body(&src, "backup_full_zip");
        // 早返守卫必须覆盖新增目录:任一存在就要备份
        for dir in ["image_library", "pin_images"] {
            assert!(
                body.contains(&format!("{dir}_dir")),
                "备份必须声明 {} 目录变量",
                dir
            );
        }
        // 每个被清目录都必须出现在 zip 前缀里(format!("xxx/{{}}", fname))
        for prefix in ["clipboard_images/", "image_library/", "pin_images/", "app_icons/"] {
            assert!(
                body.contains(&format!("{prefix}{{}}")),
                "备份必须写入 {} 前缀的 zip 条目",
                prefix
            );
        }
    }

    // D2(关库早返不重开):export_data_zip close_database 后必须保证
    // init_database 一定执行——错误路径一律经闭包收敛,外层统一重开库。
    // 否则任一导出步骤失败(建目录/建文件/读图库/zip 收尾)都跳过末尾
    // init_database,剪贴板监听与所有 DB 命令本会话全部失效且无恢复入口。
    #[test]
    fn export_data_zip_reopens_database_on_all_paths() {
        let src = strip_line_comments(&source_file("src/services/data_management/mod.rs"));
        let body = fn_body(&src, "export_data_zip");
        let close_pos = body
            .find("close_database()")
            .expect("export 必须先关库");
        let after_close = &body[close_pos..];
        // 闭包收敛:关库后必须出现闭包启动
        let closure_pos = after_close
            .find("(|| -> Result<(), String> {")
            .expect("关库后必须用闭包收敛导出体");
        let closure_seg = &after_close[closure_pos..];
        // 闭包内不得再有裸 ? 早返越过重开(整段都要在闭包内)
        assert!(
            closure_seg.contains("zip.finish().map_err(|e| e.to_string())?;"),
            "导出体全部错误必须在闭包内返回"
        );
        // 闭包结束后、result? 之前必须无条件重开数据库
        let finish_pos = after_close
            .find("zip.finish()")
            .expect("导出体必须收尾 zip");
        let tail = &after_close[finish_pos..];
        assert!(
            tail.contains("init_database(db_path.to_str()"),
            "导出无论成败都必须重开数据库"
        );
        // 负向:init_database 不得藏在闭包内部(只在闭包之后出现)
        let close_to_finish = &after_close[..finish_pos];
        assert!(
            !close_to_finish.contains("init_database"),
            "init_database 必须位于闭包之后(错误路径也能执行到)"
        );
    }

    // H2(换存储目录关库早返不重开):change_storage_dir_internal close_database 后
    // 迁移体必须收进闭包、错误经闭包返回,外层无论成败统一重开 src 库并仅在
    // 成功后才让调用方持久化新设置——否则磁盘满/跨盘复制失败时 DB 永久关闭、
    // 剪贴板监听与所有 DB 命令本会话全部失效,且设置已被改掉。
    #[test]
    fn change_storage_dir_internal_reopens_database_on_all_paths() {
        let src = strip_line_comments(&source_file("src/services/data_management/mod.rs"));
        let body = fn_body(&src, "change_storage_dir_internal");
        let close_pos = body
            .find("close_database()")
            .expect("迁移必须先关库");
        let after_close = &body[close_pos..];
        // 关库后必须用闭包收敛迁移体
        let closure_pos = after_close
            .find("(|| -> Result<(), String> {")
            .expect("关库后必须用闭包收敛迁移体");
        // 闭包必须被调用(不是只定义不执行)
        let invoke_pos = after_close
            .find("})();")
            .expect("闭包必须立即调用");
        assert!(
            closure_pos < invoke_pos,
            "闭包调用必须位于闭包定义之后"
        );
        // init_database 必须在闭包调用之后(失败路径也能执行到重开)
        let reopen_pos = after_close
            .find("init_database(src_db_str)")
            .expect("迁移无论成败都必须重开 src 库");
        assert!(
            invoke_pos < reopen_pos,
            "init_database 必须位于闭包调用之后"
        );
        // result? 必须在重开之后:迁移失败也不吞掉错误,且重开先于早返
        let result_pos = after_close
            .find("result?;")
            .expect("闭包结果必须向调用方传播");
        assert!(
            reopen_pos < result_pos,
            "result? 必须位于重开库之后(错误路径也重开)"
        );
        // 负向:闭包定义到调用之间不得出现闭包外的重开锚点(init_database(src_db_str)
        // 是外层无论成败都执行的重开;merge 分支内临时开库走的是
        // init_database(src_db.to_str()...?)? 带错误传播的合法迁移逻辑,不算)。
        let closure_def_to_call = &after_close[..invoke_pos];
        assert!(
            !closure_def_to_call.contains("init_database(src_db_str)"),
            "闭包外的无条件重开(init_database(src_db_str))不得藏在闭包内部"
        );
    }

    // H1(替换导入关库早返不重开):import_data_zip 的 replace 分支与 D2 export
    // 同款病——close_database 后替换步骤若散布 `?` 早返,失败时既不重开库又已
    // 改掉设置,下次启动指向半替换目录。闭包收敛 + 无条件重开 + 设置后置。
    #[test]
    fn import_replace_reopens_database_before_persisting_settings() {
        let src = strip_line_comments(&source_file("src/services/data_management/mod.rs"));
        let body = fn_body(&src, "import_data_zip");
        let close_pos = body
            .find("close_database()")
            .expect("导入必须先关库");
        let after_close = &body[close_pos..];
        // 关库后必须用闭包收敛替换体
        let closure_pos = after_close
            .find("(|| -> Result<(), String> {")
            .expect("关库后必须用闭包收敛替换体");
        let invoke_pos = after_close
            .find("})();")
            .expect("闭包必须立即调用");
        assert!(closure_pos < invoke_pos, "闭包调用必须位于闭包定义之后");
        // 无论成败都必须重开库(闭包之后)
        let reopen_pos = after_close
            .find("init_database(db_path.to_str()")
            .expect("替换无论成败都必须重开库");
        assert!(invoke_pos < reopen_pos, "init_database 必须位于闭包调用之后");
        // 顺序:重开 -> 传播重开结果 -> 成功后才持久化设置
        let reopen_result_pos = after_close
            .find("reopen?;")
            .expect("重开结果必须向调用方传播");
        let update_pos = after_close
            .find("update_settings(new_settings.clone())")
            .expect("替换与重开都成功后才持久化设置");
        assert!(
            reopen_pos < reopen_result_pos && reopen_result_pos < update_pos,
            "顺序必须为重开 -> 重开结果 -> 持久化设置"
        );
        // 负向:闭包定义到调用之间不得出现 init_database
        let closure_def_to_call = &after_close[..invoke_pos];
        assert!(
            !closure_def_to_call.contains("init_database"),
            "init_database 不得藏在闭包内部"
        );
    }

    // M1(ATTACH 导入无事务):merge_database 的各 merge_* 步骤(先 groups/favorites
    // 再 clipboard/clipboard_data)必须包进同一个事务——任一失败时事务回滚,
    // 前面已落地的合并数据不残留,重复导入不会因半合并越积越多
    // (INSERT OR IGNORE 幂等只兜部分)。DETACH 必须在 commit 之后。
    #[test]
    fn merge_database_wraps_attach_import_in_transaction() {
        let src = strip_line_comments(&source_file("src/services/data_management/mod.rs"));
        let body = fn_body(&src, "merge_database");
        // 事务必须存在且用 unchecked(不自动 BEGIN,因为 ATTACH 后 SQLite
        // 禁止隐式 BEGIN 的事务敏感语句顺序,直接手动管理事务边界)
        let tx_pos = body
            .find("unchecked_transaction()")
            .expect("merge 导入必须包事务");
        // 所有写库步骤都必须在事务内
        for step in [
            "merge_groups_from_importdb(&tx)",
            "merge_favorites_from_importdb(&tx)",
            "merge_clipboard_from_importdb(&tx)",
            "merge_clipboard_data_from_importdb(&tx, &id_mapping)",
            "tx.commit()",
        ] {
            assert!(
                body.find(step).expect("缺少合并步骤") > tx_pos,
                "合并步骤 {} 必须在事务内",
                step
            );
        }
        // DETACH 必须在 commit 之后(否则迁移数据写库失败时会话级 importdb
        // 已拆,无法回滚重试)
        let commit_pos = body.find("tx.commit()").expect("必须提交事务");
        let detach_pos = body
            .find("DETACH DATABASE importdb")
            .expect("合并结束后必须拆离导入库");
        assert!(
            commit_pos < detach_pos,
            "DETACH 必须位于 commit 之后(失败时事务回滚仍需 importdb)"
        );
        // r7-db-4:删除墓碑必须并入合并——源库已删除记录不能在导入后复活。
        // 复用的 record_sync_tombstone_in_conn 自带 LWW 语义,必须出现在事务内。
        let tombstone_pos = body
            .find("record_sync_tombstone_in_conn")
            .expect("合并必须并入源库删除墓碑(sync_tombstones)");
        assert!(
            tx_pos < tombstone_pos && tombstone_pos < commit_pos,
            "墓碑合并必须在事务内、且先于提交(否则导入后删除复活)"
        );
    }

    #[test]
    fn safe_zip_entry_name_allows_normal_relative_paths() {
        for name in [
            "quickclipboard.db",
            "clipboard_images/a1b2c3.png",
            "settings.json",
            "image_library/dir/icon.png",
            "pin_images/a1b2c3.png",
        ] {
            assert_eq!(safe_zip_entry_name(name), Some(name), "合法路径被拒: {}", name);
        }
    }

    // M2(pin_images 数据孤儿):贴图目录必须走完整的"备份/导出/替换导入/合并"
    // 四路闭环——D3 补过备份与清理侧清单,但 export_data_zip 只收 db +
    // clipboard_images + image_library + app_icons,import replace/merge 也只
    // 处理这三目录,pin_images 成了"会被替换清掉、但导出包里没有"的数据孤儿。
    #[test]
    fn export_import_zip_covers_pin_images_on_all_paths() {
        let src = strip_line_comments(&source_file("src/services/data_management/mod.rs"));

        // export_data_zip 必须收 pin_images 目录
        let export = fn_body(&src, "export_data_zip");
        assert!(
            export.contains("add_dir_to_zip(&pin_images_dir, &pin_images_dir, \"pin_images\""),
            "导出 zip 必须收 pin_images"
        );

        // import replace/merge 都必须声明 imported_pin_images 并落地到 target
        let import = fn_body(&src, "import_data_zip");
        let imported_decl = import
            .find("imported_pin_images = temp_root.join(\"pin_images\")")
            .expect("导入必须声明 imported_pin_images");
        let replace_pin = import
            .find("copy_dir_all(&imported_pin_images, &target_pin_images)")
            .expect("replace 分支必须复制贴图到目标");
        assert!(imported_decl < replace_pin, "replace 必须先声明再复制");
        let merge_pin = import
            .find("merge_dir_overwrite(&imported_pin_images, &target_pin_images)")
            .expect("merge 分支必须合并贴图到目标");
        assert!(imported_decl < merge_pin, "merge 必须先声明再合并");
        // 三个 pin_images 操作全部先于 imported_db 落地(同 clipboard_images 序)
        let db_pos = import
            .find("if imported_db.exists()")
            .expect("合并尾段必须先处理目录再合库");
        assert!(
            replace_pin < db_pos && merge_pin < db_pos,
            "贴图落地必须早于数据库合并"
        );
    }

    #[test]
    fn safe_zip_entry_name_rejects_traversal_and_absolute() {
        for name in [
            "../escape.db",
            "a/../../b.png",
            "/etc/passwd",
            "\\windows\\system32\\x",
            "C:\\users\\me\\y.png",
            "C:/users/me/y.png",
            "C:evil.png",
            "\\\\server\\share\\z",
        ] {
            assert_eq!(safe_zip_entry_name(name), None, "危险路径被放行: {:?}", name);
        }
    }

    #[test]
    fn safe_zip_entry_name_rejects_empty() {
        assert_eq!(safe_zip_entry_name(""), None);
    }
}
