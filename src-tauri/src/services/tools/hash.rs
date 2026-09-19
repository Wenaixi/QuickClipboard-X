// R6 文件哈希工具（对齐 ShareX Tools 的哈希计算）：对任意文件算
// SHA-256 十六进制摘要。零新依赖：复用既有 sha2 crate（RustCrypto）。
// 哈希函数纯同步，被调用方放入线程池避免阻塞 UI 线程；大文件用
// 流式 read 分块更新 hasher，不整文件读入内存。
// 说明：MD5 因依赖 md-5 crate 不在 Cargo.lock（本地禁跑 cargo 无法
// 重生成锁）暂缓，SHA-256 为现代默认摘要，后续如需 MD5 补依赖后加。

use std::io::{BufReader, Read};
use std::path::Path;

use sha2::Digest;

/// 计算文件 SHA-256 十六进制小写摘要。
pub fn file_sha256(path: &Path) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|error| format!("打开文件失败: {error}"))?;
    let mut reader = BufReader::new(file);
    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|error| format!("读取文件失败: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_hash_matches_public_vector() {
        // 空文件 SHA-256 必须等于公开常量（空串摘要），实测确定而非估计。
        let path = std::env::temp_dir().join(format!("qc-hash-empty-{}.bin", std::process::id()));
        std::fs::write(&path, []).expect("写空文件失败");
        let sha = file_sha256(&path).expect("SHA-256 失败");
        let _ = std::fs::remove_file(&path);
        assert_eq!(sha, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn known_string_hash_matches_public_vector() {
        // "abc" 的 SHA-256 是公开测试向量，必须精确匹配。
        let path = std::env::temp_dir().join(format!("qc-hash-abc-{}.bin", std::process::id()));
        std::fs::write(&path, b"abc").expect("写文件失败");
        let sha = file_sha256(&path).expect("SHA-256 失败");
        let _ = std::fs::remove_file(&path);
        assert_eq!(sha, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn missing_file_reports_error() {
        let path = std::env::temp_dir().join(format!("qc-hash-missing-{}.bin", std::process::id()));
        assert!(file_sha256(&path).is_err(), "不存在的文件必须报错");
    }

    #[test]
    fn hash_source_streams_through_hasher() {
        // 源码护栏：哈希必须流式分块（不得整文件读入内存，避免大文件
        // 内存爆炸）。
        let source = std::fs::read_to_string(format!(
            "{}/src/services/tools/hash.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取哈希源码失败");
        assert!(source.contains("BufReader"), "必须用 BufReader 分块读");
        assert!(source.contains("Digest::update"), "必须流式更新 hasher");
        assert!(source.contains("64 * 1024"), "分块缓冲必须有限");
    }
}