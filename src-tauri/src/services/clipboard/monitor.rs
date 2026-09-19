use super::capture::ClipboardContent;
use super::processor::process_content;
use super::storage::store_clipboard_item;
use crate::commands::window::{emit_clipboard_updated_event, ClipboardUpdatedEventPayload};
use clipboard_rs::{
    ClipboardContent as RsClipboardContent, ClipboardHandler, ClipboardWatcher,
    ClipboardWatcherContext, WatcherShutdown,
};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

static IS_RUNNING: AtomicBool = AtomicBool::new(false);

static GENERATION: AtomicU64 = AtomicU64::new(0);

// 监听器状态
struct MonitorState {
    watcher_handle: Option<thread::JoinHandle<()>>,
    watcher_shutdown: Option<WatcherShutdown>,
    current_generation: u64,
}

static MONITOR_STATE: Lazy<Arc<Mutex<MonitorState>>> = Lazy::new(|| {
    Arc::new(Mutex::new(MonitorState {
        watcher_handle: None,
        watcher_shutdown: None,
        current_generation: 0,
    }))
});

// 上一次捕获的内容哈希集合（用于去重）
static LAST_CONTENT_HASHES: Lazy<Arc<Mutex<Vec<String>>>> =
    Lazy::new(|| Arc::new(Mutex::new(Vec::new())));

static MONITOR_PAUSE_COUNT: AtomicU64 = AtomicU64::new(0);
static MONITOR_SUPPRESS_UNTIL_MS: AtomicU64 = AtomicU64::new(0);
static CAPTURE_IN_FLIGHT: AtomicBool = AtomicBool::new(false);
static CAPTURE_PENDING: AtomicBool = AtomicBool::new(false);
fn current_time_ms() -> u64 {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as u64
}

pub struct ClipboardMonitorPauseGuard;

impl Drop for ClipboardMonitorPauseGuard {
    fn drop(&mut self) {
        let remaining = MONITOR_PAUSE_COUNT.fetch_sub(1, Ordering::SeqCst);
        // fetch_sub 返回减之前的旧值:最后一个 guard 是 1。
        // 只有确认这是最后一个 guard 后才 take 待捕获标志,
        // 避免嵌套暂停把标志提前清掉,也避免实参先求值吞掉事件。
        if remaining == 1 {
            let has_pending = take_capture_pending();
            if should_schedule_deferred_capture(remaining, is_monitor_running(), has_pending) {
                schedule_capture_worker();
            }
        }
    }
}

pub fn pause_clipboard_monitor_for(duration_ms: u64) -> ClipboardMonitorPauseGuard {
    MONITOR_PAUSE_COUNT.fetch_add(1, Ordering::SeqCst);
    let until = current_time_ms().saturating_add(duration_ms);
    loop {
        let current = MONITOR_SUPPRESS_UNTIL_MS.load(Ordering::SeqCst);
        if current >= until {
            break;
        }
        if MONITOR_SUPPRESS_UNTIL_MS
            .compare_exchange(current, until, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            break;
        }
    }
    ClipboardMonitorPauseGuard
}

pub fn is_clipboard_monitor_paused() -> bool {
    MONITOR_PAUSE_COUNT.load(Ordering::SeqCst) > 0
        || current_time_ms() < MONITOR_SUPPRESS_UNTIL_MS.load(Ordering::SeqCst)
}

/// 决定是否可以在 Drop 末尾启动一次延期 worker：
/// 1) 仅剩一个活跃的暂停 guard (pause_count == 1)；
/// 2) 监听器仍在运行；
/// 3) 期间至少记录过一次待捕获事件。
pub(crate) fn should_schedule_deferred_capture(
    remaining_pause_guards: u64,
    monitor_running: bool,
    has_pending_capture: bool,
) -> bool {
    remaining_pause_guards == 1 && monitor_running && has_pending_capture
}

pub fn take_capture_pending() -> bool {
    CAPTURE_PENDING.swap(false, Ordering::SeqCst)
}

fn hash_clipboard_content(content: &RsClipboardContent) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();

    match content {
        RsClipboardContent::Text(text)
        | RsClipboardContent::Rtf(text)
        | RsClipboardContent::Html(text) => {
            hasher.update(text.as_bytes());
        }
        RsClipboardContent::Files(files) => {
            for file in files {
                let normalized = crate::services::normalize_path_for_hash(file);
                hasher.update(normalized.as_bytes());
                hasher.update([0u8]);
            }
        }
        RsClipboardContent::Other(format_name, data) => {
            // 与 capture::calculate_hash 同口径:主内容哈希优先。
            // 粘贴文本的 payload 是 Other(CF_UNICODETEXT, raw_data),
            // 只哈希 format+data 会与捕获侧(convert 后 Text 内容哈希)失配,
            // 导致自粘贴 fast-path 失效、重复项落到 find_duplicate_item 兜底。
            // raw_data 是 UTF-16/ANSI 字节,直接摘主文本不现实,这里让
            // set_last_hash_contents 侧按"粘贴内容"进 Text/files 走简哈希,
            // Other 仅作为无处安放时的回退,几乎不会命中。
            hasher.update(format_name.as_bytes());
            hasher.update([0u8]);
            hasher.update(data);
        }
        RsClipboardContent::Image(image) => {
            use clipboard_rs::common::RustImage;
            let (width, height) = image.get_size();
            hasher.update(width.to_le_bytes());
            hasher.update(height.to_le_bytes());
        }
    }

    format!("{:x}", hasher.finalize())
}

// 清除上一次内容缓存（用于删除剪贴板项后允许重新添加相同内容）
pub fn clear_last_content_cache() {
    let mut last_hashes = LAST_CONTENT_HASHES.lock();
    last_hashes.clear();
}

// 预设粘贴后的内容哈希缓存（多格式）
// 只按主内容口径哈希(payload 各元素与 capture::calculate_hash 同判据),
// 粘贴写入后系统捕获得到的 Text/Files/纯图 主内容哈希能与预置相等,
// 自粘贴才会被 fast-path 去重掉。
pub fn set_last_hash_contents(contents: &[RsClipboardContent]) {
    let hashes = contents
        .iter()
        .map(|c| match c {
            // Text/Rtf/Html 直接哈希文本(与捕获侧主内容哈希对齐),
            // Files 哈希路径,Other 走 hash_clipboard_content 回退(带
            // HTML/RTF 等多格式的 payload 主元素仍是 Text,只有格式无文本
            // 回收的冷路径会落到 Other)。
            RsClipboardContent::Text(text)
            | RsClipboardContent::Rtf(text)
            | RsClipboardContent::Html(text) => hash_plain_text(text),
            RsClipboardContent::Files(files) => {
                let mut hasher = sha2::Sha256::new();
                use sha2::Digest;
                for file in files {
                    let normalized = crate::services::normalize_path_for_hash(file);
                    hasher.update(normalized.as_bytes());
                    hasher.update([0u8]);
                }
                format!("{:x}", hasher.finalize())
            }
            other => hash_clipboard_content(other),
        })
        .collect::<Vec<_>>();
    let mut last_hashes = LAST_CONTENT_HASHES.lock();
    *last_hashes = hashes;
}

// 纯文本简哈希(与 set_last_hash_text 同口径)
fn hash_plain_text(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

// 剪贴板监听管理器
struct ClipboardMonitorManager {
    generation: u64,
}

impl ClipboardMonitorManager {
    pub fn new(generation: u64) -> Result<Self, String> {
        Ok(ClipboardMonitorManager { generation })
    }
}

impl ClipboardHandler for ClipboardMonitorManager {
    fn on_clipboard_change(&mut self) {
        if !IS_RUNNING.load(Ordering::Relaxed) {
            return;
        }

        if self.generation != GENERATION.load(Ordering::Relaxed) {
            return;
        }

        if let Err(e) = handle_clipboard_change() {
            if !e.contains("重复内容") {
                eprintln!("处理剪贴板内容失败: {}", e);
            }
        }
    }
}

pub fn start_clipboard_monitor() -> Result<(), String> {
    if IS_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(());
    }

    // 启动剪贴板来源监控
    #[cfg(target_os = "windows")]
    crate::services::system::start_clipboard_source_monitor();

    let new_generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    // 在主线程创建 watcher 并拿到关闭通道。创建失败时回滚运行标志与来源监控，
    // 避免留下 IS_RUNNING=true 但无监听线程的撕裂状态。
    let mut watcher = match ClipboardWatcherContext::new() {
        Ok(watcher) => watcher,
        Err(error) => {
            IS_RUNNING.store(false, Ordering::SeqCst);
            #[cfg(target_os = "windows")]
            crate::services::system::stop_clipboard_source_monitor();
            return Err(format!("创建剪贴板监听器失败: {}", error));
        }
    };
    let watcher_shutdown = watcher.get_shutdown_channel();
    watcher.add_handler(ClipboardMonitorManager::new(new_generation)?);

    let mut state = MONITOR_STATE.lock();
    state.current_generation = new_generation;
    state.watcher_shutdown = Some(watcher_shutdown);

    let handle = thread::spawn(move || {
        watcher.start_watch();
        IS_RUNNING.store(false, Ordering::SeqCst);
    });

    state.watcher_handle = Some(handle);
    Ok(())
}

pub fn stop_clipboard_monitor() -> Result<(), String> {
    let was_running = IS_RUNNING.swap(false, Ordering::SeqCst);
    // 推进代数使在飞 capture worker 下一轮自检退出——stop 只 join watcher,
    // worker 是 fire-and-forget 线程,依赖代数检查自行终止。
    // 同时复位在飞标志:stop→start 后若 CAPTURE_IN_FLIGHT 仍为 true,
    // schedule_capture_worker 的 CAS 永远失败,新捕获只置 PENDING 不启动
    // worker,监控永久停摆。
    GENERATION.fetch_add(1, Ordering::SeqCst);
    CAPTURE_IN_FLIGHT.store(false, Ordering::SeqCst);
    if was_running {
        // 停止剪贴板来源监控
        #[cfg(target_os = "windows")]
        crate::services::system::stop_clipboard_source_monitor();

        let _ = CAPTURE_PENDING.swap(false, Ordering::SeqCst);
    }

    // 先丢关闭通道让 start_watch 返回，再 join 回收线程，避免僵尸线程泄漏。
    let (watcher_shutdown, watcher_handle) = {
        let mut state = MONITOR_STATE.lock();
        (state.watcher_shutdown.take(), state.watcher_handle.take())
    };

    drop(watcher_shutdown);
    if let Some(handle) = watcher_handle {
        let _ = handle.join();
    }
    Ok(())
}

pub fn is_monitor_running() -> bool {
    IS_RUNNING.load(Ordering::Relaxed)
}

// 在启动时快照代数;每轮捕获前校验运行标志与本代代数——
// stop_clipboard_monitor 只 join watcher 不 join worker,停止后 worker
// 若继续执行会把剪贴板写进历史;stop→start 后旧代捕获也会污染新代。
fn spawn_capture_worker_loop() {
    let worker_generation = GENERATION.load(Ordering::SeqCst);
    thread::spawn(move || {
        loop {
            if !IS_RUNNING.load(Ordering::SeqCst)
                || GENERATION.load(Ordering::SeqCst) != worker_generation
            {
                break;
            }
            CAPTURE_PENDING.store(false, Ordering::SeqCst);
            process_clipboard_change_once();

            // 处理运行期间积压的变更，合并成下一轮单次捕获，避免并发访问剪贴板。
            if !CAPTURE_PENDING.swap(false, Ordering::SeqCst) {
                break;
            }
        }

        // 尾处理放在循环之外仍属本 worker:只有在飞标志仍归本代所有时
        // 才复位——旧代 worker 完成捕获时 stop→start 已推进代数,这里
        // 不得清掉新代 worker 拉起的在飞标志,否则下一次事件 CAS 又拉起
        // 第三个 worker,新旧代并发 OpenClipboard 丢项。
        if GENERATION.load(Ordering::SeqCst) == worker_generation {
            CAPTURE_IN_FLIGHT.store(false, Ordering::SeqCst);
        }

        // 防止退出与新事件到达之间的竞态，必要时再拉起下一轮 worker。
        if CAPTURE_PENDING.swap(false, Ordering::SeqCst) {
            if CAPTURE_IN_FLIGHT
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                spawn_capture_worker_loop();
            }
        }
    });
}

fn schedule_capture_worker() {
    if CAPTURE_IN_FLIGHT
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        spawn_capture_worker_loop();
    } else {
        CAPTURE_PENDING.store(true, Ordering::SeqCst);
    }
}

fn process_clipboard_change_once() {
    let contents = match ClipboardContent::capture() {
        Ok(contents) => contents,
        Err(e) => {
            eprintln!("抓取剪贴板内容失败: {}", e);
            return;
        }
    };

    if contents.is_empty() {
        return;
    }

    let current_hashes: Vec<String> = contents.iter().map(|c| c.calculate_hash()).collect();

    {
        let last_hashes = LAST_CONTENT_HASHES.lock();
        if *last_hashes == current_hashes {
            return;
        }
    }

    let new_contents: Vec<_> = contents
        .into_iter()
        .filter(|c| {
            let hash = c.calculate_hash();
            let last_hashes = LAST_CONTENT_HASHES.lock();
            !last_hashes.contains(&hash)
        })
        .collect();

    {
        let mut last_hashes = LAST_CONTENT_HASHES.lock();
        *last_hashes = current_hashes;
    }

    if new_contents.is_empty() {
        return;
    }

    let mut any_stored = false;
    for content in new_contents {
        match process_content(content) {
            Ok(processed) => match store_clipboard_item(processed) {
                Ok(id) => {
                    any_stored = true;
                    match crate::services::database::get_clipboard_item_by_id(id) {
                        Ok(Some(mut item)) => {
                            crate::commands::clipboard::hydrate_clipboard_item_for_ui(&mut item);
                            let insert_index = crate::services::database::get_clipboard_item_position(id)
                                .ok()
                                .flatten();
                            let total_count = crate::services::database::get_clipboard_count().ok();
                            let _ = emit_clipboard_updated(ClipboardUpdatedEventPayload {
                                kind: "created".to_string(),
                                item: Some(item.clone()),
                                insert_index,
                                total_count,
                            });
                        }
                        _ => {
                            let _ = emit_clipboard_updated(ClipboardUpdatedEventPayload {
                                kind: "unknown".to_string(),
                                item: None,
                                insert_index: None,
                                total_count: None,
                            });
                        }
                    }

                }
                Err(e) if e.contains("重复内容") || e.contains("已禁止保存图片") => {}
                Err(e) => eprintln!("存储剪贴板内容失败: {}", e),
            },
            Err(e) => eprintln!("处理剪贴板内容失败: {}", e),
        }
    }

    if any_stored {
        if let Some(app) = get_app_handle() {
            crate::services::sync_transfer::lan_notify_local_change(app, "clipboard");
        }
        crate::AppSounds::play_copy_on_success();
    }
}

fn handle_clipboard_change() -> Result<(), String> {
    if is_clipboard_monitor_paused() {
        // 暂停期内不能立即捕获(目标应用会回写剪贴板,会导致自粘贴重复),
        // 但也不能静默 return 丢弃外部复制事件——标记待捕获,
        // 让最后一个暂停 guard Drop 时按 should_schedule_deferred_capture 决定
        // 是否启动延期 worker 重新拉取剪贴板。
        CAPTURE_PENDING.store(true, Ordering::SeqCst);
        return Ok(());
    }
    // 检查应用过滤
    let settings = crate::services::get_settings();

    if crate::services::system::is_front_app_globally_disabled(
        settings.app_filter_enabled,
        &settings.app_filter_blocklist,
        &settings.app_filter_effect,
    ) {
        return Ok(());
    }

    if !crate::services::system::is_current_app_allowed(
        settings.app_filter_enabled,
        &settings.app_filter_blocklist,
    ) {
        return Ok(());
    }

    // 过滤判定通过后才播放复制音效——被过滤应用里复制不泄漏"用户在复制"信号
    crate::AppSounds::play_copy_immediate();

    schedule_capture_worker();

    Ok(())
}

static APP_HANDLE: Lazy<Arc<Mutex<Option<tauri::AppHandle>>>> =
    Lazy::new(|| Arc::new(Mutex::new(None)));

pub fn set_app_handle(handle: tauri::AppHandle) {
    *APP_HANDLE.lock() = Some(handle);
}

pub fn get_app_handle() -> Option<tauri::AppHandle> {
    APP_HANDLE.lock().clone()
}

fn emit_clipboard_updated(payload: ClipboardUpdatedEventPayload) -> Result<(), String> {
    let app_handle = APP_HANDLE.lock();
    let handle = app_handle.as_ref().ok_or("应用未初始化")?;

    if crate::services::low_memory::is_low_memory_mode() {
        let _ = crate::windows::tray::native_menu::update_native_menu(handle);
    }

    emit_clipboard_updated_event(handle, Some(payload))
}

// 预设哈希缓存（文本类型）
pub fn set_last_hash_text(text: &str) {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    let mut last_hashes = LAST_CONTENT_HASHES.lock();
    *last_hashes = vec![hash];
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_item() -> crate::services::database::ClipboardItem {
        crate::services::database::ClipboardItem {
            id: 1,
            uuid: Some("u".to_string()),
            favorite_id: None,
            source_device_id: None,
            is_remote: false,
            content: "c".to_string(),
            html_content: None,
            content_type: "text".to_string(),
            image_id: None,
            item_order: 1,
            is_pinned: false,
            paste_count: 0,
            source_app: None,
            source_icon_hash: None,
            char_count: Some(1),
            created_at: 1,
            updated_at: 1,
        }
    }

    // §10.3 源码护栏：capture worker 必须在每轮捕获前检查运行标志与代数。
    // stop_clipboard_monitor 只 join watcher 不 join worker(worker 是
    // fire-and-forget 线程)——不检查则停止后 worker 仍把剪贴板写进历史,
    // stop→start 后旧代捕获污染新代。stop 侧必须推进代数并复位在飞标志,
    // 否则 stop→start 后 schedule_capture_worker 的 CAS 永远失败,
    // 新捕获只置 PENDING 不启动 worker,监控永久停摆。
    #[test]
    fn capture_worker_checks_generation_and_stop_resets_inflight() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/clipboard/monitor.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 monitor.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        let worker_start = stripped
            .find("fn spawn_capture_worker_loop")
            .expect("缺 spawn_capture_worker_loop");
        let worker_end = stripped[worker_start + 1..]
            .find("\nfn schedule_capture_worker")
            .map(|i| worker_start + 1 + i)
            .unwrap_or(stripped.len());
        let worker_body = &stripped[worker_start..worker_end];
        // 循环守卫必须以运行标志开头,且代数校验用"不匹配比较"明确出现在
        // 同一守卫里——快照行 let worker_generation = GENERATION.load(...)
        // 也含 GENERATION.load 字面,存在性断言区分不了"快照"与"一致性检查",
        // 必须锚定 `!= worker_generation` 比较。
        let guard_start = worker_body
            .find("if !IS_RUNNING.load(Ordering::SeqCst)")
            .expect("worker 循环守卫必须以运行标志检查开头");
        let guard_end = worker_body[guard_start..]
            .find("{\n")
            .map(|i| guard_start + i + 1)
            .unwrap_or(worker_body.len());
        let guard = &worker_body[guard_start..guard_end];
        assert!(
            guard.contains("IS_RUNNING.load("),
            "循环守卫必须校验运行标志,否则停止后 worker 仍把剪贴板写进历史"
        );
        assert!(
            guard.contains("!= worker_generation"),
            "循环守卫必须含代数不匹配比较,否则 stop→start 后旧代捕获污染新代"
        );
        assert!(
            worker_body[..guard_start].contains("worker_generation = GENERATION.load("),
            "worker 必须在守卫前快照代数供一致性比较"
        );
        let capture_call = worker_body
            .find("process_clipboard_change_once(")
            .expect("worker 必须执行捕获");
        assert!(
            guard_start < capture_call,
            "守卫必须早于捕获调用,否则停止后仍执行捕获"
        );

        let stop_start = stripped
            .find("pub fn stop_clipboard_monitor")
            .expect("缺 stop_clipboard_monitor");
        let stop_end = stripped[stop_start + 1..]
            .find("\npub fn ")
            .map(|i| stop_start + 1 + i)
            .unwrap_or(stripped.len());
        let stop_body = &stripped[stop_start..stop_end];
        assert!(
            stop_body.contains("GENERATION.fetch_add(1,"),
            "stop 必须推进代数使在飞 worker 失效"
        );
        assert!(
            stop_body.contains("CAPTURE_IN_FLIGHT.store(false,"),
            "stop 必须复位在飞标志,否则 CAS 永久失败、监控停摆"
        );

        // worker 尾部:在飞标志复位必须受代数保护——旧代 worker 完成捕获
        // 后若无条件 store(false),stop→start 后它会把新代 worker 拉起的
        // 在飞标志清掉,下一次事件 CAS 又拉起第三个 worker,新旧代并发
        // OpenClipboard 丢项。只有仍处于自己代次时才允许复位。用下标
        // 比较锁死"代数守卫在 store(false) 之前"(contains 区分不了顺序)。
        let store_pos = worker_body
            .find("CAPTURE_IN_FLIGHT.store(false,")
            .expect("worker 尾部必须有在飞标志复位");
        let guard_pos = worker_body
            .find("if GENERATION.load(Ordering::SeqCst) == worker_generation")
            .expect("worker 尾部复位前必须有代数校验守卫");
        assert!(
            guard_pos < store_pos,
            "代数校验守卫必须出现在 store(false) 之前,否则旧代清掉新代在飞标志"
        );
    }

    #[test]
    fn deferred_capture_starts_only_after_last_pause_guard() {
        assert!(should_schedule_deferred_capture(1, true, true));
        assert!(!should_schedule_deferred_capture(2, true, true));
        assert!(!should_schedule_deferred_capture(1, false, true));
        assert!(!should_schedule_deferred_capture(1, true, false));
    }

    // §10.3 源码护栏:暂停期间必须把待捕获事件记下,Drop 末尾必须按决策函数
    // 触发延期 worker。否则用户后续复制会永久丢失,等价于把 “暂停时直接
    // return” 重新引入。
    #[test]
    fn pause_handler_marks_pending_and_drop_dispatches_deferred_worker() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/clipboard/monitor.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 monitor.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        let handle_start = stripped
            .find("fn handle_clipboard_change")
            .expect("handle_clipboard_change 必须存在");
        let handle_end = stripped[handle_start + 1..]
            .find("\nfn ")
            .map(|i| handle_start + 1 + i)
            .unwrap_or(stripped.len());
        let handle_body = &stripped[handle_start..handle_end];
        assert!(
            handle_body.contains("CAPTURE_PENDING.store(true"),
            "暂停期内必须记录待捕获标志,禁止直接 return 吞掉事件"
        );

        let drop_start = stripped
            .find("impl Drop for ClipboardMonitorPauseGuard")
            .expect("Drop 实现必须存在");
        let drop_end = stripped[drop_start + 1..]
            .find("\npub fn ")
            .map(|i| drop_start + 1 + i)
            .unwrap_or(stripped.len());
        let drop_body = &stripped[drop_start..drop_end];
        assert!(
            drop_body.contains("should_schedule_deferred_capture("),
            "Drop 末尾必须用决策函数判断是否启动延期 worker"
        );
        assert!(
            drop_body.contains("schedule_capture_worker()"),
            "Drop 末尾必须真正触发延期 worker"
        );
        // fetch_sub 返回减之前的旧值:最后一个 guard Drop 时旧值==1。
        // 再 saturating_sub(1) 会把 1 变成 0,决策函数永远不调度。
        assert!(
            !drop_body.contains("saturating_sub(1)"),
            "Drop 必须把 fetch_sub 旧值直接交给决策函数,禁止再减一次"
        );
        // 函数实参会先求值:take_capture_pending() 若写在 should_schedule 参数里,
        // 非最后一个 guard 也会把待捕获标志清掉,最后一个 guard 反而调度不了。
        let schedule_call = drop_body
            .find("should_schedule_deferred_capture(")
            .expect("缺决策函数调用");
        let schedule_line_end = drop_body[schedule_call..]
            .find('\n')
            .map(|i| schedule_call + i)
            .unwrap_or(drop_body.len());
        let schedule_line = &drop_body[schedule_call..schedule_line_end];
        assert!(
            !schedule_line.contains("take_capture_pending()"),
            "take_capture_pending 不得作为 should_schedule 实参先求值"
        );
    }

    // 源码护栏:复制音效必须晚于应用过滤检查——被过滤应用里复制
    // 仍播音效等于泄漏"用户在复制"信号。断言 play_copy_immediate
    // 必须出现在过滤判定之后。
    #[test]
    fn copy_sound_plays_after_app_filter_check() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/clipboard/monitor.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 monitor.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let start = stripped
            .find("fn handle_clipboard_change")
            .expect("缺 handle_clipboard_change");
        let rest = &stripped[start..];
        let end = rest
            .find("\nfn ")
            .map(|i| start + i)
            .unwrap_or(stripped.len());
        let body = &stripped[start..end];
        let sound_pos = body
            .find("play_copy_immediate()")
            .expect("复制音效调用必须存在");
        let filter_pos = body
            .find("is_front_app_globally_disabled(")
            .expect("必须存在全局应用过滤判定");
        assert!(
            filter_pos < sound_pos,
            "复制音效必须晚于应用过滤检查,否则被过滤应用复制仍播音效"
        );
    }

    // §10.3 源码护栏：停止监听必须先丢关闭通道再 join，否则 start_watch
    // 收不到停止信号、join 永久阻塞，watcher 线程变成僵尸线程（后台资源泄漏）。
    #[test]
    fn stop_monitor_shuts_down_watcher_before_joining() {
        let source = std::fs::read_to_string(format!(
            "{}/src/services/clipboard/monitor.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 monitor.rs");
        let stripped: String = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        let start_pos = stripped
            .find("pub fn start_clipboard_monitor")
            .expect("缺 start_clipboard_monitor");
        let start_end = stripped[start_pos + 1..]
            .find("\npub fn ")
            .map(|i| start_pos + 1 + i)
            .unwrap_or(stripped.len());
        let start_body = &stripped[start_pos..start_end];
        assert!(
            start_body.contains("get_shutdown_channel()"),
            "启动监听必须拿到关闭通道，否则无法主动停止 watcher"
        );
        assert!(
            start_body.contains("watcher_shutdown"),
            "启动监听必须把关闭通道存进 MonitorState 供停止时取用"
        );

        let stop_pos = stripped
            .find("pub fn stop_clipboard_monitor")
            .expect("缺 stop_clipboard_monitor");
        let stop_end = stripped[stop_pos + 1..]
            .find("\npub fn ")
            .map(|i| stop_pos + 1 + i)
            .unwrap_or(stripped.len());
        let stop_body = &stripped[stop_pos..stop_end];
        let drop_pos = stop_body
            .find("drop(watcher_shutdown)")
            .expect("停止监听必须先丢关闭通道");
        let join_pos = stop_body
            .find("handle.join()")
            .expect("停止监听必须 join watcher 线程");
        assert!(
            drop_pos < join_pos,
            "必须先丢关闭通道让 start_watch 返回，再 join；顺序颠倒 join 永久阻塞"
        );
        assert!(
            stop_body.contains("watcher_shutdown.take()"),
            "停止监听必须从 MonitorState 取出关闭通道"
        );
    }
}

// 预设哈希缓存（文件类型）
pub fn set_last_hash_files(content: &str) {
    use sha2::{Digest, Sha256};

    if let Some(json_str) = content.strip_prefix("files:") {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
            let mut hasher = Sha256::new();

            if let Some(files) = json["files"].as_array() {
                for file in files {
                    if let Some(path) = file["path"].as_str() {
                        let normalized = crate::services::normalize_path_for_hash(path);
                        hasher.update(normalized.as_bytes());
                    }
                }
            }

            let hash = format!("{:x}", hasher.finalize());
            let mut last_hashes = LAST_CONTENT_HASHES.lock();
            *last_hashes = vec![hash];
        }
    }
}

// 预设哈希缓存（单文件路径）
pub fn set_last_hash_file(file_path: &str) {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    let normalized = crate::services::normalize_path_for_hash(file_path);
    hasher.update(normalized.as_bytes());

    let hash = format!("{:x}", hasher.finalize());
    let mut last_hashes = LAST_CONTENT_HASHES.lock();
    *last_hashes = vec![hash];
}
