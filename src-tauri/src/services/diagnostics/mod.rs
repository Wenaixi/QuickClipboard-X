// 内存报告(诊断)
//
// 目标:先测量后优化(总体计划 §0.2 铁律 1)。WebView2 全部窗口共享
// browser+GPU 进程,每窗口一个 renderer 进程(约 40-100MB),内存大头在
// renderer 数量。`--memory-report` 入口枚举进程树过滤 msedgewebview2,
// 输出各 renderer 的 PID + 工作集,给窗口治理/低占用改造前后 diff 基线。
//
// 采集用 Process32FirstW 遍历(与 memory/mod.rs 同款),工作集用
// GetProcessMemoryInfo(须以 PROCESS_QUERY_LIMITED_INFORMATION 打开,
// 与 focus.rs 的 OpenProcess 同权限)。输出写 stdout,由调用方(CLI)
// 展示;不落盘、不常驻、不触碰 webview_guard 参数白名单。

#[cfg(windows)]
use windows::Win32::Foundation::CloseHandle;
#[cfg(windows)]
use windows::Win32::System::{
    Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    },
    ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
    Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
};

#[cfg(windows)]
fn collect_webview_renderers() -> Vec<(u32, u64, String)> {
    let mut renderers = Vec::new();

    let snapshot = match unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) } {
        Ok(snapshot) => snapshot,
        Err(_) => return renderers,
    };

    let mut entry = PROCESSENTRY32W::default();
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

    let mut has_entry = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
    while has_entry {
        let name = String::from_utf16_lossy(&entry.szExeFile);
        if is_webview_renderer_name(&name) {
            let (pid, working_set) = process_working_set(entry.th32ProcessID);
            renderers.push((pid, working_set, name));
        }
        has_entry = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }

    let _ = unsafe { CloseHandle(snapshot) };
    renderers
}

#[cfg(windows)]
fn is_webview_renderer_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("msedgewebview2")
}

#[cfg(windows)]
fn process_working_set(pid: u32) -> (u32, u64) {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) };
    let Ok(handle) = handle else {
        return (pid, 0);
    };

    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    let working_set = unsafe {
        if GetProcessMemoryInfo(handle, &mut counters, size).is_ok() {
            counters.WorkingSetSize as u64
        } else {
            0
        }
    };

    let _ = unsafe { CloseHandle(handle) };
    (pid, working_set)
}

/// 输出 WebView renderer 内存报告(CLI 入口调用,返回人类可读文本)
pub fn print_memory_report() {
    #[cfg(windows)]
    {
        println!("=== WebView2 renderer 内存报告 ===");
        let renderers = collect_webview_renderers();
        if renderers.is_empty() {
            println!("未发现 WebView2 renderer 进程(应用可能未启动)");
            return;
        }
        let mut total: u64 = 0;
        for (pid, working_set, name) in &renderers {
            total += working_set;
            println!("PID {pid:>6}  {working_set:>10} bytes  {name}");
        }
        println!("renderer 数量: {:<4}  工作集合计: {} bytes ({} MB)", renderers.len(), total, total / (1024 * 1024));
    }
    #[cfg(not(windows))]
    {
        println!("内存报告仅支持 Windows");
    }
}

#[cfg(test)]
mod tests {
    // 报告必须按 renderer 名称过滤 + 用 GetProcessMemoryInfo 采集工作集
    // (护栏目标:采集路径存在且只统计 WebView renderer,mis-naming 修正
    // 会导致报告失真。源码字面可反证。)
    #[test]
    fn memory_report_filters_and_measures_renderers() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/diagnostics/mod.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("读取内存报告源码失败");
        // 生产代码域:剥离测试模块,避免测试自身的断言字面自命中(§10.4)
        let prod: String = source
            .split("#[cfg(test)]")
            .next()
            .unwrap_or(&source)
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            prod.contains("msedgewebview2"),
            "renderer 过滤必须含 msedgewebview2 名称匹配"
        );
        assert!(
            prod.contains("GetProcessMemoryInfo"),
            "工作集采集必须用 GetProcessMemoryInfo"
        );
        assert!(
            prod.contains("WorkingSetSize"),
            "必须读取 WorkingSetSize"
        );
        assert!(
            prod.contains("CloseHandle"),
            "进程句柄与快照句柄必须关闭"
        );
    }
}