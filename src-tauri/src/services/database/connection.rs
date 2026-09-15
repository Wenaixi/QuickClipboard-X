use rusqlite::{Connection, params};
use parking_lot::Mutex;
use once_cell::sync::Lazy;

pub const MAX_CONTENT_LENGTH: usize = 1600;

// 数据库连接
static DB_CONNECTION: Lazy<Mutex<Option<Connection>>> = 
    Lazy::new(|| Mutex::new(None));

// 初始化数据库连接
pub fn init_database(db_path: &str) -> Result<(), String> {
    let conn = Connection::open(db_path)
        .map_err(|e| format!("打开数据库失败: {}", e))?;
    
    // 创建表结构
    create_tables(&conn)?;

    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA cache_size = 10000;
         PRAGMA temp_store = MEMORY;"
    ).map_err(|e| format!("设置数据库参数失败: {}", e))?;

    // 一次性迁移收藏为全局序号（使用 user_version 控制，仅执行一次）
    migrate_favorites_global_order_if_needed(&conn)?;
    
    let mut db_conn = DB_CONNECTION.lock();
    *db_conn = Some(conn);
    
    Ok(())
}

// 关闭数据库连接
pub fn close_database() {
    let mut db_conn = DB_CONNECTION.lock();
    if db_conn.is_some() {
        *db_conn = None;
    }
}

// 创建数据库表
fn create_tables(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS clipboard (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            content TEXT NOT NULL,
            html_content TEXT,
            content_type TEXT NOT NULL DEFAULT 'text',
            image_id TEXT,
            item_order INTEGER NOT NULL DEFAULT 0,
            is_pinned INTEGER NOT NULL DEFAULT 0,
            uuid TEXT,
            source_device_id TEXT,
            is_remote INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    ).map_err(|e| format!("创建剪贴板表失败: {}", e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS clipboard_data (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            target_kind TEXT NOT NULL,
            target_id TEXT NOT NULL,
            format_name TEXT NOT NULL,
            raw_data BLOB NOT NULL,
            is_primary INTEGER NOT NULL DEFAULT 0,
            format_order INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    ).map_err(|e| format!("创建剪贴板原始数据表失败: {}", e))?;

    let pinned_exists = conn
        .prepare("PRAGMA table_info(clipboard)")
        .and_then(|mut stmt| {
            let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(columns.iter().any(|c| c == "is_pinned"))
        })
        .unwrap_or(false);
    
    if !pinned_exists {
        conn.execute(
            "ALTER TABLE clipboard ADD COLUMN is_pinned INTEGER NOT NULL DEFAULT 0",
            [],
        ).map_err(|e| format!("添加置顶字段失败: {}", e))?;
    }

    let paste_count_exists = conn
        .prepare("PRAGMA table_info(clipboard)")
        .and_then(|mut stmt| {
            let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(columns.iter().any(|c| c == "paste_count"))
        })
        .unwrap_or(false);
    
    if !paste_count_exists {
        conn.execute(
            "ALTER TABLE clipboard ADD COLUMN paste_count INTEGER NOT NULL DEFAULT 0",
            [],
        ).map_err(|e| format!("添加粘贴次数字段失败: {}", e))?;
    }
    
    migrate_clipboard_order(conn);

    conn.execute(
        "CREATE TABLE IF NOT EXISTS favorites (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            html_content TEXT,
            content_type TEXT NOT NULL DEFAULT 'text',
            image_id TEXT,
            source_clipboard_uuid TEXT,
            group_name TEXT NOT NULL DEFAULT '全部',
            item_order INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    ).map_err(|e| format!("创建收藏表失败: {}", e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS groups (
            name TEXT PRIMARY KEY,
            icon TEXT NOT NULL DEFAULT 'ti ti-folder',
            color TEXT NOT NULL DEFAULT '#dc2626',
            order_index INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    ).map_err(|e| format!("创建分组表失败: {}", e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS sync_tombstones (
            collection TEXT NOT NULL,
            item_id TEXT NOT NULL,
            source_device_id TEXT NOT NULL,
            deleted_at INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            PRIMARY KEY (collection, item_id)
        )",
        [],
    ).map_err(|e| format!("创建同步删除状态表失败: {}", e))?;

    let color_exists = conn
        .prepare("PRAGMA table_info(groups)")
        .and_then(|mut stmt| {
            let columns = stmt.query_map([], |row| {
                Ok(row.get::<_, String>(1)?)
            })?;
            Ok(columns.into_iter().any(|col| col.map(|c| c == "color").unwrap_or(false)))
        })
        .unwrap_or(false);
    
    if !color_exists {
        conn.execute(
            "ALTER TABLE groups ADD COLUMN color TEXT NOT NULL DEFAULT '#dc2626'",
            [],
        ).map_err(|e| format!("添加颜色字段失败: {}", e))?;
    }

    let fav_paste_count_exists = conn
        .prepare("PRAGMA table_info(favorites)")
        .and_then(|mut stmt| {
            let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(columns.iter().any(|c| c == "paste_count"))
        })
        .unwrap_or(false);
    
    if !fav_paste_count_exists {
        conn.execute(
            "ALTER TABLE favorites ADD COLUMN paste_count INTEGER NOT NULL DEFAULT 0",
            [],
        ).map_err(|e| format!("添加收藏粘贴次数字段失败: {}", e))?;
    }

    let source_app_exists = conn
        .prepare("PRAGMA table_info(clipboard)")
        .and_then(|mut stmt| {
            let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(columns.iter().any(|c| c == "source_app"))
        })
        .unwrap_or(false);
    
    if !source_app_exists {
        conn.execute("ALTER TABLE clipboard ADD COLUMN source_app TEXT", [])
            .map_err(|e| format!("添加来源应用字段失败: {}", e))?;
    }

    let source_icon_hash_exists = conn
        .prepare("PRAGMA table_info(clipboard)")
        .and_then(|mut stmt| {
            let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(columns.iter().any(|c| c == "source_icon_hash"))
        })
        .unwrap_or(false);
    
    if !source_icon_hash_exists {
        conn.execute("ALTER TABLE clipboard ADD COLUMN source_icon_hash TEXT", [])
            .map_err(|e| format!("添加来源图标哈希字段失败: {}", e))?;
    }

    let clip_char_count_exists = conn
        .prepare("PRAGMA table_info(clipboard)")
        .and_then(|mut stmt| {
            let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(columns.iter().any(|c| c == "char_count"))
        })
        .unwrap_or(false);
    
    if !clip_char_count_exists {
        conn.execute("ALTER TABLE clipboard ADD COLUMN char_count INTEGER", [])
            .map_err(|e| format!("添加剪贴板字符数量字段失败: {}", e))?;
    }

    let clip_uuid_exists = conn
        .prepare("PRAGMA table_info(clipboard)")
        .and_then(|mut stmt| {
            let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(columns.iter().any(|c| c == "uuid"))
        })
        .unwrap_or(false);

    if !clip_uuid_exists {
        conn.execute("ALTER TABLE clipboard ADD COLUMN uuid TEXT", [])
            .map_err(|e| format!("添加剪贴板 UUID 字段失败: {}", e))?;
    }

    let clip_source_device_id_exists = conn
        .prepare("PRAGMA table_info(clipboard)")
        .and_then(|mut stmt| {
            let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(columns.iter().any(|c| c == "source_device_id"))
        })
        .unwrap_or(false);

    if !clip_source_device_id_exists {
        conn.execute("ALTER TABLE clipboard ADD COLUMN source_device_id TEXT", [])
            .map_err(|e| format!("添加剪贴板来源设备字段失败: {}", e))?;
    }

    let clip_is_remote_exists = conn
        .prepare("PRAGMA table_info(clipboard)")
        .and_then(|mut stmt| {
            let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(columns.iter().any(|c| c == "is_remote"))
        })
        .unwrap_or(false);

    if !clip_is_remote_exists {
        conn.execute("ALTER TABLE clipboard ADD COLUMN is_remote INTEGER NOT NULL DEFAULT 0", [])
            .map_err(|e| format!("添加剪贴板远端标记字段失败: {}", e))?;
    }

    let fav_char_count_exists = conn
        .prepare("PRAGMA table_info(favorites)")
        .and_then(|mut stmt| {
            let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(columns.iter().any(|c| c == "char_count"))
        })
        .unwrap_or(false);
    
    if !fav_char_count_exists {
        conn.execute("ALTER TABLE favorites ADD COLUMN char_count INTEGER", [])
            .map_err(|e| format!("添加收藏字符数量字段失败: {}", e))?;
    }

    let fav_source_device_id_exists = conn
        .prepare("PRAGMA table_info(favorites)")
        .and_then(|mut stmt| {
            let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(columns.iter().any(|c| c == "source_device_id"))
        })
        .unwrap_or(false);

    if !fav_source_device_id_exists {
        conn.execute("ALTER TABLE favorites ADD COLUMN source_device_id TEXT", [])
            .map_err(|e| format!("添加收藏来源设备字段失败: {}", e))?;
    }

    let fav_source_clipboard_uuid_exists = conn
        .prepare("PRAGMA table_info(favorites)")
        .and_then(|mut stmt| {
            let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(columns.iter().any(|c| c == "source_clipboard_uuid"))
        })
        .unwrap_or(false);

    if !fav_source_clipboard_uuid_exists {
        conn.execute("ALTER TABLE favorites ADD COLUMN source_clipboard_uuid TEXT", [])
            .map_err(|e| format!("添加收藏来源历史字段失败: {}", e))?;
    }

    let group_source_device_id_exists = conn
        .prepare("PRAGMA table_info(groups)")
        .and_then(|mut stmt| {
            let columns = stmt.query_map([], |row| Ok(row.get::<_, String>(1)?))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(columns.iter().any(|c| c == "source_device_id"))
        })
        .unwrap_or(false);

    if !group_source_device_id_exists {
        conn.execute("ALTER TABLE groups ADD COLUMN source_device_id TEXT", [])
            .map_err(|e| format!("添加分组来源设备字段失败: {}", e))?;
    }

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_clipboard_order ON clipboard(is_pinned DESC, item_order DESC, updated_at DESC)",
        [],
    ).map_err(|e| format!("创建剪贴板排序索引失败: {}", e))?;
    
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_clipboard_content_type ON clipboard(content_type)",
        [],
    ).map_err(|e| format!("创建内容类型索引失败: {}", e))?;

    // M3(唯一索引遇存量重复数据阻断启动):CREATE UNIQUE INDEX 前必须先去重,
    // 否则历史数据里已存在的重复行(如旧版本 NULL uuid 同步 bug 真实产生过的
    // 重复 uuid)会让索引创建失败,连带 init_database 失败,应用起不来。
    // 三个去重 helper 与索引一一对应;索引创建失败只告警,绝不阻断启动——
    // 先能起来访问数据,重复问题另行修复,比锁死启动致命性低得多。
    dedupe_clipboard_data_unique_key(conn);
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_data_unique
         ON clipboard_data(target_kind, target_id, format_name)",
        [],
    ).map_err(|e| format!("创建剪贴板原始数据唯一索引失败: {}", e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_clipboard_data_target_order
         ON clipboard_data(target_kind, target_id, format_order, id)",
        [],
    ).map_err(|e| format!("创建剪贴板原始数据索引失败: {}", e))?;

    dedupe_clipboard_uuid(conn);
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_uuid_unique ON clipboard(uuid) WHERE uuid IS NOT NULL AND uuid <> ''",
        [],
    ).map_err(|e| format!("创建剪贴板 UUID 唯一索引失败: {}", e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_favorites_group ON favorites(group_name, item_order)",
        [],
    ).map_err(|e| format!("创建收藏索引失败: {}", e))?;

    dedupe_favorites_source_clipboard_uuid(conn);
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_favorites_source_clipboard_uuid
         ON favorites(source_clipboard_uuid)
         WHERE source_clipboard_uuid IS NOT NULL AND source_clipboard_uuid <> ''",
        [],
    ).map_err(|e| format!("创建收藏来源历史唯一索引失败: {}", e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_sync_tombstones_deleted_at ON sync_tombstones(deleted_at)",
        [],
    ).map_err(|e| format!("创建同步删除状态索引失败: {}", e))?;
    migrate_favorites_auto_titles(conn);

    Ok(())
}

// M3 去重 helper:三个 UNIQUE 索引前各查一次重复,就地保留"最新"行。
// - clipboard_data(target_kind,target_id,format_name):重复时保留 id 最大的
//   (后写入的原始格式),其余删除——raw data 以最新一次覆盖语义为准。
// - clipboard(uuid):partial 索引豁免 NULL/空串,重复时保留 updated_at 最新、
//   其余 uuid 置 NULL(进入豁免区,不再撞唯一)。
// - favorites(source_clipboard_uuid):partial 索引同款,重复时保留 updated_at
//   最新、其余置 NULL。
// 每个 helper 都是尽力而为:查询/去重失败仅告警,不阻断启动。
fn dedupe_clipboard_data_unique_key(conn: &Connection) {
    let dup: Vec<(String, String, String, i64)> = conn
        .prepare(
            "SELECT target_kind, target_id, format_name, MAX(id)
             FROM clipboard_data
             GROUP BY target_kind, target_id, format_name
             HAVING COUNT(*) > 1",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect()
        })
        .unwrap_or_default();
    for (kind, target, format, keep_id) in dup {
        conn.execute(
            "DELETE FROM clipboard_data
             WHERE target_kind = ?1 AND target_id = ?2 AND format_name = ?3 AND id <> ?4",
            rusqlite::params![kind, target, format, keep_id],
        )
        .ok();
    }
}

fn dedupe_clipboard_uuid(conn: &Connection) {
    let dup: Vec<(String, i64)> = conn
        .prepare(
            "SELECT uuid, MAX(updated_at)
             FROM clipboard
             WHERE uuid IS NOT NULL AND uuid <> ''
             GROUP BY uuid
             HAVING COUNT(*) > 1",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect()
        })
        .unwrap_or_default();
    for (uuid, keep_updated_at) in dup {
        conn.execute(
            "UPDATE clipboard SET uuid = NULL
             WHERE uuid = ?1 AND (updated_at < ?2 OR (updated_at = ?2 AND id <
               (SELECT MIN(id) FROM clipboard WHERE uuid = ?1 AND updated_at = ?2)))",
            rusqlite::params![uuid, keep_updated_at],
        )
        .ok();
    }
}

fn dedupe_favorites_source_clipboard_uuid(conn: &Connection) {
    let dup: Vec<(String, i64)> = conn
        .prepare(
            "SELECT source_clipboard_uuid, MAX(updated_at)
             FROM favorites
             WHERE source_clipboard_uuid IS NOT NULL AND source_clipboard_uuid <> ''
             GROUP BY source_clipboard_uuid
             HAVING COUNT(*) > 1",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                ))
            })?
            .collect()
        })
        .unwrap_or_default();
    for (uuid, keep_updated_at) in dup {
        conn.execute(
            "UPDATE favorites SET source_clipboard_uuid = NULL
             WHERE source_clipboard_uuid = ?1 AND (updated_at < ?2 OR (updated_at = ?2 AND id <
               (SELECT MIN(id) FROM favorites WHERE source_clipboard_uuid = ?1 AND updated_at = ?2)))",
            rusqlite::params![uuid, keep_updated_at],
        )
        .ok();
    }
}

// 迁移 item_order（ASC → DESC）
pub fn migrate_clipboard_order(conn: &Connection) {
    let need_migrate: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM clipboard WHERE item_order < 0) 
         OR (SELECT MAX(item_order) FROM clipboard) < (SELECT COUNT(*) FROM clipboard)",
        [], |row| row.get(0)
    ).unwrap_or(false);
    
    if need_migrate {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id FROM clipboard ORDER BY is_pinned DESC, item_order ASC, updated_at DESC"
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
}

// 一次性迁移收藏为全局序号
fn migrate_favorites_global_order_if_needed(conn: &Connection) -> Result<(), String> {
    let current_version: i32 = conn
        .query_row("PRAGMA user_version;", [], |row| row.get(0))
        .unwrap_or(0);

    if current_version >= 1 {
        return Ok(());
    }

    let favorites_table_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'favorites'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if favorites_table_exists == 0 {
        let _ = conn.execute("PRAGMA user_version = 1;", []);
        return Ok(());
    }

    let favorites_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM favorites", [], |row| row.get(0))
        .unwrap_or(0);
    if favorites_count == 0 {
        let _ = conn.execute("PRAGMA user_version = 1;", []);
        return Ok(());
    }

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("启动收藏序号迁移事务失败: {}", e))?;

    let ids: Vec<String> = {
        let mut stmt = tx
            .prepare(
                "SELECT id FROM favorites 
                 ORDER BY item_order DESC, updated_at DESC, created_at DESC, id ASC",
            )
            .map_err(|e| format!("准备收藏序号迁移查询失败: {}", e))?;

        let ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| format!("查询收藏序号迁移数据失败: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        ids
    };

    let total = ids.len() as i64;
    for (idx, id) in ids.iter().enumerate() {
        let new_order = total - idx as i64;
        tx.execute(
            "UPDATE favorites SET item_order = ?1 WHERE id = ?2",
            params![new_order, id],
        )
        .ok();
    }

    tx.commit()
        .map_err(|e| format!("提交收藏序号迁移事务失败: {}", e))?;

    conn.execute("PRAGMA user_version = 1;", [])
        .map_err(|e| format!("更新 user_version 失败: {}", e))?;

    Ok(())
}

// 获取数据库连接
pub fn with_connection<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&Connection) -> Result<R, rusqlite::Error>,
{
    let conn_guard = DB_CONNECTION.lock();
    let conn = conn_guard.as_ref()
        .ok_or("数据库未初始化")?;
    f(conn).map_err(|e| format!("数据库操作失败: {}", e))
}


// 清理文件和图片类型收藏项的自动生成标题
fn migrate_favorites_auto_titles(conn: &Connection) {
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, title, content FROM favorites WHERE content_type LIKE '%file%' OR content_type LIKE '%image%'"
    ) {
        let items: Vec<(String, String, String)> = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        }).map(|rows| rows.filter_map(|r| r.ok()).collect()).unwrap_or_default();
        
        for (id, title, content) in items {
            let content_chars: Vec<char> = content.chars().collect();
            let expected_title = if content_chars.len() > 50 {
                format!("{}...", content_chars[..50].iter().collect::<String>())
            } else {
                content_chars.iter().collect::<String>()
            };
            
            if title == expected_title {
                conn.execute("UPDATE favorites SET title = '' WHERE id = ?", [&id]).ok();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> rusqlite::Connection {
        rusqlite::Connection::open_in_memory().expect("内存库打开失败")
    }

    // M3:clipboard_data 重复 (target_kind,target_id,format_name) 只保留 id 最大
    // (最新写入),去重后必须能创建 UNIQUE 索引(不去重会 CREATE 失败)。
    #[test]
    fn dedupe_clipboard_data_keeps_latest_then_index_creates() {
        let conn = mem();
        conn.execute_batch(
            "CREATE TABLE clipboard_data (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                target_kind TEXT NOT NULL,
                target_id TEXT NOT NULL,
                format_name TEXT NOT NULL,
                raw_data BLOB NOT NULL,
                is_primary INTEGER NOT NULL DEFAULT 0
            )",
        )
        .unwrap();
        for (id, raw) in [(1i64, b"old".to_vec()), (2, b"new".to_vec())] {
            conn.execute(
                "INSERT INTO clipboard_data (id, target_kind, target_id, format_name, raw_data)
                 VALUES (?1, 'clipboard', '5', 'text/plain', ?2)",
                rusqlite::params![id, raw],
            )
            .unwrap();
        }
        dedupe_clipboard_data_unique_key(&conn);
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM clipboard_data", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 1, "重复格式只保留一条");
        let raw: Vec<u8> = conn
            .query_row("SELECT raw_data FROM clipboard_data", [], |r| r.get(0))
            .unwrap();
        assert_eq!(raw, b"new", "保留 id 最大(最新)的一条");
        // 去重后必须能建 UNIQUE 索引(红:不去重则 CREATE 失败)
        conn.execute(
            "CREATE UNIQUE INDEX idx_test ON clipboard_data(target_kind, target_id, format_name)",
            [],
        )
        .expect("去重后必须能建唯一索引");
    }

    // M3:重复 uuid 行保留 updated_at 最新,其余置 NULL(豁免 partial 索引),
    // 去重后必须能建部分唯一索引。
    #[test]
    fn dedupe_clipboard_uuid_keeps_newest_then_index_creates() {
        let conn = mem();
        conn.execute_batch(
            "CREATE TABLE clipboard (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                content TEXT NOT NULL,
                uuid TEXT,
                updated_at INTEGER NOT NULL
            )",
        )
        .unwrap();
        for (id, uuid, updated_at) in [(1i64, Some("dup"), 100i64), (2, Some("dup"), 200), (3, Some("dup"), 150)] {
            conn.execute(
                "INSERT INTO clipboard (id, content, uuid, updated_at)
                 VALUES (?1, 'c', ?2, ?3)",
                rusqlite::params![id, uuid, updated_at],
            )
            .unwrap();
        }
        dedupe_clipboard_uuid(&conn);
        let kept: i64 = conn
            .query_row("SELECT COUNT(*) FROM clipboard WHERE uuid = 'dup'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kept, 1, "同 uuid 只保留一条");
        let kept_id: i64 = conn
            .query_row("SELECT id FROM clipboard WHERE uuid = 'dup'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kept_id, 2, "保留 updated_at 最新(200)的一条");
        // 其余已置 NULL,partial 索引不含 NULL 行,必能创建
        conn.execute(
            "CREATE UNIQUE INDEX idx_test ON clipboard(uuid) WHERE uuid IS NOT NULL AND uuid <> ''",
            [],
        )
        .expect("去重后必须能建 uuid 部分唯一索引");
    }

    // M3:favorites.source_clipboard_uuid 重复行去重后同款可建索引。
    #[test]
    fn dedupe_favorites_source_uuid_keeps_newest_then_index_creates() {
        let conn = mem();
        conn.execute_batch(
            "CREATE TABLE favorites (
                id TEXT PRIMARY KEY,
                source_clipboard_uuid TEXT,
                updated_at INTEGER NOT NULL
            )",
        )
        .unwrap();
        for (id, uuid, updated_at) in [("a", Some("src1"), 100i64), ("b", Some("src1"), 300), ("c", Some("src1"), 200)] {
            conn.execute(
                "INSERT INTO favorites (id, source_clipboard_uuid, updated_at)
                 VALUES (?1, ?2, ?3)",
                rusqlite::params![id, uuid, updated_at],
            )
            .unwrap();
        }
        dedupe_favorites_source_clipboard_uuid(&conn);
        let kept: i64 = conn
            .query_row("SELECT COUNT(*) FROM favorites WHERE source_clipboard_uuid = 'src1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kept, 1);
        let kept_id: String = conn
            .query_row("SELECT id FROM favorites WHERE source_clipboard_uuid = 'src1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kept_id, "b", "保留 updated_at 最新的一条");
        conn.execute(
            "CREATE UNIQUE INDEX idx_test ON favorites(source_clipboard_uuid)
             WHERE source_clipboard_uuid IS NOT NULL AND source_clipboard_uuid <> ''",
            [],
        )
        .expect("去重后必须能建收藏来源唯一索引");
    }
}
