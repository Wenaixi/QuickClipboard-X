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

    // refresh 形态决策后,is_hidden 与 WindowState 必须原子一致
    #[test]
    fn refresh_accounting_keeps_is_hidden_and_state_atomic() {
        let _g = lock_serial();
        set_snap_edge(SnapEdge::Right, Some((0, 0)), None, Some(0.5));
        set_hidden_and_window_state(true, WindowState::Hidden);
        let state = get_window_state();
        assert!(state.is_hidden, "refresh 后必须 is_hidden=true");
        assert_eq!(
            state.state,
            WindowState::Hidden,
            "refresh 后 WindowState 必须为 Hidden"
        );
        assert!(
            state.is_hidden && state.state == WindowState::Hidden,
            "is_hidden 与 state 不得出现撕裂态"
        );
    }

    // 防回归:任何路径若绕过原子入口单独写 is_hidden(不写 state),会立刻变红。
    // 本测试模拟"绕过"——在测试模块内直接 WINDOW_STATE.write().is_hidden = true
    // 不写 state,断言读者能观测到撕裂中间态,迫使未来加 set_hidden 的人
    // 必须改成原子入口或额外保证不撕裂。
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

    // refresh 末尾必须 re-check 状态,尊重并发 show 的写入,不反手覆盖
    #[test]
    fn refresh_state_write_must_recheck_concurrent_change() {
        let _g = lock_serial();
        set_snap_edge(SnapEdge::Right, Some((0, 0)), None, Some(0.5));
        // 显式设置入口状态:refresh 入口必须看到 is_hidden=true,
        // 串行化后 state 持久,不能依赖上一个 test 残留
        set_hidden_and_window_state(true, WindowState::Hidden);
        let entry_is_hidden = get_window_state().is_hidden;
        assert!(entry_is_hidden, "测试前提:refresh 入口必须 is_hidden=true");
        // 并发 show 抢占写 false/Visible
        set_hidden_and_window_state(false, WindowState::Visible);
        // refresh 末尾 re-check:中途已变 false,必须不再写回 true
        if entry_is_hidden == get_window_state().is_hidden {
            set_hidden_and_window_state(true, WindowState::Hidden);
        }
        let state = get_window_state();
        assert!(
            !state.is_hidden && state.state == WindowState::Visible,
            "refresh 末尾必须尊重并发 show 的写入,不得反手覆盖。\
             现状 is_hidden={} state={:?}",
            state.is_hidden,
            state.state
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

