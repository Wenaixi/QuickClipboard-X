#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let is_maintenance = std::env::var("QUICKCLIPBOARD_MAINTENANCE").map_or(false, |v| v == "1")
        || std::env::args().any(|a| a == "--maintenance");
    let is_memory_report = std::env::args().any(|a| a == "--memory-report");

    if is_maintenance {
        quickclipboard_lib::install_startup_panic_hook();
        #[cfg(windows)]
        quickclipboard_lib::maintenance::ensure_console();
        quickclipboard_lib::maintenance::run();
        return;
    }

    if is_memory_report {
        // 诊断入口:枚举 WebView renderer 进程并输出工作集,给窗口治理
        // 与低占用改造前后的内存 diff 基线(先测量后优化)。
        #[cfg(windows)]
        quickclipboard_lib::maintenance::ensure_console();
        quickclipboard_lib::diagnostics::print_memory_report();
        return;
    }

    quickclipboard_lib::install_startup_panic_hook();
    quickclipboard_lib::maintenance::ensure_bat_file();
    quickclipboard_lib::run();
}
