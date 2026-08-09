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
// 返回从函数签名到闭合 `}`(含)的切片,按花括号深度扫描——
// 首个 `\n}` 在嵌套 fn / 内层 match / 内层 struct 字面量等场景下
// 会过早截断,所以必须维护 depth,直到外层 depth 归零才收尾。
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
    // 从签名 `{` 后的字符开始扫描花括号深度,
    // 跳过字符串/字符字面量内的 `{` `}` 避免误判。
    let open_rel = src[start..]
        .find('{')
        .unwrap_or_else(|| panic!("fn {name} 缺开括号"));
    let body_start = start + open_rel + 1;
    let mut depth: i32 = 1;
    let mut i = body_start;
    let bytes = src.as_bytes();
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[start..=i];
                }
            }
            b'"' => {
                // 跳过双引号字符串字面量(不含转义,因为源码内很少出现 \" ,且
                // 即使错判也只是把内部 `{` 当成外层,但深度计数仍是 +1 / -1 对称,
                // 最终会在真正的外层 `}` 归零——保守行为即可)。
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += 1;
                }
            }
            b'\'' => {
                // 跳过字符字面量 `'{...'`(生命周期/路径段很少出现)
                i += 1;
                if i < bytes.len() && bytes[i] == b'\\' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    panic!("fn {name} 花括号未闭合");
}

#[cfg(test)]
mod tests {
    use super::*;

    // F16: fn_body 必须按花括号深度扫描,不能被嵌套 fn / 内层 match /
    // 内层 struct 字面量等场景的 `\n}` 过早截断。
    #[test]
    fn fn_body_handles_nested_function() {
        let src = r#"
fn outer() {
    fn inner() {
        let x = 1;
    }
    inner();
}
fn after() {}
"#;
        let body = fn_body(src, "outer");
        assert!(body.contains("fn inner"), "嵌套 fn_body 必须包含内层 fn");
        assert!(
            body.contains("let x = 1;"),
            "嵌套 fn_body 必须包含内层函数体"
        );
        assert!(
            !body.contains("fn after"),
            "嵌套 fn_body 不得包含下一个顶层 fn"
        );
    }

    #[test]
    fn fn_body_handles_nested_match() {
        let src = r#"
fn outer() {
    match x {
        Some(y) => {
            let z = 1;
        }
        None => {}
    }
    tail();
}
"#;
        let body = fn_body(src, "outer");
        assert!(body.contains("tail()"), "match 内含 `}}` 必须不截断外层");
        assert!(body.contains("None => {}"));
    }

    #[test]
    fn fn_body_handles_struct_literal() {
        let src = r#"
fn outer() -> Config {
    Config {
        nested: { 1 + 1 },
    }
}
"#;
        let body = fn_body(src, "outer");
        assert!(body.contains("nested:"));
        assert!(body.contains("1 + 1"));
    }

    // F16 兜底:旧实现用首个 `\n}` 截断,嵌套 fn 必截短。
    // 新实现按深度扫描,本测试断言旧实现的"截短到内层 }"行为不会再现。
    #[test]
    fn fn_body_does_not_truncate_at_inner_brace() {
        let src = "fn outer() { fn inner() { let x = 1; } inner(); }";
        let body = fn_body(src, "outer");
        // 旧实现会在 inner 的 `}` 截断,返回 "fn outer() { fn inner() {"
        // 新实现必须包含完整的 "inner();" 调用以及闭合外层 "}"
        assert!(
            body.contains("inner();"),
            "新实现不得在外层闭合前截断——必须包含完整函数体"
        );
        assert!(
            body.ends_with('}'),
            "新实现必须以函数体闭合 `}}` 结尾"
        );
    }
}
