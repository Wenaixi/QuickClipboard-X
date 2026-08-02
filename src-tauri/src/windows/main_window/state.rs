use parking_lot::RwLock;
use once_cell::sync::Lazy;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowState {
    Hidden,
    Visible,
    Minimized,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SnapEdge {
    None,
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone)]
pub struct MainWindowState {
    pub state: WindowState,
    pub is_dragging: bool,
    pub is_snapped: bool,
    pub is_hidden: bool,
    pub is_pinned: bool,
    pub snap_edge: SnapEdge,
    pub snap_position: Option<(i32, i32)>,
    pub snap_monitor_id: Option<String>,
    pub snap_ratio: Option<f64>,
    pub clipboard_refresh_pending: bool,
    pub favorites_refresh_pending: bool,
    pub groups_refresh_pending: bool,
}

impl Default for MainWindowState {
    fn default() -> Self {
        Self {
            state: WindowState::Hidden,
            is_dragging: false,
            is_snapped: false,
            is_hidden: false,
            is_pinned: false,
            snap_edge: SnapEdge::None,
            snap_position: None,
            snap_monitor_id: None,
            snap_ratio: None,
            clipboard_refresh_pending: false,
            favorites_refresh_pending: false,
            groups_refresh_pending: false,
        }
    }
}

static WINDOW_STATE: Lazy<RwLock<MainWindowState>> = 
    Lazy::new(|| RwLock::new(MainWindowState::default()));

pub fn get_window_state() -> MainWindowState {
    WINDOW_STATE.read().clone()
}

pub fn set_window_state(state: WindowState) {
    WINDOW_STATE.write().state = state;
}

pub fn is_main_window_visible_for_updates() -> bool {
    let state = WINDOW_STATE.read();
    state.state == WindowState::Visible && !state.is_hidden
}

pub fn set_dragging(is_dragging: bool) {
    WINDOW_STATE.write().is_dragging = is_dragging;
}

pub fn set_snap_edge(
    edge: SnapEdge,
    position: Option<(i32, i32)>,
    monitor_id: Option<String>,
    ratio: Option<f64>,
) {
    let mut state = WINDOW_STATE.write();
    state.is_snapped = edge != SnapEdge::None;
    state.snap_edge = edge;
    state.snap_position = position;
    state.snap_monitor_id = monitor_id;
    state.snap_ratio = ratio;
}

pub fn set_hidden_and_window_state(is_hidden: bool, window_state: WindowState) {
    let mut state = WINDOW_STATE.write();
    state.is_hidden = is_hidden;
    state.state = window_state;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, MutexGuard};
    use std::thread;

    // 串行化所有写 WINDOW_STATE 的测试。
    // 否则并发跑时 hidden_accounting 的 10000 次紧贴写会覆盖其他测试的写入,
    // 读线程观测到撕裂中间态或读到错的 is_hidden。
    // 并发读线程不持锁(只观测单次原子写后的快照),但本静态保证同一时刻
    // 只有一个测试的写线程在跑,读者看到的"写"必属当前测试。
    static SERIAL: Mutex<()> = Mutex::new(());

    fn lock_serial() -> MutexGuard<'static, ()> {
        SERIAL.lock().unwrap_or_else(|e| e.into_inner())
    }

    // 回归:hide_snapped_window 的隐藏记账必须原子。
    // 并发读线程在写入期间不得观察到 is_hidden=true 但 state=Visible 的撕裂态,
    // 否则 toggle(热键/原始输入线程)会误判 should_show,让 hide 静默失败。
    #[test]
    fn hidden_accounting_is_atomic_under_concurrent_reads() {
        let _g = lock_serial();
        // 先置入与 hide 前的状态:is_snapped + is_hidden=false + state=Visible
        set_snap_edge(SnapEdge::Right, Some((0, 0)), None, Some(0.5));
        set_hidden_and_window_state(false, WindowState::Visible);

        let stop = Arc::new(AtomicBool::new(false));
        let stop_read = stop.clone();
        let observed_tear = Arc::new(AtomicBool::new(false));
        let tear_read = observed_tear.clone();

        // 读线程:持续快照,检测撕裂态
        let reader = thread::spawn(move || {
            while !stop_read.load(Ordering::Relaxed) {
                let state = get_window_state();
                if state.is_snapped {
                    if state.is_hidden && state.state != WindowState::Hidden {
                        tear_read.store(true, Ordering::Relaxed);
                    }
                }
            }
        });

        // 写线程:反复原子写入
        for _ in 0..10000 {
            set_hidden_and_window_state(true, WindowState::Hidden);
            set_hidden_and_window_state(false, WindowState::Visible);
        }

        stop.store(true, Ordering::Relaxed);
        reader.join().unwrap();
        assert!(
            !observed_tear.load(Ordering::Relaxed),
            "并发读观察到撕裂中间态:is_hidden=true 但 state != Hidden"
        );
    }

    // 读取 snap.rs 中 refresh_hidden_snapped_window 的函数体源码。
    // 该函数需要 WebviewWindow,无法在 lib test 中构造调用,
    // 故按 §10.3 用源码字面存在性护栏锁死其不变量。
    // 运行时读源(include_str! 自指会编译期递归,不可用)。
    fn refresh_hidden_snapped_window_body() -> String {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/main_window/snap.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 src/windows/main_window/snap.rs 源文件");
        let start = source
            .find("pub fn refresh_hidden_snapped_window")
            .expect("找不到 refresh_hidden_snapped_window 定义");
        let after = &source[start..];
        let end_rel = after[1..]
            .find("\npub fn ")
            .map(|rel| rel + 1)
            .unwrap_or(after.len());
        // 剥掉行注释再匹配:否则注释里出现被测字面会误命中(§10.4 记录的陷阱)
        after[..end_rel]
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // 读取 snap.rs 中 show_snapped_window 的函数体源码。锚点从
    // "pub fn show_snapped_window" 到下一个 "fn begin_animation" 之前——
    // show 之后是 begin_animation / share_animation_version / animate_window_position
    // 三个私有小函数,与隐藏/可见记账无关,截在它们之前刚好收尾。
    fn show_snapped_window_body() -> String {
        let source = std::fs::read_to_string(format!(
            "{}/src/windows/main_window/snap.rs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("找不到 src/windows/main_window/snap.rs 源文件");
        let start = source
            .find("pub fn show_snapped_window")
            .expect("找不到 show_snapped_window 定义");
        let after = &source[start..];
        let end_rel = after[1..]
            .find("\nfn begin_animation")
            .map(|rel| rel + 1)
            .unwrap_or(after.len());
        after[..end_rel]
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // §5.4 护栏:refresh 的隐藏记账必须走原子入口 set_hidden_and_window_state,
    // 禁止裸写 set_hidden —— 两次独立 RwLock 写会让并发 toggle
    // 观测到 is_hidden=true 但 state=Visible 的撕裂态,误判 should_show。
    //
    // 负向断言 !body.contains("set_hidden(") 定位:
    // 当下源码 snap.rs 函数体不含任何 set_hidden( 模式(全仓 fn set_hidden
    // 零命中,该函数自 5a34ae1a 引入原子入口后已删),所以此断言当下始终通过;
    // 它对"未来引入同名字面"防御——任何人加同名新函数会立刻让它失败。
    // 属"未来防御"而非"当下行为护栏";d2150411 替换旧测试时未单独反证
    // 见红,§7.13 主代理用 sed 临时注入已实测 FAILED,可证伪性成立。
    #[test]
    fn refresh_writes_hidden_accounting_through_atomic_entry() {
        let body = refresh_hidden_snapped_window_body();
        assert!(
            body.contains("set_hidden_and_window_state(true, super::state::WindowState::Hidden)"),
            "refresh_hidden_snapped_window 必须用原子入口写隐藏记账"
        );
        // 负向断言既挡 set_hidden( 又挡 set_window_state(:§5.4 撕裂由两次独立
        // RwLock 写引起,任何裸写 set_hidden 或裸写 set_window_state 都会破坏
        // 原子入口的语义;show 路径若照搬 visibility 写法(裸 set_window_state + 裸
        // set_hidden),本断言同样红——确保两条路径都不被反过来。
        // 现场反证:临时 sed 在 refresh 体内注入 set_window_state(test, ...) 见红。
        assert!(
            !body.contains("set_hidden(") && !body.contains("set_window_state("),
            "refresh_hidden_snapped_window 禁止裸写 set_hidden 或 set_window_state,必须走原子入口"
        );
    }

    // 文档化演示:说明"绕过原子入口会撕裂"这个前提本身成立。
    // 注意本测试不拦截生产代码 —— 真正的防回归由
    // refresh_writes_hidden_accounting_through_atomic_entry 的源码护栏负责。
    // 此处只固化撕裂的可观测性,让 set_hidden_and_window_state 的存在理由自解释。
    #[test]
    fn bypass_atomic_entry_must_tear_with_visible_state() {
        let _g = lock_serial();
        set_snap_edge(SnapEdge::Right, Some((0, 0)), None, Some(0.5));
        // 入口:不撕裂的可见态
        set_hidden_and_window_state(false, WindowState::Visible);
        let pre = get_window_state();
        assert!(!pre.is_hidden, "测试前提:入口必须 is_hidden=false");

        // 绕过原子入口:只写 is_hidden,不写 state
        WINDOW_STATE.write().is_hidden = true;

        // 验证:中间态可观测(is_hidden=true 但 state=Visible),即"绕过会撕裂"
        let torn = get_window_state();
        assert!(
            torn.is_hidden && torn.state == WindowState::Visible,
            "绕过原子入口应产生撕裂中间态:is_hidden=true 但 state=Visible \
             (现状 is_hidden={} state={:?})",
            torn.is_hidden,
            torn.state
        );
    }

    // §5.4 + §6 护栏:refresh 末尾写回 Hidden 之前必须 re-check 当前状态,
    // 若中途被并发 show 抢先写 (false, Visible),尊重对方写入不反手覆盖,
    // 否则 toggle 会反复走 hide 路径。
    // 断言 re-check 的 if 字面存在,且下标严格早于原子写 —— 只 contains
    // 无法区分"检查在写之前"还是"写完才检查"。
    #[test]
    fn refresh_rechecks_state_before_writing_hidden_back() {
        let body = refresh_hidden_snapped_window_body();
        let recheck = body
            .find("if super::state::get_window_state().is_hidden {")
            .expect(
                "refresh_hidden_snapped_window 必须在末尾写回前 re-check \
                 get_window_state().is_hidden,尊重并发 show 的写入",
            );
        let write = body
            .find("set_hidden_and_window_state(true, super::state::WindowState::Hidden)")
            .expect("找不到 refresh 末尾的原子写");
        assert!(
            recheck < write,
            "re-check 必须早于原子写回,现状 recheck={} write={}",
            recheck,
            write
        );
    }

    // §5.4 护栏:show_snapped_window 的可见记账必须也走原子入口,杜绝两个对称
    // 路径互相撕裂。refresh 路径有 recheck 兜底(被并发 show 抢先写回则尊重对方),
    // show 路径是主动写方——必须自己原子写 is_hidden=false + state=Visible,
    // 否则并发 refresh 之后可见到 is_hidden=false 但 state=Hidden 的对称撕裂。
    #[test]
    fn show_writes_visible_accounting_through_atomic_entry() {
        let body = show_snapped_window_body();
        assert!(
            body.contains("set_hidden_and_window_state(false, super::state::WindowState::Visible)"),
            "show_snapped_window 必须用原子入口写可见记账"
        );
        // 负向断言同 refresh 路径:不允许裸写 set_hidden / set_window_state。
        // 现场反证:临时 sed 删 snap.rs:816 的原子写行 → 本测试 FAILED。
        assert!(
            !body.contains("set_hidden(") && !body.contains("set_window_state("),
            "show_snapped_window 禁止裸写 set_hidden 或 set_window_state,必须走原子入口"
        );
    }
}

pub fn mark_clipboard_refresh_pending() {
    WINDOW_STATE.write().clipboard_refresh_pending = true;
}

pub fn mark_favorites_refresh_pending() {
    let mut state = WINDOW_STATE.write();
    state.favorites_refresh_pending = true;
    state.clipboard_refresh_pending = true;
}

pub fn mark_groups_refresh_pending() {
    WINDOW_STATE.write().groups_refresh_pending = true;
}

pub fn take_pending_refresh_flags() -> (bool, bool, bool) {
    let mut state = WINDOW_STATE.write();
    let flags = (
        state.clipboard_refresh_pending,
        state.favorites_refresh_pending,
        state.groups_refresh_pending,
    );
    state.clipboard_refresh_pending = false;
    state.favorites_refresh_pending = false;
    state.groups_refresh_pending = false;
    flags
}

pub fn is_snapped() -> bool {
    WINDOW_STATE.read().is_snapped
}

pub fn clear_snap() {
    let mut state = WINDOW_STATE.write();
    state.is_snapped = false;
    state.is_hidden = false;
    state.snap_edge = SnapEdge::None;
    state.snap_position = None;
    state.snap_monitor_id = None;
    state.snap_ratio = None;
}

pub fn set_pinned(is_pinned: bool) {
    WINDOW_STATE.write().is_pinned = is_pinned;
}

pub fn is_pinned() -> bool {
    WINDOW_STATE.read().is_pinned
}

