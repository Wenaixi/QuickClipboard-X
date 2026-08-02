mod models;
pub mod connection;
pub mod clipboard;
pub mod favorites;
pub mod groups;
pub mod tombstones;

pub use models::*;
pub use connection::init_database;
pub use clipboard::*;
pub use favorites::*;
pub use groups::*;
pub use tombstones::*;

/// 把用户搜索词转成带两侧 % 的 LIKE 模式,并转义 \, %, _ 三个通配符。
/// 配合 SQL 端 `ESCAPE '\\'` 使用,避免用户输入 `%`/`_` 被当成通配符。
pub fn like_pattern(input: &str) -> String {
    let escaped = input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{}%", escaped)
}

pub fn webdav_local_sync_parts_signature() -> Result<WebdavLocalSyncSignature, String> {
    connection::with_connection(|conn| {
        let clipboard: (i64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(MAX(updated_at), 0) FROM clipboard",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let favorites: (i64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(MAX(updated_at), 0) FROM favorites",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let groups: (i64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(MAX(updated_at), 0) FROM groups",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let tombstones: (i64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(MAX(deleted_at), 0) FROM sync_tombstones",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(WebdavLocalSyncSignature {
            clipboard: format!("{}:{}", clipboard.0, clipboard.1),
            favorites: format!("{}:{}", favorites.0, favorites.1),
            groups: format!("{}:{}", groups.0, groups.1),
            tombstones: format!("{}:{}", tombstones.0, tombstones.1),
        })
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WebdavLocalSyncSignature {
    pub clipboard: String,
    pub favorites: String,
    pub groups: String,
    pub tombstones: String,
}

#[cfg(test)]
mod like_pattern_tests {
    use super::like_pattern;

    #[test]
    fn like_pattern_wraps_plain_text() {
        assert_eq!(like_pattern("hello"), "%hello%");
    }

    #[test]
    fn like_pattern_escapes_percent() {
        assert_eq!(like_pattern("100%"), r"%100\%%");
    }

    #[test]
    fn like_pattern_escapes_underscore() {
        assert_eq!(like_pattern("a_b"), r"%a\_b%");
    }

    #[test]
    fn like_pattern_escapes_backslash_first() {
        // first escape backslash, then %/_
        assert_eq!(like_pattern(r"a\b%c_d"), r"%a\\b\%c\_d%");
    }
}
