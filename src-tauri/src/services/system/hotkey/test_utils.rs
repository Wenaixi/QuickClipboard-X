// 源码文本护栏测试共享工具:strip_line_comments / fn_body / source_file
// 在 hotkey.rs、hotkey/global.rs、hotkey/navigation.rs、low_memory/manager.rs
// 各有一份逐字相同副本,finding 7 抽取到此共享模块,各测试模块 import 复用。
// 仅 #[cfg(test)] 编译,模块声明处(父级)也用 #[cfg(test)] 守卫。
use std::fs;

pub fn source_file(relative: &str) -> String {
    fs::read_to_string(format!(
        "{}/{}",
        env!("CARGO_MANIFEST_DIR"),
        relative
    ))
    .expect("读取源码失败")
}

// 剔除整行注释(// 开头),让护栏测试断言只命中可执行代码,
// 注释措辞变化不会导致护栏误报。
pub fn strip_line_comments(src: &str) -> String {
    src.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

// 提取函数体:兼容 pub fn / fn / 带泛型 <F> 三种签名。
// 返回从函数签名到闭合 `}`(含)的切片。
pub fn fn_body<'a>(src: &'a str, name: &str) -> &'a str {
    let markers = [
        format!("pub fn {name}("),
        format!("pub fn {name}<"),
        format!("fn {name}("),
        format!("fn {name}<"),
    ];
    let start = markers
        .iter()
        .filter_map(|m| src.find(m))
        .min()
        .unwrap_or_else(|| panic!("缺 {name}"));
    let rest = &src[start..];
    // 取函数体的闭合 `}` 之后，跳过 `\n}` 两字符的尾巴。
    // fallback 罕见（如找遍到 EOF），直接到 src 末尾。
    let end = rest
        .find("\n}")
        .map(|i| start + i + 2)
        .unwrap_or(src.len());
    &src[start..end]
}
