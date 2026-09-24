//! 剪贴板基础服务（阶段 1 迁入的部分）：仅含纯数据与解析函数。
//! 文件剪贴板数据结构（FileInfo/FilesData）抽出至此，无平台依赖。

pub mod files;

pub use files::{FileInfo, FilesData};

#[cfg(test)]
mod tests {
    // 护栏:paste 域不得在此处再次定义 is_image_only_html 副本,
    // 统一走 utils::html。Regex 编译亦只允许存在于 utils。
    use crate::utils::is_image_only_html;

    #[test]
    fn paste_reuses_shared_is_image_only_html() {
        assert!(is_image_only_html(Some(r#"<img src="x.png"/>"#)));
    }
}